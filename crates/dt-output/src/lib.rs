//! `dt-output` — simulation output writers for the rust_dt framework.
//!
//! Three backends are provided behind Cargo features:
//!
//! | Feature   | Backend     | Files created                                           |
//! |-----------|-------------|---------------------------------------------------------|
//! | *(none)*  | CSV         | `agent_snapshots.csv`, `tick_summaries.csv`             |
//! | `sqlite`  | SQLite      | `output.db`                                             |
//! | `parquet` | Parquet     | `agent_snapshots.parquet`, `tick_summaries.parquet`     |
//!
//! All backends implement [`OutputWriter`] and are driven by
//! [`SimOutputObserver`], which implements `dt_sim::SimObserver`.
//!
//! # Usage
//!
//! ```rust,ignore
//! use dt_output::{CsvWriter, SimOutputObserver};
//!
//! let writer = CsvWriter::new(Path::new("./output")).unwrap();
//! let mut obs = SimOutputObserver::new(writer, &config);
//! sim.run(&mut obs).unwrap();
//! obs.take_error().map(|e| eprintln!("output error: {e}"));
//! ```

pub mod csv;
pub mod error;
pub mod observer;
pub mod row;
pub mod writer;

#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "parquet")]
pub mod parquet;

#[cfg(test)]
mod tests;

pub use csv::CsvWriter;
pub use error::{OutputError, OutputResult};
pub use observer::SimOutputObserver;
pub use row::{AgentSnapshotRow, TickSummaryRow};
pub use writer::OutputWriter;

#[cfg(feature = "sqlite")]
pub use sqlite::SqliteWriter;

#[cfg(feature = "parquet")]
pub use parquet::ParquetWriter;
