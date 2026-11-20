use serde_json::Value;

use crate::config::{build_client, AppState, SessionState};
use crate::error::{ApiError, ApiResult};
use crate::models::{SessionStatus, StoryItem, StoryResponse};

use super::profile;
use super::util::{
    as_navigation, best_candidate, extract_json_object, form_urlencode, value_to_string,
};

/// Check whether the configured sessionid cookie is currently accepted by
/// Instagram. The reels_tray feed answers 200 for a valid session.
pub async fn check_session(state: &AppState) -> SessionStatus {
    let s = state.session.read().expect("session lock poisoned").clone();
    let Some(_) = s.session_id else {
        return SessionStatus {
            configured: false,
            valid: false,
        };
    };

    let valid = probe_tray(&s.authed_http).await;
    SessionStatus {
        configured: true,
        valid,
    }
}

/// Probe the reels tray feed — 200 means the cookie is accepted.
pub async fn probe_tray(http: &wreq::Client) -> bool {
    matches!(
        http.get("https://www.instagram.com/api/v1/feed/reels_tray/")
            .send()
            .await,
        Ok(r) if r.status().is_success()
    )
}

/// Fetch active stories for a user. Requires a `sessionid` cookie (env or
/// per-request header). Uses the web app's persisted graphql query
/// (`PolarisStoriesV3ReelPageStandaloneQuery`), which returns stories for any
/// public account — followed or not. The per-user story feeds and the
/// tray-walk used earlier only cover followed accounts, so graphql is the
/// primary (and only general) path.
pub async fn fetch_stories(
    state: &AppState,
    username: &str,
    session_override: Option<&str>,
) -> ApiResult<StoryResponse> {
    let s = state.session.read().expect("session lock poisoned").clone();
    let has_session = session_override.is_some() || s.session_id.is_some();

    // Resolve the profile over the session when one exists — the app search
    // API is the only username -> id path (web_profile_info is retired,
    // profile pages don't embed the user object, and anonymous requests are
    // login-walled on datacenter IPs anyway).
    let prof = if has_session {
        let app = match session_override {
            Some(sid) => SessionState::app_client(sid),
            None => s.app_http.clone(),
        };
        match profile::fetch_profile_search(&app, username).await {
            Ok(p) => p,
            Err(_) => profile::fetch_profile(&s.authed_http, username).await?,
        }
    } else {
        profile::fetch_profile(&state.http, username).await?
    };

    if !has_session {
        return Ok(StoryResponse {
            username: prof.username,
            auth_configured: false,
            note: Some(
                "stories need a logged-in session — set IG_SESSIONID in backend/.env".into(),
            ),
            items: vec![],
        });
    }
    if prof.user_id.is_empty() {
        return Err(ApiError::Upstream(
            "could not resolve the user id for stories".into(),
        ));
    }

    let http = match session_override {
        Some(sid) => build_client(None, Some(sid)),
        None => s.authed_http.clone(),
    };

    let mut fb_dtsg = ensure_fb_dtsg(state, &http).await?;
    let mut items = stories_items(state, &http, &prof.user_id, &fb_dtsg).await?;

    // An absent items array usually means the cached fb_dtsg expired —
    // refresh it once and retry before reporting "no stories".
    if items.is_none() {
        *state.fb_dtsg.lock().await = None;
        fb_dtsg = ensure_fb_dtsg(state, &http).await?;
        items = stories_items(state, &http, &prof.user_id, &fb_dtsg).await?;
    }

    let Some(items) = items else {
        return Ok(StoryResponse {
            username: prof.username,
            auth_configured: true,
            note: Some(
                "no active stories for this account right now (stories of private accounts are only visible to followers)".into(),
            ),
            items: vec![],
        });
    };

    let mut out = Vec::new();
    for it in items.iter().take(50) {
        let media_type = it["media_type"].as_i64().unwrap_or(1);
        let image = best_candidate(&it["image_versions2"]["candidates"]);
        let video = best_candidate(&it["video_versions"]);
        let kind = if media_type == 2 { "video" } else { "image" };
        let chosen = if kind == "video" {
            video.or_else(|| image.clone())
        } else {
            image.clone()
        };
        let Some((url, width, height)) = chosen else {
            continue;
        };

        out.push(StoryItem {
            id: value_to_string(&it["pk"])
                .or_else(|| value_to_string(&it["id"]))
                .unwrap_or_default(),
            kind: kind.into(),
            url,
            thumbnail_url: image.map(|(u, _, _)| u),
            width,
            height,
            taken_at: it["taken_at"].as_i64(),
            expires_at: it["expiring_at"].as_i64(),
        });
    }

    Ok(StoryResponse {
        username: prof.username,
        auth_configured: true,
        note: None,
        items: out,
    })
}

/// The cached `fb_dtsg` token for the graphql query, scraped from the
/// homepage's `__eqmc` blob. Heavy (a ~1 MB page), so cached per process.
async fn ensure_fb_dtsg(state: &AppState, http: &wreq::Client) -> ApiResult<String> {
    let mut cached = state.fb_dtsg.lock().await;
    if let Some(dtsg) = cached.as_ref() {
        return Ok(dtsg.clone());
    }
    let dtsg = fetch_fb_dtsg(http).await?;
    *cached = Some(dtsg.clone());
    Ok(dtsg)
}

async fn fetch_fb_dtsg(http: &wreq::Client) -> ApiResult<String> {
    let resp = as_navigation(http.get("https://www.instagram.com/"))
        .send()
        .await
        .map_err(|_| ApiError::Upstream("homepage request failed".into()))?;
    let status = resp.status().as_u16();
    let html = resp
        .text()
        .await
        .map_err(|_| ApiError::Upstream("homepage request failed".into()))?;
    if !(200..300).contains(&status) {
        return Err(ApiError::Upstream(format!(
            "homepage request failed (HTTP {status})"
        )));
    }
    let Some(eqmc) = html
        .find("__eqmc")
        .map(|i| &html[i..(i + 4000).min(html.len())])
    else {
        return Err(ApiError::Upstream(
            "homepage did not contain the fb_dtsg blob".into(),
        ));
    };
    let obj = extract_json_object(eqmc, "")
        .ok_or_else(|| ApiError::Upstream("homepage did not contain the fb_dtsg blob".into()))?;
    let v: Value = serde_json::from_str(obj)
        .map_err(|_| ApiError::Upstream("could not parse the fb_dtsg blob".into()))?;
    v["f"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| ApiError::Upstream("fb_dtsg missing from homepage blob".into()))
}

/// POST the persisted stories query and return the `items` array of the
/// user's story reel — `None` when the response has no such array.
async fn stories_items(
    state: &AppState,
    http: &wreq::Client,
    user_id: &str,
    fb_dtsg: &str,
) -> ApiResult<Option<Vec<Value>>> {
    let variables = format!(
        r#"{{"reel_ids_arr":["{user_id}"],"__relay_internal__pv__PolarisCommunityNoteStoriesLabelEnabledrelayprovider":true}}"#
    );
    // `lsd`/`__spin_*` mirror instagrapi's defaults; Instagram accepts them.
    let form = format!(
        "av=0&__d=www&__user=0&__a=1&__req=k&__hs=0&dpr=2&__ccg=UNKNOWN&__rev=0&\
         __comet_req=7&lsd=6b2800R9u4biJOYjcdXFEI&__spin_r=1011444902&__spin_b=trunk&\
         __spin_t=1708019550&fb_api_caller_class=RelayModern&\
         fb_api_req_friendly_name=PolarisStoriesV3ReelPageStandaloneQuery&\
         server_timestamps=true&variables={}&doc_id=29184890191114309&fb_dtsg={}",
        form_urlencode(&variables),
        form_urlencode(fb_dtsg),
    );

    let resp = http
        .post("https://www.instagram.com/api/graphql")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "*/*")
        .header("origin", "https://www.instagram.com")
        .header("referer", "https://www.instagram.com/")
        .header(
            "x-fb-friendly-name",
            "PolarisStoriesV3ReelPageStandaloneQuery",
        )
        .header("x-asbd-id", "129477")
        .body(form)
        .send()
        .await
        .map_err(|_| ApiError::Upstream("graphql request failed".into()))?;
    let status = resp.status().as_u16();
    // Instagram may have rolled the sessionid — adopt it before consuming.
    state.capture_session_roll(resp.headers());
    if status == 401 || status == 403 {
        return Err(ApiError::Unauthorized(
            "Instagram rejected the session cookie — it may be expired or invalid".into(),
        ));
    }
    if !(200..300).contains(&status) {
        return Err(ApiError::Upstream(format!(
            "graphql stories request failed (HTTP {status})"
        )));
    }
    let body: Value = serde_json::from_str(
        &resp
            .text()
            .await
            .map_err(|_| ApiError::Upstream("non-JSON graphql stories response".into()))?,
    )
    .map_err(|_| ApiError::Upstream("non-JSON graphql stories response".into()))?;

    if let Some(msg) = body["errors"][0]["message"].as_str() {
        return Err(ApiError::Upstream(format!("graphql error: {msg}")));
    }

    Ok(
        body["data"]["xdt_api__v1__feed__reels_media"]["reels_media"][0]["items"]
            .as_array()
            .cloned(),
    )
}
