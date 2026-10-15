use axum::Router;
use axum::extract::{ConnectInfo, State};
use axum::http::{Method, StatusCode, Uri};
use bytes::Bytes;
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use std::net::SocketAddr;

#[derive(Clone)]
struct AppState {
    client: Client<HttpConnector, Full<Bytes>>,
    origin: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = Client::builder(TokioExecutor::new()).build_http();

    let state = AppState {
        client,
        origin: "http://127.0.0.1:3000".to_string(),
    };

    let router = Router::new()
        .fallback(handler)
        .with_state(state)
        .into_make_service_with_connect_info::<SocketAddr>();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;

    axum::serve(listener, router).await?;

    Ok(())
}

async fn handler(
    method: Method,
    uri: Uri,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    state: State<AppState>,
) -> Result<Bytes, StatusCode> {
    println!("{} {} {}", method, uri, addr.ip());
    let pq: &str = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
    let origin_uri = format!("{}{}", state.origin, pq).parse::<Uri>().map_err(|e| {
        eprintln!("Error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let body = state
        .client
        .get(origin_uri)
        .await
        .map_err(|e| {
            eprintln!("{}", e);
            StatusCode::BAD_GATEWAY
        })?
        .into_body();
    let collected_body = body.collect().await.map_err(|e| {
        eprintln!("Error: {}", e);
        StatusCode::BAD_GATEWAY
    })?;
    Ok(collected_body.to_bytes())
}
