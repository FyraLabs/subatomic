use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("libsubatomic error: {0}")]
    Libsubatomic(#[from] libsubatomic::err::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Repository not found")]
    NotFound,
    #[error("Invalid JWT token")]
    Unauthorized,
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Conflict: {0}")]
    Conflict(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Libsubatomic(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Self::Io(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            Self::NotFound => (StatusCode::NOT_FOUND, "Repository not found".to_owned()),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "Invalid or missing token".to_owned()),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg),
            Self::Database(e) => {
                tracing::error!(error = %e, "Database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_owned())
            }
        };
        (status, axum::Json(serde_json::json!({ "error": message }))).into_response()
    }
}

pub type Result<T, E = ApiError> = std::result::Result<T, E>;
