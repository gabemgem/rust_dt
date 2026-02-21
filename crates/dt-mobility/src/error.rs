use dt_core::AgentId;
use dt_spatial::SpatialError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MobilityError {
    #[error("agent {0:?} is already in transit")]
    AlreadyInTransit(AgentId),

    #[error("agent {0:?} has not been placed on the network")]
    NotPlaced(AgentId),

    #[error("routing failed: {0}")]
    Routing(#[from] SpatialError),
}

pub type MobilityResult<T> = Result<T, MobilityError>;
