use anyhow::{Context, Result};
use sqlx::{Pool, Postgres, Sqlite};
use sqlx::sqlite::SqliteConnectOptions;
use std::str::FromStr;

#[derive(Debug)]
pub enum DbPool {
    Sqlite(Pool<Sqlite>),
    Postgres(Pool<Postgres>),
}

impl DbPool {
    pub async fn connect(url: &str) -> Result<Self> {
        if url.starts_with("sqlite") {
            let opts = SqliteConnectOptions::from_str(url)
                .context("Invalid SQLite URL")?
                .create_if_missing(true);
            let pool = sqlx::SqlitePool::connect_with(opts)
                .await
                .context("Failed to connect to SQLite")?;
            Ok(DbPool::Sqlite(pool))
        } else if url.starts_with("postgres") {
            let pool = sqlx::PgPool::connect(url)
                .await
                .context("Failed to connect to PostgreSQL")?;
            Ok(DbPool::Postgres(pool))
        } else {
            anyhow::bail!("Unsupported database URL scheme: {url}");
        }
    }

    pub async fn run_migrations(&self) -> Result<()> {
        match self {
            DbPool::Sqlite(pool) => {
                sqlx::migrate!("./migrations/sqlite")
                    .run(pool)
                    .await
                    .context("SQLite migrations failed")?;
            }
            DbPool::Postgres(pool) => {
                sqlx::migrate!("./migrations/postgres")
                    .run(pool)
                    .await
                    .context("Postgres migrations failed")?;
            }
        }
        Ok(())
    }
}

/// Creates an isolated in-memory SQLite database with migrations applied.
/// Used exclusively by tests — each call yields a fresh, independent schema.
#[cfg(test)]
pub(crate) async fn create_test_db() -> DbPool {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .create_if_missing(true);
    let pool = sqlx::SqlitePool::connect_with(opts).await.unwrap();
    let db = DbPool::Sqlite(pool);
    db.run_migrations().await.unwrap();
    db
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_scheme_errors() {
        // We can verify the scheme-detection branch synchronously by
        // inspecting what prefix check DbPool::connect would follow.
        // The real test is the tokio one below, but we can confirm the
        // string logic here.
        let url = "mysql://localhost/db";
        assert!(!url.starts_with("sqlite"));
        assert!(!url.starts_with("postgres"));
    }

    #[tokio::test]
    async fn unsupported_scheme_returns_err() {
        let result = DbPool::connect("mysql://localhost/db").await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("Unsupported"), "Expected 'Unsupported' in: {msg}");
    }

    #[tokio::test]
    async fn sqlite_memory_connects_and_migrates() {
        let db = create_test_db().await;
        // Verify we can query the migrated schema
        match &db {
            DbPool::Sqlite(pool) => {
                let count: (i64,) =
                    sqlx::query_as("SELECT COUNT(*) FROM config")
                        .fetch_one(pool)
                        .await
                        .expect("config table should exist after migration");
                assert_eq!(count.0, 0);
            }
            _ => panic!("Expected Sqlite variant"),
        }
    }

    #[tokio::test]
    async fn sqlite_memory_has_sync_jobs_table() {
        let db = create_test_db().await;
        match &db {
            DbPool::Sqlite(pool) => {
                let count: (i64,) =
                    sqlx::query_as("SELECT COUNT(*) FROM sync_jobs")
                        .fetch_one(pool)
                        .await
                        .expect("sync_jobs table should exist after migration");
                assert_eq!(count.0, 0);
            }
            _ => panic!("Expected Sqlite variant"),
        }
    }
}
