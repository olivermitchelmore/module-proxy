use crate::AppState;
use crate::pipeline::{Decision, RequestView};
use axum::extract::{ConnectInfo, State};
use axum::http;
use axum::http::uri::PathAndQuery;
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
    #[error("module returned stop")]
    Module {
        status_code: StatusCode,
    },
}

impl IntoResponse for RequestError {
    fn into_response(self) -> axum::response::Response {
        match self {
            RequestError::Module {
                status_code,
            } => status_code.into_response(),
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

    let pq: PathAndQuery = uri.path_and_query().unwrap_or(&PathAndQuery::from_static("/")).clone();
    let path = pq.path();

    let request_view = RequestView::new(path.to_string());

    for module in state.modules.iter() {
        let decision = module.run(&request_view);
        match decision {
            Decision::Continue => continue,
            Decision::Stop(status_code) => {
                return Err(RequestError::Module {
                    status_code,
                });
            }
        }
    }

    let request = build_request(origin, headers, method, body, pq)?;
    let origin_response = state.client.request(request).await?;

    let response = build_response(origin_response).await?;

    Ok(response)
}

fn build_request(
    origin_uri: &Uri,
    headers: HeaderMap,
    method: Method,
    body: Bytes,
    pq: PathAndQuery,
) -> Result<Request<Full<Bytes>>, RequestError> {
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
