//! Framework error type.
//!
//! Sub-crates may define their own error enums and convert them into `DtError`
//! via `From` impls, or keep them separate and wrap `DtError` as one variant.
//! Both patterns are acceptable; prefer whichever keeps error sites clean.

use thiserror::Error;

use crate::{AgentId, NodeId};

/// The top-level error type for `dt-core` and a common base for sub-crates.
#[derive(Debug, Error)]
pub enum DtError {
    #[error("agent {0} not found")]
    AgentNotFound(AgentId),

    #[error("node {0} not found")]
    NodeNotFound(NodeId),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Shorthand result type for all `dt-*` crates.
pub type DtResult<T> = Result<T, DtError>;
