use dt_mobility::MobilityError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SimError {
    #[error("simulation configuration error: {0}")]
    Config(String),

    #[error("{what} length {got} does not match agent count {expected}")]
    AgentCountMismatch {
        expected: usize,
        got:      usize,
        what:     &'static str,
    },

    #[error("mobility error for agent: {0}")]
    Mobility(#[from] MobilityError),
}

pub type SimResult<T> = Result<T, SimError>;
