pub mod config;
pub mod project;
pub mod env;

use axum::http::StatusCode;

/// Log DB errors server-side instead of swallowing them into a bare 500.
pub fn db_err<E: std::fmt::Display>(e: E) -> StatusCode {
    tracing::warn!(error = %e, "database error");
    StatusCode::INTERNAL_SERVER_ERROR
}
