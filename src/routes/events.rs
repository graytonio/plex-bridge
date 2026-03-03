use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::Stream;
use tracing::warn;

use crate::state::{AppState, ProgressEvent};

pub async fn sse_events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let mut rx = state.subscribe_progress();

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let html = render_progress_event(&event);
                    yield Ok(Event::default().data(html));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    warn!("SSE receiver lagged by {n} messages");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

fn format_speed(bps: f64) -> String {
    if bps <= 0.0 {
        return "—".to_string();
    }
    const MB: f64 = 1_048_576.0;
    const KB: f64 = 1024.0;
    if bps >= MB {
        format!("{:.1} MB/s", bps / MB)
    } else {
        format!("{:.0} KB/s", bps / KB)
    }
}

fn render_progress_event(event: &ProgressEvent) -> String {
    let pct = if event.total_bytes > 0 {
        (event.bytes_downloaded as f64 / event.total_bytes as f64 * 100.0).min(100.0)
    } else {
        0.0
    };
    let speed = format_speed(event.speed_bps);
    let status_class = match event.status.as_str() {
        "downloading" => "badge-blue",
        "complete" => "badge-green",
        "failed" => "badge-red",
        _ => "badge-gray",
    };
    let status_label = &event.status;
    let error_html = event
        .error_message
        .as_deref()
        .map(|e| format!(r#"<span class="error-msg" title="{e}">&#9888;</span>"#))
        .unwrap_or_default();

    format!(
        r#"<tr id="job-{id}" hx-swap-oob="true">
  <td class="job-status"><span class="badge {status_class}">{status_label}</span>{error_html}</td>
  <td class="job-progress">
    <div class="progress-bar"><div class="progress-fill" style="width:{pct:.1}%"></div></div>
    <span class="progress-text">{pct:.1}%</span>
  </td>
  <td class="job-speed">{speed}</td>
</tr>"#,
        id = event.job_id,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ProgressEvent;

    fn make_event(job_id: i64, bytes: i64, total: i64, speed: f64, status: &str) -> ProgressEvent {
        ProgressEvent {
            job_id,
            bytes_downloaded: bytes,
            total_bytes: total,
            speed_bps: speed,
            status: status.to_string(),
            error_message: None,
        }
    }

    // ── format_speed ─────────────────────────────────────────────────────────

    #[test]
    fn format_speed_zero_returns_dash() {
        assert_eq!(format_speed(0.0), "—");
    }

    #[test]
    fn format_speed_negative_returns_dash() {
        assert_eq!(format_speed(-1.0), "—");
    }

    #[test]
    fn format_speed_below_1mb_shows_kb() {
        let s = format_speed(512.0 * 1024.0); // 512 KB/s
        assert!(s.ends_with(" KB/s"), "Expected KB/s, got: {s}");
        assert!(s.starts_with("512"), "Expected ~512, got: {s}");
    }

    #[test]
    fn format_speed_above_1mb_shows_mb() {
        let s = format_speed(2.5 * 1_048_576.0); // 2.5 MB/s
        assert!(s.ends_with(" MB/s"), "Expected MB/s, got: {s}");
        assert!(s.starts_with("2.5"), "Expected 2.5, got: {s}");
    }

    #[test]
    fn format_speed_exactly_1mb() {
        assert_eq!(format_speed(1_048_576.0), "1.0 MB/s");
    }

    #[test]
    fn format_speed_small_value_shows_kb() {
        let s = format_speed(1024.0); // 1 KB/s
        assert!(s.ends_with(" KB/s"), "Expected KB/s, got: {s}");
    }

    // ── render_progress_event ─────────────────────────────────────────────────

    #[test]
    fn render_contains_job_id_in_tr() {
        let html = render_progress_event(&make_event(42, 0, 0, 0.0, "queued"));
        assert!(html.contains(r#"id="job-42""#), "Expected job-42: {html}");
    }

    #[test]
    fn render_has_hx_swap_oob() {
        let html = render_progress_event(&make_event(1, 0, 0, 0.0, "queued"));
        assert!(
            html.contains(r#"hx-swap-oob="true""#),
            "Expected hx-swap-oob: {html}"
        );
    }

    #[test]
    fn render_shows_correct_percentage() {
        let html = render_progress_event(&make_event(1, 500, 1000, 0.0, "downloading"));
        assert!(html.contains("50.0%"), "Expected 50.0%: {html}");
    }

    #[test]
    fn render_zero_percent_when_total_is_zero() {
        let html = render_progress_event(&make_event(1, 0, 0, 0.0, "queued"));
        assert!(html.contains("0.0%"), "Expected 0.0%: {html}");
    }

    #[test]
    fn render_pct_clamped_to_100() {
        let html = render_progress_event(&make_event(1, 2000, 1000, 0.0, "complete"));
        assert!(html.contains("100.0%"), "Expected 100.0%: {html}");
        assert!(!html.contains("200.0%"), "Should not exceed 100%: {html}");
    }

    #[test]
    fn render_downloading_has_badge_blue() {
        let html = render_progress_event(&make_event(1, 0, 100, 0.0, "downloading"));
        assert!(html.contains("badge-blue"), "Expected badge-blue: {html}");
    }

    #[test]
    fn render_complete_has_badge_green() {
        let html = render_progress_event(&make_event(1, 100, 100, 0.0, "complete"));
        assert!(html.contains("badge-green"), "Expected badge-green: {html}");
    }

    #[test]
    fn render_failed_has_badge_red() {
        let html = render_progress_event(&make_event(1, 0, 100, 0.0, "failed"));
        assert!(html.contains("badge-red"), "Expected badge-red: {html}");
    }

    #[test]
    fn render_unknown_status_has_badge_gray() {
        let html = render_progress_event(&make_event(1, 0, 100, 0.0, "queued"));
        assert!(html.contains("badge-gray"), "Expected badge-gray: {html}");
    }

    #[test]
    fn render_includes_error_html_when_error_present() {
        let mut ev = make_event(1, 0, 100, 0.0, "failed");
        ev.error_message = Some("connection refused".to_string());
        let html = render_progress_event(&ev);
        assert!(
            html.contains("error-msg"),
            "Expected error-msg class: {html}"
        );
        assert!(
            html.contains("connection refused"),
            "Expected error text: {html}"
        );
    }

    #[test]
    fn render_no_error_html_when_error_absent() {
        let html = render_progress_event(&make_event(1, 0, 100, 0.0, "downloading"));
        assert!(
            !html.contains("error-msg"),
            "Should not have error-msg: {html}"
        );
    }

    #[test]
    fn render_includes_speed_when_nonzero() {
        let html = render_progress_event(&make_event(1, 0, 100, 2.0 * 1_048_576.0, "downloading"));
        assert!(html.contains("MB/s"), "Expected speed in output: {html}");
    }

    #[test]
    fn render_shows_status_label_in_badge() {
        let html = render_progress_event(&make_event(1, 0, 100, 0.0, "downloading"));
        assert!(
            html.contains("downloading"),
            "Expected status label: {html}"
        );
    }
}
