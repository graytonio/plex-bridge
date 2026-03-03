use std::sync::Arc;

use askama::Template;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Response},
    Form,
};
use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::models::config::{fetch_config, upsert_config, Config};
use crate::plex::client::PlexClient;
use crate::state::AppState;

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate {
    config: Config,
    saved: bool,
    test_result: Option<String>,
    test_error: Option<String>,
}

pub async fn get_settings(State(state): State<Arc<AppState>>) -> Result<Response> {
    let config = fetch_config(&state.db).await?.unwrap_or_default();
    Ok(SettingsTemplate {
        config,
        saved: false,
        test_result: None,
        test_error: None,
    }
    .into_response())
}

#[derive(Deserialize)]
pub struct SettingsForm {
    pub home_server_url: String,
    pub home_plex_token: String,
    pub local_server_url: String,
    pub local_plex_token: String,
    pub movies_path: String,
    pub tv_path: String,
    pub max_concurrent: i64,
}

pub async fn post_settings(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SettingsForm>,
) -> Result<Response> {
    let config = Config {
        id: 1,
        home_server_url: form.home_server_url,
        home_plex_token: form.home_plex_token,
        local_server_url: form.local_server_url,
        local_plex_token: form.local_plex_token,
        movies_path: form.movies_path,
        tv_path: form.tv_path,
        max_concurrent: form.max_concurrent.clamp(1, 5),
    };
    upsert_config(&state.db, &config)
        .await
        .map_err(AppError::Anyhow)?;

    Ok(SettingsTemplate {
        config,
        saved: true,
        test_result: None,
        test_error: None,
    }
    .into_response())
}

#[derive(Deserialize)]
pub struct TestForm {
    pub home_server_url: String,
    pub home_plex_token: String,
}

pub async fn test_connection(Form(form): Form<TestForm>) -> Response {
    let client = PlexClient::new(&form.home_server_url, &form.home_plex_token);
    match client.test_connection().await {
        Ok(name) => Html(format!(
            r#"<div class="test-result success">✓ Connected: {name}</div>"#
        ))
        .into_response(),
        Err(e) => Html(format!(r#"<div class="test-result error">✗ {e}</div>"#)).into_response(),
    }
}
