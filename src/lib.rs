pub mod config;
pub mod modules;
pub mod pipeline;
pub mod proxy;

use crate::modules::blocked::BlockPath;
use crate::pipeline::Module;
use crate::proxy::handler;
use axum::Router;
use bytes::Bytes;
use config::Config;
use http_body_util::Full;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    config: Config,
    client: Client<HttpConnector, Full<Bytes>>,
    modules: Arc<Vec<Box<dyn Module>>>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        let client = Client::builder(TokioExecutor::new()).build_http();
        let block_path = Box::new(BlockPath {});
        let modules: Vec<Box<dyn Module>> = vec![block_path];
        let modules = Arc::new(modules);
        Self {
            config,
            client,
            modules,
        }
    }
}
pub fn app(config: Config) -> Router {
    let state = AppState::new(config);
    Router::new().fallback(handler).with_state(state)
}
