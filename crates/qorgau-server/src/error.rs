use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    PayloadTooLarge(String),
    #[error("{0}")]
    TooManyRequests(String),
    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn from_db_unique(err: sqlx::Error, conflict_msg: &str) -> Self {
        if let sqlx::Error::Database(ref db_err) = err {
            if db_err.code().as_deref() == Some("23505") {
                return AppError::Conflict(conflict_msg.to_string());
            }
        }
        AppError::Db(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, detail) = match &self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.clone()),
            AppError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            AppError::Forbidden(m) => (StatusCode::FORBIDDEN, m.clone()),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            AppError::Conflict(m) => (StatusCode::CONFLICT, m.clone()),
            AppError::PayloadTooLarge(m) => (StatusCode::PAYLOAD_TOO_LARGE, m.clone()),
            AppError::TooManyRequests(m) => (StatusCode::TOO_MANY_REQUESTS, m.clone()),
            AppError::Db(e) => {
                tracing::error!(error = %e, "Database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal database error".to_string(),
                )
            }
            AppError::Io(e) => {
                tracing::error!(error = %e, "IO error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal storage error".to_string(),
                )
            }
            AppError::Internal(m) => {
                tracing::error!(error = %m, "Internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".to_string(),
                )
            }
        };

        (status, Json(json!({ "detail": detail }))).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
