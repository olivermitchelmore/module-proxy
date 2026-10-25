use crate::pipeline::{Decision, Module, RequestView};
use axum::http::StatusCode;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::error;

struct UserBucket {
    tokens: f64,
    last_request_time: Instant,
    increment_value_time: Duration,
    max_tokens: f64,
}

impl UserBucket {
    pub fn new(now: Instant) -> Self {
        // this value will come from config
        let duration = Duration::from_secs(1);
        let max_tokens = 5.0;

        Self {
            tokens: max_tokens,
            last_request_time: now,
            increment_value_time: duration,
            max_tokens,
        }
    }

    pub fn check_and_update(&mut self, now: Instant) -> Result<(), StatusCode> {
        let elapsed_time = now.duration_since(self.last_request_time);
        let current_token_count =
            self.tokens + elapsed_time.as_secs_f64() / self.increment_value_time.as_secs_f64();
        let current_token_count = current_token_count.min(self.max_tokens);

        if current_token_count < 1.0 {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        } else {
            self.tokens = current_token_count - 1.0;
            self.last_request_time = now;
        }
        Ok(())
    }
}

struct RateLimiter {
    user_buckets: Mutex<HashMap<IpAddr, UserBucket>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        RateLimiter {
            user_buckets: Mutex::new(HashMap::new()),
        }
    }
}
impl Module for RateLimiter {
    fn run(&self, request: &RequestView) -> Decision {
        let now = Instant::now();
        if let Ok(mut user_buckets) = self.user_buckets.lock() {
            match user_buckets.get_mut(&request.ip_addr) {
                Some(bucket) => match bucket.check_and_update(now) {
                    Ok(..) => Decision::Continue,
                    Err(sc) => Decision::Stop(sc),
                },
                None => {
                    let mut bucket = UserBucket::new(now);
                    bucket.tokens -= 1.0;
                    user_buckets.insert(request.ip_addr, bucket);
                    Decision::Continue
                }
            }
        } else {
            error!("poisoned lock on user buckets");
            Decision::Stop(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::assert_matches;
    use std::net::Ipv4Addr;
    use std::ops::Add;
    #[test]
    fn create_bucket_with_tokens_if_ip_unknown() {
        let addr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let request_view = RequestView::new("/".to_string(), addr);
        let rate_limiter = RateLimiter::new();
        let decision = rate_limiter.run(&request_view);
        let user_buckets = rate_limiter.user_buckets.lock().unwrap();
        let user_bucket = user_buckets.get(&addr).unwrap();
        assert_eq!(user_bucket.tokens, 4.0);
        assert_matches!(decision, Decision::Continue);
    }

    #[test]
    fn reject_too_many_requests() {
        let addr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let request_view = RequestView::new("/".to_string(), addr);
        let rate_limiter = RateLimiter::new();

        for _ in 1..5 {
            rate_limiter.run(&request_view);
        }
        // last request with eligible token count goes through
        let decision = rate_limiter.run(&request_view);

        let ineligible_decision = rate_limiter.run(&request_view);

        assert_matches!(decision, Decision::Continue);
        assert_matches!(ineligible_decision, Decision::Stop(StatusCode::TOO_MANY_REQUESTS));
    }

    #[test]
    fn allow_if_tokens_refilled() {
        let addr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let request_view = RequestView::new("/".to_string(), addr);
        let rate_limiter = RateLimiter::new();
        rate_limiter.run(&request_view);

        let mut user_buckets = rate_limiter.user_buckets.lock().unwrap();
        let user_bucket = user_buckets.get_mut(&addr).unwrap();

        user_bucket.tokens = 0.0;

        let time_after_refill = Instant::now().add(Duration::from_secs(5));
        user_bucket.check_and_update(time_after_refill).unwrap();

        assert_eq!(user_bucket.tokens, 4.0);
        drop(user_buckets);
        let decision = rate_limiter.run(&request_view);
        assert_matches!(decision, Decision::Continue);
    }
}
