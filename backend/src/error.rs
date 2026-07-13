use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

pub struct ApiError(pub StatusCode, pub String);

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, msg.into())
    }

    pub fn unauthorized() -> Self {
        Self(StatusCode::UNAUTHORIZED, "not signed in".into())
    }

    pub fn forbidden() -> Self {
        Self(
            StatusCode::FORBIDDEN,
            "you don't have access to that".into(),
        )
    }

    pub fn not_found() -> Self {
        Self(StatusCode::NOT_FOUND, "not found".into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        Self(StatusCode::CONFLICT, msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, msg.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        tracing::error!("database error: {e}");
        Self::internal("internal error")
    }
}

impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        tracing::error!("io error: {e}");
        Self::internal("internal error")
    }
}
