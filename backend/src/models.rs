use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct MediaItem {
    /// "image" | "video"
    pub kind: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct PostResponse {
    pub shortcode: String,
    /// "image" | "video" | "carousel"
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_avatar: Option<String>,
    pub items: Vec<MediaItem>,
}

#[derive(Debug, Serialize)]
pub struct ProfileResponse {
    pub user_id: String,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub biography: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_pic_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_pic_url_hd: Option<String>,
    pub is_private: bool,
    pub is_verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub followers: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub following: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub posts: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct StoryItem {
    pub id: String,
    /// "image" | "video"
    pub kind: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taken_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct StoryResponse {
    pub username: String,
    /// Whether a sessionid cookie is configured — stories need one.
    pub auth_configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub items: Vec<StoryItem>,
}

#[derive(Debug, Serialize)]
pub struct HighlightSummary {
    /// Numeric highlight id (without the `highlight:` prefix).
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_count: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct HighlightsResponse {
    pub username: String,
    /// Whether a sessionid cookie is configured — highlights need one.
    pub auth_configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub highlights: Vec<HighlightSummary>,
}

#[derive(Debug, Serialize)]
pub struct HighlightResponse {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub items: Vec<StoryItem>,
}

#[derive(Debug, Serialize)]
pub struct SessionStatus {
    /// Whether IG_SESSIONID is configured at all.
    pub configured: bool,
    /// Whether Instagram currently accepts the cookie.
    pub valid: bool,
}
