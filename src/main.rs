use std::net::SocketAddr;
use axum::extract::{ConnectInfo, State};
use axum::http::{Method, StatusCode, Uri};
use axum::Router;
use bytes::Bytes;
use http_body_util::Full;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

#[derive(Clone)]
struct AppState {
    client: Client<HttpConnector, Full<Bytes>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {

    let client: Client<HttpConnector, Full<Bytes>> = Client::builder(TokioExecutor::new())
        .build_http();

     let state = AppState { client };

    let router = Router::new()
        .fallback(handler)
        .with_state(state)
        .into_make_service_with_connect_info::<SocketAddr>();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;

    axum::serve(listener, router).await?;

    Ok(())
}

async fn handler(method: Method, uri: Uri, ip_addr: ConnectInfo<SocketAddr>, state: State<AppState>) -> Result<String, StatusCode>{
    println!("{} {} {}", method, uri, ip_addr.ip());
    state.client.get(Uri::from_static("http://127.0.0.1:3000/api")).await.map_err(|e| { eprintln!("{}", e); StatusCode::BAD_GATEWAY })?;
    Ok("hello".to_string())
}
