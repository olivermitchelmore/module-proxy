pub mod config;
pub mod proxy;

use crate::proxy::handler;
use axum::Router;
use axum::handler::HandlerWithoutStateExt;
use bytes::Bytes;
use config::Config;
use http_body_util::Full;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

#[derive(Clone)]
pub struct AppState {
    config: Config,
    client: Client<HttpConnector, Full<Bytes>>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let client = Client::builder(TokioExecutor::new()).build_http();
        Self {
            config,
            client,
        }
    }
}
pub fn app(config: Config) -> Router {
    let state = AppState::new(config);
    let router = Router::new().fallback(handler).with_state(state);
    router
}
