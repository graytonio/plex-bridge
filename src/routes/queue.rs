use std::sync::Arc;

use askama::Template;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    Form,
};
use serde::Deserialize;
use tracing::warn;

use crate::error::{AppError, Result};
use crate::models::config::fetch_config;
use crate::models::sync_job::{
    cancel_job, insert_job, list_jobs, requeue_failed_job, InsertJob, SyncJob,
};
use crate::state::AppState;

#[derive(Template)]
#[template(path = "partials/queue_list.html")]
struct QueueListTemplate {
    jobs: Vec<SyncJob>,
}

pub async fn get_queue_list(State(state): State<Arc<AppState>>) -> Result<Response> {
    let jobs = list_jobs(&state.db).await?;
    Ok(QueueListTemplate { jobs }.into_response())
}

#[derive(Deserialize, Debug)]
pub struct QueueForm {
    pub plex_rating_key: String,
    pub media_type: String,
    pub title: String,
    pub show_title: Option<String>,
    pub season_number: Option<i64>,
    pub episode_number: Option<i64>,
    pub file_size_bytes: Option<i64>,
    pub source_url: String,
}

pub async fn post_queue(
    State(state): State<Arc<AppState>>,
    Form(form): Form<QueueForm>,
) -> Result<Response> {
    let config = fetch_config(&state.db)
        .await?
        .ok_or_else(|| AppError::BadRequest("Not configured".into()))?;

    let destination_path = build_destination_path(&config, &form);

    let job = InsertJob {
        plex_rating_key: form.plex_rating_key,
        media_type: form.media_type,
        title: form.title,
        show_title: form.show_title,
        season_number: form.season_number,
        episode_number: form.episode_number,
        file_size_bytes: form.file_size_bytes.unwrap_or(0),
        destination_path,
        source_url: form.source_url,
    };

    let job_id = insert_job(&state.db, &job)
        .await
        .map_err(AppError::Anyhow)?;

    if let Err(e) = state.job_tx.send(job_id).await {
        warn!("Failed to send job {job_id} to worker: {e}");
    }

    let jobs = list_jobs(&state.db).await?;
    Ok(QueueListTemplate { jobs }.into_response())
}

fn build_destination_path(
    config: &crate::models::config::Config,
    form: &QueueForm,
) -> String {
    let safe_title = sanitize_filename(&form.title);

    if form.media_type == "movie" {
        let base = &config.movies_path;
        format!("{base}/{safe_title}/{safe_title}.mkv")
    } else {
        let base = &config.tv_path;
        let show = form
            .show_title
            .as_deref()
            .map(sanitize_filename)
            .unwrap_or_else(|| safe_title.clone());
        let s = form.season_number.unwrap_or(1);
        let e = form.episode_number.unwrap_or(1);
        format!("{base}/{show}/Season {s:02}/S{s:02}E{e:02} - {safe_title}.mkv")
    }
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}

pub async fn delete_queue_item(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<i64>,
) -> Result<Response> {
    if let Some(token) = state.cancellation_tokens.get(&job_id) {
        token.cancel();
    }

    cancel_job(&state.db, job_id)
        .await
        .map_err(AppError::Anyhow)?;

    let jobs = list_jobs(&state.db).await?;
    Ok(QueueListTemplate { jobs }.into_response())
}

pub async fn retry_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<i64>,
) -> Result<Response> {
    requeue_failed_job(&state.db, job_id)
        .await
        .map_err(AppError::Anyhow)?;

    if let Err(e) = state.job_tx.send(job_id).await {
        warn!("Failed to re-queue job {job_id}: {e}");
    }

    let jobs = list_jobs(&state.db).await?;
    Ok(QueueListTemplate { jobs }.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::config::Config;

    // ── sanitize_filename ─────────────────────────────────────────────────────

    #[test]
    fn sanitize_filename_leaves_safe_chars_unchanged() {
        assert_eq!(sanitize_filename("Breaking Bad"), "Breaking Bad");
        assert_eq!(sanitize_filename("movie.mkv"), "movie.mkv");
        assert_eq!(sanitize_filename("S01E01 - Pilot"), "S01E01 - Pilot");
        assert_eq!(sanitize_filename(""), "");
    }

    #[test]
    fn sanitize_filename_replaces_forward_slash() {
        assert_eq!(sanitize_filename("path/to/file"), "path_to_file");
    }

    #[test]
    fn sanitize_filename_replaces_backslash() {
        assert_eq!(sanitize_filename("path\\file"), "path_file");
    }

    #[test]
    fn sanitize_filename_replaces_colon() {
        assert_eq!(sanitize_filename("Mission: Impossible"), "Mission_ Impossible");
    }

    #[test]
    fn sanitize_filename_replaces_asterisk() {
        assert_eq!(sanitize_filename("file*name"), "file_name");
    }

    #[test]
    fn sanitize_filename_replaces_question_mark() {
        assert_eq!(sanitize_filename("what?"), "what_");
    }

    #[test]
    fn sanitize_filename_replaces_double_quote() {
        assert_eq!(sanitize_filename(r#"say "hello""#), "say _hello_");
    }

    #[test]
    fn sanitize_filename_replaces_angle_brackets() {
        assert_eq!(sanitize_filename("<tag>"), "_tag_");
    }

    #[test]
    fn sanitize_filename_replaces_pipe() {
        assert_eq!(sanitize_filename("a|b"), "a_b");
    }

    #[test]
    fn sanitize_filename_replaces_all_special_chars_at_once() {
        let input = r#"/\:*?"<>|"#;
        let output = sanitize_filename(input);
        assert_eq!(output, "_________");
    }

    #[test]
    fn sanitize_filename_preserves_unicode() {
        assert_eq!(sanitize_filename("Amélie"), "Amélie");
        assert_eq!(sanitize_filename("東京"), "東京");
    }

    // ── build_destination_path ────────────────────────────────────────────────

    fn movie_config() -> Config {
        Config {
            movies_path: "/media/Movies".into(),
            tv_path: "/media/TV".into(),
            ..Default::default()
        }
    }

    fn movie_form(title: &str) -> QueueForm {
        QueueForm {
            plex_rating_key: "rk-1".into(),
            media_type: "movie".into(),
            title: title.into(),
            show_title: None,
            season_number: None,
            episode_number: None,
            file_size_bytes: None,
            source_url: "/parts/1".into(),
        }
    }

    fn episode_form(title: &str, show: &str, season: Option<i64>, ep: Option<i64>) -> QueueForm {
        QueueForm {
            plex_rating_key: "rk-2".into(),
            media_type: "episode".into(),
            title: title.into(),
            show_title: Some(show.into()),
            season_number: season,
            episode_number: ep,
            file_size_bytes: None,
            source_url: "/parts/2".into(),
        }
    }

    #[test]
    fn movie_path_uses_movies_base_dir_and_title() {
        let path = build_destination_path(&movie_config(), &movie_form("Inception"));
        assert_eq!(path, "/media/Movies/Inception/Inception.mkv");
    }

    #[test]
    fn movie_path_sanitizes_colon_in_title() {
        let path = build_destination_path(&movie_config(), &movie_form("Mission: Impossible"));
        assert_eq!(path, "/media/Movies/Mission_ Impossible/Mission_ Impossible.mkv");
    }

    #[test]
    fn movie_path_ends_with_mkv() {
        let path = build_destination_path(&movie_config(), &movie_form("Test"));
        assert!(path.ends_with(".mkv"), "Expected .mkv: {path}");
    }

    #[test]
    fn episode_path_formats_season_and_episode_numbers() {
        let path = build_destination_path(
            &movie_config(),
            &episode_form("Pilot", "Breaking Bad", Some(1), Some(1)),
        );
        assert_eq!(path, "/media/TV/Breaking Bad/Season 01/S01E01 - Pilot.mkv");
    }

    #[test]
    fn episode_path_zero_pads_double_digit_season_and_episode() {
        let path = build_destination_path(
            &movie_config(),
            &episode_form("Ozymandias", "Breaking Bad", Some(5), Some(14)),
        );
        assert_eq!(path, "/media/TV/Breaking Bad/Season 05/S05E14 - Ozymandias.mkv");
    }

    #[test]
    fn episode_path_defaults_to_s01e01_when_numbers_are_none() {
        let path = build_destination_path(
            &movie_config(),
            &episode_form("Pilot", "The Show", None, None),
        );
        assert_eq!(path, "/media/TV/The Show/Season 01/S01E01 - Pilot.mkv");
    }

    #[test]
    fn episode_path_uses_episode_title_as_show_when_show_title_absent() {
        let form = QueueForm {
            plex_rating_key: "rk-3".into(),
            media_type: "episode".into(),
            title: "Pilot".into(),
            show_title: None,
            season_number: Some(1),
            episode_number: Some(1),
            file_size_bytes: None,
            source_url: "/parts/3".into(),
        };
        let path = build_destination_path(&movie_config(), &form);
        assert_eq!(path, "/media/TV/Pilot/Season 01/S01E01 - Pilot.mkv");
    }

    #[test]
    fn episode_path_sanitizes_show_title() {
        let path = build_destination_path(
            &movie_config(),
            &episode_form("Ep 1", "Show: Subtitle", Some(1), Some(1)),
        );
        assert_eq!(path, "/media/TV/Show_ Subtitle/Season 01/S01E01 - Ep 1.mkv");
    }

    #[test]
    fn episode_path_sanitizes_episode_title() {
        let path = build_destination_path(
            &movie_config(),
            &episode_form("Title: Subtitle", "My Show", Some(2), Some(3)),
        );
        assert_eq!(path, "/media/TV/My Show/Season 02/S02E03 - Title_ Subtitle.mkv");
    }

    #[test]
    fn episode_path_uses_tv_base_dir() {
        let path = build_destination_path(
            &movie_config(),
            &episode_form("Ep", "Show", Some(1), Some(1)),
        );
        assert!(path.starts_with("/media/TV/"), "Expected /media/TV/ prefix: {path}");
    }
}
