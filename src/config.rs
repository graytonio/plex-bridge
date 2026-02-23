use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "plexbridge", about = "Self-hosted Plex sync service")]
pub struct AppConfig {
    /// Database URL (sqlite:// or postgres://)
    #[arg(long, env = "PLEXBRIDGE_DATABASE_URL", default_value = "sqlite://./plexbridge.db")]
    pub database_url: String,

    /// Port to listen on
    #[arg(long, env = "PLEXBRIDGE_PORT", default_value = "7878")]
    pub port: u16,
}
