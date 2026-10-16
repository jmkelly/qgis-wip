//! Bounded in-memory cache for repeatable HTTP responses.

use bytes::Bytes;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A cached response body and its HTTP metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedResponse {
    /// HTTP status code.
    pub status: u16,
    /// MIME type for the response body.
    pub content_type: String,
    /// Fully buffered response body.
    pub body: Bytes,
}

/// Thread-safe LRU cache with a fixed time-to-live.
#[derive(Debug, Clone)]
pub struct ResponseCache {
    inner: Arc<Mutex<CacheInner>>,
}

#[derive(Debug)]
struct CacheInner {
    ttl: Duration,
    capacity: usize,
    entries: HashMap<String, (Instant, CachedResponse)>,
    order: VecDeque<String>,
}

impl ResponseCache {
    /// Creates a cache with the requested TTL and maximum entry count.
    pub fn new(ttl: Duration, capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CacheInner {
                ttl,
                capacity,
                entries: HashMap::new(),
                order: VecDeque::new(),
            })),
        }
    }

    /// Returns a live entry and marks it most recently used.
    pub fn get(&self, key: &str) -> Option<CachedResponse> {
        let mut inner = self.inner.lock().expect("response cache poisoned");
        let (inserted, response) = inner.entries.get(key)?;
        if inserted.elapsed() >= inner.ttl {
            inner.entries.remove(key);
            inner.order.retain(|candidate| candidate != key);
            return None;
        }
        let response = response.clone();
        inner.order.retain(|candidate| candidate != key);
        inner.order.push_front(key.to_owned());
        Some(response)
    }

    /// Inserts or replaces an entry, evicting least-recently-used entries.
    pub fn insert(&self, key: impl Into<String>, response: CachedResponse) {
        let key = key.into();
        let mut inner = self.inner.lock().expect("response cache poisoned");
        if inner.capacity == 0 || inner.ttl.is_zero() {
            return;
        }
        inner.entries.remove(&key);
        inner.order.retain(|candidate| candidate != &key);
        while inner.entries.len() >= inner.capacity {
            if let Some(lru) = inner.order.pop_back() {
                inner.entries.remove(&lru);
            } else {
                break;
            }
        }
        inner.entries.insert(key.clone(), (Instant::now(), response));
        inner.order.push_front(key);
    }

    /// Removes all cached responses.
    pub fn clear(&self) {
        let mut inner = self.inner.lock().expect("response cache poisoned");
        inner.entries.clear();
        inner.order.clear();
    }

    /// Returns the number of currently stored entries (including expired ones
    /// which have not yet been accessed).
    pub fn len(&self) -> usize {
        self.inner.lock().expect("response cache poisoned").entries.len()
    }

    /// Returns true if no entries are currently stored.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(value: &'static str) -> CachedResponse {
        CachedResponse {
            status: 200,
            content_type: "application/xml".to_string(),
            body: Bytes::from_static(value.as_bytes()),
        }
    }

    #[test]
    fn response_cache_round_trip_and_lru() {
        let cache = ResponseCache::new(Duration::from_secs(60), 2);
        cache.insert("a", response("one"));
        cache.insert("b", response("two"));
        assert_eq!(cache.get("a"), Some(response("one")));
        cache.insert("c", response("three"));
        assert!(cache.get("b").is_none());
        assert_eq!(cache.get("c"), Some(response("three")));
    }

    #[test]
    fn response_cache_expires_and_zero_capacity_disables_storage() {
        let cache = ResponseCache::new(Duration::from_millis(1), 2);
        cache.insert("a", response("one"));
        std::thread::sleep(Duration::from_millis(3));
        assert!(cache.get("a").is_none());

        let disabled = ResponseCache::new(Duration::from_secs(1), 0);
        disabled.insert("a", response("one"));
        assert!(disabled.is_empty());
    }
}
