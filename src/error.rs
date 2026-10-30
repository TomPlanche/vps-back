//! Error handling for the API
//!
//! This module defines the error types used throughout the application.
//! It follows the pattern recommended in ERRORS.md for web service error handling,
//! using thiserror for the public API boundary and anyhow for internal errors.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;

/// API error type for the public HTTP boundary.
///
/// This enum represents all possible errors that can occur in the API,
/// with variants that map to specific HTTP status codes.
#[derive(Error, Debug)]
pub enum ApiError {
    /// Validation error - returned when input data is invalid
    #[error("validation failed: {0}")]
    ValidationFailed(String),

    /// Resource not found error
    #[error("resource not found: {0}")]
    NotFound(String),

    /// Internal server error - wraps anyhow errors from internal operations
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl ApiError {
    /// Map the error to an HTTP status code
    #[must_use]
    pub const fn status_code(&self) -> StatusCode {
        match self {
            Self::ValidationFailed(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Create a validation error
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::ValidationFailed(msg.into())
    }

    /// Create a not found error
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();

        // For internal errors, log the full error chain but only expose a generic message
        let error_message = match &self {
            Self::Internal(e) => {
                tracing::error!("Internal error: {:#}", e);
                "Internal server error".to_string()
            }
            // User-facing errors can show details
            _ => self.to_string(),
        };

        let body = Json(json!({
            "error": {
                "message": error_message
            }
        }));

        (status, body).into_response()
    }
}

/// Result type alias for API operations
pub type ApiResult<T> = Result<T, ApiError>;
