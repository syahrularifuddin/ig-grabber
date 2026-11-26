use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    Unauthorized(String),
    Upstream(String),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m),
            ApiError::Upstream(m) => (StatusCode::BAD_GATEWAY, m),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<wreq::Error> for ApiError {
    fn from(e: wreq::Error) -> Self {
        // Instagram bounces an expired session and soft-blocks with 302 loops.
        if e.is_redirect() {
            return ApiError::Upstream(
                "Instagram kept redirecting the request (redirect loop) — the session cookie has likely expired; paste a fresh sessionid and retry".into(),
            );
        }
        ApiError::Upstream(format!("request to Instagram failed: {e}"))
    }
}
