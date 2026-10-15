use axum;
use axum::Router;

#[tokio::main]
async fn main() {

    let router = Router::new()
        .fallback("hello world");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();

    let app = axum::serve(listener, router).await.unwrap();
}
