use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::db::DbPool;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum JobStatus {
    Queued,
    Downloading,
    Complete,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            JobStatus::Queued => "queued",
            JobStatus::Downloading => "downloading",
            JobStatus::Complete => "complete",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "downloading" => JobStatus::Downloading,
            "complete" => JobStatus::Complete,
            "failed" => JobStatus::Failed,
            "cancelled" => JobStatus::Cancelled,
            _ => JobStatus::Queued,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SyncJob {
    pub id: i64,
    pub plex_rating_key: String,
    pub media_type: String,
    pub title: String,
    pub show_title: Option<String>,
    pub season_number: Option<i64>,
    pub episode_number: Option<i64>,
    pub file_size_bytes: i64,
    pub destination_path: String,
    pub source_url: String,
    pub status: String,
    pub bytes_downloaded: i64,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SyncJob {
    pub fn status_enum(&self) -> JobStatus {
        JobStatus::from_str(&self.status)
    }

    pub fn progress_pct(&self) -> f64 {
        if self.file_size_bytes > 0 {
            (self.bytes_downloaded as f64 / self.file_size_bytes as f64 * 100.0).min(100.0)
        } else {
            0.0
        }
    }

    pub fn display_title(&self) -> String {
        if let Some(show) = &self.show_title {
            let s = self.season_number.unwrap_or(0);
            let e = self.episode_number.unwrap_or(0);
            format!("{show} S{s:02}E{e:02} — {}", self.title)
        } else {
            self.title.clone()
        }
    }

    pub fn human_size(&self) -> String {
        let bytes = self.file_size_bytes;
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

    pub fn progress_pct_str(&self) -> String {
        format!("{:.1}", self.progress_pct())
    }

    pub fn has_error(&self) -> bool {
        self.error_message.is_some()
    }

    pub fn error_str(&self) -> &str {
        self.error_message.as_deref().unwrap_or("")
    }
}

#[derive(Debug, Clone)]
pub struct InsertJob {
    pub plex_rating_key: String,
    pub media_type: String,
    pub title: String,
    pub show_title: Option<String>,
    pub season_number: Option<i64>,
    pub episode_number: Option<i64>,
    pub file_size_bytes: i64,
    pub destination_path: String,
    pub source_url: String,
}

pub async fn insert_job(db: &DbPool, job: &InsertJob) -> Result<i64> {
    let id = match db {
        DbPool::Sqlite(pool) => {
            let row: (i64,) = sqlx::query_as(
                "INSERT INTO sync_jobs (plex_rating_key, media_type, title, show_title, season_number, episode_number, file_size_bytes, destination_path, source_url, status)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'queued')
                 RETURNING id"
            )
            .bind(&job.plex_rating_key)
            .bind(&job.media_type)
            .bind(&job.title)
            .bind(&job.show_title)
            .bind(job.season_number)
            .bind(job.episode_number)
            .bind(job.file_size_bytes)
            .bind(&job.destination_path)
            .bind(&job.source_url)
            .fetch_one(pool)
            .await?;
            row.0
        }
        DbPool::Postgres(pool) => {
            let row: (i64,) = sqlx::query_as(
                "INSERT INTO sync_jobs (plex_rating_key, media_type, title, show_title, season_number, episode_number, file_size_bytes, destination_path, source_url, status)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'queued')
                 RETURNING id"
            )
            .bind(&job.plex_rating_key)
            .bind(&job.media_type)
            .bind(&job.title)
            .bind(&job.show_title)
            .bind(job.season_number)
            .bind(job.episode_number)
            .bind(job.file_size_bytes)
            .bind(&job.destination_path)
            .bind(&job.source_url)
            .fetch_one(pool)
            .await?;
            row.0
        }
    };
    Ok(id)
}

pub async fn list_jobs(db: &DbPool) -> Result<Vec<SyncJob>> {
    let sql = "SELECT id, plex_rating_key, media_type, title, show_title, season_number, episode_number, file_size_bytes, destination_path, source_url, status, bytes_downloaded, error_message, created_at, updated_at FROM sync_jobs ORDER BY created_at DESC";
    let jobs = match db {
        DbPool::Sqlite(pool) => {
            sqlx::query_as::<_, SyncJob>(sql).fetch_all(pool).await?
        }
        DbPool::Postgres(pool) => {
            sqlx::query_as::<_, SyncJob>(sql).fetch_all(pool).await?
        }
    };
    Ok(jobs)
}

pub async fn get_job(db: &DbPool, id: i64) -> Result<Option<SyncJob>> {
    let jobs = match db {
        DbPool::Sqlite(pool) => {
            sqlx::query_as::<_, SyncJob>(
                "SELECT id, plex_rating_key, media_type, title, show_title, season_number, episode_number, file_size_bytes, destination_path, source_url, status, bytes_downloaded, error_message, created_at, updated_at FROM sync_jobs WHERE id = ?"
            )
            .bind(id)
            .fetch_optional(pool)
            .await?
        }
        DbPool::Postgres(pool) => {
            sqlx::query_as::<_, SyncJob>(
                "SELECT id, plex_rating_key, media_type, title, show_title, season_number, episode_number, file_size_bytes, destination_path, source_url, status, bytes_downloaded, error_message, created_at, updated_at FROM sync_jobs WHERE id = $1"
            )
            .bind(id)
            .fetch_optional(pool)
            .await?
        }
    };
    Ok(jobs)
}

pub async fn update_job_status(db: &DbPool, id: i64, status: JobStatus) -> Result<()> {
    let s = status.as_str();
    match db {
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE sync_jobs SET status = ?, updated_at = datetime('now') WHERE id = ?")
                .bind(s)
                .bind(id)
                .execute(pool)
                .await?;
        }
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE sync_jobs SET status = $1, updated_at = NOW() WHERE id = $2")
                .bind(s)
                .bind(id)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

pub async fn update_job_progress(db: &DbPool, id: i64, bytes_downloaded: i64) -> Result<()> {
    match db {
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE sync_jobs SET bytes_downloaded = ?, updated_at = datetime('now') WHERE id = ?")
                .bind(bytes_downloaded)
                .bind(id)
                .execute(pool)
                .await?;
        }
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE sync_jobs SET bytes_downloaded = $1, updated_at = NOW() WHERE id = $2")
                .bind(bytes_downloaded)
                .bind(id)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

pub async fn update_job_error(db: &DbPool, id: i64, error_message: &str) -> Result<()> {
    match db {
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE sync_jobs SET status = 'failed', error_message = ?, updated_at = datetime('now') WHERE id = ?")
                .bind(error_message)
                .bind(id)
                .execute(pool)
                .await?;
        }
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE sync_jobs SET status = 'failed', error_message = $1, updated_at = NOW() WHERE id = $2")
                .bind(error_message)
                .bind(id)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

pub async fn cancel_job(db: &DbPool, id: i64) -> Result<()> {
    match db {
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE sync_jobs SET status = 'cancelled', updated_at = datetime('now') WHERE id = ? AND status IN ('queued', 'downloading')")
                .bind(id)
                .execute(pool)
                .await?;
        }
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE sync_jobs SET status = 'cancelled', updated_at = NOW() WHERE id = $1 AND status IN ('queued', 'downloading')")
                .bind(id)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

pub async fn clear_completed_jobs(db: &DbPool) -> Result<()> {
    match db {
        DbPool::Sqlite(pool) => {
            sqlx::query("DELETE FROM sync_jobs WHERE status = 'complete'")
                .execute(pool)
                .await?;
        }
        DbPool::Postgres(pool) => {
            sqlx::query("DELETE FROM sync_jobs WHERE status = 'complete'")
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

pub async fn requeue_failed_job(db: &DbPool, id: i64) -> Result<()> {
    match db {
        DbPool::Sqlite(pool) => {
            sqlx::query("UPDATE sync_jobs SET status = 'queued', bytes_downloaded = 0, error_message = NULL, updated_at = datetime('now') WHERE id = ? AND status = 'failed'")
                .bind(id)
                .execute(pool)
                .await?;
        }
        DbPool::Postgres(pool) => {
            sqlx::query("UPDATE sync_jobs SET status = 'queued', bytes_downloaded = 0, error_message = NULL, updated_at = NOW() WHERE id = $1 AND status = 'failed'")
                .bind(id)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

pub async fn completed_rating_keys(db: &DbPool) -> Result<Vec<String>> {
    let keys: Vec<(String,)> = match db {
        DbPool::Sqlite(pool) => {
            sqlx::query_as("SELECT plex_rating_key FROM sync_jobs WHERE status = 'complete'")
                .fetch_all(pool)
                .await?
        }
        DbPool::Postgres(pool) => {
            sqlx::query_as("SELECT plex_rating_key FROM sync_jobs WHERE status = 'complete'")
                .fetch_all(pool)
                .await?
        }
    };
    Ok(keys.into_iter().map(|(k,)| k).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_test_db;

    // ── Test helpers ──────────────────────────────────────────────────────────

    fn movie_job() -> SyncJob {
        SyncJob {
            id: 1,
            plex_rating_key: "rk-movie-1".into(),
            media_type: "movie".into(),
            title: "Inception".into(),
            show_title: None,
            season_number: None,
            episode_number: None,
            file_size_bytes: 4 * 1024 * 1024 * 1024, // 4 GB
            destination_path: "/movies/Inception/Inception.mkv".into(),
            source_url: "/library/parts/1".into(),
            status: "queued".into(),
            bytes_downloaded: 0,
            error_message: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn episode_job() -> SyncJob {
        SyncJob {
            id: 2,
            plex_rating_key: "rk-ep-1".into(),
            media_type: "episode".into(),
            title: "Pilot".into(),
            show_title: Some("Breaking Bad".into()),
            season_number: Some(1),
            episode_number: Some(1),
            file_size_bytes: 800 * 1024 * 1024, // 800 MB
            destination_path: "/tv/Breaking Bad/Season 01/S01E01 - Pilot.mkv".into(),
            source_url: "/library/parts/2".into(),
            status: "downloading".into(),
            bytes_downloaded: 400 * 1024 * 1024,
            error_message: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn insert_job_fixture() -> InsertJob {
        InsertJob {
            plex_rating_key: "rk-1".into(),
            media_type: "movie".into(),
            title: "Test Movie".into(),
            show_title: None,
            season_number: None,
            episode_number: None,
            file_size_bytes: 1_000_000_000,
            destination_path: "/movies/Test Movie/Test Movie.mkv".into(),
            source_url: "/library/parts/99".into(),
        }
    }

    // ── status_enum ───────────────────────────────────────────────────────────

    #[test]
    fn status_enum_maps_all_status_strings() {
        let statuses = [
            ("queued", JobStatus::Queued),
            ("downloading", JobStatus::Downloading),
            ("complete", JobStatus::Complete),
            ("failed", JobStatus::Failed),
            ("cancelled", JobStatus::Cancelled),
        ];
        let mut job = movie_job();
        for (s, expected) in statuses {
            job.status = s.into();
            assert_eq!(job.status_enum(), expected, "status_enum mismatch for '{s}'");
        }
    }

    // ── JobStatus ─────────────────────────────────────────────────────────────

    #[test]
    fn job_status_as_str_all_variants() {
        assert_eq!(JobStatus::Queued.as_str(), "queued");
        assert_eq!(JobStatus::Downloading.as_str(), "downloading");
        assert_eq!(JobStatus::Complete.as_str(), "complete");
        assert_eq!(JobStatus::Failed.as_str(), "failed");
        assert_eq!(JobStatus::Cancelled.as_str(), "cancelled");
    }

    #[test]
    fn job_status_from_str_known_values() {
        assert_eq!(JobStatus::from_str("queued"), JobStatus::Queued);
        assert_eq!(JobStatus::from_str("downloading"), JobStatus::Downloading);
        assert_eq!(JobStatus::from_str("complete"), JobStatus::Complete);
        assert_eq!(JobStatus::from_str("failed"), JobStatus::Failed);
        assert_eq!(JobStatus::from_str("cancelled"), JobStatus::Cancelled);
    }

    #[test]
    fn job_status_from_str_unknown_defaults_to_queued() {
        assert_eq!(JobStatus::from_str(""), JobStatus::Queued);
        assert_eq!(JobStatus::from_str("COMPLETE"), JobStatus::Queued); // case-sensitive
        assert_eq!(JobStatus::from_str("pending"), JobStatus::Queued);
        assert_eq!(JobStatus::from_str("random"), JobStatus::Queued);
    }

    #[test]
    fn job_status_roundtrip_via_string() {
        for status in [
            JobStatus::Queued,
            JobStatus::Downloading,
            JobStatus::Complete,
            JobStatus::Failed,
            JobStatus::Cancelled,
        ] {
            assert_eq!(JobStatus::from_str(status.as_str()), status);
        }
    }

    // ── progress_pct ─────────────────────────────────────────────────────────

    #[test]
    fn progress_pct_zero_when_no_file_size() {
        let mut job = movie_job();
        job.file_size_bytes = 0;
        job.bytes_downloaded = 0;
        assert_eq!(job.progress_pct(), 0.0);
    }

    #[test]
    fn progress_pct_zero_when_nothing_downloaded() {
        let job = movie_job(); // bytes_downloaded = 0
        assert_eq!(job.progress_pct(), 0.0);
    }

    #[test]
    fn progress_pct_fifty_percent() {
        let mut job = movie_job();
        job.file_size_bytes = 1000;
        job.bytes_downloaded = 500;
        assert!((job.progress_pct() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn progress_pct_one_hundred_when_complete() {
        let mut job = movie_job();
        job.file_size_bytes = 1000;
        job.bytes_downloaded = 1000;
        assert!((job.progress_pct() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn progress_pct_clamped_to_100_when_over() {
        let mut job = movie_job();
        job.file_size_bytes = 1000;
        job.bytes_downloaded = 1500; // more than total
        assert_eq!(job.progress_pct(), 100.0);
    }

    #[test]
    fn progress_pct_str_one_decimal_place() {
        let mut job = movie_job();
        job.file_size_bytes = 3;
        job.bytes_downloaded = 1;
        // 1/3 ≈ 33.333...
        assert_eq!(job.progress_pct_str(), "33.3");
    }

    // ── human_size ───────────────────────────────────────────────────────────

    #[test]
    fn human_size_zero_returns_dash() {
        let mut job = movie_job();
        job.file_size_bytes = 0;
        assert_eq!(job.human_size(), "—");
    }

    #[test]
    fn human_size_negative_returns_dash() {
        let mut job = movie_job();
        job.file_size_bytes = -1;
        assert_eq!(job.human_size(), "—");
    }

    #[test]
    fn human_size_under_1gb_shows_mb() {
        let mut job = movie_job();
        job.file_size_bytes = 500 * 1024 * 1024; // 500 MB
        let s = job.human_size();
        assert!(s.ends_with(" MB"), "Expected MB, got: {s}");
        assert!(s.starts_with("500"), "Expected ~500, got: {s}");
    }

    #[test]
    fn human_size_over_1gb_shows_gb() {
        let mut job = movie_job();
        job.file_size_bytes = 2 * 1024 * 1024 * 1024; // 2 GB
        let s = job.human_size();
        assert!(s.ends_with(" GB"), "Expected GB, got: {s}");
        assert!(s.starts_with("2.0"), "Expected 2.0, got: {s}");
    }

    #[test]
    fn human_size_exactly_1gb() {
        let mut job = movie_job();
        job.file_size_bytes = 1_073_741_824; // exactly 1 GiB
        let s = job.human_size();
        assert_eq!(s, "1.0 GB");
    }

    #[test]
    fn human_size_fractional_gb() {
        let mut job = movie_job();
        job.file_size_bytes = (1.5 * 1_073_741_824.0) as i64;
        let s = job.human_size();
        assert!(s.ends_with(" GB"), "Expected GB, got: {s}");
        assert!(s.starts_with("1.5"), "Expected 1.5, got: {s}");
    }

    // ── display_title ────────────────────────────────────────────────────────

    #[test]
    fn display_title_movie_is_just_title() {
        let job = movie_job();
        assert_eq!(job.display_title(), "Inception");
    }

    #[test]
    fn display_title_episode_includes_show_and_numbers() {
        let job = episode_job();
        assert_eq!(job.display_title(), "Breaking Bad S01E01 — Pilot");
    }

    #[test]
    fn display_title_episode_pads_single_digit_numbers() {
        let mut job = episode_job();
        job.season_number = Some(2);
        job.episode_number = Some(9);
        assert_eq!(job.display_title(), "Breaking Bad S02E09 — Pilot");
    }

    #[test]
    fn display_title_episode_with_none_numbers_shows_zeros() {
        let mut job = episode_job();
        job.season_number = None;
        job.episode_number = None;
        assert_eq!(job.display_title(), "Breaking Bad S00E00 — Pilot");
    }

    // ── error helpers ────────────────────────────────────────────────────────

    #[test]
    fn has_error_false_when_no_error() {
        let job = movie_job();
        assert!(!job.has_error());
    }

    #[test]
    fn has_error_true_when_error_present() {
        let mut job = movie_job();
        job.error_message = Some("connection refused".into());
        assert!(job.has_error());
    }

    #[test]
    fn error_str_empty_when_no_error() {
        let job = movie_job();
        assert_eq!(job.error_str(), "");
    }

    #[test]
    fn error_str_returns_message_when_present() {
        let mut job = movie_job();
        job.error_message = Some("timeout after 30s".into());
        assert_eq!(job.error_str(), "timeout after 30s");
    }

    // ── DB integration tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn insert_job_returns_positive_id() {
        let db = create_test_db().await;
        let id = insert_job(&db, &insert_job_fixture()).await.unwrap();
        assert!(id > 0);
    }

    #[tokio::test]
    async fn insert_job_ids_are_sequential() {
        let db = create_test_db().await;
        let id1 = insert_job(&db, &insert_job_fixture()).await.unwrap();
        let id2 = insert_job(&db, &insert_job_fixture()).await.unwrap();
        assert!(id2 > id1);
    }

    #[tokio::test]
    async fn get_job_returns_none_for_missing_id() {
        let db = create_test_db().await;
        let result = get_job(&db, 9999).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_job_returns_inserted_job() {
        let db = create_test_db().await;
        let fixture = insert_job_fixture();
        let id = insert_job(&db, &fixture).await.unwrap();
        let job = get_job(&db, id).await.unwrap().expect("should exist");

        assert_eq!(job.id, id);
        assert_eq!(job.plex_rating_key, "rk-1");
        assert_eq!(job.media_type, "movie");
        assert_eq!(job.title, "Test Movie");
        assert_eq!(job.status, "queued");
        assert_eq!(job.bytes_downloaded, 0);
        assert!(job.error_message.is_none());
    }

    #[tokio::test]
    async fn list_jobs_empty_on_fresh_db() {
        let db = create_test_db().await;
        let jobs = list_jobs(&db).await.unwrap();
        assert!(jobs.is_empty());
    }

    #[tokio::test]
    async fn list_jobs_returns_all_inserted_jobs() {
        let db = create_test_db().await;
        insert_job(&db, &insert_job_fixture()).await.unwrap();
        insert_job(&db, &insert_job_fixture()).await.unwrap();
        let jobs = list_jobs(&db).await.unwrap();
        assert_eq!(jobs.len(), 2);
    }

    #[tokio::test]
    async fn update_job_status_changes_status() {
        let db = create_test_db().await;
        let id = insert_job(&db, &insert_job_fixture()).await.unwrap();

        update_job_status(&db, id, JobStatus::Downloading).await.unwrap();
        let job = get_job(&db, id).await.unwrap().unwrap();
        assert_eq!(job.status, "downloading");

        update_job_status(&db, id, JobStatus::Complete).await.unwrap();
        let job = get_job(&db, id).await.unwrap().unwrap();
        assert_eq!(job.status, "complete");
    }

    #[tokio::test]
    async fn update_job_progress_sets_bytes_downloaded() {
        let db = create_test_db().await;
        let id = insert_job(&db, &insert_job_fixture()).await.unwrap();

        update_job_progress(&db, id, 500_000).await.unwrap();
        let job = get_job(&db, id).await.unwrap().unwrap();
        assert_eq!(job.bytes_downloaded, 500_000);
    }

    #[tokio::test]
    async fn update_job_error_sets_failed_status_and_message() {
        let db = create_test_db().await;
        let id = insert_job(&db, &insert_job_fixture()).await.unwrap();

        update_job_error(&db, id, "connection timed out").await.unwrap();
        let job = get_job(&db, id).await.unwrap().unwrap();
        assert_eq!(job.status, "failed");
        assert_eq!(job.error_message.as_deref(), Some("connection timed out"));
    }

    #[tokio::test]
    async fn cancel_job_when_queued_sets_cancelled() {
        let db = create_test_db().await;
        let id = insert_job(&db, &insert_job_fixture()).await.unwrap();
        // Default status is "queued"
        cancel_job(&db, id).await.unwrap();
        let job = get_job(&db, id).await.unwrap().unwrap();
        assert_eq!(job.status, "cancelled");
    }

    #[tokio::test]
    async fn cancel_job_when_downloading_sets_cancelled() {
        let db = create_test_db().await;
        let id = insert_job(&db, &insert_job_fixture()).await.unwrap();
        update_job_status(&db, id, JobStatus::Downloading).await.unwrap();

        cancel_job(&db, id).await.unwrap();
        let job = get_job(&db, id).await.unwrap().unwrap();
        assert_eq!(job.status, "cancelled");
    }

    #[tokio::test]
    async fn cancel_job_when_complete_does_not_change_status() {
        let db = create_test_db().await;
        let id = insert_job(&db, &insert_job_fixture()).await.unwrap();
        update_job_status(&db, id, JobStatus::Complete).await.unwrap();

        cancel_job(&db, id).await.unwrap();
        let job = get_job(&db, id).await.unwrap().unwrap();
        assert_eq!(job.status, "complete"); // unchanged
    }

    #[tokio::test]
    async fn cancel_job_when_failed_does_not_change_status() {
        let db = create_test_db().await;
        let id = insert_job(&db, &insert_job_fixture()).await.unwrap();
        update_job_error(&db, id, "err").await.unwrap();

        cancel_job(&db, id).await.unwrap();
        let job = get_job(&db, id).await.unwrap().unwrap();
        assert_eq!(job.status, "failed"); // unchanged
    }

    #[tokio::test]
    async fn clear_completed_removes_only_complete_jobs() {
        let db = create_test_db().await;
        let id_complete = insert_job(&db, &insert_job_fixture()).await.unwrap();
        let id_queued = insert_job(&db, &insert_job_fixture()).await.unwrap();
        let id_failed = insert_job(&db, &insert_job_fixture()).await.unwrap();

        update_job_status(&db, id_complete, JobStatus::Complete).await.unwrap();
        update_job_error(&db, id_failed, "err").await.unwrap();

        clear_completed_jobs(&db).await.unwrap();

        assert!(get_job(&db, id_complete).await.unwrap().is_none());
        assert!(get_job(&db, id_queued).await.unwrap().is_some());
        assert!(get_job(&db, id_failed).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn requeue_failed_job_resets_to_queued() {
        let db = create_test_db().await;
        let id = insert_job(&db, &insert_job_fixture()).await.unwrap();
        update_job_progress(&db, id, 123_456).await.unwrap();
        update_job_error(&db, id, "network error").await.unwrap();

        requeue_failed_job(&db, id).await.unwrap();

        let job = get_job(&db, id).await.unwrap().unwrap();
        assert_eq!(job.status, "queued");
        assert_eq!(job.bytes_downloaded, 0);
        assert!(job.error_message.is_none());
    }

    #[tokio::test]
    async fn requeue_failed_job_does_not_affect_non_failed_job() {
        let db = create_test_db().await;
        let id = insert_job(&db, &insert_job_fixture()).await.unwrap();
        // Job starts as "queued"
        requeue_failed_job(&db, id).await.unwrap();
        let job = get_job(&db, id).await.unwrap().unwrap();
        assert_eq!(job.status, "queued"); // unchanged (wasn't failed)
    }

    #[tokio::test]
    async fn completed_rating_keys_returns_only_complete_keys() {
        let db = create_test_db().await;

        let mut job1 = insert_job_fixture();
        job1.plex_rating_key = "key-complete-1".into();
        let id1 = insert_job(&db, &job1).await.unwrap();
        update_job_status(&db, id1, JobStatus::Complete).await.unwrap();

        let mut job2 = insert_job_fixture();
        job2.plex_rating_key = "key-complete-2".into();
        let id2 = insert_job(&db, &job2).await.unwrap();
        update_job_status(&db, id2, JobStatus::Complete).await.unwrap();

        let mut job3 = insert_job_fixture();
        job3.plex_rating_key = "key-queued".into();
        insert_job(&db, &job3).await.unwrap();

        let keys = completed_rating_keys(&db).await.unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"key-complete-1".to_string()));
        assert!(keys.contains(&"key-complete-2".to_string()));
        assert!(!keys.contains(&"key-queued".to_string()));
    }

    #[tokio::test]
    async fn completed_rating_keys_empty_when_no_complete_jobs() {
        let db = create_test_db().await;
        let id = insert_job(&db, &insert_job_fixture()).await.unwrap();
        update_job_error(&db, id, "err").await.unwrap();

        let keys = completed_rating_keys(&db).await.unwrap();
        assert!(keys.is_empty());
    }

    #[tokio::test]
    async fn episode_job_roundtrip_preserves_all_fields() {
        let db = create_test_db().await;
        let job = InsertJob {
            plex_rating_key: "rk-ep-99".into(),
            media_type: "episode".into(),
            title: "Ozymandias".into(),
            show_title: Some("Breaking Bad".into()),
            season_number: Some(5),
            episode_number: Some(14),
            file_size_bytes: 2_000_000_000,
            destination_path: "/tv/Breaking Bad/Season 05/S05E14 - Ozymandias.mkv".into(),
            source_url: "/library/parts/500".into(),
        };
        let id = insert_job(&db, &job).await.unwrap();
        let fetched = get_job(&db, id).await.unwrap().unwrap();

        assert_eq!(fetched.media_type, "episode");
        assert_eq!(fetched.title, "Ozymandias");
        assert_eq!(fetched.show_title.as_deref(), Some("Breaking Bad"));
        assert_eq!(fetched.season_number, Some(5));
        assert_eq!(fetched.episode_number, Some(14));
        assert_eq!(fetched.file_size_bytes, 2_000_000_000);
    }
}
