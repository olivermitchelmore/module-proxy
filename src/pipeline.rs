use axum::http::StatusCode;

pub struct RequestView {
    pub path: String,
}

impl RequestView {
    pub fn new(path: String) -> Self {
        Self {
            path,
        }
    }
}

#[derive(Debug)]
pub enum Decision {
    Continue,
    Stop(StatusCode),
}

pub trait Module: Send + Sync {
    fn run(&self, request: &RequestView) -> Decision;
}
