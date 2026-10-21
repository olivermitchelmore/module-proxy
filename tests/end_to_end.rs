use axum::Router;
use axum::routing::get;
use bytes::Bytes;
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::{Request, StatusCode, Uri};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use module_proxy::app;
use module_proxy::config::Config;
use std::net::SocketAddr;

#[tokio::test]
async fn forwarding() {
    let origin = Router::new().route(
        "/test",
        get(|uri: Uri| async move {
            let path = uri.path();
            path.to_owned()
        }),
    );
    let origin_server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_local_addr = origin_server.local_addr().unwrap();
    let origin_ip = origin_local_addr.ip();
    let origin_port = origin_local_addr.port();

    tokio::spawn(async move {
        axum::serve(origin_server, origin.into_make_service()).await.unwrap();
    });

    let toml = format!(
        r#"
    [[origin]]
    host="example.com"
    url="http://{origin_ip}:{origin_port}""#
    );

    let config = Config::from_toml(&toml).unwrap();
    let app = app(config).into_make_service_with_connect_info::<SocketAddr>();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_local_addr = listener.local_addr().unwrap();
    let proxy_port = proxy_local_addr.port();
    let proxy_ip = proxy_local_addr.ip();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let uri: Uri = format!("http://{proxy_ip}:{proxy_port}/test").parse().unwrap();
    let client: Client<HttpConnector, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();

    let request = Request::builder()
        .header("host", "example.com")
        .uri(uri)
        .body(Full::new(Bytes::new()))
        .unwrap();

    let response = client.request(request).await.unwrap();
    let status_code = response.status();
    let response_bytes = response.collect().await.unwrap().to_bytes();

    assert_eq!("/test", std::str::from_utf8(&response_bytes).unwrap());
    assert_eq!(status_code, StatusCode::OK)
}

#[tokio::test]
async fn reject_bad_gateway() {
    let origin_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_local_addr = origin_listener.local_addr().unwrap();
    let origin_port = origin_local_addr.port();
    let origin_ip = origin_local_addr.ip();

    let toml = format!(
        r#"
    [[origin]]
    host="example.com"
    url="http://{origin_ip}:{origin_port}""#
    );

    let config = Config::from_toml(&toml).unwrap();
    let app = app(config).into_make_service_with_connect_info::<SocketAddr>();

    let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_local_addr = proxy_listener.local_addr().unwrap();
    let proxy_port = proxy_local_addr.port();
    let proxy_ip = proxy_local_addr.ip();

    tokio::spawn(async {
        axum::serve(proxy_listener, app).await.unwrap();
    });

    let uri: Uri = format!("http://{proxy_ip}:{proxy_port}/").parse().unwrap();
    let client: Client<HttpConnector, Full<Bytes>> =
        Client::builder(TokioExecutor::new()).build_http();

    drop(origin_listener);

    let request = Request::builder()
        .header("host", "example.com")
        .uri(uri)
        .body(Full::new(Bytes::new()))
        .unwrap();

    let response = client.request(request).await.unwrap();
    let status_code = response.status();
    assert_eq!(status_code, StatusCode::BAD_GATEWAY);
}
