use std::sync::Arc;

use askama::Template;
use axum::{
    extract::State,
    response::{IntoResponse, Redirect, Response},
};

use crate::error::Result;
use crate::models::config::fetch_config;
use crate::models::sync_job::{clear_completed_jobs, list_jobs, SyncJob};
use crate::state::AppState;

#[derive(Template)]
#[template(path = "index.html")]
struct DashboardTemplate {
    jobs: Vec<SyncJob>,
    queued_count: usize,
    downloading_count: usize,
    complete_count: usize,
    failed_count: usize,
}

#[derive(Template)]
#[template(path = "partials/queue_stats.html")]
struct QueueStatsTemplate {
    queued_count: usize,
    downloading_count: usize,
    complete_count: usize,
    failed_count: usize,
}

pub async fn get_dashboard(State(state): State<Arc<AppState>>) -> Result<Response> {
    let config = fetch_config(&state.db).await?;
    if config
        .as_ref()
        .map(|c| !c.is_configured())
        .unwrap_or(true)
    {
        return Ok(Redirect::to("/settings").into_response());
    }

    let jobs = list_jobs(&state.db).await?;

    let queued_count = jobs.iter().filter(|j| j.status == "queued").count();
    let downloading_count = jobs.iter().filter(|j| j.status == "downloading").count();
    let complete_count = jobs.iter().filter(|j| j.status == "complete").count();
    let failed_count = jobs.iter().filter(|j| j.status == "failed").count();

    Ok(DashboardTemplate {
        jobs,
        queued_count,
        downloading_count,
        complete_count,
        failed_count,
    }
    .into_response())
}

pub async fn get_queue_stats(State(state): State<Arc<AppState>>) -> Result<Response> {
    let jobs = list_jobs(&state.db).await?;
    let queued_count = jobs.iter().filter(|j| j.status == "queued").count();
    let downloading_count = jobs.iter().filter(|j| j.status == "downloading").count();
    let complete_count = jobs.iter().filter(|j| j.status == "complete").count();
    let failed_count = jobs.iter().filter(|j| j.status == "failed").count();
    Ok(QueueStatsTemplate {
        queued_count,
        downloading_count,
        complete_count,
        failed_count,
    }
    .into_response())
}

pub async fn clear_completed(State(state): State<Arc<AppState>>) -> Result<Response> {
    clear_completed_jobs(&state.db)
        .await
        .map_err(crate::error::AppError::Anyhow)?;
    Ok(Redirect::to("/").into_response())
}
