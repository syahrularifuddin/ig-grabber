use serde_json::Value;

use crate::config::{build_client, AppState, SessionState};
use crate::error::{ApiError, ApiResult};
use crate::models::{HighlightResponse, HighlightSummary, HighlightsResponse, StoryItem};

use super::profile;
use super::util::{best_candidate, form_urlencode, value_to_string};

/// Fetch a user's story highlight albums. Highlights are session-only, like
/// stories: the tray endpoint needs a logged-in `sessionid`.
pub async fn fetch_highlights(
    state: &AppState,
    username: &str,
    session_override: Option<&str>,
) -> ApiResult<HighlightsResponse> {
    let s = state.session.read().expect("session lock poisoned").clone();
    let has_session = session_override.is_some() || s.session_id.is_some();

    // Resolve the user id over the session when one exists; anonymous lookup
    // is only used to validate/fill in the username.
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
        return Ok(HighlightsResponse {
            username: prof.username,
            auth_configured: false,
            note: Some(
                "highlights need a logged-in session — set IG_SESSIONID in backend/.env".into(),
            ),
            highlights: vec![],
        });
    }
    if prof.user_id.is_empty() {
        return Err(ApiError::Upstream(
            "could not resolve the user id for highlights — Instagram did not serve profile data, try again shortly".into(),
        ));
    }

    let http = match session_override {
        Some(sid) => build_client(None, Some(sid)),
        None => s.authed_http.clone(),
    };

    let url = format!(
        "https://i.instagram.com/api/v1/highlights/{}/highlights_tray/",
        prof.user_id
    );
    let resp = http
        .get(&url)
        .send()
        .await
        .map_err(|_| ApiError::Upstream("highlights request failed".into()))?;
    // Instagram may have rolled the sessionid — adopt it before consuming.
    state.capture_session_roll(resp.headers());
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        return Err(ApiError::Unauthorized(
            "Instagram rejected the session cookie — it may be expired or invalid".into(),
        ));
    }
    if !(200..300).contains(&status) {
        return Err(ApiError::Upstream(format!(
            "highlights request failed (HTTP {status})"
        )));
    }
    let body: Value = serde_json::from_str(
        &resp
            .text()
            .await
            .map_err(|_| ApiError::Upstream("non-JSON highlights response".into()))?,
    )
    .map_err(|_| ApiError::Upstream("non-JSON highlights response".into()))?;

    if let Some(msg) = body["message"].as_str() {
        if body["status"].as_str() == Some("fail") {
            return Err(ApiError::Upstream(format!("Instagram error: {msg}")));
        }
    }

    let highlights = body["tray"]
        .as_array()
        .map(|tray| tray.iter().filter_map(summary_from_tray).collect())
        .unwrap_or_default();

    Ok(HighlightsResponse {
        username: prof.username,
        auth_configured: true,
        note: None,
        highlights,
    })
}

/// Fetch the media items of one highlight album by its numeric id.
pub async fn fetch_highlight(
    state: &AppState,
    highlight_id: &str,
    session_override: Option<&str>,
) -> ApiResult<HighlightResponse> {
    let s = state.session.read().expect("session lock poisoned").clone();
    let http = match session_override {
        Some(sid) => build_client(None, Some(sid)),
        None if s.session_id.is_some() => s.authed_http.clone(),
        None => {
            return Err(ApiError::Unauthorized(
                "highlights need a logged-in session — set IG_SESSIONID in backend/.env".into(),
            ))
        }
    };

    let reel_id = format!("highlight:{highlight_id}");
    let url = format!(
        "https://i.instagram.com/api/v1/feed/reels_media/?reel_ids={}",
        form_urlencode(&reel_id)
    );
    let resp = http
        .get(&url)
        .send()
        .await
        .map_err(|_| ApiError::Upstream("highlight request failed".into()))?;
    state.capture_session_roll(resp.headers());
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        return Err(ApiError::Unauthorized(
            "Instagram rejected the session cookie — it may be expired or invalid".into(),
        ));
    }
    if !(200..300).contains(&status) {
        return Err(ApiError::Upstream(format!(
            "highlight request failed (HTTP {status})"
        )));
    }
    let body: Value = serde_json::from_str(
        &resp
            .text()
            .await
            .map_err(|_| ApiError::Upstream("non-JSON highlight response".into()))?,
    )
    .map_err(|_| ApiError::Upstream("non-JSON highlight response".into()))?;

    // The app API answers with `reels` keyed by id, the web API with a
    // `reels_media` array — accept either.
    let reel = body["reels"]
        .get(reel_id.as_str())
        .or_else(|| {
            body["reels_media"].as_array().and_then(|arr| {
                arr.iter()
                    .find(|r| r["id"].as_str() == Some(reel_id.as_str()))
            })
        })
        .ok_or_else(|| {
            if let Some(msg) = body["message"].as_str() {
                ApiError::Upstream(format!("Instagram error: {msg}"))
            } else {
                ApiError::NotFound(
                    "highlight is unavailable — it may be empty, deleted, or belong to a private account"
                        .into(),
                )
            }
        })?;

    let items = reel["items"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(media_item)
                .take(200)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(HighlightResponse {
        id: highlight_id.to_string(),
        title: reel["title"]
            .as_str()
            .map(str::to_string)
            .filter(|s| !s.is_empty()),
        username: reel["user"]["username"].as_str().map(str::to_string),
        items,
    })
}

fn summary_from_tray(v: &Value) -> Option<HighlightSummary> {
    let id = value_to_string(&v["id"]).map(|s| {
        s.strip_prefix("highlight:")
            .map(str::to_string)
            .unwrap_or(s)
    })?;
    let cover = &v["cover_media"];
    Some(HighlightSummary {
        id,
        title: v["title"].as_str().unwrap_or_default().to_string(),
        cover_url: image_url(cover).or_else(|| image_url(&cover["media"])),
        item_count: v["media_count"]
            .as_u64()
            .or_else(|| v["items"].as_array().map(|a| a.len() as u64)),
    })
}

/// Pick a usable still from a media object (cropped cover, candidates, or
/// thumbnail).
fn image_url(media: &Value) -> Option<String> {
    if let Some(url) = media["cropped_image_version"]["url"].as_str() {
        return Some(url.to_string());
    }
    if let Some((url, _, _)) = best_candidate(&media["image_versions2"]["candidates"]) {
        return Some(url);
    }
    media["thumbnail_src"].as_str().map(str::to_string)
}

/// Map one highlight story item to the shared story media shape (see tests).
fn media_item(it: &Value) -> Option<StoryItem> {
    let media_type = it["media_type"].as_i64().unwrap_or(1);
    let image = best_candidate(&it["image_versions2"]["candidates"]);
    let video = best_candidate(&it["video_versions"]);
    let kind = if media_type == 2 { "video" } else { "image" };
    let chosen = if kind == "video" {
        video.or_else(|| image.clone())
    } else {
        image.clone()
    };
    let (url, width, height) = chosen?;

    Some(StoryItem {
        id: value_to_string(&it["pk"])
            .or_else(|| value_to_string(&it["id"]))
            .unwrap_or_default(),
        kind: kind.into(),
        url,
        thumbnail_url: image.map(|(u, _, _)| u),
        width,
        height,
        taken_at: it["taken_at"].as_i64(),
        expires_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_tray_entry() {
        let entry = json!({
            "id": "highlight:17895485201104054",
            "title": "Travel",
            "media_count": 3,
            "cover_media": {
                "cropped_image_version": {
                    "url": "https://cdn.example/cover.jpg",
                    "width": 640,
                    "height": 640
                }
            }
        });
        let h = summary_from_tray(&entry).expect("tray entry");
        assert_eq!(h.id, "17895485201104054");
        assert_eq!(h.title, "Travel");
        assert_eq!(h.item_count, Some(3));
        assert_eq!(
            h.cover_url.as_deref(),
            Some("https://cdn.example/cover.jpg")
        );
    }

    #[test]
    fn tray_cover_falls_back_to_best_candidate() {
        let entry = json!({
            "id": "highlight:1",
            "title": "",
            "cover_media": {
                "image_versions2": {
                    "candidates": [
                        {"url": "small.jpg", "width": 100, "height": 100},
                        {"url": "big.jpg", "width": 640, "height": 640}
                    ]
                }
            }
        });
        let h = summary_from_tray(&entry).expect("tray entry");
        assert_eq!(h.title, "");
        assert_eq!(h.cover_url.as_deref(), Some("big.jpg"));
    }

    #[test]
    fn maps_video_story_item() {
        let item = json!({
            "pk": 123,
            "media_type": 2,
            "taken_at": 1700000000,
            "image_versions2": {
                "candidates": [{"url": "poster.jpg", "width": 1080, "height": 1920}]
            },
            "video_versions": [
                {"url": "video.mp4", "width": 720, "height": 1280},
                {"url": "hd.mp4", "width": 1080, "height": 1920}
            ]
        });
        let m = media_item(&item).expect("story item");
        assert_eq!(m.id, "123");
        assert_eq!(m.kind, "video");
        assert_eq!(m.url, "hd.mp4");
        assert_eq!(m.thumbnail_url.as_deref(), Some("poster.jpg"));
        assert_eq!(m.width, Some(1080));
    }

    #[test]
    fn maps_image_story_item() {
        let item = json!({
            "pk": 456,
            "media_type": 1,
            "image_versions2": {
                "candidates": [{"url": "photo.jpg", "width": 1080, "height": 1350}]
            }
        });
        let m = media_item(&item).expect("story item");
        assert_eq!(m.kind, "image");
        assert_eq!(m.url, "photo.jpg");
        assert_eq!(m.height, Some(1350));
    }
}
