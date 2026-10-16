use anyhow::bail;
use axum::Router;
use axum::extract::{ConnectInfo, State};
use axum::http::uri::Scheme;
use axum::http::{HeaderMap, Method, Request, Response, StatusCode, Uri};
use bytes::Bytes;
use http_body_util::Full;
use http_body_util::{BodyExt, Collected};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;

#[derive(Clone)]
struct AppState {
    client: Client<HttpConnector, Full<Bytes>>,
    origins: HashMap<String, Uri>,
}

impl AppState {
    pub fn new(origin_list: Vec<Origin>) -> anyhow::Result<Self> {
        let client = Client::builder(TokioExecutor::new()).build_http();
        let mut origins: HashMap<String, Uri> = HashMap::new();
        for origin in origin_list {
            let valid_uri = validate_uri(origin.url.parse()?);
            origins.insert(origin.host, valid_uri?);
        }
        Ok(Self {
            client,
            origins,
        })
    }
}

#[derive(Deserialize, Debug)]
struct OriginList {
    origin: Vec<Origin>,
}

pub fn validate_uri(uri: Uri) -> anyhow::Result<Uri> {
    let valid_schemes = [Some(&Scheme::HTTP)];
    if !valid_schemes.contains(&uri.scheme()) {
        bail!("Invalid scheme for uri: {uri}");
    }
    Ok(uri)
}

#[derive(Deserialize, Debug, Clone)]
struct Origin {
    service: String,
    host: String,
    url: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config_str = fs::read_to_string("config.toml")?;
    let origin_list = toml::from_str::<OriginList>(&config_str)?.origin;
    let state = AppState::new(origin_list)?;
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
    mut headers: HeaderMap,
    state: State<AppState>,
    body: Bytes,
) -> Result<Response<Collected<Bytes>>, StatusCode> {
    println!("{} {} {}", method, uri, addr.ip());
    let pq: &str = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");
    clean_headers(&mut headers);
    let host = headers.get("host").and_then(|h| h.to_str().ok()).ok_or(StatusCode::BAD_REQUEST)?;

    let origin_uri = state.origins.get(host).ok_or(StatusCode::NOT_FOUND)?;

    let origin_request_uri = Uri::builder()
        .scheme(origin_uri.scheme().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?.clone())
        .authority(origin_uri.authority().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?.clone())
        .path_and_query(pq)
        .build()
        .map_err(|e| {
            eprintln!("{e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut request_builder = Request::builder().method(method).uri(origin_request_uri);
    let request_headers = request_builder.headers_mut().ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    request_headers.extend(headers.clone());
    let request = request_builder.body(Full::new(body)).map_err(|e| {
        eprintln!("Body error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let origin_response = state.client.request(request).await.map_err(|e| {
        eprintln!("{}", e);
        StatusCode::BAD_GATEWAY
    })?;
    let mut origin_headers = origin_response.headers().clone();
    clean_headers(&mut origin_headers);
    let status_code = origin_response.status();
    println!("status code: {}", status_code);
    let collected_body = origin_response.into_body().collect().await.map_err(|e| {
        eprintln!("Error: {}", e);
        StatusCode::BAD_GATEWAY
    })?;

    let mut response_builder = Response::builder().status(status_code);
    let proxy_headers = response_builder.headers_mut().ok_or(StatusCode::BAD_GATEWAY)?;

    proxy_headers.extend(origin_headers);

    let response = response_builder.body(collected_body).map_err(|e| {
        eprintln!("{}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    println!("host: {}", host);

    Ok(response)
}

pub fn clean_headers(headers: &mut HeaderMap) {
    let hop_headers = [
        "Connection",
        "Keep-Alive",
        "Proxy-Connection",
        "TE",
        "Transfer-Encoding",
        "Upgrade",
        "Proxy-Authenticate",
        "Proxy-Authorization",
    ];

    for header in hop_headers {
        headers.remove(header);
    }
}
