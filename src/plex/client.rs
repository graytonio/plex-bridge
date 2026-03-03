use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;

use super::types::PlexMetadata;

#[derive(Clone)]
pub struct PlexClient {
    client: Client,
    base_url: String,
    token: String,
}

impl PlexClient {
    pub fn new(base_url: &str, token: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    async fn get_json(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .header("X-Plex-Token", &self.token)
            .header("Accept", "application/json")
            .send()
            .await
            .context("Request failed")?
            .error_for_status()
            .context("Plex API error")?;
        let json = resp.json::<Value>().await.context("Failed to parse JSON")?;
        Ok(json)
    }

    pub async fn test_connection(&self) -> Result<String> {
        let json = self.get_json("/").await?;
        let name = json["MediaContainer"]["friendlyName"]
            .as_str()
            .unwrap_or("Plex Server")
            .to_string();
        Ok(name)
    }

    pub async fn libraries(&self) -> Result<Vec<PlexMetadata>> {
        let json = self.get_json("/library/sections").await?;
        let dirs = json["MediaContainer"]["Directory"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let sections: Vec<PlexMetadata> = dirs
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        Ok(sections)
    }

    pub async fn movies(&self, section_id: &str) -> Result<Vec<PlexMetadata>> {
        let path = format!("/library/sections/{section_id}/all");
        let json = self.get_json(&path).await?;
        let items = extract_metadata(&json);
        Ok(items)
    }

    pub async fn shows(&self, section_id: &str) -> Result<Vec<PlexMetadata>> {
        let path = format!("/library/sections/{section_id}/all");
        let json = self.get_json(&path).await?;
        let items = extract_metadata(&json);
        Ok(items)
    }

    pub async fn seasons(&self, show_rating_key: &str) -> Result<Vec<PlexMetadata>> {
        let path = format!("/library/metadata/{show_rating_key}/children");
        let json = self.get_json(&path).await?;
        let items = extract_metadata(&json);
        Ok(items)
    }

    pub async fn episodes(&self, season_rating_key: &str) -> Result<Vec<PlexMetadata>> {
        let path = format!("/library/metadata/{season_rating_key}/children");
        let json = self.get_json(&path).await?;
        let items = extract_metadata(&json);
        Ok(items)
    }

    pub async fn refresh_library(&self, section_id: &str) -> Result<()> {
        let url = format!("{}/library/sections/{}/refresh", self.base_url, section_id);
        self.client
            .get(&url)
            .query(&[("X-Plex-Token", &self.token)])
            .send()
            .await
            .context("Failed to trigger library refresh")?;
        Ok(())
    }

    pub fn download_url(&self, file_key: &str) -> String {
        format!("{}{}?X-Plex-Token={}", self.base_url, file_key, self.token)
    }

    pub async fn get_image_bytes(&self, path: &str) -> Result<(bytes::Bytes, String)> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .header("X-Plex-Token", &self.token)
            .send()
            .await
            .context("Thumbnail request failed")?
            .error_for_status()
            .context("Plex thumbnail error")?;
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/jpeg")
            .to_string();
        let data = resp.bytes().await.context("Failed to read thumbnail bytes")?;
        Ok((data, content_type))
    }
}

fn extract_metadata(json: &Value) -> Vec<PlexMetadata> {
    // Try Metadata first, then Video, then Directory
    for key in &["Metadata", "Video", "Directory"] {
        if let Some(arr) = json["MediaContainer"][key].as_array() {
            let items: Vec<PlexMetadata> = arr
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            if !items.is_empty() {
                return items;
            }
        }
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── PlexClient::new ───────────────────────────────────────────────────────

    #[test]
    fn new_trims_single_trailing_slash() {
        let c = PlexClient::new("http://192.168.1.1:32400/", "tok");
        assert_eq!(c.base_url, "http://192.168.1.1:32400");
    }

    #[test]
    fn new_trims_multiple_trailing_slashes() {
        let c = PlexClient::new("http://192.168.1.1:32400///", "tok");
        assert_eq!(c.base_url, "http://192.168.1.1:32400");
    }

    #[test]
    fn new_does_not_modify_url_without_trailing_slash() {
        let c = PlexClient::new("http://192.168.1.1:32400", "tok");
        assert_eq!(c.base_url, "http://192.168.1.1:32400");
    }

    #[test]
    fn new_stores_token() {
        let c = PlexClient::new("http://server:32400", "mytoken123");
        assert_eq!(c.token, "mytoken123");
    }

    // ── download_url ─────────────────────────────────────────────────────────

    #[test]
    fn download_url_builds_correct_url() {
        let c = PlexClient::new("http://server:32400", "abc123");
        let url = c.download_url("/library/parts/9876/file.mkv");
        assert_eq!(
            url,
            "http://server:32400/library/parts/9876/file.mkv?X-Plex-Token=abc123"
        );
    }

    #[test]
    fn download_url_handles_no_leading_slash_on_key() {
        let c = PlexClient::new("http://server:32400", "tok");
        let url = c.download_url("library/parts/1");
        assert_eq!(url, "http://server:32400library/parts/1?X-Plex-Token=tok");
    }

    #[test]
    fn download_url_includes_token_as_query_param() {
        let c = PlexClient::new("http://server:32400", "secret-token");
        let url = c.download_url("/parts/1");
        assert!(
            url.contains("X-Plex-Token=secret-token"),
            "Token missing from URL: {url}"
        );
    }

    // ── extract_metadata ─────────────────────────────────────────────────────

    #[test]
    fn extract_metadata_empty_json_returns_empty_vec() {
        let json = json!({});
        let items = extract_metadata(&json);
        assert!(items.is_empty());
    }

    #[test]
    fn extract_metadata_uses_metadata_key_first() {
        let json = json!({
            "MediaContainer": {
                "Metadata": [{"title": "From Metadata", "ratingKey": "1"}],
                "Video":    [{"title": "From Video",    "ratingKey": "2"}],
            }
        });
        let items = extract_metadata(&json);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title.as_deref(), Some("From Metadata"));
    }

    #[test]
    fn extract_metadata_falls_back_to_video_when_no_metadata() {
        let json = json!({
            "MediaContainer": {
                "Video": [{"title": "A Movie", "ratingKey": "5"}]
            }
        });
        let items = extract_metadata(&json);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title.as_deref(), Some("A Movie"));
    }

    #[test]
    fn extract_metadata_falls_back_to_directory_when_no_metadata_or_video() {
        let json = json!({
            "MediaContainer": {
                "Directory": [{"title": "Season 1", "ratingKey": "10"}]
            }
        });
        let items = extract_metadata(&json);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title.as_deref(), Some("Season 1"));
    }

    #[test]
    fn extract_metadata_prefers_metadata_over_directory() {
        let json = json!({
            "MediaContainer": {
                "Metadata":  [{"title": "Metadata Item",  "ratingKey": "1"}],
                "Directory": [{"title": "Directory Item", "ratingKey": "2"}],
            }
        });
        let items = extract_metadata(&json);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title.as_deref(), Some("Metadata Item"));
    }

    #[test]
    fn extract_metadata_skips_empty_arrays_and_continues() {
        // Metadata is present but empty — should fall through to Video
        let json = json!({
            "MediaContainer": {
                "Metadata": [],
                "Video": [{"title": "Good Item", "ratingKey": "7"}],
            }
        });
        let items = extract_metadata(&json);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title.as_deref(), Some("Good Item"));
    }

    #[test]
    fn extract_metadata_returns_multiple_items() {
        let json = json!({
            "MediaContainer": {
                "Video": [
                    {"title": "Movie A", "ratingKey": "1"},
                    {"title": "Movie B", "ratingKey": "2"},
                    {"title": "Movie C", "ratingKey": "3"},
                ]
            }
        });
        let items = extract_metadata(&json);
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn extract_metadata_silently_ignores_invalid_items() {
        // Items that fail serde deserialization are filtered out
        // PlexMetadata has all optional fields so almost anything deserializes.
        // Use an array with a valid and a clearly-typed wrong value.
        let json = json!({
            "MediaContainer": {
                "Metadata": [
                    {"title": "Valid", "ratingKey": "1"},
                    null  // null item — serde_json::from_value(null) → Err
                ]
            }
        });
        let items = extract_metadata(&json);
        // null gets filtered; only the valid item remains
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn extract_metadata_all_empty_arrays_returns_empty() {
        let json = json!({
            "MediaContainer": {
                "Metadata": [],
                "Video": [],
                "Directory": [],
            }
        });
        let items = extract_metadata(&json);
        assert!(items.is_empty());
    }
}
