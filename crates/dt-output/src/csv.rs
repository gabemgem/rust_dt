//! CSV output backend.
//!
//! Creates two files in the configured output directory:
//! - `agent_snapshots.csv`
//! - `tick_summaries.csv`

use std::fs::File;
use std::path::Path;

use csv::Writer;

use crate::{AgentSnapshotRow, OutputResult, TickSummaryRow};
use crate::writer::OutputWriter;

/// Writes simulation output to two CSV files.
pub struct CsvWriter {
    snapshots:  Writer<File>,
    summaries:  Writer<File>,
    finished:   bool,
}

impl CsvWriter {
    /// Open (or create) the two CSV files in `dir` and write the header rows.
    pub fn new(dir: &Path) -> OutputResult<Self> {
        let mut snapshots = Writer::from_path(dir.join("agent_snapshots.csv"))?;
        snapshots.write_record(["agent_id", "tick", "departure_node", "in_transit", "destination_node"])?;

        let mut summaries = Writer::from_path(dir.join("tick_summaries.csv"))?;
        summaries.write_record(["tick", "unix_time_secs", "woken_agents"])?;

        Ok(Self {
            snapshots,
            summaries,
            finished: false,
        })
    }
}

impl OutputWriter for CsvWriter {
    fn write_snapshots(&mut self, rows: &[AgentSnapshotRow]) -> OutputResult<()> {
        for row in rows {
            self.snapshots.write_record(&[
                row.agent_id.to_string(),
                row.tick.to_string(),
                row.departure_node.to_string(),
                (row.in_transit as u8).to_string(),
                row.destination_node.to_string(),
            ])?;
        }
        Ok(())
    }

    fn write_tick_summary(&mut self, row: &TickSummaryRow) -> OutputResult<()> {
        self.summaries.write_record(&[
            row.tick.to_string(),
            row.unix_time_secs.to_string(),
            row.woken_agents.to_string(),
        ])?;
        Ok(())
    }

    fn finish(&mut self) -> OutputResult<()> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        self.snapshots.flush()?;
        self.summaries.flush()?;
        Ok(())
    }
}
