use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use bytes::Bytes;
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::models::config::fetch_config;
use crate::models::sync_job::{
    get_job, update_job_error, update_job_progress, update_job_status, JobStatus,
};
use crate::plex::client::PlexClient;
use crate::state::{AppState, ProgressEvent};

pub async fn process_single_job(state: Arc<AppState>, job_id: i64) -> Result<()> {
    let job = match get_job(&state.db, job_id).await? {
        Some(j) => j,
        None => {
            warn!("Job {job_id} not found");
            return Ok(());
        }
    };

    // Check if job was cancelled before we started
    if job.status == "cancelled" {
        return Ok(());
    }

    let config = match fetch_config(&state.db).await? {
        Some(c) => c,
        None => {
            update_job_error(&state.db, job_id, "No configuration found").await?;
            return Ok(());
        }
    };

    // Create cancellation token for this job
    let cancel_token = CancellationToken::new();
    state.cancellation_tokens.insert(job_id, cancel_token.clone());

    let result = download_file(&state, &job, &config, &cancel_token).await;

    // Remove cancellation token
    state.cancellation_tokens.remove(&job_id);

    match result {
        Ok(_) => {
            update_job_status(&state.db, job_id, JobStatus::Complete).await?;
            info!("Job {job_id} completed: {}", job.title);

            let _ = state.progress_tx.send(ProgressEvent {
                job_id,
                bytes_downloaded: job.file_size_bytes,
                total_bytes: job.file_size_bytes,
                speed_bps: 0.0,
                status: "complete".to_string(),
                error_message: None,
            });

            // Trigger local Plex library scan
            if !config.local_server_url.is_empty() && !config.local_plex_token.is_empty() {
                let local_client =
                    PlexClient::new(&config.local_server_url, &config.local_plex_token);
                let section_id = if job.media_type == "movie" { "1" } else { "2" };
                if let Err(e) = local_client.refresh_library(section_id).await {
                    warn!("Failed to refresh local library: {e}");
                }
            }
        }
        Err(ref _e) if cancel_token.is_cancelled() => {
            warn!("Job {job_id} was cancelled");
            let _ = state.progress_tx.send(ProgressEvent {
                job_id,
                bytes_downloaded: job.bytes_downloaded,
                total_bytes: job.file_size_bytes,
                speed_bps: 0.0,
                status: "cancelled".to_string(),
                error_message: None,
            });
        }
        Err(e) => {
            let msg = e.to_string();
            error!("Job {job_id} download error: {msg}");
            update_job_error(&state.db, job_id, &msg).await?;
            let _ = state.progress_tx.send(ProgressEvent {
                job_id,
                bytes_downloaded: job.bytes_downloaded,
                total_bytes: job.file_size_bytes,
                speed_bps: 0.0,
                status: "failed".to_string(),
                error_message: Some(msg),
            });
        }
    }

    Ok(())
}

async fn download_file(
    state: &Arc<AppState>,
    job: &crate::models::sync_job::SyncJob,
    config: &crate::models::config::Config,
    cancel_token: &CancellationToken,
) -> Result<()> {
    let job_id = job.id;

    // Mark as downloading
    update_job_status(&state.db, job_id, JobStatus::Downloading).await?;

    // Create destination directory if needed
    let dest_path = std::path::Path::new(&job.destination_path);
    if let Some(parent) = dest_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let plex_client = PlexClient::new(&config.home_server_url, &config.home_plex_token);
    let download_url = plex_client.download_url(&job.source_url);

    let http_client = reqwest::Client::new();
    let mut req = http_client.get(&download_url);

    // Resume if we have partial progress
    let resume_from = job.bytes_downloaded;
    if resume_from > 0 {
        req = req.header("Range", format!("bytes={resume_from}-"));
    }

    let response = req.send().await?;
    let status = response.status();

    // Open file for writing (append if resuming)
    let mut file = if resume_from > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&job.destination_path)
            .await?
    } else {
        tokio::fs::File::create(&job.destination_path).await?
    };

    let mut bytes_downloaded = if resume_from > 0 && status == reqwest::StatusCode::PARTIAL_CONTENT
    {
        resume_from
    } else {
        0
    };

    let mut stream = response.bytes_stream();
    let mut last_progress_time = Instant::now();
    let mut last_bytes = bytes_downloaded;
    let mut bytes_since_last_db_update = 0i64;
    const DB_UPDATE_INTERVAL: i64 = 1024 * 1024; // 1MB

    loop {
        tokio::select! {
            chunk_opt = stream.next() => {
                match chunk_opt {
                    None => break,
                    Some(Err(e)) => return Err(e.into()),
                    Some(Ok(chunk)) => {
                        let chunk: Bytes = chunk;
                        file.write_all(&chunk).await?;
                        bytes_downloaded += chunk.len() as i64;
                        bytes_since_last_db_update += chunk.len() as i64;

                        let elapsed = last_progress_time.elapsed().as_secs_f64();
                        if elapsed >= 0.5 || bytes_since_last_db_update >= DB_UPDATE_INTERVAL {
                            let speed_bps = if elapsed > 0.0 {
                                (bytes_downloaded - last_bytes) as f64 / elapsed
                            } else {
                                0.0
                            };

                            update_job_progress(&state.db, job_id, bytes_downloaded).await?;
                            bytes_since_last_db_update = 0;

                            let _ = state.progress_tx.send(ProgressEvent {
                                job_id,
                                bytes_downloaded,
                                total_bytes: job.file_size_bytes,
                                speed_bps,
                                status: "downloading".to_string(),
                                error_message: None,
                            });

                            last_progress_time = Instant::now();
                            last_bytes = bytes_downloaded;
                        }
                    }
                }
            }
            _ = cancel_token.cancelled() => {
                update_job_progress(&state.db, job_id, bytes_downloaded).await?;
                file.flush().await?;
                return Err(anyhow::anyhow!("Cancelled"));
            }
        }
    }

    // Final DB update
    update_job_progress(&state.db, job_id, bytes_downloaded).await?;
    file.flush().await?;
    Ok(())
}
