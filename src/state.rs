use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio_util::sync::CancellationToken;

use crate::db::DbPool;

#[derive(Debug, Clone)]
pub struct ProgressEvent {
    pub job_id: i64,
    pub bytes_downloaded: i64,
    pub total_bytes: i64,
    pub speed_bps: f64,
    pub status: String,
    pub error_message: Option<String>,
}

pub struct AppState {
    pub db: Arc<DbPool>,
    pub progress_tx: broadcast::Sender<ProgressEvent>,
    pub job_tx: mpsc::Sender<i64>,
    pub cancellation_tokens: DashMap<i64, CancellationToken>,
}

impl AppState {
    pub fn new(
        db: Arc<DbPool>,
        progress_tx: broadcast::Sender<ProgressEvent>,
        job_tx: mpsc::Sender<i64>,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            progress_tx,
            job_tx,
            cancellation_tokens: DashMap::new(),
        })
    }

    pub fn subscribe_progress(&self) -> broadcast::Receiver<ProgressEvent> {
        self.progress_tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_test_db;

    async fn make_state() -> Arc<AppState> {
        let db = Arc::new(create_test_db().await);
        let (progress_tx, _) = broadcast::channel(16);
        let (job_tx, _) = mpsc::channel(16);
        AppState::new(db, progress_tx, job_tx)
    }

    #[tokio::test]
    async fn new_creates_state_with_empty_cancellation_tokens() {
        let state = make_state().await;
        assert!(state.cancellation_tokens.is_empty());
    }

    #[tokio::test]
    async fn subscribe_progress_receives_sent_event() {
        let state = make_state().await;
        let mut rx = state.subscribe_progress();

        let event = ProgressEvent {
            job_id: 7,
            bytes_downloaded: 512,
            total_bytes: 1024,
            speed_bps: 1000.0,
            status: "downloading".to_string(),
            error_message: None,
        };
        state.progress_tx.send(event).unwrap();

        let received = rx.try_recv().unwrap();
        assert_eq!(received.job_id, 7);
        assert_eq!(received.bytes_downloaded, 512);
        assert_eq!(received.total_bytes, 1024);
    }

    #[tokio::test]
    async fn subscribe_progress_multiple_subscribers_both_receive() {
        let state = make_state().await;
        let mut rx1 = state.subscribe_progress();
        let mut rx2 = state.subscribe_progress();

        let event = ProgressEvent {
            job_id: 99,
            bytes_downloaded: 0,
            total_bytes: 100,
            speed_bps: 0.0,
            status: "queued".to_string(),
            error_message: None,
        };
        state.progress_tx.send(event).unwrap();

        assert_eq!(rx1.try_recv().unwrap().job_id, 99);
        assert_eq!(rx2.try_recv().unwrap().job_id, 99);
    }

    #[tokio::test]
    async fn new_returns_arc_wrapped_state() {
        let state = make_state().await;
        // Strong count is at least 1; the test holds a reference
        assert!(Arc::strong_count(&state) >= 1);
    }
}
