use lodestone_core::FetchError;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CtError {
    #[error("fetch failed: {0}")]
    Fetch(#[from] FetchError),
    #[error("unexpected HTTP status {0}")]
    Status(u16),
    #[error("response body was truncated")]
    Truncated,
    #[error("malformed data: {0}")]
    Malformed(String),
}
