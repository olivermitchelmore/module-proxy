use crate::AppState;
use axum::extract::{ConnectInfo, State};
use axum::http;
use axum::http::{HeaderMap, Method, Request, Response, StatusCode, Uri};
use axum::response::IntoResponse;
use bytes::Bytes;
use http_body_util::BodyExt;
use http_body_util::{Collected, Full};
use hyper::body::Incoming;
use hyper::header::ToStrError;
use std::net::SocketAddr;
use thiserror::Error;
use tracing::error;

#[derive(Error, Debug)]
pub enum RequestError {
    #[error("missing host")]
    MissingHost,
    #[error("invalid host header")]
    InvalidHostHeader {
        #[from]
        source: ToStrError,
    },
    #[error("host {host:?} not configured")]
    UnconfiguredHost {
        host: String,
    },
    #[error("origin {origin:?} missing authority")]
    MissingOriginAuthority {
        origin: String,
    },
    #[error("origin {origin:?} missing Scheme")]
    MissingScheme {
        origin: String,
    },
    #[error("failed to build request or response")]
    Build {
        #[from]
        source: http::Error,
    },
    #[error("failed request")]
    OriginRequest {
        #[from]
        source: hyper_util::client::legacy::Error,
    },
    #[error("failed to collect body")]
    OriginBody {
        #[from]
        source: hyper::Error,
    },
}

impl IntoResponse for RequestError {
    fn into_response(self) -> axum::response::Response {
        match self {
            RequestError::InvalidHostHeader {
                ..
            }
            | RequestError::MissingHost
            | RequestError::UnconfiguredHost {
                ..
            } => StatusCode::BAD_REQUEST.into_response(),
            RequestError::MissingOriginAuthority {
                ..
            } => {
                error!("request failed");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            RequestError::Build {
                ..
            } => {
                error!("request builder failed");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            RequestError::OriginRequest {
                ..
            } => {
                error!("request failed");
                StatusCode::BAD_GATEWAY.into_response()
            }
            RequestError::OriginBody {
                ..
            } => StatusCode::BAD_GATEWAY.into_response(),
            RequestError::MissingScheme {
                ..
            } => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

pub async fn handler(
    method: Method,
    uri: Uri,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut headers: HeaderMap,
    state: State<AppState>,
    body: Bytes,
) -> Result<Response<Collected<Bytes>>, RequestError> {
    let host = headers
        .get("host")
        .ok_or(RequestError::MissingHost)?
        .to_str()
        .map_err(|_| RequestError::MissingHost)?;
    let origin = state.config.get_origin(host).ok_or(RequestError::UnconfiguredHost {
        host: host.to_string(),
    })?;

    clean_headers(&mut headers);

    let request = build_request(&uri, &origin, headers, method, body)?;
    let origin_response = state.client.request(request).await?;

    let response = build_response(origin_response).await?;

    Ok(response)
}

fn build_request(
    uri: &Uri,
    origin_uri: &Uri,
    headers: HeaderMap,
    method: Method,
    body: Bytes,
) -> Result<Request<Full<Bytes>>, RequestError> {
    let pq = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("/");

    let request_uri = Uri::builder()
        .scheme(
            origin_uri
                .scheme()
                .ok_or(RequestError::MissingScheme {
                    origin: origin_uri.to_string(),
                })?
                .clone(),
        )
        .authority(
            origin_uri
                .authority()
                .ok_or(RequestError::MissingOriginAuthority {
                    origin: origin_uri.to_string(),
                })?
                .clone(),
        )
        .path_and_query(pq)
        .build()
        .map_err(|e| RequestError::Build {
            source: e,
        })?;

    let mut request = Request::builder().method(method).uri(request_uri).body(Full::new(body))?;
    let request_headers = request.headers_mut();
    request_headers.extend(headers);
    Ok(request)
}

async fn build_response(
    origin_response: Response<Incoming>,
) -> Result<Response<Collected<Bytes>>, RequestError> {
    let mut headers = origin_response.headers().clone();
    clean_headers(&mut headers);

    let (parts, body) = origin_response.into_parts();

    let status = parts.status;
    let mut response = Response::builder().status(status).body(body.collect().await?)?;
    let response_headers = response.headers_mut();
    response_headers.extend(headers);
    Ok(response)
}

fn clean_headers(headers: &mut HeaderMap) {
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
