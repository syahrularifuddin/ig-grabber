use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::config::AppState;
use crate::error::{ApiError, ApiResult};
use crate::ig::util::{sanitize_username, valid_shortcode};
use crate::ig::{post, profile, story};
use crate::models::{PostResponse, ProfileResponse, SessionStatus, StoryResponse};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(index))
        .route("/api/profile/{username}", get(get_profile))
        .route("/api/post/{shortcode}", get(get_post))
        .route("/api/stories/{username}", get(get_stories))
        .route("/api/session", get(get_session).post(update_session))
        .route("/download", get(download))
}

async fn index() -> impl IntoResponse {
    Json(json!({
        "service": "ig-backend",
        "endpoints": [
            "GET /api/profile/{username}",
            "GET /api/post/{shortcode}",
            "GET /api/stories/{username}",
            "GET /download?url=<cdn-url>&filename=<name>",
            "GET /health"
        ]
    }))
}

async fn get_profile(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> ApiResult<Json<ProfileResponse>> {
    let username = sanitize_username(&username)
        .ok_or_else(|| ApiError::BadRequest("invalid username".into()))?;
    tracing::info!(username, "profile request");
    // Prefer the session-backed search when configured — the profile page
    // no longer embeds the user object and web_profile_info is retired.
    let s = state.session.read().expect("session lock poisoned").clone();
    let prof = match s.session_id.as_deref() {
        Some(_) => match profile::fetch_profile_search(&s.app_http, &username).await {
            Ok(p) => p,
            Err(_) => profile::fetch_profile(&s.authed_http, &username).await?,
        },
        None => profile::fetch_profile(&state.http, &username).await?,
    };
    Ok(Json(prof))
}

async fn get_post(
    State(state): State<AppState>,
    Path(shortcode): Path<String>,
) -> ApiResult<Json<PostResponse>> {
    if !valid_shortcode(&shortcode) {
        return Err(ApiError::BadRequest("invalid post shortcode".into()));
    }
    tracing::info!(shortcode, "post request");
    Ok(Json(post::fetch_post(&state, &shortcode).await?))
}

async fn get_session(State(state): State<AppState>) -> ApiResult<Json<SessionStatus>> {
    Ok(Json(story::check_session(&state).await))
}

#[derive(Deserialize)]
struct SessionBody {
    sessionid: String,
}

/// Renew the session cookie without restarting: validate, hot-swap the
/// clients, persist to `.env`.
async fn update_session(
    State(state): State<AppState>,
    Json(body): Json<SessionBody>,
) -> ApiResult<Json<SessionStatus>> {
    let sid = body.sessionid.trim();
    if sid.is_empty() || sid.len() > 200 || sid.chars().any(|c| c.is_control()) {
        return Err(ApiError::BadRequest("invalid sessionid".into()));
    }
    let test = crate::config::build_client(None, Some(sid));
    if !story::probe_tray(&test).await {
        return Err(ApiError::Unauthorized(
            "Instagram rejected this sessionid — copy a fresh one from a logged-in browser".into(),
        ));
    }
    state.set_session(sid);
    // Re-scrape the graphql token under the new session.
    *state.fb_dtsg.lock().await = None;
    tracing::info!("session renewed via API");
    Ok(Json(SessionStatus {
        configured: true,
        valid: true,
    }))
}

async fn get_stories(
    State(state): State<AppState>,
    Path(username): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Json<StoryResponse>> {
    let username = sanitize_username(&username)
        .ok_or_else(|| ApiError::BadRequest("invalid username".into()))?;
    let session = headers.get("x-session-id").and_then(|v| v.to_str().ok());
    tracing::info!(username, "stories request");
    Ok(Json(
        story::fetch_stories(&state, &username, session).await?,
    ))
}

#[derive(Deserialize)]
struct DownloadQuery {
    url: String,
    filename: Option<String>,
}

/// Stream an Instagram CDN URL back to the browser as a file attachment.
/// Doubles as a CORS-safe download proxy for the frontend.
async fn download(
    State(state): State<AppState>,
    Query(q): Query<DownloadQuery>,
) -> ApiResult<Response> {
    let url = url::Url::parse(&q.url).map_err(|_| ApiError::BadRequest("invalid URL".into()))?;
    let host = url
        .host_str()
        .ok_or_else(|| ApiError::BadRequest("URL has no host".into()))?
        .to_ascii_lowercase();

    // SSRF guard: only proxy Instagram CDN hosts.
    let allowed = host == "cdninstagram.com"
        || host.ends_with(".cdninstagram.com")
        || host == "fbcdn.net"
        || host.ends_with(".fbcdn.net");
    if !allowed {
        return Err(ApiError::BadRequest(
            "only Instagram CDN URLs may be downloaded".into(),
        ));
    }

    let resp = state.dl_http.get(url.as_str()).send().await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(ApiError::Upstream(format!(
            "CDN download failed (HTTP {})",
            status.as_u16()
        )));
    }

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let mut headers = HeaderMap::new();
    if let Ok(v) = HeaderValue::from_str(&content_type) {
        headers.insert("content-type", v);
    }
    let ext = ext_for(&content_type);
    let filename = sanitize_filename(q.filename.as_deref().unwrap_or(""), ext);
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        headers.insert("content-disposition", v);
    }

    let body = Body::from_stream(resp.bytes_stream());
    Ok((headers, body).into_response())
}

fn ext_for(ct: &str) -> &'static str {
    if ct.contains("mp4") {
        ".mp4"
    } else if ct.contains("jpg") || ct.contains("jpeg") {
        ".jpg"
    } else if ct.contains("png") {
        ".png"
    } else if ct.contains("webp") {
        ".webp"
    } else {
        ".bin"
    }
}

fn sanitize_filename(name: &str, fallback_ext: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        .collect();
    let trimmed = cleaned.trim_matches('.').to_string();
    let mut s: String = trimmed.chars().take(80).collect();
    if s.is_empty() {
        s = format!("download{fallback_ext}");
    } else if !s.contains('.') {
        s.push_str(fallback_ext);
    }
    s
}
