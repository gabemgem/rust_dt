//! The `OutputWriter` trait implemented by all backend writers.

use crate::{AgentSnapshotRow, OutputResult, TickSummaryRow};

/// Trait implemented by CSV, SQLite, and Parquet writers.
///
/// All methods are infallible from the observer's perspective — errors are
/// stored internally and retrieved with [`SimOutputObserver::take_error`].
pub trait OutputWriter {
    /// Write a batch of agent snapshots.
    fn write_snapshots(&mut self, rows: &[AgentSnapshotRow]) -> OutputResult<()>;

    /// Write one tick summary row.
    fn write_tick_summary(&mut self, row: &TickSummaryRow) -> OutputResult<()>;

    /// Flush and close all underlying file handles.
    ///
    /// Idempotent — safe to call more than once.
    fn finish(&mut self) -> OutputResult<()>;
}
