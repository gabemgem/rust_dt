use thiserror::Error;

#[derive(Debug, Error)]
pub enum BehaviorError {
    #[error("behavior configuration error: {0}")]
    Config(String),
}

pub type BehaviorResult<T> = Result<T, BehaviorError>;
