use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::header::{HeaderName, HeaderValue, ORIGIN, SET_COOKIE, USER_AGENT};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use tokio::sync::Mutex;
use uuid::Uuid;
use webauthn_rs::prelude::{PasskeyAuthentication, PasskeyRegistration};

use crate::state::AppState;

pub const REQUEST_ID_HEADER: &str = "x-request-id";
pub const CSRF_HEADER: &str = "x-csrf-token";
pub const CSRF_COOKIE_NAME: &str = "sao_csrf";

const DEFAULT_ALLOWED_ORIGIN: &str = "http://localhost:3100";
const DEFAULT_FRONTEND_ORIGIN: &str = "http://localhost:3100";
const DEFAULT_CHALLENGE_TTL_SECONDS: u64 = 300;

#[derive(Clone, Debug)]
pub struct RequestAuditContext {
    pub request_id: String,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
    pub origin: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CookieConfig {
    pub secure: bool,
    pub domain: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SecurityState {
    pub cookie_config: CookieConfig,
    pub allowed_origins: HashSet<String>,
    pub frontend_origin: String,
    pub challenge_store: Arc<ChallengeStore>,
    pub rate_limiter: Arc<RateLimiter>,
}

impl SecurityState {
    pub fn from_env() -> Self {
        let rp_origin = std::env::var("SAO_RP_ORIGIN")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let frontend_origin = std::env::var("SAO_FRONTEND_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| rp_origin.clone())
            .unwrap_or_else(|| DEFAULT_FRONTEND_ORIGIN.to_string());
        let allowed_origins = std::env::var("SAO_ALLOWED_ORIGINS")
            .ok()
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|origin| !origin.is_empty())
                    .map(ToString::to_string)
                    .collect::<HashSet<_>>()
            })
            .filter(|origins| !origins.is_empty())
            .unwrap_or_else(|| {
                let mut origins = HashSet::new();
                origins.insert(
                    rp_origin
                        .clone()
                        .unwrap_or_else(|| DEFAULT_ALLOWED_ORIGIN.to_string()),
                );
                origins
            });
        let secure = std::env::var("SAO_COOKIE_SECURE")
            .ok()
            .and_then(|value| parse_bool(&value))
            .unwrap_or_else(|| {
                allowed_origins
                    .iter()
                    .any(|origin| origin.starts_with("https://"))
            });
        let domain = std::env::var("SAO_COOKIE_DOMAIN")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        Self {
            cookie_config: CookieConfig { secure, domain },
            allowed_origins,
            frontend_origin,
            challenge_store: Arc::new(ChallengeStore::default()),
            rate_limiter: Arc::new(RateLimiter::default()),
        }
    }

    pub fn frontend_origin(&self) -> &str {
        &self.frontend_origin
    }

    pub fn is_allowed_origin(&self, origin: &str) -> bool {
        self.allowed_origins.contains(origin.trim())
    }
}

#[derive(Debug, Default)]
pub struct ChallengeStore {
    entries: Mutex<HashMap<String, ChallengeState>>,
}

impl ChallengeStore {
    pub async fn insert_registration(
        &self,
        challenge_id: String,
        user_id: uuid::Uuid,
        state: PasskeyRegistration,
    ) {
        self.cleanup_expired().await;
        self.entries.lock().await.insert(
            challenge_id,
            ChallengeState::WebauthnRegistration {
                user_id,
                state,
                expires_at: expiry_deadline(),
            },
        );
    }

    pub async fn consume_registration(
        &self,
        challenge_id: &str,
    ) -> Option<(uuid::Uuid, PasskeyRegistration)> {
        self.cleanup_expired().await;
        let state = self.entries.lock().await.remove(challenge_id)?;
        match state {
            ChallengeState::WebauthnRegistration {
                user_id,
                state,
                expires_at,
            } if expires_at > Instant::now() => Some((user_id, state)),
            _ => None,
        }
    }

    pub async fn insert_authentication(
        &self,
        challenge_id: String,
        user_id: uuid::Uuid,
        state: PasskeyAuthentication,
    ) {
        self.cleanup_expired().await;
        self.entries.lock().await.insert(
            challenge_id,
            ChallengeState::WebauthnAuthentication {
                user_id,
                state,
                expires_at: expiry_deadline(),
            },
        );
    }

    pub async fn consume_authentication(
        &self,
        challenge_id: &str,
    ) -> Option<(uuid::Uuid, PasskeyAuthentication)> {
        self.cleanup_expired().await;
        let state = self.entries.lock().await.remove(challenge_id)?;
        match state {
            ChallengeState::WebauthnAuthentication {
                user_id,
                state,
                expires_at,
            } if expires_at > Instant::now() => Some((user_id, state)),
            _ => None,
        }
    }

    pub async fn insert_oidc_state(&self, state_id: String, provider_key: String, nonce: String) {
        self.cleanup_expired().await;
        self.entries.lock().await.insert(
            state_id,
            ChallengeState::Oidc {
                provider_key,
                nonce,
                expires_at: expiry_deadline(),
            },
        );
    }

    pub async fn consume_oidc_state(&self, state_id: &str) -> Option<(String, String)> {
        self.cleanup_expired().await;
        let state = self.entries.lock().await.remove(state_id)?;
        match state {
            ChallengeState::Oidc {
                provider_key,
                nonce,
                expires_at,
            } if expires_at > Instant::now() => Some((provider_key, nonce)),
            _ => None,
        }
    }

    async fn cleanup_expired(&self) {
        let now = Instant::now();
        self.entries
            .lock()
            .await
            .retain(|_, state| state.expires_at() > now);
    }
}

#[derive(Debug)]
enum ChallengeState {
    WebauthnRegistration {
        user_id: uuid::Uuid,
        state: PasskeyRegistration,
        expires_at: Instant,
    },
    WebauthnAuthentication {
        user_id: uuid::Uuid,
        state: PasskeyAuthentication,
        expires_at: Instant,
    },
    Oidc {
        provider_key: String,
        nonce: String,
        expires_at: Instant,
    },
}

impl ChallengeState {
    fn expires_at(&self) -> Instant {
        match self {
            Self::WebauthnRegistration { expires_at, .. }
            | Self::WebauthnAuthentication { expires_at, .. }
            | Self::Oidc { expires_at, .. } => *expires_at,
        }
    }
}

#[derive(Debug, Default)]
pub struct RateLimiter {
    buckets: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl RateLimiter {
    pub async fn allow(&self, key: &str, limit: usize, window: Duration) -> bool {
        let now = Instant::now();
        let mut guard = self.buckets.lock().await;
        let bucket = guard.entry(key.to_string()).or_default();
        while matches!(bucket.front(), Some(front) if now.duration_since(*front) > window) {
            bucket.pop_front();
        }
        if bucket.len() >= limit {
            return false;
        }
        bucket.push_back(now);
        true
    }
}

pub async fn enforce_request_security(
    State(state): State<AppState>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let context = build_request_context(request.headers());
    let request_id = context.request_id.clone();
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let cookies = parse_cookie_header(request.headers());
    let missing_csrf_cookie = !cookies.contains_key(CSRF_COOKIE_NAME);

    request.extensions_mut().insert(context.clone());

    if let Some((bucket, limit, window)) = rate_limit_bucket(&method, &path) {
        let subject = context.client_ip.as_deref().unwrap_or("unknown");
        let key = format!("{bucket}:{subject}");
        if !state
            .inner
            .security
            .rate_limiter
            .allow(&key, limit, window)
            .await
        {
            tracing::warn!(
                request_id = %request_id,
                client_ip = ?context.client_ip,
                path = %path,
                bucket = bucket,
                "Rate limit exceeded"
            );
            let mut response = (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({
                    "error": "Too many requests",
                    "request_id": request_id,
                })),
            )
                .into_response();
            set_request_id_header(response.headers_mut(), &request_id);
            return response;
        }
    }

    if path.starts_with("/api/")
        && requires_csrf(&method)
        && !is_orion_machine_request(&path, request.headers())
    {
        if let Some(origin) = context.origin.as_deref() {
            if !state.inner.security.is_allowed_origin(origin) {
                tracing::warn!(
                    request_id = %request_id,
                    origin = %origin,
                    path = %path,
                    "Rejected request due to disallowed origin"
                );
                let mut response = (
                    StatusCode::FORBIDDEN,
                    Json(json!({
                        "error": "Origin is not allowed",
                        "request_id": request_id,
                    })),
                )
                    .into_response();
                set_request_id_header(response.headers_mut(), &request_id);
                return response;
            }
        }

        let csrf_cookie = cookies.get(CSRF_COOKIE_NAME).map(String::as_str);
        let csrf_header = request
            .headers()
            .get(CSRF_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::trim);
        if csrf_cookie.is_none() || csrf_cookie != csrf_header {
            tracing::warn!(
                request_id = %request_id,
                path = %path,
                "Rejected request due to CSRF validation failure"
            );
            let mut response = (
                StatusCode::FORBIDDEN,
                Json(json!({
                    "error": "CSRF validation failed",
                    "request_id": request_id,
                })),
            )
                .into_response();
            set_request_id_header(response.headers_mut(), &request_id);
            return response;
        }
    }

    let mut response = next.run(request).await;
    set_request_id_header(response.headers_mut(), &request_id);

    if missing_csrf_cookie {
        append_set_cookie(
            response.headers_mut(),
            &build_cookie(
                CSRF_COOKIE_NAME,
                &generate_token(),
                false,
                Some(Duration::from_secs(86_400)),
                &state.inner.security.cookie_config,
            ),
        );
    }

    response
}

pub fn parse_cookie_header(headers: &HeaderMap) -> HashMap<String, String> {
    let mut cookies = HashMap::new();
    for value in headers.get_all(axum::http::header::COOKIE).iter() {
        if let Ok(raw) = value.to_str() {
            for pair in raw.split(';') {
                let mut parts = pair.trim().splitn(2, '=');
                let Some(name) = parts.next() else {
                    continue;
                };
                let Some(value) = parts.next() else {
                    continue;
                };
                cookies.insert(name.trim().to_string(), value.trim().to_string());
            }
        }
    }
    cookies
}

pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    parse_cookie_header(headers).remove(name)
}

pub fn build_cookie(
    name: &str,
    value: &str,
    http_only: bool,
    max_age: Option<Duration>,
    config: &CookieConfig,
) -> String {
    let mut cookie = format!("{name}={value}; Path=/; SameSite=Lax");
    if let Some(max_age) = max_age {
        cookie.push_str(&format!("; Max-Age={}", max_age.as_secs()));
    }
    if http_only {
        cookie.push_str("; HttpOnly");
    }
    if config.secure {
        cookie.push_str("; Secure");
    }
    if let Some(domain) = &config.domain {
        cookie.push_str(&format!("; Domain={domain}"));
    }
    cookie
}

pub fn build_expired_cookie(name: &str, config: &CookieConfig) -> String {
    let mut cookie =
        format!("{name}=; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; SameSite=Lax");
    if config.secure {
        cookie.push_str("; Secure");
    }
    cookie.push_str("; HttpOnly");
    if let Some(domain) = &config.domain {
        cookie.push_str(&format!("; Domain={domain}"));
    }
    cookie
}

pub fn append_set_cookie(headers: &mut HeaderMap, cookie: &str) {
    if let Ok(value) = HeaderValue::from_str(cookie) {
        headers.append(SET_COOKIE, value);
    }
}

fn build_request_context(headers: &HeaderMap) -> RequestAuditContext {
    let request_id = headers
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(generate_token);
    let client_ip = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let user_agent = headers
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    let origin = headers
        .get(ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    RequestAuditContext {
        request_id,
        client_ip,
        user_agent,
        origin,
    }
}

fn set_request_id_header(headers: &mut HeaderMap, request_id: &str) {
    if let Ok(value) = HeaderValue::from_str(request_id) {
        headers.insert(HeaderName::from_static(REQUEST_ID_HEADER), value);
    }
}

fn requires_csrf(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

fn is_orion_machine_request(path: &str, headers: &HeaderMap) -> bool {
    let has_bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .is_some_and(|value| value.starts_with("Bearer "));
    has_bearer
        && (path == "/api/orion/egress"
            || path.starts_with("/api/orion/")
            || path == "/api/llm/generate"
            || path.starts_with("/api/llm/"))
}

fn rate_limit_bucket(method: &Method, path: &str) -> Option<(&'static str, usize, Duration)> {
    match (method.as_str(), path) {
        ("POST", "/api/auth/webauthn/register/start")
        | ("POST", "/api/auth/webauthn/register/finish")
        | ("POST", "/api/auth/webauthn/login/start")
        | ("POST", "/api/auth/webauthn/login/finish")
        | ("GET", "/api/auth/oidc/callback") => Some(("auth", 20, Duration::from_secs(60))),
        ("GET", path) if path.starts_with("/api/auth/oidc/") => {
            Some(("oidc", 20, Duration::from_secs(60)))
        }
        ("POST", "/api/auth/refresh") => Some(("refresh", 30, Duration::from_secs(60))),
        ("POST", "/api/vault/unseal") => Some(("vault-unseal", 5, Duration::from_secs(300))),
        ("POST", "/api/admin/oidc/providers") => Some(("admin-oidc", 30, Duration::from_secs(60))),
        ("PUT", path) if path.starts_with("/api/admin/oidc/providers/") => {
            Some(("admin-oidc", 30, Duration::from_secs(60)))
        }
        ("DELETE", path) if path.starts_with("/api/admin/oidc/providers/") => {
            Some(("admin-oidc", 30, Duration::from_secs(60)))
        }
        ("POST", "/api/agents") => Some(("agents-create", 20, Duration::from_secs(60))),
        ("POST", "/api/orion/egress") => Some(("orion-egress", 120, Duration::from_secs(60))),
        ("POST", path) if path.starts_with("/api/agents/") && path.ends_with("/skills/checkin") => {
            Some(("skill-checkin", 30, Duration::from_secs(60)))
        }
        _ => None,
    }
}

fn expiry_deadline() -> Instant {
    Instant::now() + Duration::from_secs(DEFAULT_CHALLENGE_TTL_SECONDS)
}

fn generate_token() -> String {
    Uuid::new_v4().simple().to_string()
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_bool, parse_cookie_header, RateLimiter};
    use axum::http::{HeaderMap, HeaderValue};
    use std::time::Duration;

    #[test]
    fn cookie_parser_extracts_named_values() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            HeaderValue::from_static("foo=bar; sao_csrf=token-1"),
        );

        let cookies = parse_cookie_header(&headers);
        assert_eq!(cookies.get("foo").map(String::as_str), Some("bar"));
        assert_eq!(cookies.get("sao_csrf").map(String::as_str), Some("token-1"));
    }

    #[test]
    fn parse_bool_understands_common_variants() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("OFF"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
    }

    #[test]
    fn orion_machine_paths_are_csrf_scoped() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer token"),
        );
        assert!(super::is_orion_machine_request(
            "/api/orion/egress",
            &headers
        ));
        assert!(super::is_orion_machine_request(
            "/api/orion/birth",
            &headers
        ));
        assert!(super::is_orion_machine_request(
            "/api/llm/generate",
            &headers
        ));
        assert!(!super::is_orion_machine_request("/api/agents", &headers));
        assert!(!super::is_orion_machine_request(
            "/api/orion/egress",
            &HeaderMap::new()
        ));
        assert!(!super::is_orion_machine_request(
            "/api/llm/generate",
            &HeaderMap::new()
        ));
    }

    #[tokio::test]
    async fn rate_limiter_blocks_after_limit_until_window_expires() {
        let limiter = RateLimiter::default();
        assert!(limiter.allow("auth:1", 2, Duration::from_secs(60)).await);
        assert!(limiter.allow("auth:1", 2, Duration::from_secs(60)).await);
        assert!(!limiter.allow("auth:1", 2, Duration::from_secs(60)).await);
    }
}
