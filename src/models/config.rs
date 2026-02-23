use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::db::DbPool;

#[derive(Debug, Clone, Serialize, Deserialize, Default, sqlx::FromRow)]
pub struct Config {
    pub id: i64,
    pub home_server_url: String,
    pub home_plex_token: String,
    pub local_server_url: String,
    pub local_plex_token: String,
    pub movies_path: String,
    pub tv_path: String,
    pub max_concurrent: i64,
}

impl Config {
    pub fn is_configured(&self) -> bool {
        !self.home_server_url.is_empty() && !self.home_plex_token.is_empty()
    }
}

pub async fn fetch_config(db: &DbPool) -> Result<Option<Config>> {
    let row = match db {
        DbPool::Sqlite(pool) => {
            sqlx::query_as::<_, Config>(
                "SELECT id, home_server_url, home_plex_token, local_server_url, local_plex_token, movies_path, tv_path, max_concurrent FROM config WHERE id = 1"
            )
            .fetch_optional(pool)
            .await?
        }
        DbPool::Postgres(pool) => {
            sqlx::query_as::<_, Config>(
                "SELECT id, home_server_url, home_plex_token, local_server_url, local_plex_token, movies_path, tv_path, max_concurrent FROM config WHERE id = 1"
            )
            .fetch_optional(pool)
            .await?
        }
    };
    Ok(row)
}

pub async fn upsert_config(db: &DbPool, cfg: &Config) -> Result<()> {
    match db {
        DbPool::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO config (id, home_server_url, home_plex_token, local_server_url, local_plex_token, movies_path, tv_path, max_concurrent)
                 VALUES (1, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(id) DO UPDATE SET
                   home_server_url = excluded.home_server_url,
                   home_plex_token = excluded.home_plex_token,
                   local_server_url = excluded.local_server_url,
                   local_plex_token = excluded.local_plex_token,
                   movies_path = excluded.movies_path,
                   tv_path = excluded.tv_path,
                   max_concurrent = excluded.max_concurrent"
            )
            .bind(&cfg.home_server_url)
            .bind(&cfg.home_plex_token)
            .bind(&cfg.local_server_url)
            .bind(&cfg.local_plex_token)
            .bind(&cfg.movies_path)
            .bind(&cfg.tv_path)
            .bind(cfg.max_concurrent)
            .execute(pool)
            .await?;
        }
        DbPool::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO config (id, home_server_url, home_plex_token, local_server_url, local_plex_token, movies_path, tv_path, max_concurrent)
                 VALUES (1, $1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT(id) DO UPDATE SET
                   home_server_url = EXCLUDED.home_server_url,
                   home_plex_token = EXCLUDED.home_plex_token,
                   local_server_url = EXCLUDED.local_server_url,
                   local_plex_token = EXCLUDED.local_plex_token,
                   movies_path = EXCLUDED.movies_path,
                   tv_path = EXCLUDED.tv_path,
                   max_concurrent = EXCLUDED.max_concurrent"
            )
            .bind(&cfg.home_server_url)
            .bind(&cfg.home_plex_token)
            .bind(&cfg.local_server_url)
            .bind(&cfg.local_plex_token)
            .bind(&cfg.movies_path)
            .bind(&cfg.tv_path)
            .bind(cfg.max_concurrent)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_test_db;

    // ── is_configured ────────────────────────────────────────────────────────

    #[test]
    fn not_configured_when_both_empty() {
        let cfg = Config::default(); // home_server_url = "", home_plex_token = ""
        assert!(!cfg.is_configured());
    }

    #[test]
    fn not_configured_when_url_missing() {
        let cfg = Config {
            home_plex_token: "abc123".into(),
            ..Default::default()
        };
        assert!(!cfg.is_configured());
    }

    #[test]
    fn not_configured_when_token_missing() {
        let cfg = Config {
            home_server_url: "http://192.168.1.1:32400".into(),
            ..Default::default()
        };
        assert!(!cfg.is_configured());
    }

    #[test]
    fn configured_when_both_present() {
        let cfg = Config {
            home_server_url: "http://192.168.1.1:32400".into(),
            home_plex_token: "mytoken".into(),
            ..Default::default()
        };
        assert!(cfg.is_configured());
    }

    // ── DB round-trip tests ───────────────────────────────────────────────────

    #[tokio::test]
    async fn fetch_config_returns_none_on_empty_db() {
        let db = create_test_db().await;
        let result = fetch_config(&db).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn upsert_and_fetch_roundtrip() {
        let db = create_test_db().await;
        let cfg = Config {
            id: 1,
            home_server_url: "http://home:32400".into(),
            home_plex_token: "hometoken".into(),
            local_server_url: "http://localhost:32400".into(),
            local_plex_token: "localtoken".into(),
            movies_path: "/movies".into(),
            tv_path: "/tv".into(),
            max_concurrent: 3,
        };
        upsert_config(&db, &cfg).await.unwrap();

        let fetched = fetch_config(&db).await.unwrap().expect("should have a row");
        assert_eq!(fetched.home_server_url, "http://home:32400");
        assert_eq!(fetched.home_plex_token, "hometoken");
        assert_eq!(fetched.local_server_url, "http://localhost:32400");
        assert_eq!(fetched.local_plex_token, "localtoken");
        assert_eq!(fetched.movies_path, "/movies");
        assert_eq!(fetched.tv_path, "/tv");
        assert_eq!(fetched.max_concurrent, 3);
    }

    #[tokio::test]
    async fn upsert_overwrites_existing_row() {
        let db = create_test_db().await;

        let first = Config {
            id: 1,
            home_server_url: "http://old:32400".into(),
            home_plex_token: "oldtoken".into(),
            max_concurrent: 1,
            ..Default::default()
        };
        upsert_config(&db, &first).await.unwrap();

        let updated = Config {
            id: 1,
            home_server_url: "http://new:32400".into(),
            home_plex_token: "newtoken".into(),
            max_concurrent: 4,
            ..Default::default()
        };
        upsert_config(&db, &updated).await.unwrap();

        let fetched = fetch_config(&db).await.unwrap().expect("should have a row");
        assert_eq!(fetched.home_server_url, "http://new:32400");
        assert_eq!(fetched.home_plex_token, "newtoken");
        assert_eq!(fetched.max_concurrent, 4);

        // Only one row ever exists
        if let crate::db::DbPool::Sqlite(pool) = &db {
            let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM config")
                .fetch_one(pool)
                .await
                .unwrap();
            assert_eq!(count.0, 1);
        }
    }

    #[tokio::test]
    async fn fetched_config_is_configured_check() {
        let db = create_test_db().await;
        let cfg = Config {
            id: 1,
            home_server_url: "http://home:32400".into(),
            home_plex_token: "tok".into(),
            ..Default::default()
        };
        upsert_config(&db, &cfg).await.unwrap();
        let fetched = fetch_config(&db).await.unwrap().unwrap();
        assert!(fetched.is_configured());
    }
}
