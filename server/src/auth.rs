//! Authentication support for the QGIS server.
//!
//! Provides the building blocks for server authentication:
//!
//! - [`AuthCache`] — an in-memory LRU cache of validated credentials with a
//!   time-to-live, so repeated requests with the same credential avoid
//!   re-validation.
//! - [`TrustedHeaders`] — extraction of caller identity from HTTP headers that
//!   the server has been configured to trust (e.g. set by a reverse proxy).
//! - [`ApiKeyAuth`] — API key validation backed by an [`AuthCache`].
//! - [`parse_basic_auth`] — parsing of `Authorization: Basic` headers.
//!
//! All components are thread-safe and async-compatible (they never block the
//! executor for more than a short critical section).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::config::ServerConfig;

/// Default time-to-live for cached authenticated credentials (5 minutes).
pub const DEFAULT_AUTH_CACHE_TTL: Duration = Duration::from_secs(300);

/// Default maximum number of cached authenticated credentials.
pub const DEFAULT_AUTH_CACHE_CAPACITY: usize = 1024;

/// A cached authentication decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthEntry {
    /// The validated credential subject (API key, user name, etc.).
    pub subject: String,
    /// The instant at which this entry expires.
    pub expires_at: Instant,
}

/// An in-memory LRU cache of validated credentials with a TTL.
///
/// The cache is keyed by credential subject. Entries older than the TTL are
/// treated as absent and evicted lazily. When the capacity is exceeded the
/// least-recently-used entry is evicted.
#[derive(Debug, Clone)]
pub struct AuthCache {
    inner: Arc<Mutex<AuthCacheInner>>,
}

#[derive(Debug)]
struct AuthCacheInner {
    /// Time-to-live for each entry.
    ttl: Duration,
    /// Maximum number of entries before LRU eviction.
    capacity: usize,
    /// Access order, front = most recently used.
    order: VecDeque<String>,
    /// Stored entries keyed by subject.
    entries: HashMap<String, Instant>,
}

impl AuthCache {
    /// Creates a new auth cache with the given TTL and capacity.
    pub fn new(ttl: Duration, capacity: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(AuthCacheInner {
                ttl,
                capacity,
                order: VecDeque::new(),
                entries: HashMap::new(),
            })),
        }
    }

    /// Creates a new auth cache with the default TTL and the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self::new(DEFAULT_AUTH_CACHE_TTL, capacity)
    }

    /// Returns the default TTL used for auth caches.
    pub fn default_ttl() -> Duration {
        DEFAULT_AUTH_CACHE_TTL
    }

    /// Caches a validated credential subject.
    pub fn insert(&self, subject: impl Into<String>) {
        let mut inner = self.inner.lock().expect("auth cache poisoned");
        inner.insert(subject.into());
    }

    /// Returns true if the subject is cached and not expired.
    pub fn contains(&self, subject: &str) -> bool {
        let mut inner = self.inner.lock().expect("auth cache poisoned");
        inner.contains(subject)
    }

    /// Validates a credential subject against the cache.
    ///
    /// This is an alias for [`AuthCache::contains`] matching the
    /// authentication vocabulary.
    pub fn validate(&self, subject: &str) -> bool {
        self.contains(subject)
    }

    /// Returns the cache entry for a subject if present and not expired.
    pub fn get(&self, subject: &str) -> Option<AuthEntry> {
        let mut inner = self.inner.lock().expect("auth cache poisoned");
        inner.get(subject)
    }

    /// Removes a subject from the cache.
    pub fn remove(&self, subject: &str) -> bool {
        let mut inner = self.inner.lock().expect("auth cache poisoned");
        inner.remove(subject)
    }

    /// Clears all entries from the cache.
    pub fn clear(&self) {
        let mut inner = self.inner.lock().expect("auth cache poisoned");
        inner.clear();
    }

    /// Returns the number of live (non-expired) entries.
    pub fn len(&self) -> usize {
        let mut inner = self.inner.lock().expect("auth cache poisoned");
        inner.len()
    }

    /// Returns true if the cache holds no live entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the configured TTL.
    pub fn ttl(&self) -> Duration {
        self.inner.lock().expect("auth cache poisoned").ttl
    }

    /// Returns the configured capacity.
    pub fn capacity(&self) -> usize {
        self.inner.lock().expect("auth cache poisoned").capacity
    }
}

impl AuthCacheInner {
    fn insert(&mut self, subject: String) {
        let now = Instant::now();
        // Refresh existing entry.
        if self.entries.contains_key(&subject) {
            self.entries.insert(subject.clone(), now + self.ttl);
            self.touch(&subject);
            return;
        }
        // Evict expired entries first.
        self.purge_expired(now);
        // Evict LRU entries until there is room.
        while self.entries.len() >= self.capacity {
            if let Some(lru) = self.order.pop_back() {
                self.entries.remove(&lru);
            } else {
                break;
            }
        }
        self.entries.insert(subject.clone(), now + self.ttl);
        self.order.push_front(subject);
    }

    fn contains(&mut self, subject: &str) -> bool {
        let now = Instant::now();
        match self.entries.get(subject) {
            Some(&expires_at) if expires_at > now => {
                self.touch(subject);
                true
            }
            Some(_) => {
                // Expired: evict lazily.
                self.entries.remove(subject);
                false
            }
            None => false,
        }
    }

    fn get(&mut self, subject: &str) -> Option<AuthEntry> {
        let now = Instant::now();
        let &expires_at = self.entries.get(subject)?;
        if expires_at <= now {
            self.entries.remove(subject);
            return None;
        }
        self.touch(subject);
        Some(AuthEntry { subject: subject.to_string(), expires_at })
    }

    fn remove(&mut self, subject: &str) -> bool {
        if self.entries.remove(subject).is_some() {
            self.order.retain(|s| s != subject);
            true
        } else {
            false
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    fn len(&mut self) -> usize {
        self.purge_expired(Instant::now());
        self.entries.len()
    }

    fn touch(&mut self, subject: &str) {
        self.order.retain(|s| s != subject);
        self.order.push_front(subject.to_string());
    }

    fn purge_expired(&mut self, now: Instant) {
        let expired: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, &expires_at)| expires_at <= now)
            .map(|(subject, _)| subject.clone())
            .collect();
        for subject in expired {
            self.entries.remove(&subject);
            self.order.retain(|s| s != &subject);
        }
    }
}

/// A set of HTTP headers the server trusts to carry caller identity.
///
/// A reverse proxy in front of the server can set these headers after it
/// has authenticated the caller. Only headers listed here are consulted;
/// arbitrary client-supplied headers are ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustedHeaders {
    /// Trusted header names (compared case-insensitively).
    headers: Vec<String>,
}

impl TrustedHeaders {
    /// Creates a new trusted header set from the given names.
    pub fn new(headers: Vec<String>) -> Self {
        Self { headers }
    }

    /// Creates a trusted header set from string slices.
    pub fn from_slice(headers: &[&str]) -> Self {
        Self {
            headers: headers.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Returns true if the given header name is trusted (case-insensitive).
    pub fn is_trusted(&self, name: &str) -> bool {
        self.headers.iter().any(|h| h.eq_ignore_ascii_case(name))
    }

    /// Extracts the caller identity from a header map.
    ///
    /// Returns the value of the first trusted header that is present.
    pub fn extract<'a>(&self, headers: &'a HashMap<String, String>) -> Option<&'a str> {
        for trusted in &self.headers {
            for (name, value) in headers {
                if name.eq_ignore_ascii_case(trusted) && !value.is_empty() {
                    return Some(value.as_str());
                }
            }
        }
        None
    }

    /// Returns the number of trusted header names.
    pub fn len(&self) -> usize {
        self.headers.len()
    }

    /// Returns true if no headers are trusted.
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }

    /// Returns the trusted header names.
    pub fn as_slice(&self) -> &[String] {
        &self.headers
    }
}

/// API key authentication backed by an [`AuthCache`].
///
/// Valid API keys are cached for the cache TTL so repeated requests skip
/// re-validation. Key comparison is constant-time to avoid timing leaks.
#[derive(Debug, Clone)]
pub struct ApiKeyAuth {
    /// Cache of validated keys.
    cache: AuthCache,
    /// The set of valid API keys.
    keys: Vec<String>,
}

impl ApiKeyAuth {
    /// Creates an API key authenticator from the server configuration.
    pub fn from_config(config: &ServerConfig) -> Self {
        Self {
            cache: config.auth_cache(),
            keys: config.api_keys.clone(),
        }
    }

    /// Creates an API key authenticator with an explicit key set and cache.
    pub fn new(keys: Vec<String>, cache: AuthCache) -> Self {
        Self { cache, keys }
    }

    /// Validates an API key, caching successful validations.
    pub fn validate(&self, key: &str) -> bool {
        let cache_key = credential_cache_key("api", key.as_bytes());
        if self.cache.contains(&cache_key) {
            return true;
        }
        if self.keys.iter().any(|k| constant_time_eq(k.as_bytes(), key.as_bytes())) {
            self.cache.insert(cache_key);
            return true;
        }
        false
    }

    /// Returns a reference to the underlying auth cache.
    pub fn cache(&self) -> &AuthCache {
        &self.cache
    }

    /// Returns the number of configured API keys.
    pub fn key_count(&self) -> usize {
        self.keys.len()
    }
}

/// Authentication policy supporting API keys, HTTP Basic, and HS256 JWTs.
///
/// Authentication is disabled when no credentials are configured, preserving
/// the default development-server behavior. A successfully verified
/// credential is cached for the configured auth-cache TTL (five minutes by
/// default). Secrets are represented in the cache by SHA-256 fingerprints,
/// never stored as cache keys in plaintext.
#[derive(Debug, Clone)]
pub struct Authenticator {
    api_keys: Vec<String>,
    basic_credentials: Vec<(String, String)>,
    jwt_secret: Option<String>,
    cache: AuthCache,
}

impl Authenticator {
    /// Constructs the configured authenticator.
    pub fn from_config(config: &ServerConfig) -> Self {
        Self {
            api_keys: config.api_keys.clone(),
            basic_credentials: config.basic_credentials.clone(),
            jwt_secret: config.jwt_secret.clone(),
            cache: config.auth_cache(),
        }
    }

    /// Returns whether at least one authentication mechanism is configured.
    pub fn is_enabled(&self) -> bool {
        !self.api_keys.is_empty()
            || !self.basic_credentials.is_empty()
            || self.jwt_secret.is_some()
    }

    /// Validates the request credentials. Bearer credentials may be either an
    /// API key or an HS256 JWT; `x-api-key` is also accepted for API keys.
    pub fn authenticate(
        &self,
        authorization: Option<&str>,
        api_key_header: Option<&str>,
    ) -> bool {
        if !self.is_enabled() {
            return true;
        }

        let bearer = authorization.and_then(parse_bearer);
        if let Some(key) = api_key_header.or(bearer) {
            let cache_key = credential_cache_key("api", key.as_bytes());
            if self.cache.contains(&cache_key)
                || self.api_keys.iter().any(|valid| constant_time_eq(valid.as_bytes(), key.as_bytes()))
            {
                self.cache.insert(cache_key);
                return true;
            }

            if let Some(secret) = &self.jwt_secret {
                let jwt_cache_key = credential_cache_key("jwt", key.as_bytes());
                if self.cache.contains(&jwt_cache_key) && parse_jwt_claims(key).is_some()
                    || verify_hs256_jwt(key, secret).is_some()
                {
                    self.cache.insert(jwt_cache_key);
                    return true;
                }
            }
        }

        if let Some(header) = authorization {
            if let Some((username, password)) = parse_basic_auth(header) {
                let mut material = username.as_bytes().to_vec();
                material.push(0);
                material.extend_from_slice(password.as_bytes());
                let cache_key = credential_cache_key("basic", &material);
                if self.cache.contains(&cache_key)
                    || self.basic_credentials.iter().any(|(user, pass)| {
                        constant_time_eq(user.as_bytes(), username.as_bytes())
                            && constant_time_eq(pass.as_bytes(), password.as_bytes())
                    })
                {
                    self.cache.insert(cache_key);
                    return true;
                }
            }
        }
        false
    }

    /// Returns the shared credential cache.
    pub fn cache(&self) -> &AuthCache {
        &self.cache
    }
}

fn parse_bearer(header: &str) -> Option<&str> {
    let (scheme, token) = header.split_once(' ')?;
    (scheme.eq_ignore_ascii_case("bearer") && !token.trim().is_empty())
        .then(|| token.trim())
}

fn credential_cache_key(kind: &str, credential: &[u8]) -> String {
    let digest = Sha256::digest(credential);
    let mut key = String::with_capacity(kind.len() + 1 + digest.len() * 2);
    key.push_str(kind);
    key.push(':');
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(key, "{byte:02x}");
    }
    key
}

/// Validates an HS256 JWT's signature and time claims, returning its subject
/// when present. Only HS256 is accepted; algorithm negotiation is forbidden.
fn verify_hs256_jwt(token: &str, secret: &str) -> Option<Option<String>> {
    let mut parts = token.split('.');
    let (header_part, claims_part, signature_part) =
        (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }

    let header: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header_part).ok()?).ok()?;
    if header.get("alg")?.as_str()? != "HS256" {
        return None;
    }
    let claims = parse_jwt_claims(token)?;

    let signature = URL_SAFE_NO_PAD.decode(signature_part).ok()?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(format!("{header_part}.{claims_part}").as_bytes());
    mac.verify_slice(&signature).ok()?;
    Some(claims.get("sub").and_then(serde_json::Value::as_str).map(str::to_owned))
}

fn parse_jwt_claims(token: &str) -> Option<serde_json::Value> {
    let mut parts = token.split('.');
    let (header_part, claims_part, _) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let header: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header_part).ok()?).ok()?;
    if header.get("alg")?.as_str()? != "HS256" {
        return None;
    }
    let claims: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims_part).ok()?).ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    if let Some(exp) = claims.get("exp") {
        if exp.as_u64()?.le(&now) {
            return None;
        }
    }
    if let Some(nbf) = claims.get("nbf") {
        if nbf.as_u64()? > now {
            return None;
        }
    }
    Some(claims)
}

/// Compares two byte strings in constant time.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Parses an `Authorization: Basic <base64(user:password)>` header value.
///
/// Returns the `(username, password)` pair, or `None` if the header is not
/// a valid Basic auth header.
pub fn parse_basic_auth(header: &str) -> Option<(String, String)> {
    let (scheme, rest) = header.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let decoded = base64_decode(rest.trim())?;
    let text = String::from_utf8(decoded).ok()?;
    let (user, password) = text.split_once(':')?;
    Some((user.to_string(), password.to_string()))
}

/// Decodes a base64 string (standard alphabet, padding optional).
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lut = [255u8; 256];
    for (i, &c) in TABLE.iter().enumerate() {
        lut[c as usize] = i as u8;
    }
    let mut out = Vec::with_capacity(input.len() * 3 / 4 + 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for &c in input.as_bytes() {
        if c == b'=' || c == b'\n' || c == b'\r' {
            continue;
        }
        let value = lut[c as usize];
        if value == 255 {
            return None;
        }
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    fn cache() -> AuthCache {
        AuthCache::new(Duration::from_secs(300), 16)
    }

    fn jwt(secret: &str, claims: &str) -> String {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#);
        let claims = URL_SAFE_NO_PAD.encode(claims);
        let message = format!("{header}.{claims}");
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(message.as_bytes());
        format!("{message}.{}", URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn auth_cache_insert_and_contains() {
        let cache = cache();
        assert!(cache.is_empty());
        cache.insert("key-1");
        assert!(cache.contains("key-1"));
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());
    }

    #[test]
    fn auth_cache_miss() {
        let cache = cache();
        assert!(!cache.contains("missing"));
    }

    #[test]
    fn auth_cache_expiry() {
        let cache = AuthCache::new(Duration::from_millis(20), 16);
        cache.insert("key-1");
        assert!(cache.contains("key-1"));
        std::thread::sleep(Duration::from_millis(30));
        assert!(!cache.contains("key-1"));
        assert!(cache.is_empty());
    }

    #[test]
    fn auth_cache_lru_eviction() {
        let cache = AuthCache::new(Duration::from_secs(300), 2);
        cache.insert("a");
        cache.insert("b");
        assert!(cache.contains("a"));
        assert!(cache.contains("b"));
        // Inserting a third entry evicts the least-recently-used ("a").
        cache.insert("c");
        assert!(!cache.contains("a"));
        assert!(cache.contains("b"));
        assert!(cache.contains("c"));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn auth_cache_remove_and_clear() {
        let cache = cache();
        cache.insert("a");
        cache.insert("b");
        assert!(cache.remove("a"));
        assert!(!cache.contains("a"));
        assert!(cache.contains("b"));
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn auth_cache_validate_alias() {
        let cache = cache();
        cache.insert("k");
        assert!(cache.validate("k"));
        assert!(!cache.validate("other"));
    }

    #[test]
    fn auth_cache_default_ttl_is_five_minutes() {
        assert_eq!(AuthCache::default_ttl(), Duration::from_secs(300));
        assert_eq!(AuthCache::with_capacity(4).ttl(), Duration::from_secs(300));
    }

    #[test]
    fn auth_cache_get_returns_entry() {
        let cache = cache();
        cache.insert("k");
        let entry = cache.get("k").unwrap();
        assert_eq!(entry.subject, "k");
        assert!(entry.expires_at > Instant::now());
        assert!(cache.get("missing").is_none());
    }

    #[test]
    fn trusted_headers_is_trusted_case_insensitive() {
        let trusted = TrustedHeaders::from_slice(&["X-Qgis-User", "Remote-User"]);
        assert!(trusted.is_trusted("x-qgis-user"));
        assert!(trusted.is_trusted("REMOTE-USER"));
        assert!(!trusted.is_trusted("x-other"));
    }

    #[test]
    fn trusted_headers_extract() {
        let trusted = TrustedHeaders::from_slice(&["X-Qgis-User"]);
        let mut headers = HashMap::new();
        headers.insert("X-Qgis-User".to_string(), "alice".to_string());
        assert_eq!(trusted.extract(&headers), Some("alice"));

        let mut headers = HashMap::new();
        headers.insert("X-Qgis-User".to_string(), "alice".to_string());
        headers.insert("X-Qgis-Role".to_string(), "admin".to_string());
        let trusted_role = TrustedHeaders::from_slice(&["X-Qgis-Role"]);
        assert_eq!(trusted_role.extract(&headers), Some("admin"));

        let untrusted = TrustedHeaders::default();
        assert_eq!(untrusted.extract(&headers), None);
    }

    #[test]
    fn trusted_headers_empty_value_ignored() {
        let trusted = TrustedHeaders::from_slice(&["X-Qgis-User"]);
        let mut headers = HashMap::new();
        headers.insert("X-Qgis-User".to_string(), "".to_string());
        assert_eq!(trusted.extract(&headers), None);
    }

    #[test]
    fn api_key_auth_validate_and_cache() {
        let keys = vec!["secret-key".to_string()];
        let cache = cache();
        let auth = ApiKeyAuth::new(keys, cache.clone());
        assert_eq!(auth.key_count(), 1);
        assert!(auth.validate("secret-key"));
        assert!(!auth.validate("wrong-key"));
        // The cache is consulted first; an empty key set still validates
        // previously validated keys.
        let auth2 = ApiKeyAuth::new(vec![], cache);
        assert!(auth2.validate("secret-key"));
    }

    #[test]
    fn api_key_auth_from_config() {
        let config = ServerConfig::default();
        let auth = ApiKeyAuth::from_config(&config);
        assert_eq!(auth.key_count(), 0);
        assert!(!auth.validate("any"));
    }

    #[test]
    fn constant_time_eq_works() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn parse_basic_auth_valid() {
        // base64("user:pass") = "dXNlcjpwYXNz"
        let (user, pass) = parse_basic_auth("Basic dXNlcjpwYXNz").unwrap();
        assert_eq!(user, "user");
        assert_eq!(pass, "pass");
    }

    #[test]
    fn parse_basic_auth_invalid() {
        assert!(parse_basic_auth("Bearer token").is_none());
        assert!(parse_basic_auth("Basic not-base64!").is_none());
        assert!(parse_basic_auth("").is_none());
    }

    #[test]
    fn authenticator_supports_api_key_basic_and_jwt() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let config = ServerConfig::default()
            .with_api_keys(vec!["api-secret".to_string()])
            .with_basic_credentials(vec![("alice".to_string(), "password".to_string())])
            .with_jwt_secret("jwt-secret");
        let auth = Authenticator::from_config(&config);
        assert!(!auth.authenticate(None, None));
        assert!(auth.authenticate(Some("Bearer api-secret"), None));
        assert!(auth.authenticate(Some("Basic YWxpY2U6cGFzc3dvcmQ="), None));

        let token = jwt("jwt-secret", &format!(r#"{{"sub":"alice","exp":{}}}"#, now + 60));
        assert!(auth.authenticate(Some(&format!("Bearer {token}")), None));
        assert!(auth.cache().len() >= 3);
        assert!(!auth.authenticate(Some("Bearer invalid"), None));
    }

    #[test]
    fn jwt_verification_rejects_expired_and_wrong_signature() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(verify_hs256_jwt(
            &jwt("secret", &format!(r#"{{"exp":{}}}"#, now - 1)),
            "secret"
        )
        .is_none());
        assert!(verify_hs256_jwt(
            &jwt("wrong", &format!(r#"{{"exp":{}}}"#, now + 60)),
            "secret"
        )
        .is_none());
    }

    #[test]
    fn base64_decode_known_vectors() {
        assert_eq!(base64_decode("dXNlcjpwYXNz").unwrap(), b"user:pass");
        assert_eq!(base64_decode("YQ==").unwrap(), b"a");
        assert_eq!(base64_decode("YWI=").unwrap(), b"ab");
        assert_eq!(base64_decode("YWJj").unwrap(), b"abc");
    }

    #[test]
    fn auth_cache_threaded_insert() {
        let cache = Arc::new(cache());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let cache = Arc::clone(&cache);
            handles.push(std::thread::spawn(move || {
                for _ in 0..16 {
                    cache.insert("static-key");
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert!(cache.contains("static-key"));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn trusted_headers_as_slice() {
        let trusted = TrustedHeaders::from_slice(&["A", "B"]);
        assert_eq!(trusted.as_slice(), &["A", "B"]);
    }
}
