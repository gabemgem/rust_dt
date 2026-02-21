//! Parquet output backend (feature `parquet`).
//!
//! Creates two files in the configured output directory:
//! - `agent_snapshots.parquet`
//! - `tick_summaries.parquet`

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use arrow::array::{
    BooleanBuilder, Int64Builder, UInt32Builder, UInt64Builder,
};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;

use crate::writer::OutputWriter;
use crate::{AgentSnapshotRow, OutputResult, TickSummaryRow};

fn snapshot_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("agent_id",         DataType::UInt32,  false),
        Field::new("tick",             DataType::UInt64,  false),
        Field::new("departure_node",   DataType::UInt32,  false),
        Field::new("in_transit",       DataType::Boolean, false),
        Field::new("destination_node", DataType::UInt32,  false),
    ]))
}

fn summary_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("tick",           DataType::UInt64, false),
        Field::new("unix_time_secs", DataType::Int64,  false),
        Field::new("woken_agents",   DataType::UInt64, false),
    ]))
}

fn snappy_props() -> WriterProperties {
    WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .build()
}

/// Writes simulation output to two Parquet files.
///
/// `finish()` **must** be called to write the Parquet file footer; files
/// written without calling `finish()` cannot be opened by Parquet readers.
pub struct ParquetWriter {
    snapshots:   Option<ArrowWriter<File>>,
    summaries:   Option<ArrowWriter<File>>,
    snap_schema: Arc<Schema>,
    summ_schema: Arc<Schema>,
}

impl ParquetWriter {
    /// Create both Parquet files in `dir`.
    pub fn new(dir: &Path) -> OutputResult<Self> {
        let snap_schema = snapshot_schema();
        let summ_schema = summary_schema();

        let snap_file = File::create(dir.join("agent_snapshots.parquet"))?;
        let snapshots = ArrowWriter::try_new(
            snap_file,
            Arc::clone(&snap_schema),
            Some(snappy_props()),
        )?;

        let summ_file = File::create(dir.join("tick_summaries.parquet"))?;
        let summaries = ArrowWriter::try_new(
            summ_file,
            Arc::clone(&summ_schema),
            Some(snappy_props()),
        )?;

        Ok(Self {
            snapshots: Some(snapshots),
            summaries: Some(summaries),
            snap_schema,
            summ_schema,
        })
    }
}

impl OutputWriter for ParquetWriter {
    fn write_snapshots(&mut self, rows: &[AgentSnapshotRow]) -> OutputResult<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let Some(writer) = self.snapshots.as_mut() else {
            return Ok(());
        };

        let mut agent_ids         = UInt32Builder::new();
        let mut ticks             = UInt64Builder::new();
        let mut departure_nodes   = UInt32Builder::new();
        let mut in_transits       = BooleanBuilder::new();
        let mut destination_nodes = UInt32Builder::new();

        for row in rows {
            agent_ids.append_value(row.agent_id);
            ticks.append_value(row.tick);
            departure_nodes.append_value(row.departure_node);
            in_transits.append_value(row.in_transit);
            destination_nodes.append_value(row.destination_node);
        }

        let batch = RecordBatch::try_new(
            Arc::clone(&self.snap_schema),
            vec![
                Arc::new(agent_ids.finish()),
                Arc::new(ticks.finish()),
                Arc::new(departure_nodes.finish()),
                Arc::new(in_transits.finish()),
                Arc::new(destination_nodes.finish()),
            ],
        )?;
        writer.write(&batch)?;
        Ok(())
    }

    fn write_tick_summary(&mut self, row: &TickSummaryRow) -> OutputResult<()> {
        let Some(writer) = self.summaries.as_mut() else {
            return Ok(());
        };

        let mut ticks      = UInt64Builder::new();
        let mut unix_times = Int64Builder::new();
        let mut woken      = UInt64Builder::new();

        ticks.append_value(row.tick);
        unix_times.append_value(row.unix_time_secs);
        woken.append_value(row.woken_agents);

        let batch = RecordBatch::try_new(
            Arc::clone(&self.summ_schema),
            vec![
                Arc::new(ticks.finish()),
                Arc::new(unix_times.finish()),
                Arc::new(woken.finish()),
            ],
        )?;
        writer.write(&batch)?;
        Ok(())
    }

    fn finish(&mut self) -> OutputResult<()> {
        if let Some(w) = self.snapshots.take() {
            w.close()?;
        }
        if let Some(w) = self.summaries.take() {
            w.close()?;
        }
        Ok(())
    }
}
