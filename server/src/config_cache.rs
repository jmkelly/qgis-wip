//! Server configuration cache.
//!
//! Provides [`ConfigCache`], an in-memory LRU cache with TTL for server
//! configuration values. Expensive configuration lookups (parsing settings
//! files, resolving environment overrides, etc.) can be cached here so
//! repeated requests do not pay the lookup cost.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Default time-to-live for cached configuration values (5 minutes).
pub const DEFAULT_CONFIG_CACHE_TTL: Duration = Duration::from_secs(300);

/// Default maximum number of cached configuration entries.
pub const DEFAULT_CONFIG_CACHE_CAPACITY: usize = 1024;

/// An in-memory LRU cache of configuration values with a TTL.
///
/// Entries older than the TTL are treated as absent and evicted lazily.
/// When the capacity is exceeded the least-recently-used entry is evicted.
#[derive(Debug, Clone)]
pub struct ConfigCache {
    inner: Arc<Mutex<ConfigCacheInner>>,
}

#[derive(Debug)]
struct ConfigCacheInner {
    /// Time-to-live for each entry.
    ttl: Duration,
    /// Maximum number of entries before LRU eviction.
    capacity: usize,
    /// Access order, front = most recently used.
    order: VecDeque<String>,
    /// Stored values keyed by name.
    values: HashMap<String, (String, Instant)>,
}

impl ConfigCache {
    /// Creates a new config cache with the given TTL and capacity.
    pub fn new(ttl: Duration, capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ConfigCacheInner {
                ttl,
                capacity,
                order: VecDeque::new(),
                values: HashMap::new(),
            })),
        }
    }

    /// Creates a new config cache with the default TTL and the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self::new(DEFAULT_CONFIG_CACHE_TTL, capacity)
    }

    /// Returns the default TTL used for config caches.
    pub fn default_ttl() -> Duration {
        DEFAULT_CONFIG_CACHE_TTL
    }

    /// Returns the cached value for a key if present and not expired.
    pub fn get(&self, key: &str) -> Option<String> {
        let mut inner = self.inner.lock().expect("config cache poisoned");
        inner.get(key)
    }

    /// Inserts a value into the cache.
    pub fn insert(&self, key: impl Into<String>, value: impl Into<String>) {
        let mut inner = self.inner.lock().expect("config cache poisoned");
        inner.insert(key.into(), value.into());
    }

    /// Returns the cached value for a key, inserting it via `f` on a miss.
    pub fn get_or_insert_with(&self, key: &str, f: impl FnOnce() -> String) -> String {
        if let Some(value) = self.get(key) {
            return value;
        }
        let value = f();
        self.insert(key, value.clone());
        value
    }

    /// Removes a key from the cache.
    pub fn remove(&self, key: &str) -> bool {
        let mut inner = self.inner.lock().expect("config cache poisoned");
        inner.remove(key)
    }

    /// Clears all entries from the cache.
    pub fn clear(&self) {
        let mut inner = self.inner.lock().expect("config cache poisoned");
        inner.clear();
    }

    /// Returns the number of live (non-expired) entries.
    pub fn len(&self) -> usize {
        let mut inner = self.inner.lock().expect("config cache poisoned");
        inner.len()
    }

    /// Returns true if the cache holds no live entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the configured TTL.
    pub fn ttl(&self) -> Duration {
        self.inner.lock().expect("config cache poisoned").ttl
    }

    /// Returns the configured capacity.
    pub fn capacity(&self) -> usize {
        self.inner.lock().expect("config cache poisoned").capacity
    }
}

impl ConfigCacheInner {
    fn get(&mut self, key: &str) -> Option<String> {
        let now = Instant::now();
        let live = matches!(self.values.get(key), Some((_, exp)) if *exp > now);
        if !live {
            self.values.remove(key);
            return None;
        }
        self.touch(key);
        self.values.get(key).map(|(value, _)| value.clone())
    }

    fn insert(&mut self, key: String, value: String) {
        let now = Instant::now();
        if self.values.contains_key(&key) {
            self.values.insert(key.clone(), (value, now + self.ttl));
            self.touch(&key);
            return;
        }
        self.purge_expired(now);
        while self.values.len() >= self.capacity {
            if let Some(lru) = self.order.pop_back() {
                self.values.remove(&lru);
            } else {
                break;
            }
        }
        self.values.insert(key.clone(), (value, now + self.ttl));
        self.order.push_front(key);
    }

    fn remove(&mut self, key: &str) -> bool {
        if self.values.remove(key).is_some() {
            self.order.retain(|k| k != key);
            true
        } else {
            false
        }
    }

    fn clear(&mut self) {
        self.values.clear();
        self.order.clear();
    }

    fn len(&mut self) -> usize {
        self.purge_expired(Instant::now());
        self.values.len()
    }

    fn touch(&mut self, key: &str) {
        self.order.retain(|k| k != key);
        self.order.push_front(key.to_string());
    }

    fn purge_expired(&mut self, now: Instant) {
        let expired: Vec<String> = self
            .values
            .iter()
            .filter(|(_, (_, expires_at))| *expires_at <= now)
            .map(|(key, _)| key.clone())
            .collect();
        for key in expired {
            self.values.remove(&key);
            self.order.retain(|k| k != &key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache() -> ConfigCache {
        ConfigCache::new(Duration::from_secs(300), 16)
    }

    #[test]
    fn config_cache_insert_and_get() {
        let cache = cache();
        assert!(cache.is_empty());
        cache.insert("project.root", "/srv/qgis");
        assert_eq!(cache.get("project.root"), Some("/srv/qgis".to_string()));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn config_cache_miss() {
        let cache = cache();
        assert_eq!(cache.get("missing"), None);
    }

    #[test]
    fn config_cache_expiry() {
        let cache = ConfigCache::new(Duration::from_millis(20), 16);
        cache.insert("k", "v");
        assert_eq!(cache.get("k"), Some("v".to_string()));
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(cache.get("k"), None);
        assert!(cache.is_empty());
    }

    #[test]
    fn config_cache_overwrite_refreshes() {
        let cache = cache();
        cache.insert("k", "v1");
        assert_eq!(cache.get("k"), Some("v1".to_string()));
        cache.insert("k", "v2");
        assert_eq!(cache.get("k"), Some("v2".to_string()));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn config_cache_lru_eviction() {
        let cache = ConfigCache::new(Duration::from_secs(300), 2);
        cache.insert("a", "1");
        cache.insert("b", "2");
        assert!(cache.get("a").is_some());
        assert!(cache.get("b").is_some());
        cache.insert("c", "3");
        assert_eq!(cache.get("a"), None);
        assert!(cache.get("b").is_some());
        assert!(cache.get("c").is_some());
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn config_cache_remove_and_clear() {
        let cache = cache();
        cache.insert("a", "1");
        cache.insert("b", "2");
        assert!(cache.remove("a"));
        assert_eq!(cache.get("a"), None);
        assert!(cache.get("b").is_some());
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn config_cache_get_or_insert_with() {
        let cache = cache();
        let value = cache.get_or_insert_with("key", || "computed".to_string());
        assert_eq!(value, "computed");
        // Second call hits the cache and does not recompute.
        let value = cache.get_or_insert_with("key", || "other".to_string());
        assert_eq!(value, "computed");
    }

    #[test]
    fn config_cache_default_ttl_is_five_minutes() {
        assert_eq!(ConfigCache::default_ttl(), Duration::from_secs(300));
        assert_eq!(ConfigCache::with_capacity(4).ttl(), Duration::from_secs(300));
    }

    #[test]
    fn config_cache_threaded_access() {
        let cache = Arc::new(cache());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let cache = Arc::clone(&cache);
            handles.push(std::thread::spawn(move || {
                for i in 0..16 {
                    cache.insert(format!("key-{i}"), format!("value-{i}"));
                    let _ = cache.get(&format!("key-{i}"));
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(cache.len(), 16);
    }
}
