//! Error types for the QGIS server.

use thiserror::Error;

/// Errors that can occur in the server.
#[derive(Debug, Error)]
pub enum ServerError {
    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid FastCGI record.
    #[error("invalid FastCGI record: {0}")]
    InvalidRecord(&'static str),

    /// Unknown FastCGI record type.
    #[error("unknown FastCGI record type: {0}")]
    UnknownRecordType(u8),

    /// Invalid FastCGI name-value pair.
    #[error("invalid FastCGI name-value pair")]
    InvalidNameValuePair,

    /// Request body too large.
    #[error("request body too large: {0} bytes")]
    BodyTooLarge(usize),

    /// Invalid query parameter encoding.
    #[error("invalid query parameter encoding: {0}")]
    InvalidQueryEncoding(String),

    /// Missing required query parameter.
    #[error("missing required query parameter: {0}")]
    MissingParameter(&'static str),

    /// Invalid query parameter value.
    #[error("invalid query parameter value for {0}: {1}")]
    InvalidParameterValue(&'static str, String),

    /// Unsupported service.
    #[error("unsupported service: {0}")]
    UnsupportedService(String),

    /// Unsupported request type.
    #[error("unsupported request type: {0}")]
    UnsupportedRequestType(String),

    /// Internal server error.
    #[error("internal server error: {0}")]
    Internal(String),
}

/// Result type alias for the server.
pub type Result<T> = std::result::Result<T, ServerError>;
