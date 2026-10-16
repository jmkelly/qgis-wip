//! Per-client token-bucket request throttling.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Thread-safe per-client token-bucket limiter.
#[derive(Debug, Clone, Default)]
pub struct RateLimiter {
    buckets: Arc<Mutex<HashMap<String, Bucket>>>,
}

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    updated_at: Instant,
}

impl RateLimiter {
    /// Returns whether this client may make a request at the configured rate.
    /// A zero rate disables throttling. The bucket capacity equals the rate,
    /// allowing a one-second burst while enforcing the sustained rate.
    pub fn allow(&self, client: &str, requests_per_second: u32) -> bool {
        if requests_per_second == 0 {
            return true;
        }
        let now = Instant::now();
        let rate = f64::from(requests_per_second);
        let mut buckets = self.buckets.lock().expect("rate limiter poisoned");
        let bucket = buckets.entry(client.to_owned()).or_insert(Bucket {
            tokens: rate,
            updated_at: now,
        });
        bucket.tokens = (bucket.tokens + now.duration_since(bucket.updated_at).as_secs_f64() * rate)
            .min(rate);
        bucket.updated_at = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Removes all client buckets, useful when reloading server policy.
    pub fn clear(&self) {
        self.buckets.lock().expect("rate limiter poisoned").clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn token_bucket_limits_each_client_independently_and_refills() {
        let limiter = RateLimiter::default();
        assert!(limiter.allow("a", 1));
        assert!(!limiter.allow("a", 1));
        assert!(limiter.allow("b", 1));
        thread::sleep(Duration::from_millis(1050));
        assert!(limiter.allow("a", 1));
    }

    #[test]
    fn zero_rate_disables_throttling() {
        let limiter = RateLimiter::default();
        assert!(limiter.allow("a", 0));
        assert!(limiter.allow("a", 0));
    }
}
