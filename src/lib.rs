pub mod brew;
pub mod config;
pub mod db;
pub mod entities;
pub mod error;
pub mod github_stats;
pub mod middlewares;
pub mod pagination;
pub mod response;
pub mod source;
pub mod static_files;
pub mod stats;
pub mod status;
pub mod sticker;

// Re-export error types for convenience
pub use error::{ApiError, ApiResult};
pub use pagination::PaginationParams;
pub use response::{ApiResponse, Metadata, data_response, data_response_with_metadata};
