//! Spatial-subsystem error type.

use thiserror::Error;

use dt_core::NodeId;

/// Errors produced by `dt-spatial`.
#[derive(Debug, Error)]
pub enum SpatialError {
    #[error("no route from {from} to {to}")]
    NoRoute { from: NodeId, to: NodeId },

    #[error("node {0} not found in network")]
    NodeNotFound(NodeId),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[cfg(feature = "osm")]
    #[error("OSM parse error: {0}")]
    Osm(String),
}

pub type SpatialResult<T> = Result<T, SpatialError>;
