use module_proxy::app;
use module_proxy::config::{Config, ConfigError};
use std::fs;
use std::net::SocketAddr;
use thiserror::Error;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Error, Debug)]
enum StartupError {
    #[error("Config Error")]
    ConfigError(#[from] ConfigError),
    #[error("IO Error")]
    IoError(#[from] std::io::Error),
}
#[tokio::main]
async fn main() -> Result<(), StartupError> {
    tracing_subscriber::registry().with(fmt::layer()).with(EnvFilter::from_default_env()).init();

    let toml = fs::read_to_string("config.toml")?;
    let config = Config::from_toml(&toml)?;
    let app = app(config).into_make_service_with_connect_info::<SocketAddr>();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    axum::serve(listener, app).await?;

    Ok(())
}
