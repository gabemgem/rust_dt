//! Integration tests for dt-output.

#[cfg(test)]
mod csv_tests {
    use tempfile::TempDir;

    use crate::csv::CsvWriter;
    use crate::row::{AgentSnapshotRow, TickSummaryRow};
    use crate::writer::OutputWriter;

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("create temp dir")
    }

    fn snap_row(agent_id: u32, tick: u64) -> AgentSnapshotRow {
        AgentSnapshotRow {
            agent_id,
            tick,
            departure_node:   agent_id * 10,
            in_transit:       false,
            destination_node: u32::MAX,
        }
    }

    fn summary_row(tick: u64) -> TickSummaryRow {
        TickSummaryRow { tick, unix_time_secs: tick as i64 * 3600, woken_agents: tick }
    }

    #[test]
    fn csv_files_created() {
        let dir = tmp();
        let _w = CsvWriter::new(dir.path()).unwrap();
        assert!(dir.path().join("agent_snapshots.csv").exists());
        assert!(dir.path().join("tick_summaries.csv").exists());
    }

    #[test]
    fn csv_headers_correct() {
        let dir = tmp();
        let mut w = CsvWriter::new(dir.path()).unwrap();
        w.finish().unwrap();

        let mut rdr = csv::Reader::from_path(dir.path().join("agent_snapshots.csv")).unwrap();
        let headers: Vec<_> = rdr.headers().unwrap().iter().map(str::to_owned).collect();
        assert_eq!(headers, ["agent_id", "tick", "departure_node", "in_transit", "destination_node"]);

        let mut rdr2 = csv::Reader::from_path(dir.path().join("tick_summaries.csv")).unwrap();
        let headers2: Vec<_> = rdr2.headers().unwrap().iter().map(str::to_owned).collect();
        assert_eq!(headers2, ["tick", "unix_time_secs", "woken_agents"]);
    }

    #[test]
    fn csv_snapshot_round_trip() {
        let dir = tmp();
        let mut w = CsvWriter::new(dir.path()).unwrap();
        let rows = vec![snap_row(0, 5), snap_row(1, 5), snap_row(2, 5)];
        w.write_snapshots(&rows).unwrap();
        w.finish().unwrap();

        let mut rdr = csv::Reader::from_path(dir.path().join("agent_snapshots.csv")).unwrap();
        let read_rows: Vec<_> = rdr.records().map(|r| r.unwrap()).collect();
        assert_eq!(read_rows.len(), 3);
        assert_eq!(&read_rows[0][0], "0"); // agent_id
        assert_eq!(&read_rows[0][1], "5"); // tick
        assert_eq!(&read_rows[1][0], "1");
        assert_eq!(&read_rows[2][0], "2");
    }

    #[test]
    fn csv_tick_summary_round_trip() {
        let dir = tmp();
        let mut w = CsvWriter::new(dir.path()).unwrap();
        w.write_tick_summary(&summary_row(3)).unwrap();
        w.finish().unwrap();

        let mut rdr = csv::Reader::from_path(dir.path().join("tick_summaries.csv")).unwrap();
        let read_rows: Vec<_> = rdr.records().map(|r| r.unwrap()).collect();
        assert_eq!(read_rows.len(), 1);
        assert_eq!(&read_rows[0][0], "3");          // tick
        assert_eq!(&read_rows[0][1], "10800");      // 3 * 3600
        assert_eq!(&read_rows[0][2], "3");          // woken_agents
    }

    #[test]
    fn csv_finish_idempotent() {
        let dir = tmp();
        let mut w = CsvWriter::new(dir.path()).unwrap();
        w.finish().unwrap();
        w.finish().unwrap(); // second call should not panic
    }

    #[test]
    fn csv_empty_snapshot_ok() {
        let dir = tmp();
        let mut w = CsvWriter::new(dir.path()).unwrap();
        w.write_snapshots(&[]).unwrap(); // should return Ok(())
    }

    #[test]
    fn integration_csv() {
        use dt_agent::AgentStoreBuilder;
        use dt_behavior::NoopBehavior;
        use dt_core::{NodeId, SimConfig};
        use dt_sim::SimBuilder;
        use dt_spatial::DijkstraRouter;

        use crate::observer::SimOutputObserver;

        let config = SimConfig {
            start_unix_secs:       0,
            tick_duration_secs:    3600,
            total_ticks:           6,
            seed:                  1,
            num_threads:           Some(1),
            output_interval_ticks: 2,
        };

        let (store, rngs) = AgentStoreBuilder::new(3, 1).build();
        let mut sim = SimBuilder::new(config.clone(), store, rngs, NoopBehavior, DijkstraRouter)
            .initial_positions(vec![NodeId(0), NodeId(1), NodeId(2)])
            .build()
            .unwrap();

        let dir = tmp();
        let writer = CsvWriter::new(dir.path()).unwrap();
        let mut obs = SimOutputObserver::new(writer, &config);
        sim.run(&mut obs).unwrap();
        assert!(obs.take_error().is_none(), "no write errors expected");

        // output_interval = 2 → snapshots fired at ticks 0, 2, 4 (3 ticks × 3 agents = 9 rows)
        let mut rdr = csv::Reader::from_path(dir.path().join("agent_snapshots.csv")).unwrap();
        let rows: Vec<_> = rdr.records().map(|r| r.unwrap()).collect();
        assert_eq!(rows.len(), 9, "expected 3 ticks × 3 agents = 9 snapshot rows, got {}", rows.len());
    }
}

// ── SQLite tests ──────────────────────────────────────────────────────────────

#[cfg(all(test, feature = "sqlite"))]
mod sqlite_tests {
    use tempfile::TempDir;

    use crate::row::{AgentSnapshotRow, TickSummaryRow};
    use crate::sqlite::SqliteWriter;
    use crate::writer::OutputWriter;

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("create temp dir")
    }

    #[test]
    fn sqlite_db_created() {
        let dir = tmp();
        let _w = SqliteWriter::new(dir.path()).unwrap();
        assert!(dir.path().join("output.db").exists());
    }

    #[test]
    fn sqlite_snapshot_count() {
        let dir = tmp();
        let mut w = SqliteWriter::new(dir.path()).unwrap();
        let rows = vec![
            AgentSnapshotRow { agent_id: 0, tick: 1, departure_node: 10, in_transit: false, destination_node: u32::MAX },
            AgentSnapshotRow { agent_id: 1, tick: 1, departure_node: 11, in_transit: true,  destination_node: 20 },
            AgentSnapshotRow { agent_id: 2, tick: 1, departure_node: 12, in_transit: false, destination_node: u32::MAX },
        ];
        w.write_snapshots(&rows).unwrap();
        w.finish().unwrap();

        let conn = rusqlite::Connection::open(dir.path().join("output.db")).unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_snapshots", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn sqlite_in_transit_as_integer() {
        let dir = tmp();
        let mut w = SqliteWriter::new(dir.path()).unwrap();
        w.write_snapshots(&[AgentSnapshotRow {
            agent_id: 0, tick: 0, departure_node: 5, in_transit: true, destination_node: 9,
        }]).unwrap();
        w.finish().unwrap();

        let conn = rusqlite::Connection::open(dir.path().join("output.db")).unwrap();
        let val: i64 = conn.query_row(
            "SELECT in_transit FROM agent_snapshots WHERE agent_id = 0", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(val, 1, "in_transit=true should be stored as 1");
    }

    #[test]
    fn sqlite_invalid_node_stored() {
        let dir = tmp();
        let mut w = SqliteWriter::new(dir.path()).unwrap();
        w.write_snapshots(&[AgentSnapshotRow {
            agent_id: 0, tick: 0, departure_node: u32::MAX, in_transit: false, destination_node: u32::MAX,
        }]).unwrap();
        w.finish().unwrap();

        let conn = rusqlite::Connection::open(dir.path().join("output.db")).unwrap();
        // SQLite INTEGER is signed 64-bit; u32::MAX fits without loss.
        let val: i64 = conn.query_row(
            "SELECT departure_node FROM agent_snapshots WHERE agent_id = 0", [], |r| r.get(0)
        ).unwrap();
        assert_eq!(val, u32::MAX as i64);
    }

    #[test]
    fn sqlite_tick_summary() {
        let dir = tmp();
        let mut w = SqliteWriter::new(dir.path()).unwrap();
        w.write_tick_summary(&TickSummaryRow {
            tick: 7, unix_time_secs: 25_200, woken_agents: 42,
        }).unwrap();
        w.finish().unwrap();

        let conn = rusqlite::Connection::open(dir.path().join("output.db")).unwrap();
        let (tick, unix_time, woken): (i64, i64, i64) = conn.query_row(
            "SELECT tick, unix_time_secs, woken_agents FROM tick_summaries WHERE tick = 7",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ).unwrap();
        assert_eq!(tick, 7);
        assert_eq!(unix_time, 25_200);
        assert_eq!(woken, 42);
    }
}

// ── Parquet tests ─────────────────────────────────────────────────────────────

#[cfg(all(test, feature = "parquet"))]
mod parquet_tests {
    use tempfile::TempDir;

    use arrow::datatypes::DataType;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    use crate::parquet::ParquetWriter;
    use crate::row::AgentSnapshotRow;
    use crate::writer::OutputWriter;

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("create temp dir")
    }

    #[test]
    fn parquet_files_created() {
        let dir = tmp();
        let mut w = ParquetWriter::new(dir.path()).unwrap();
        w.finish().unwrap();
        assert!(dir.path().join("agent_snapshots.parquet").exists());
        assert!(dir.path().join("tick_summaries.parquet").exists());
    }

    #[test]
    fn parquet_snapshot_round_trip() {
        let dir = tmp();
        let mut w = ParquetWriter::new(dir.path()).unwrap();
        let rows = vec![
            AgentSnapshotRow { agent_id: 0, tick: 2, departure_node: 10, in_transit: false, destination_node: u32::MAX },
            AgentSnapshotRow { agent_id: 1, tick: 2, departure_node: 11, in_transit: true,  destination_node: 20 },
        ];
        w.write_snapshots(&rows).unwrap();
        w.finish().unwrap();

        let file = std::fs::File::open(dir.path().join("agent_snapshots.parquet")).unwrap();
        let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
        let schema = builder.schema().clone();
        let reader = builder.build().unwrap();

        let batches: Vec<_> = reader.map(|b| b.unwrap()).collect();
        let total_rows: usize = batches.iter().map(|b| b.num_rows()).sum();
        assert_eq!(total_rows, 2, "expected 2 rows");

        // Check schema field names
        let field_names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        assert_eq!(field_names, ["agent_id", "tick", "departure_node", "in_transit", "destination_node"]);
    }

    #[test]
    fn parquet_boolean_column_type() {
        let dir = tmp();
        let mut w = ParquetWriter::new(dir.path()).unwrap();
        w.write_snapshots(&[AgentSnapshotRow {
            agent_id: 0, tick: 0, departure_node: 1, in_transit: true, destination_node: 2,
        }]).unwrap();
        w.finish().unwrap();

        let file = std::fs::File::open(dir.path().join("agent_snapshots.parquet")).unwrap();
        let builder = ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
        let schema = builder.schema().clone();

        let in_transit_field = schema.field_with_name("in_transit").unwrap();
        assert_eq!(*in_transit_field.data_type(), DataType::Boolean);
    }

    #[test]
    fn parquet_finish_required() {
        // A Parquet file whose writer was NOT closed is invalid (missing footer).
        // We verify that a dropped-without-finish writer produces an unreadable file.
        let dir = tmp();
        {
            let mut w = ParquetWriter::new(dir.path()).unwrap();
            w.write_snapshots(&[AgentSnapshotRow {
                agent_id: 0, tick: 0, departure_node: 1, in_transit: false, destination_node: u32::MAX,
            }]).unwrap();
            // Drop without calling finish() — ArrowWriter's Drop will NOT write the footer.
        }

        let file = std::fs::File::open(dir.path().join("agent_snapshots.parquet")).unwrap();
        let result = ParquetRecordBatchReaderBuilder::try_new(file);
        assert!(result.is_err(), "file without Parquet footer should fail to open");
    }
}
