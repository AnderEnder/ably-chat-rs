//! placeholder; replaced in Phase 1
#[derive(Debug, thiserror::Error)]
#[error("placeholder")]
pub struct Error;
pub struct ErrorInfo;
pub type Result<T> = std::result::Result<T, Error>;
