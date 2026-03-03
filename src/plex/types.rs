use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlexMedia {
    #[serde(rename = "Part")]
    #[serde(default)]
    pub parts: Vec<PlexPart>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlexPart {
    #[serde(rename = "key")]
    pub key: String,
    #[serde(rename = "file")]
    pub file: String,
    #[serde(rename = "size")]
    pub size: Option<i64>,
}

/// Generic metadata item used for all Plex API responses
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PlexMetadata {
    #[serde(rename = "ratingKey")]
    pub rating_key: Option<String>,
    #[serde(rename = "key")]
    pub key: Option<String>,
    #[serde(rename = "title")]
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub media_type: Option<String>,
    #[serde(rename = "year")]
    pub year: Option<i64>,
    #[serde(rename = "thumb")]
    pub thumb: Option<String>,
    #[serde(rename = "index")]
    pub index: Option<i64>,
    #[serde(rename = "parentIndex")]
    pub parent_index: Option<i64>,
    #[serde(rename = "parentTitle")]
    pub parent_title: Option<String>,
    #[serde(rename = "grandparentTitle")]
    pub grandparent_title: Option<String>,
    #[serde(rename = "leafCount")]
    pub leaf_count: Option<i64>,
    #[serde(rename = "parentRatingKey")]
    pub parent_rating_key: Option<String>,
    #[serde(rename = "Media")]
    #[serde(default)]
    pub media: Vec<PlexMedia>,
}

impl PlexMetadata {
    pub fn file_size(&self) -> i64 {
        self.media
            .first()
            .and_then(|m| m.parts.first())
            .and_then(|p| p.size)
            .unwrap_or(0)
    }

    pub fn file_key(&self) -> Option<String> {
        self.media
            .first()
            .and_then(|m| m.parts.first())
            .map(|p| p.key.clone())
    }

    pub fn file_name(&self) -> Option<String> {
        self.media.first().and_then(|m| m.parts.first()).map(|p| {
            let path = std::path::Path::new(&p.file);
            path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| p.file.clone())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_metadata_with_part(key: &str, file: &str, size: Option<i64>) -> PlexMetadata {
        PlexMetadata {
            media: vec![PlexMedia {
                parts: vec![PlexPart {
                    key: key.to_string(),
                    file: file.to_string(),
                    size,
                }],
            }],
            ..Default::default()
        }
    }

    // ── file_size ─────────────────────────────────────────────────────────────

    #[test]
    fn file_size_zero_when_no_media() {
        let m = PlexMetadata::default();
        assert_eq!(m.file_size(), 0);
    }

    #[test]
    fn file_size_zero_when_media_has_no_parts() {
        let m = PlexMetadata {
            media: vec![PlexMedia { parts: vec![] }],
            ..Default::default()
        };
        assert_eq!(m.file_size(), 0);
    }

    #[test]
    fn file_size_zero_when_part_has_no_size() {
        let m = make_metadata_with_part("/parts/1", "/movies/film.mkv", None);
        assert_eq!(m.file_size(), 0);
    }

    #[test]
    fn file_size_returns_first_part_size() {
        let m = make_metadata_with_part("/parts/1", "/movies/film.mkv", Some(4_294_967_296));
        assert_eq!(m.file_size(), 4_294_967_296);
    }

    #[test]
    fn file_size_uses_only_first_media_item() {
        // If somehow there are multiple Media items, we only care about the first
        let m = PlexMetadata {
            media: vec![
                PlexMedia {
                    parts: vec![PlexPart {
                        key: "/parts/1".into(),
                        file: "/f1.mkv".into(),
                        size: Some(1_000),
                    }],
                },
                PlexMedia {
                    parts: vec![PlexPart {
                        key: "/parts/2".into(),
                        file: "/f2.mkv".into(),
                        size: Some(999_999),
                    }],
                },
            ],
            ..Default::default()
        };
        assert_eq!(m.file_size(), 1_000); // first Media item wins
    }

    // ── file_key ──────────────────────────────────────────────────────────────

    #[test]
    fn file_key_none_when_no_media() {
        let m = PlexMetadata::default();
        assert!(m.file_key().is_none());
    }

    #[test]
    fn file_key_returns_part_key() {
        let m = make_metadata_with_part("/library/parts/12345", "/movies/f.mkv", None);
        assert_eq!(m.file_key(), Some("/library/parts/12345".to_string()));
    }

    // ── file_name ─────────────────────────────────────────────────────────────

    #[test]
    fn file_name_none_when_no_media() {
        let m = PlexMetadata::default();
        assert!(m.file_name().is_none());
    }

    #[test]
    fn file_name_extracts_basename_from_absolute_path() {
        let m = make_metadata_with_part("/parts/1", "/movies/Inception (2010)/Inception.mkv", None);
        assert_eq!(m.file_name(), Some("Inception.mkv".to_string()));
    }

    #[test]
    fn file_name_from_filename_only() {
        let m = make_metadata_with_part("/parts/1", "movie.mkv", None);
        assert_eq!(m.file_name(), Some("movie.mkv".to_string()));
    }

    #[test]
    fn file_name_from_nested_path() {
        let m = make_metadata_with_part(
            "/parts/1",
            "/TV/Breaking Bad/Season 05/S05E14 - Ozymandias.mkv",
            None,
        );
        assert_eq!(m.file_name(), Some("S05E14 - Ozymandias.mkv".to_string()));
    }

    // ── JSON deserialization ──────────────────────────────────────────────────

    #[test]
    fn deserializes_from_plex_json_format() {
        let json = serde_json::json!({
            "ratingKey": "12345",
            "title": "Inception",
            "type": "movie",
            "year": 2010,
            "Media": [{
                "Part": [{
                    "key": "/library/parts/99",
                    "file": "/movies/Inception.mkv",
                    "size": 7516192768_i64
                }]
            }]
        });

        let m: PlexMetadata = serde_json::from_value(json).unwrap();
        assert_eq!(m.rating_key.as_deref(), Some("12345"));
        assert_eq!(m.title.as_deref(), Some("Inception"));
        assert_eq!(m.media_type.as_deref(), Some("movie"));
        assert_eq!(m.year, Some(2010));
        assert_eq!(m.file_size(), 7516192768);
        assert_eq!(m.file_key().as_deref(), Some("/library/parts/99"));
        assert_eq!(m.file_name().as_deref(), Some("Inception.mkv"));
    }

    #[test]
    fn deserializes_with_missing_optional_fields() {
        let json = serde_json::json!({ "title": "Unknown" });
        let m: PlexMetadata = serde_json::from_value(json).unwrap();
        assert_eq!(m.title.as_deref(), Some("Unknown"));
        assert!(m.rating_key.is_none());
        assert!(m.media.is_empty());
        assert_eq!(m.file_size(), 0);
    }
}
