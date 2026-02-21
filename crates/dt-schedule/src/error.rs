use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScheduleError {
    #[error("schedule parse error: {0}")]
    Parse(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type ScheduleResult<T> = Result<T, ScheduleError>;
