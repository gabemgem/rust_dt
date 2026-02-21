//! Error types for dt-output.

use thiserror::Error;

/// Errors that can occur when writing simulation output.
#[derive(Debug, Error)]
pub enum OutputError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("CSV write error: {0}")]
    Csv(#[from] csv::Error),

    #[cfg(feature = "sqlite")]
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[cfg(feature = "parquet")]
    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    #[cfg(feature = "parquet")]
    #[error("Parquet error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),
}

/// Alias for `Result<T, OutputError>`.
pub type OutputResult<T> = Result<T, OutputError>;
