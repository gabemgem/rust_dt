//! SQLite output backend (feature `sqlite`).
//!
//! Creates a single `output.db` file in the configured output directory with
//! two tables: `agent_snapshots` and `tick_summaries`.

use std::path::Path;

use rusqlite::Connection;

use crate::{AgentSnapshotRow, OutputResult, TickSummaryRow};
use crate::writer::OutputWriter;

/// Writes simulation output to an SQLite database.
pub struct SqliteWriter {
    conn:     Connection,
    finished: bool,
}

impl SqliteWriter {
    /// Open (or create) `output.db` in `dir` and initialise the schema.
    pub fn new(dir: &Path) -> OutputResult<Self> {
        let conn = Connection::open(dir.join("output.db"))?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous  = NORMAL;
             CREATE TABLE IF NOT EXISTS agent_snapshots (
                 agent_id         INTEGER NOT NULL,
                 tick             INTEGER NOT NULL,
                 departure_node   INTEGER NOT NULL,
                 in_transit       INTEGER NOT NULL,
                 destination_node INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS tick_summaries (
                 tick           INTEGER PRIMARY KEY,
                 unix_time_secs INTEGER NOT NULL,
                 woken_agents   INTEGER NOT NULL
             );",
        )?;

        Ok(Self { conn, finished: false })
    }
}

impl OutputWriter for SqliteWriter {
    fn write_snapshots(&mut self, rows: &[AgentSnapshotRow]) -> OutputResult<()> {
        if rows.is_empty() {
            return Ok(());
        }
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO agent_snapshots \
                 (agent_id, tick, departure_node, in_transit, destination_node) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for row in rows {
                stmt.execute(rusqlite::params![
                    row.agent_id,
                    row.tick,
                    row.departure_node,
                    row.in_transit as i64,
                    row.destination_node,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn write_tick_summary(&mut self, row: &TickSummaryRow) -> OutputResult<()> {
        self.conn.execute(
            "INSERT INTO tick_summaries (tick, unix_time_secs, woken_agents) \
             VALUES (?1, ?2, ?3)",
            rusqlite::params![row.tick, row.unix_time_secs, row.woken_agents],
        )?;
        Ok(())
    }

    fn finish(&mut self) -> OutputResult<()> {
        if self.finished {
            return Ok(());
        }
        self.finished = true;
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }
}
