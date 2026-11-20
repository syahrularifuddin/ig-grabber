use std::{
    env,
    sync::{Arc, RwLock},
    time::Duration,
};

use wreq::header::{HeaderMap, HeaderValue};
use wreq::Client;
use wreq_util::Emulation;

/// Instagram's public web app id — accepted by the unauthenticated endpoints.
pub const APP_ID: &str = "936619743392459";

/// Instagram Android app user-agent — accepted by media-info endpoints that
/// reject the browser profile.
pub const APP_UA: &str = "Instagram 263.0.0.20.104 Android (33/13; 420dpi; 1080x2400; samsung; SM-G991B; o1s; exynos2100; en_US; 411253487)";

/// The sessionid-dependent clients. Hot-swappable: rebuilt whenever the
/// session changes (cookie roll from Instagram or in-app renewal) so callers
/// always read the current one through `state.session`.
#[derive(Clone)]
pub struct SessionState {
    pub session_id: Option<String>,
    /// Browser-emulating client with cookie — tray, graphql, web endpoints.
    pub authed_http: Client,
    /// App-emulating client with cookie — media-info endpoints. Uses OkHttp
    /// TLS emulation to match the Android app UA (Safari TLS + app UA gets
    /// 302-redirect-loop soft-blocked by Instagram).
    pub app_http: Client,
}

impl SessionState {
    fn new(session_id: Option<String>) -> Self {
        Self {
            authed_http: build_client(None, session_id.as_deref()),
            app_http: build_client_with_emulation(
                Emulation::OkHttp3_13,
                Some(APP_UA),
                session_id.as_deref(),
            ),
            session_id,
        }
    }

    /// App-emulating client for an ad-hoc sessionid (per-request override).
    pub fn app_client(session_id: &str) -> Client {
        build_client_with_emulation(Emulation::OkHttp3_13, Some(APP_UA), Some(session_id))
    }
}

#[derive(Clone)]
pub struct AppState {
    /// Anonymous client (browser-emulating).
    pub http: Client,
    /// Client for proxying CDN downloads (no total timeout — videos are big).
    pub dl_http: Client,
    pub session: Arc<RwLock<SessionState>>,
    /// Scraped `fb_dtsg` token for the graphql API, cached across requests.
    pub fb_dtsg: Arc<tokio::sync::Mutex<Option<String>>>,
    pub port: u16,
}

impl AppState {
    pub fn new() -> Self {
        load_dotenv();

        let session_id = env::var("IG_SESSIONID")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let port = env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);

        Self {
            http: build_client(None, None),
            dl_http: build_client(None, None),
            session: Arc::new(RwLock::new(SessionState::new(session_id))),
            fb_dtsg: Arc::new(tokio::sync::Mutex::new(None)),
            port,
        }
    }

    /// Swap in an already-validated session and persist it to `.env`.
    pub fn set_session(&self, sid: &str) {
        *self.session.write().expect("session lock poisoned") =
            SessionState::new(Some(sid.to_string()));
        persist_session(sid);
        tracing::info!("session updated and persisted to .env");
    }

    /// If Instagram refreshed the sessionid cookie on this response, adopt the
    /// new value (the old copy would otherwise break while the browser stays
    /// logged in). Call before consuming the response body.
    pub fn capture_session_roll(&self, headers: &wreq::header::HeaderMap) {
        let Some(new_sid) = extract_session_roll(headers) else {
            return;
        };
        let current = self
            .session
            .read()
            .expect("session lock poisoned")
            .session_id
            .clone();
        if current.as_deref() == Some(new_sid.as_str()) {
            return;
        }
        tracing::info!("Instagram rolled the sessionid cookie — adopting the new value");
        self.set_session(&new_sid);
    }
}

fn extract_session_roll(headers: &HeaderMap) -> Option<String> {
    for v in headers.get_all("set-cookie") {
        let Ok(s) = v.to_str() else { continue };
        let Some(rest) = s.strip_prefix("sessionid=") else {
            continue;
        };
        let val = rest.split(';').next()?.trim();
        if !val.is_empty() {
            return Some(val.to_string());
        }
    }
    None
}

/// Load KEY=VALUE pairs from a local `.env` (if present) without overriding
/// real environment variables. Lets `cargo run` pick up IG_SESSIONID/PORT
/// without exporting anything.
fn load_dotenv() {
    let Ok(content) = std::fs::read_to_string(".env") else {
        return;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim().trim_matches('"').trim_matches('\'');
            if env::var_os(k).is_none() && !v.is_empty() {
                env::set_var(k, v);
            }
        }
    }
}

/// Rewrite the IG_SESSIONID line in the local `.env` so a renewed session
/// survives restarts.
fn persist_session(sid: &str) {
    let path = std::path::Path::new(".env");
    let mut lines: Vec<String> = Vec::new();
    match std::fs::read_to_string(path) {
        Ok(content) => {
            for l in content.lines() {
                if l.trim_start().starts_with("IG_SESSIONID=") {
                    continue;
                }
                lines.push(l.to_string());
            }
        }
        Err(_) => {
            lines.push("# Local config — not committed (see .gitignore)".into());
            lines.push("PORT=8080".into());
        }
    }
    lines.push(format!("IG_SESSIONID={sid}"));
    if let Err(e) = std::fs::write(path, lines.join("\n") + "\n") {
        tracing::warn!("could not persist session to .env: {e}");
    }
}

/// Build a client: `ua` overrides the emulated profile's user-agent
/// (None = browser profile), `session` adds the sessionid cookie.
pub fn build_client(ua: Option<&str>, session: Option<&str>) -> Client {
    build_client_with_emulation(Emulation::Safari26, ua, session)
}

/// Same, but with an explicit TLS emulation profile. The claimed UA and the
/// TLS fingerprint must agree or Instagram soft-blocks with a redirect loop.
pub fn build_client_with_emulation(
    emulation: impl wreq::IntoEmulation,
    ua: Option<&str>,
    session: Option<&str>,
) -> Client {
    let mut headers = HeaderMap::new();
    headers.insert("x-ig-app-id", HeaderValue::from_static(APP_ID));
    headers.insert(
        "x-requested-with",
        HeaderValue::from_static("XMLHttpRequest"),
    );
    // Logged-in web-app headers (mirrors gallery-dl) — required shape for the
    // api/v1 endpoints.
    headers.insert("accept", HeaderValue::from_static("*/*"));
    headers.insert("x-asbd-id", HeaderValue::from_static("129477"));
    headers.insert(
        "referer",
        HeaderValue::from_static("https://www.instagram.com/"),
    );
    headers.insert("sec-fetch-dest", HeaderValue::from_static("empty"));
    headers.insert("sec-fetch-mode", HeaderValue::from_static("cors"));
    headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
    if let Some(sid) = session {
        if let Ok(v) = HeaderValue::from_str(&format!("sessionid={sid}")) {
            headers.insert("cookie", v);
        }
    }

    let mut builder = Client::builder()
        .emulation(emulation)
        // Keep Instagram's cookies (csrftoken, ds_user_id, rur…) across
        // requests like a real browser — some api/v1 endpoints 302-loop
        // when only the bare sessionid is sent.
        .cookie_store(true)
        // Instagram's CDN 302-redirects to video.xx.fbcdn.net etc.
        .redirect(wreq::redirect::Policy::limited(10));
    if let Some(ua) = ua {
        builder = builder.user_agent(ua);
    }
    builder
        .default_headers(headers)
        .connect_timeout(Duration::from_secs(10))
        .build()
        .expect("failed to build HTTP client")
}
