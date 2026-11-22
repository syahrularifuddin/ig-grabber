use serde_json::Value;

use crate::error::{ApiError, ApiResult};
use crate::models::ProfileResponse;

use super::util::{
    as_navigation, extract_json_object, find_json_string, meta_content, value_to_string,
};

/// Fetch a profile. Strategy order: HTML relay object (works unauthenticated,
/// richest data when present) -> profile-root props id + og: meta tags ->
/// private JSON API -> og: meta tags only.
pub async fn fetch_profile(http: &wreq::Client, username: &str) -> ApiResult<ProfileResponse> {
    if let Ok(p) = fetch_profile_relay(http, username).await {
        return Ok(p);
    }
    if let Ok(p) = fetch_profile_api(http, username).await {
        return Ok(p);
    }
    fetch_profile_og(http, username).await
}

/// Resolve a profile through the app search API (`api/v1/users/search`).
/// Cheap when it works, but Instagram increasingly answers it with a soft
/// fail (`{"status":"fail"}` behind HTTP 200) — the profile page's
/// profile-root props id is the general fallback. `http` must be the
/// app-emulating session client (`SessionState::app_http`).
pub async fn fetch_profile_search(
    http: &wreq::Client,
    username: &str,
) -> ApiResult<ProfileResponse> {
    let url = format!("https://i.instagram.com/api/v1/users/search/?q={username}");
    let resp = http.get(&url).send().await?;
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        return Err(ApiError::Unauthorized(
            "Instagram rejected the session cookie — it may be expired or invalid".into(),
        ));
    }
    let v: Value = serde_json::from_str(
        &resp
            .text()
            .await
            .map_err(|_| ApiError::Upstream("non-JSON search response".into()))?,
    )
    .map_err(|_| ApiError::Upstream("non-JSON search response".into()))?;

    // The search is fuzzy — pick the entry whose username matches exactly.
    let want = username.to_ascii_lowercase();
    let user = v["users"]
        .as_array()
        .and_then(|arr| {
            arr.iter().find(|u| {
                u["username"]
                    .as_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case(&want))
            })
        })
        .ok_or_else(|| ApiError::NotFound(format!("user '{username}' not found")))?;

    Ok(ProfileResponse {
        user_id: value_to_string(&user["pk"]).unwrap_or_default(),
        username: user["username"].as_str().unwrap_or(username).to_string(),
        full_name: user["full_name"]
            .as_str()
            .map(str::to_string)
            .filter(|s| !s.is_empty()),
        biography: None,
        profile_pic_url: user["profile_pic_url"].as_str().map(str::to_string),
        profile_pic_url_hd: None,
        is_private: user["is_private"].as_bool().unwrap_or(false),
        is_verified: user["is_verified"].as_bool().unwrap_or(false),
        followers: None,
        following: None,
        posts: None,
    })
}

/// Parse the `xig_user_by_username` relay object embedded in the profile page.
async fn fetch_profile_relay(http: &wreq::Client, username: &str) -> ApiResult<ProfileResponse> {
    let url = format!("https://www.instagram.com/{username}/");
    let resp = as_navigation(http.get(&url)).send().await?;
    let status = resp.status().as_u16();
    if status == 404 {
        return Err(ApiError::NotFound(format!("user '{username}' not found")));
    }
    let html = resp.text().await?;

    // The HD picture key is only present for some accounts, elsewhere in the page.
    let hd = find_json_string(&html, "profile_pic_url_hd");

    // Rich path: the page embeds the full target user object.
    if let Some(obj_str) = extract_json_object(&html, "\"xig_user_by_username\":") {
        if let Ok(obj) = serde_json::from_str::<Value>(obj_str) {
            if !obj["pk"].is_null() {
                return Ok(ProfileResponse {
                    user_id: value_to_string(&obj["pk"]).unwrap_or_default(),
                    username: obj["username"].as_str().unwrap_or(username).to_string(),
                    full_name: obj["full_name"]
                        .as_str()
                        .map(str::to_string)
                        .filter(|s| !s.is_empty()),
                    biography: obj["biography"]
                        .as_str()
                        .map(str::to_string)
                        .filter(|s| !s.is_empty()),
                    profile_pic_url: obj["profile_pic_url"].as_str().map(str::to_string),
                    profile_pic_url_hd: hd
                        .or_else(|| obj["profile_pic_url_hd"].as_str().map(str::to_string)),
                    is_private: obj["is_private"].as_bool().unwrap_or(false),
                    is_verified: obj["is_verified"].as_bool().unwrap_or(false),
                    followers: obj["follower_count"]
                        .as_u64()
                        .or_else(|| obj["edge_followed_by"]["count"].as_u64()),
                    following: obj["following_count"]
                        .as_u64()
                        .or_else(|| obj["edge_follow"]["count"].as_u64()),
                    posts: obj["media_count"].as_u64(),
                });
            }
        }
    }

    // The page no longer embeds the user object — Instagram fetches it
    // client-side over graphql. What's left in the SSR shell is the target's
    // pk in the profile root's props, plus the og:image meta tags.
    let user_id = extract_props_user_id(&html).unwrap_or_default();
    let pic = meta_content(&html, "og:image");
    if user_id.is_empty() && pic.is_none() {
        return Err(ApiError::Upstream(
            "profile page served no user data (challenge or rate limit)".into(),
        ));
    }

    Ok(ProfileResponse {
        user_id,
        username: username.to_string(),
        full_name: None,
        biography: None,
        profile_pic_url: pic.clone(),
        profile_pic_url_hd: hd.or(pic),
        is_private: false,
        is_verified: false,
        followers: None,
        following: None,
        posts: None,
    })
}

/// Extract the profile owner's pk from the SSR page's profile-root props —
/// `"props":{"id":"<pk>"` directly preceding a `profilePage` page_logging
/// entry (validated against known pks, e.g. @instagram -> 25025320).
fn extract_props_user_id(html: &str) -> Option<String> {
    let marker = "\"page_logging\":{\"name\":\"profilePage\"";
    let mut from = 0;
    while let Some(pl) = html[from..].find(marker) {
        let abs = from + pl;
        if let Some(pi) = html[..abs].rfind("\"props\":{\"id\":\"") {
            let rest = &html[pi + 15..];
            if let Some(e) = rest.find('"') {
                let id = &rest[..e];
                if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
                    return Some(id.to_string());
                }
            }
        }
        from = abs + marker.len();
    }
    None
}

async fn fetch_profile_api(http: &wreq::Client, username: &str) -> ApiResult<ProfileResponse> {
    let url = format!("https://i.instagram.com/api/v1/users/web_profile_info/?username={username}");
    let resp = http.get(&url).send().await?;
    let status = resp.status().as_u16();

    if status == 404 {
        return Err(ApiError::NotFound(format!("user '{username}' not found")));
    }
    let text = resp.text().await?;

    let v: Value = serde_json::from_str(&text).map_err(|_| {
        ApiError::Upstream(
            "Instagram answered with a non-JSON response (blocked or rate-limited); try again soon"
                .into(),
        )
    })?;

    if status == 429 {
        return Err(ApiError::Upstream(
            "rate-limited by Instagram; try again later".into(),
        ));
    }

    let user = &v["data"]["user"];
    if user.is_null() {
        return Err(ApiError::Upstream(
            "unexpected Instagram profile response".into(),
        ));
    }

    Ok(profile_from_value(user, username))
}

fn profile_from_value(user: &Value, username: &str) -> ProfileResponse {
    ProfileResponse {
        user_id: value_to_string(&user["id"]).unwrap_or_default(),
        username: user["username"].as_str().unwrap_or(username).to_string(),
        full_name: user["full_name"]
            .as_str()
            .map(str::to_string)
            .filter(|s| !s.is_empty()),
        biography: user["biography"]
            .as_str()
            .map(str::to_string)
            .filter(|s| !s.is_empty()),
        profile_pic_url: user["profile_pic_url"].as_str().map(str::to_string),
        profile_pic_url_hd: user["profile_pic_url_hd"].as_str().map(str::to_string),
        is_private: user["is_private"].as_bool().unwrap_or(false),
        is_verified: user["is_verified"].as_bool().unwrap_or(false),
        followers: user["edge_followed_by"]["count"].as_u64(),
        following: user["edge_follow"]["count"].as_u64(),
        posts: user["edge_owner_to_timeline_media"]["count"].as_u64(),
    }
}

/// Last resort: og:image on the profile page.
async fn fetch_profile_og(http: &wreq::Client, username: &str) -> ApiResult<ProfileResponse> {
    let url = format!("https://www.instagram.com/{username}/");
    let resp = as_navigation(http.get(&url)).send().await?;
    let html = resp.text().await?;

    let pic = meta_content(&html, "og:image");
    if html.contains("Page Not Found") || pic.is_none() {
        return Err(ApiError::NotFound(format!(
            "profile '{username}' not found or unreachable"
        )));
    }

    Ok(ProfileResponse {
        user_id: String::new(),
        username: username.to_string(),
        full_name: None,
        biography: None,
        profile_pic_url: pic.clone(),
        profile_pic_url_hd: pic,
        is_private: false,
        is_verified: false,
        followers: None,
        following: None,
        posts: None,
    })
}
