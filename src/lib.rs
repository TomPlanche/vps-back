pub mod config;
pub mod error;
pub mod github_stats;
pub mod middlewares;
pub mod response;
pub mod static_files;

// Re-export error types for convenience
pub use error::{ApiError, ApiResult};
pub use response::{ApiResponse, Metadata, data_response, data_response_with_metadata};
