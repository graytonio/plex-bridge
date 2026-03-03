use std::sync::Arc;

use askama::Template;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
};

use crate::error::Result;
use crate::models::config::fetch_config;
use crate::models::sync_job::completed_rating_keys;
use crate::plex::client::PlexClient;
use crate::plex::types::PlexMetadata;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "browse.html")]
struct BrowseTemplate {
    tab: String,
    movies: Vec<PlexMetadataView>,
    shows: Vec<PlexMetadataView>,
}

#[derive(Template)]
#[template(path = "partials/movie_grid.html")]
struct MovieGridTemplate {
    movies: Vec<PlexMetadataView>,
}

#[derive(Template)]
#[template(path = "partials/show_list.html")]
struct ShowListTemplate {
    shows: Vec<PlexMetadataView>,
}

#[derive(Template)]
#[template(path = "partials/season_episodes.html")]
struct SeasonEpisodesTemplate {
    #[allow(dead_code)]
    show_id: String,
    episodes_by_season: Vec<(PlexMetadataView, Vec<PlexMetadataView>)>,
}

pub struct PlexMetadataView {
    pub rating_key: String,
    pub title: String,
    pub year: Option<i64>,
    pub leaf_count: Option<i64>,
    pub index: Option<i64>,
    pub parent_index: Option<i64>,
    pub grandparent_title: Option<String>,
    pub file_size: i64,
    pub file_key: Option<String>,
    pub is_synced: bool,
}

impl PlexMetadataView {
    fn from_metadata(m: PlexMetadata, synced_keys: &[String]) -> Self {
        let rk = m.rating_key.clone().unwrap_or_default();
        let is_synced = synced_keys.contains(&rk);
        // Compute derived fields before moving m's fields
        let file_size = m.file_size();
        let file_key = m.file_key();
        Self {
            rating_key: rk,
            title: m.title.unwrap_or_default(),
            year: m.year,
            leaf_count: m.leaf_count,
            index: m.index,
            parent_index: m.parent_index,
            grandparent_title: m.grandparent_title,
            file_size,
            file_key,
            is_synced,
        }
    }

    pub fn human_size(&self) -> String {
        let bytes = self.file_size;
        if bytes <= 0 {
            return "—".to_string();
        }
        const GB: i64 = 1_073_741_824;
        const MB: i64 = 1_048_576;
        if bytes >= GB {
            format!("{:.1} GB", bytes as f64 / GB as f64)
        } else {
            format!("{:.0} MB", bytes as f64 / MB as f64)
        }
    }

    pub fn season_label(&self) -> String {
        match self.parent_index {
            Some(s) => format!("S{s:02}"),
            None => "S??".to_string(),
        }
    }

    pub fn episode_label(&self) -> String {
        match self.index {
            Some(e) => format!("E{e:02}"),
            None => "E??".to_string(),
        }
    }

    pub fn has_file(&self) -> bool {
        self.file_key.is_some()
    }

    pub fn file_key_str(&self) -> &str {
        self.file_key.as_deref().unwrap_or("")
    }

    pub fn has_grandparent(&self) -> bool {
        self.grandparent_title.is_some()
    }

    pub fn grandparent_str(&self) -> &str {
        self.grandparent_title.as_deref().unwrap_or("")
    }

    pub fn season_num(&self) -> i64 {
        self.parent_index.unwrap_or(0)
    }

    pub fn episode_num(&self) -> i64 {
        self.index.unwrap_or(0)
    }
}

async fn get_plex_client(state: &Arc<AppState>) -> Result<PlexClient> {
    let config = fetch_config(&state.db)
        .await?
        .ok_or_else(|| crate::error::AppError::BadRequest("Not configured".into()))?;
    Ok(PlexClient::new(
        &config.home_server_url,
        &config.home_plex_token,
    ))
}

async fn find_section(client: &PlexClient, section_type: &str) -> Option<String> {
    let sections = client.libraries().await.ok()?;
    sections
        .into_iter()
        .find(|s| s.media_type.as_deref() == Some(section_type))
        .and_then(|s| s.key)
}

pub async fn get_browse(State(state): State<Arc<AppState>>) -> Result<Response> {
    let config = fetch_config(&state.db).await?;
    if config.as_ref().map(|c| !c.is_configured()).unwrap_or(true) {
        return Ok(Redirect::to("/settings").into_response());
    }

    let client = get_plex_client(&state).await?;
    let synced_keys = completed_rating_keys(&state.db).await?;

    let movies_raw = if let Some(sid) = find_section(&client, "movie").await {
        client.movies(&sid).await.unwrap_or_default()
    } else {
        vec![]
    };

    let shows_raw = if let Some(sid) = find_section(&client, "show").await {
        client.shows(&sid).await.unwrap_or_default()
    } else {
        vec![]
    };

    let movies = movies_raw
        .into_iter()
        .map(|m| PlexMetadataView::from_metadata(m, &synced_keys))
        .collect();
    let shows = shows_raw
        .into_iter()
        .map(|m| PlexMetadataView::from_metadata(m, &synced_keys))
        .collect();

    Ok(BrowseTemplate {
        tab: "movies".to_string(),
        movies,
        shows,
    }
    .into_response())
}

pub async fn get_movies(State(state): State<Arc<AppState>>) -> Result<Response> {
    let client = get_plex_client(&state).await?;
    let synced_keys = completed_rating_keys(&state.db).await?;

    let movies = if let Some(sid) = find_section(&client, "movie").await {
        client.movies(&sid).await.unwrap_or_default()
    } else {
        vec![]
    };

    let movies = movies
        .into_iter()
        .map(|m| PlexMetadataView::from_metadata(m, &synced_keys))
        .collect();

    Ok(MovieGridTemplate { movies }.into_response())
}

pub async fn get_shows(State(state): State<Arc<AppState>>) -> Result<Response> {
    let client = get_plex_client(&state).await?;
    let synced_keys = completed_rating_keys(&state.db).await?;

    let shows = if let Some(sid) = find_section(&client, "show").await {
        client.shows(&sid).await.unwrap_or_default()
    } else {
        vec![]
    };

    let shows = shows
        .into_iter()
        .map(|m| PlexMetadataView::from_metadata(m, &synced_keys))
        .collect();

    Ok(ShowListTemplate { shows }.into_response())
}

pub async fn get_show_seasons(
    State(state): State<Arc<AppState>>,
    Path(show_id): Path<String>,
) -> Result<Response> {
    let client = get_plex_client(&state).await?;
    let synced_keys = completed_rating_keys(&state.db).await?;

    let seasons_raw = client.seasons(&show_id).await.unwrap_or_default();

    let mut episodes_by_season: Vec<(PlexMetadataView, Vec<PlexMetadataView>)> = vec![];
    for season_meta in seasons_raw {
        let season_key = season_meta.rating_key.clone().unwrap_or_default();
        let season_view = PlexMetadataView::from_metadata(season_meta, &synced_keys);
        let eps_raw = client.episodes(&season_key).await.unwrap_or_default();
        let eps = eps_raw
            .into_iter()
            .map(|m| PlexMetadataView::from_metadata(m, &synced_keys))
            .collect();
        episodes_by_season.push((season_view, eps));
    }

    Ok(SeasonEpisodesTemplate {
        show_id,
        episodes_by_season,
    }
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plex::types::{PlexMedia, PlexMetadata, PlexPart};

    fn make_view(
        rating_key: &str,
        file_size: i64,
        file_key: Option<&str>,
        index: Option<i64>,
        parent_index: Option<i64>,
        grandparent_title: Option<&str>,
    ) -> PlexMetadataView {
        PlexMetadataView {
            rating_key: rating_key.into(),
            title: "T".into(),
            year: None,
            leaf_count: None,
            index,
            parent_index,
            grandparent_title: grandparent_title.map(str::to_string),
            file_size,
            file_key: file_key.map(str::to_string),
            is_synced: false,
        }
    }

    // ── from_metadata ─────────────────────────────────────────────────────────

    #[test]
    fn from_metadata_maps_title_rating_key_and_year() {
        let meta = PlexMetadata {
            rating_key: Some("rk-1".into()),
            title: Some("Inception".into()),
            year: Some(2010),
            ..Default::default()
        };
        let view = PlexMetadataView::from_metadata(meta, &[]);
        assert_eq!(view.rating_key, "rk-1");
        assert_eq!(view.title, "Inception");
        assert_eq!(view.year, Some(2010));
    }

    #[test]
    fn from_metadata_defaults_missing_rating_key_to_empty() {
        let meta = PlexMetadata {
            title: Some("Unknown".into()),
            ..Default::default()
        };
        let view = PlexMetadataView::from_metadata(meta, &[]);
        assert_eq!(view.rating_key, "");
    }

    #[test]
    fn from_metadata_is_synced_when_key_in_synced_list() {
        let meta = PlexMetadata {
            rating_key: Some("rk-1".into()),
            ..Default::default()
        };
        let view = PlexMetadataView::from_metadata(meta, &["rk-1".to_string()]);
        assert!(view.is_synced);
    }

    #[test]
    fn from_metadata_not_synced_when_key_absent() {
        let meta = PlexMetadata {
            rating_key: Some("rk-2".into()),
            ..Default::default()
        };
        let view = PlexMetadataView::from_metadata(meta, &["rk-1".to_string()]);
        assert!(!view.is_synced);
    }

    #[test]
    fn from_metadata_extracts_file_size_from_media() {
        let meta = PlexMetadata {
            media: vec![PlexMedia {
                parts: vec![PlexPart {
                    key: "/parts/1".into(),
                    file: "/movies/f.mkv".into(),
                    size: Some(4_000_000_000),
                }],
            }],
            ..Default::default()
        };
        let view = PlexMetadataView::from_metadata(meta, &[]);
        assert_eq!(view.file_size, 4_000_000_000);
    }

    #[test]
    fn from_metadata_extracts_file_key() {
        let meta = PlexMetadata {
            media: vec![PlexMedia {
                parts: vec![PlexPart {
                    key: "/library/parts/99".into(),
                    file: "/f.mkv".into(),
                    size: None,
                }],
            }],
            ..Default::default()
        };
        let view = PlexMetadataView::from_metadata(meta, &[]);
        assert_eq!(view.file_key.as_deref(), Some("/library/parts/99"));
    }

    #[test]
    fn from_metadata_maps_episode_fields() {
        let meta = PlexMetadata {
            index: Some(7),
            parent_index: Some(2),
            grandparent_title: Some("Breaking Bad".into()),
            ..Default::default()
        };
        let view = PlexMetadataView::from_metadata(meta, &[]);
        assert_eq!(view.index, Some(7));
        assert_eq!(view.parent_index, Some(2));
        assert_eq!(view.grandparent_title.as_deref(), Some("Breaking Bad"));
    }

    // ── human_size ───────────────────────────────────────────────────────────

    #[test]
    fn human_size_zero_returns_dash() {
        assert_eq!(make_view("rk", 0, None, None, None, None).human_size(), "—");
    }

    #[test]
    fn human_size_negative_returns_dash() {
        assert_eq!(
            make_view("rk", -1, None, None, None, None).human_size(),
            "—"
        );
    }

    #[test]
    fn human_size_under_1gb_shows_mb() {
        let s = make_view("rk", 500 * 1024 * 1024, None, None, None, None).human_size();
        assert!(s.ends_with(" MB"), "Expected MB: {s}");
        assert!(s.starts_with("500"), "Expected ~500: {s}");
    }

    #[test]
    fn human_size_over_1gb_shows_gb() {
        let s = make_view("rk", 2 * 1_073_741_824, None, None, None, None).human_size();
        assert_eq!(s, "2.0 GB");
    }

    // ── season_label ─────────────────────────────────────────────────────────

    #[test]
    fn season_label_zero_pads_single_digit() {
        assert_eq!(
            make_view("rk", 0, None, None, Some(3), None).season_label(),
            "S03"
        );
    }

    #[test]
    fn season_label_double_digit() {
        assert_eq!(
            make_view("rk", 0, None, None, Some(12), None).season_label(),
            "S12"
        );
    }

    #[test]
    fn season_label_unknown_when_none() {
        assert_eq!(
            make_view("rk", 0, None, None, None, None).season_label(),
            "S??"
        );
    }

    // ── episode_label ────────────────────────────────────────────────────────

    #[test]
    fn episode_label_zero_pads_single_digit() {
        assert_eq!(
            make_view("rk", 0, None, Some(7), None, None).episode_label(),
            "E07"
        );
    }

    #[test]
    fn episode_label_double_digit() {
        assert_eq!(
            make_view("rk", 0, None, Some(14), None, None).episode_label(),
            "E14"
        );
    }

    #[test]
    fn episode_label_unknown_when_none() {
        assert_eq!(
            make_view("rk", 0, None, None, None, None).episode_label(),
            "E??"
        );
    }

    // ── has_file / file_key_str ───────────────────────────────────────────────

    #[test]
    fn has_file_true_when_key_present() {
        assert!(make_view("rk", 0, Some("/parts/1"), None, None, None).has_file());
    }

    #[test]
    fn has_file_false_when_key_absent() {
        assert!(!make_view("rk", 0, None, None, None, None).has_file());
    }

    #[test]
    fn file_key_str_returns_key() {
        assert_eq!(
            make_view("rk", 0, Some("/parts/99"), None, None, None).file_key_str(),
            "/parts/99"
        );
    }

    #[test]
    fn file_key_str_empty_when_none() {
        assert_eq!(
            make_view("rk", 0, None, None, None, None).file_key_str(),
            ""
        );
    }

    // ── has_grandparent / grandparent_str ────────────────────────────────────

    #[test]
    fn has_grandparent_true_when_present() {
        assert!(make_view("rk", 0, None, None, None, Some("Breaking Bad")).has_grandparent());
    }

    #[test]
    fn has_grandparent_false_when_absent() {
        assert!(!make_view("rk", 0, None, None, None, None).has_grandparent());
    }

    #[test]
    fn grandparent_str_returns_value() {
        assert_eq!(
            make_view("rk", 0, None, None, None, Some("The Wire")).grandparent_str(),
            "The Wire"
        );
    }

    #[test]
    fn grandparent_str_empty_when_none() {
        assert_eq!(
            make_view("rk", 0, None, None, None, None).grandparent_str(),
            ""
        );
    }

    // ── season_num / episode_num ─────────────────────────────────────────────

    #[test]
    fn season_num_returns_parent_index() {
        assert_eq!(
            make_view("rk", 0, None, None, Some(4), None).season_num(),
            4
        );
    }

    #[test]
    fn season_num_zero_when_none() {
        assert_eq!(make_view("rk", 0, None, None, None, None).season_num(), 0);
    }

    #[test]
    fn episode_num_returns_index() {
        assert_eq!(
            make_view("rk", 0, None, Some(12), None, None).episode_num(),
            12
        );
    }

    #[test]
    fn episode_num_zero_when_none() {
        assert_eq!(make_view("rk", 0, None, None, None, None).episode_num(), 0);
    }
}
