use crate::pipeline::{Decision, Module, RequestView};
use hyper::StatusCode;

pub struct BlockPath;

impl Module for BlockPath {
    fn run(&self, request: &RequestView) -> Decision {
        if request.path == "/blocked" {
            return Decision::Stop(StatusCode::FORBIDDEN);
        };
        Decision::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;

    #[test]
    fn blocks_blocked_path() {
        let block_path = BlockPath {};
        let request_view = RequestView::new("/blocked".to_string());

        let decision = block_path.run(&request_view);

        assert_matches!(decision, Decision::Stop(StatusCode::FORBIDDEN))
    }
}
