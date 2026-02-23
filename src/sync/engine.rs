use std::sync::Arc;

use tokio::sync::{mpsc, Semaphore};
use tracing::info;

use crate::state::AppState;

pub async fn run_worker_pool(
    state: Arc<AppState>,
    mut rx: mpsc::Receiver<i64>,
    max_concurrent: usize,
) {
    let semaphore = Arc::new(Semaphore::new(max_concurrent.max(1)));

    while let Some(job_id) = rx.recv().await {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(e) = super::worker::process_single_job(state, job_id).await {
                tracing::error!("Worker error for job {job_id}: {e}");
            }
            drop(permit);
        });
    }

    info!("Worker pool shut down");
}
