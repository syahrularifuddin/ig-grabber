use serde_json::Value;

use crate::config::AppState;
use crate::error::{ApiError, ApiResult};
use crate::models::{MediaItem, PostResponse};

use super::util::{
    best_candidate, extract_json_object, find_json_string, meta_content, shortcode_to_pk,
    valid_shortcode,
};

/// doc_id values for Instagram's PostPage GraphQL query (they rotate; try several).
const DOC_IDS: &[&str] = &["8845758582119845", "9510064595728286"];
const LSD: &str = "AVqbxe3J_YA";

/// Extract a post/reel by shortcode. Strategy order:
/// embed page with session cookie (works for feed posts) -> session media-info
/// API -> anonymous GraphQL -> anonymous embed -> og: meta tags.
pub async fn fetch_post(state: &AppState, shortcode: &str) -> ApiResult<PostResponse> {
    if !valid_shortcode(shortcode) {
        return Err(ApiError::BadRequest("invalid post shortcode".into()));
    }

    let s = state.session.read().expect("session lock poisoned").clone();
    // Embed pages serve full media data to logged-in sessions (feed posts
    // included) — the most reliable post path when a session exists.
    if s.session_id.is_some() {
        if let Some(post) = try_embed(&s.authed_http, shortcode).await {
            return Ok(post);
        }
    }
    // The media-info endpoint soft-blocks (302 redirect loop) the Android-app
    // profile but accepts the browser profile with the same cookie, so prefer
    // authed_http; fall back to app_http if Instagram changes its mind.
    let clients: [Option<&wreq::Client>; 2] = [
        s.session_id.as_ref().map(|_| &s.authed_http),
        s.session_id.as_ref().map(|_| &s.app_http),
    ];
    for client in clients.into_iter().flatten() {
        match try_media_info(state, client, shortcode).await {
            Ok(post) => return Ok(post),
            Err(err) => tracing::debug!("media-info strategy failed: {err:?}"),
        }
    }
    if let Some(media) = try_graphql(&state.http, shortcode).await {
        if let Ok(post) = media_to_post(shortcode, &media) {
            return Ok(post);
        }
    }
    if let Some(post) = try_embed(&state.http, shortcode).await {
        return Ok(post);
    }
    if let Some(post) = try_og(&state.http, shortcode).await {
        return Ok(post);
    }

    Err(ApiError::Upstream(
        "could not extract this post — Instagram requires a logged-in session (set IG_SESSIONID) or the post is private/deleted"
            .into(),
    ))
}

/// The app's own media-info API — needs a valid sessionid cookie but returns
/// everything: video versions, thumbnails, carousels, captions.
async fn try_media_info(
    state: &AppState,
    http: &wreq::Client,
    shortcode: &str,
) -> ApiResult<PostResponse> {
    let pk = shortcode_to_pk(shortcode)
        .ok_or_else(|| ApiError::BadRequest("invalid post shortcode".into()))?;
    // i.instagram.com only: the www host redirect-loops on this endpoint.
    let url = format!("https://i.instagram.com/api/v1/media/{pk}/info/");

    let resp = http.get(&url).send().await?;
    let status = resp.status().as_u16();
    // Instagram may have rolled the sessionid — adopt it before consuming.
    state.capture_session_roll(resp.headers());
    let text = resp.text().await?;

    if status == 302 || (401..=403).contains(&status) {
        return Err(ApiError::Unauthorized(
            "Instagram rejected the session cookie — it may be expired or invalid".into(),
        ));
    }
    if !(200..300).contains(&status) {
        return Err(ApiError::Upstream(format!(
            "media info failed (HTTP {status})"
        )));
    }
    let v: Value = serde_json::from_str(&text)
        .map_err(|_| ApiError::Upstream("non-JSON media info response".into()))?;

    let item = &v["items"][0];
    if item.is_null() {
        return Err(ApiError::Upstream(
            "media info response had no items".into(),
        ));
    }

    let mut items = Vec::new();
    if let Some(children) = item["carousel_media"].as_array() {
        for child in children {
            if let Some(mi) = media_item_from_api(child) {
                items.push(mi);
            }
        }
    }
    if items.is_empty() {
        if let Some(mi) = media_item_from_api(item) {
            items.push(mi);
        }
    }
    if items.is_empty() {
        return Err(ApiError::Upstream(
            "media info contained no downloadable media".into(),
        ));
    }

    Ok(PostResponse {
        shortcode: shortcode.to_string(),
        kind: kind_of(&items),
        caption: item["caption"]["text"].as_str().map(str::to_string),
        author_username: item["user"]["username"].as_str().map(str::to_string),
        author_avatar: item["user"]["profile_pic_url"].as_str().map(str::to_string),
        items,
    })
}

/// Map an app-API media node (image_versions2 / video_versions) into our MediaItem.
fn media_item_from_api(node: &Value) -> Option<MediaItem> {
    let media_type = node["media_type"].as_i64().unwrap_or(1);
    let image = best_candidate(&node["image_versions2"]["candidates"]);
    let video = best_candidate(&node["video_versions"]);

    let (kind, url, thumbnail_url, width, height) = if media_type == 2 {
        let (u, w, h) = video.or(image.clone())?;
        ("video", u, image.map(|(tu, _, _)| tu), w, h)
    } else {
        let (u, w, h) = image?;
        ("image", u, None, w, h)
    };

    Some(MediaItem {
        kind: kind.into(),
        url,
        thumbnail_url,
        width,
        height,
    })
}

async fn try_graphql(http: &wreq::Client, shortcode: &str) -> Option<Value> {
    let variables = format!(r#"{{"shortcode":"{shortcode}"}}"#);
    for &doc_id in DOC_IDS {
        let params = [
            ("variables", variables.as_str()),
            ("doc_id", doc_id),
            ("lsd", LSD),
        ];
        let resp = http
            .post("https://www.instagram.com/api/graphql")
            .header("x-fb-lsd", LSD)
            .header(
                "referer",
                format!("https://www.instagram.com/p/{shortcode}/"),
            )
            .form(&params)
            .send()
            .await
            .ok()?;
        let text = resp.text().await.ok()?;
        let v: Value = serde_json::from_str(&text).ok()?;
        let media = &v["data"]["xdt_shortcode_media"];
        if !media.is_null() {
            return Some(media.clone());
        }
    }
    None
}

fn media_to_post(shortcode: &str, media: &Value) -> ApiResult<PostResponse> {
    let mut items = Vec::new();
    if let Some(edges) = media["edge_sidecar_to_children"]["edges"].as_array() {
        for edge in edges {
            if let Some(item) = media_item_from_raw(&edge["node"]) {
                items.push(item);
            }
        }
    }
    if items.is_empty() {
        if let Some(item) = media_item_from_raw(media) {
            items.push(item);
        }
    }
    if items.is_empty() {
        return Err(ApiError::Upstream("post has no downloadable media".into()));
    }

    Ok(PostResponse {
        shortcode: shortcode.to_string(),
        kind: kind_of(&items),
        caption: media["edge_media_to_caption"]["edges"][0]["node"]["text"]
            .as_str()
            .map(str::to_string),
        author_username: media["owner"]["username"].as_str().map(str::to_string),
        author_avatar: media["owner"]["profile_pic_url"]
            .as_str()
            .map(str::to_string),
        items,
    })
}

/// Map a raw media node (GraphQL child or embed JSON) into our MediaItem.
fn media_item_from_raw(node: &Value) -> Option<MediaItem> {
    let video_url = node["video_url"].as_str().map(str::to_string);
    let image_url = node["display_url"]
        .as_str()
        .or_else(|| node["image_versions2"]["candidates"][0]["url"].as_str())
        .map(str::to_string);
    let is_video = node["is_video"].as_bool().unwrap_or(false) || video_url.is_some();

    let (kind, url, thumbnail_url) = if is_video {
        (
            "video",
            video_url?,
            image_url.or_else(|| node["thumbnail_src"].as_str().map(str::to_string)),
        )
    } else {
        ("image", image_url?, None)
    };

    Some(MediaItem {
        kind: kind.into(),
        url,
        thumbnail_url,
        width: node["dimensions"]["width"].as_u64().map(|v| v as u32),
        height: node["dimensions"]["height"].as_u64().map(|v| v as u32),
    })
}

fn kind_of(items: &[MediaItem]) -> String {
    if items.len() > 1 {
        "carousel".into()
    } else {
        items[0].kind.clone()
    }
}

async fn try_embed(http: &wreq::Client, shortcode: &str) -> Option<PostResponse> {
    let urls = [
        format!("https://www.instagram.com/p/{shortcode}/embed/captioned/"),
        format!("https://www.instagram.com/reel/{shortcode}/embed/captioned/"),
    ];
    for url in urls {
        let Ok(resp) = http
            .get(&url)
            .header("referer", "https://www.instagram.com/")
            .send()
            .await
        else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(html) = resp.text().await else {
            continue;
        };
        if let Some(post) = parse_embed(&html, shortcode) {
            return Some(post);
        }
    }
    None
}

fn parse_embed(html: &str, shortcode: &str) -> Option<PostResponse> {
    // Preferred: the full JSON payload embedded in the page.
    if let Some(raw) = extract_json_object(html, "window.__additionalDataLoaded('extra',") {
        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            let item = &v["items"][0];
            if !item.is_null() {
                let mut items = Vec::new();
                if let Some(children) = item["carousel_media"].as_array() {
                    for child in children {
                        if let Some(mi) = media_item_from_raw(child) {
                            items.push(mi);
                        }
                    }
                } else if let Some(mi) = media_item_from_raw(item) {
                    items.push(mi);
                }
                if !items.is_empty() {
                    return Some(PostResponse {
                        shortcode: shortcode.to_string(),
                        kind: kind_of(&items),
                        caption: item["caption"].as_str().map(str::to_string).or_else(|| {
                            item["edge_media_to_caption"]["edges"][0]["node"]["text"]
                                .as_str()
                                .map(str::to_string)
                        }),
                        author_username: item["user"]["username"].as_str().map(str::to_string),
                        author_avatar: item["user"]["profile_pic_url"].as_str().map(str::to_string),
                        items,
                    });
                }
            }
        }
    }

    // Fallback: scrape the escaped keys directly out of the HTML.
    let video_url = find_json_string(html, "video_url");
    let display_url = find_json_string(html, "display_url");
    let (kind, url, thumbnail_url) = match (video_url, display_url) {
        (Some(v), d) => ("video", v, d),
        (None, Some(i)) => ("image", i, None),
        (None, None) => return None,
    };
    Some(PostResponse {
        shortcode: shortcode.to_string(),
        kind: kind.into(),
        caption: None,
        author_username: find_json_string(html, "username"),
        author_avatar: None,
        items: vec![MediaItem {
            kind: kind.into(),
            url,
            thumbnail_url,
            width: None,
            height: None,
        }],
    })
}

/// Last resort: og: meta tags on the canonical post page.
async fn try_og(http: &wreq::Client, shortcode: &str) -> Option<PostResponse> {
    let resp =
        super::util::as_navigation(http.get(format!("https://www.instagram.com/p/{shortcode}/")))
            .send()
            .await
            .ok()?;
    let html = resp.text().await.ok()?;

    let video = meta_content(&html, "og:video");
    let image = meta_content(&html, "og:image");
    let (kind, url, thumbnail_url) = match (video, image.clone()) {
        (Some(v), _) => ("video", v, image),
        (None, Some(i)) => ("image", i, None),
        (None, None) => return None,
    };
    Some(PostResponse {
        shortcode: shortcode.to_string(),
        kind: kind.into(),
        caption: None,
        author_username: None,
        author_avatar: None,
        items: vec![MediaItem {
            kind: kind.into(),
            url,
            thumbnail_url,
            width: None,
            height: None,
        }],
    })
}
