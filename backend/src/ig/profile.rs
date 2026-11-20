use serde_json::Value;

use crate::error::{ApiError, ApiResult};
use crate::models::ProfileResponse;

use super::util::{as_navigation, extract_json_object, find_json_string, meta_content, value_to_string};

/// Fetch a profile. Strategy order: HTML relay object (works unauthenticated,
/// richest data) -> private JSON API -> og: meta tags.
pub async fn fetch_profile(http: &wreq::Client, username: &str) -> ApiResult<ProfileResponse> {
    if let Ok(p) = fetch_profile_relay(http, username).await {
        return Ok(p);
    }
    if let Ok(p) = fetch_profile_api(http, username).await {
        return Ok(p);
    }
    fetch_profile_og(http, username).await
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

    let obj_str = extract_json_object(&html, "\"xig_user_by_username\":").ok_or_else(|| {
        ApiError::Upstream("profile page served no user data (challenge or rate limit)".into())
    })?;
    let obj: Value = serde_json::from_str(obj_str)
        .map_err(|_| ApiError::Upstream("could not parse profile data".into()))?;

    if obj["pk"].is_null() {
        return Err(ApiError::Upstream("profile data missing user id".into()));
    }

    // The HD picture key is only present for some accounts, elsewhere in the page.
    let hd = find_json_string(&html, "profile_pic_url_hd");

    Ok(ProfileResponse {
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
        profile_pic_url_hd: hd.or_else(|| obj["profile_pic_url_hd"].as_str().map(str::to_string)),
        is_private: obj["is_private"].as_bool().unwrap_or(false),
        is_verified: obj["is_verified"].as_bool().unwrap_or(false),
        followers: obj["follower_count"]
            .as_u64()
            .or_else(|| obj["edge_followed_by"]["count"].as_u64()),
        following: obj["following_count"]
            .as_u64()
            .or_else(|| obj["edge_follow"]["count"].as_u64()),
        posts: obj["media_count"].as_u64(),
    })
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
        return Err(ApiError::Upstream("unexpected Instagram profile response".into()));
    }

    Ok(profile_from_value(user, username))
}

fn profile_from_value(user: &Value, username: &str) -> ProfileResponse {
    ProfileResponse {
        user_id: value_to_string(&user["id"]).unwrap_or_default(),
        username: user["username"].as_str().unwrap_or(username).to_string(),
        full_name: user["full_name"].as_str().map(str::to_string).filter(|s| !s.is_empty()),
        biography: user["biography"].as_str().map(str::to_string).filter(|s| !s.is_empty()),
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
