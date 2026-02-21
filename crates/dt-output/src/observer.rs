//! `SimOutputObserver<W>` — bridges `SimObserver` to an `OutputWriter`.

use dt_agent::AgentStore;
use dt_core::{NodeId, SimConfig, Tick};
use dt_mobility::MobilityStore;
use dt_sim::SimObserver;

use crate::row::{AgentSnapshotRow, TickSummaryRow};
use crate::writer::OutputWriter;
use crate::OutputError;

/// A [`SimObserver`] that writes agent snapshots and tick summaries to any
/// [`OutputWriter`] backend (CSV, SQLite, Parquet, …).
///
/// Errors from the writer are stored internally because `SimObserver` methods
/// have no return value.  After `sim.run()` returns, check for errors with
/// [`take_error`][Self::take_error].
pub struct SimOutputObserver<W: OutputWriter> {
    writer:             W,
    start_unix_secs:    i64,
    tick_duration_secs: u32,
    last_error:         Option<OutputError>,
}

impl<W: OutputWriter> SimOutputObserver<W> {
    /// Create an observer backed by `writer`, using `config` for wall-clock
    /// conversion.
    pub fn new(writer: W, config: &SimConfig) -> Self {
        Self {
            writer,
            start_unix_secs:    config.start_unix_secs,
            tick_duration_secs: config.tick_duration_secs,
            last_error:         None,
        }
    }

    /// Take the stored write error (if any) after `sim.run()` returns.
    ///
    /// Returns `None` if all writes succeeded.
    pub fn take_error(&mut self) -> Option<OutputError> {
        self.last_error.take()
    }

    /// Unwrap the inner writer (e.g. to inspect files after the sim).
    pub fn into_writer(self) -> W {
        self.writer
    }

    fn unix_time(&self, tick: Tick) -> i64 {
        self.start_unix_secs + tick.0 as i64 * self.tick_duration_secs as i64
    }

    fn store_err(&mut self, result: crate::OutputResult<()>) {
        if let Err(e) = result {
            // Keep only the first error.
            if self.last_error.is_none() {
                self.last_error = Some(e);
            }
        }
    }
}

impl<W: OutputWriter> SimObserver for SimOutputObserver<W> {
    fn on_tick_end(&mut self, tick: Tick, woken: usize) {
        let row = TickSummaryRow {
            tick:           tick.0,
            unix_time_secs: self.unix_time(tick),
            woken_agents:   woken as u64,
        };
        let result = self.writer.write_tick_summary(&row);
        self.store_err(result);
    }

    fn on_snapshot(&mut self, tick: Tick, mobility: &MobilityStore, agents: &AgentStore) {
        let rows: Vec<AgentSnapshotRow> = (0..agents.count)
            .map(|i| {
                let state = &mobility.states[i];
                AgentSnapshotRow {
                    agent_id:         i as u32,
                    tick:             tick.0,
                    departure_node:   state.departure_node.0,
                    in_transit:       state.in_transit,
                    destination_node: if state.in_transit {
                        state.destination_node.0
                    } else {
                        NodeId::INVALID.0
                    },
                }
            })
            .collect();

        if !rows.is_empty() {
            let result = self.writer.write_snapshots(&rows);
            self.store_err(result);
        }
    }

    fn on_sim_end(&mut self, _final_tick: Tick) {
        let result = self.writer.finish();
        self.store_err(result);
    }
}
