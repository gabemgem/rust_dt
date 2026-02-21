//! Unit tests for dt-core primitives.

#[cfg(test)]
mod ids {
    use crate::{AgentId, EdgeId, NodeId};

    #[test]
    fn index_roundtrip() {
        let id = AgentId(42);
        assert_eq!(id.index(), 42);
        assert_eq!(AgentId::try_from(42usize).unwrap(), id);
    }

    #[test]
    fn ordering() {
        assert!(AgentId(0) < AgentId(1));
        assert!(NodeId(100) > NodeId(99));
    }

    #[test]
    fn invalid_sentinels_are_max() {
        assert_eq!(AgentId::INVALID.0, u32::MAX);
        assert_eq!(NodeId::INVALID.0, u32::MAX);
        assert_eq!(EdgeId::INVALID.0, u32::MAX);
    }

    #[test]
    fn display() {
        assert_eq!(AgentId(7).to_string(), "AgentId(7)");
    }
}

#[cfg(test)]
mod geo {
    use crate::GeoPoint;

    #[test]
    fn zero_distance() {
        let p = GeoPoint::new(30.694, -88.043);
        assert!(p.distance_m(p) < 0.01);
    }

    #[test]
    fn mobile_al_approx_distance() {
        // ~1 degree of latitude ≈ 111 km
        let a = GeoPoint::new(30.0, -88.0);
        let b = GeoPoint::new(31.0, -88.0);
        let d = a.distance_m(b);
        assert!((d - 111_195.0).abs() < 500.0, "got {d}");
    }

    #[test]
    fn bbox_check() {
        let center = GeoPoint::new(30.694, -88.043);
        let nearby = GeoPoint::new(30.700, -88.040);
        let far = GeoPoint::new(31.5, -88.043);
        assert!(nearby.within_bbox(center, 0.1));
        assert!(!far.within_bbox(center, 0.1));
    }
}

#[cfg(test)]
mod time {
    use crate::{SimClock, SimConfig, Tick};

    #[test]
    fn tick_arithmetic() {
        let t = Tick(10);
        assert_eq!(t + 5, Tick(15));
        assert_eq!(t.offset(3), Tick(13));
        assert_eq!(Tick(15) - Tick(10), 5u64);
    }

    #[test]
    fn clock_elapsed() {
        let mut clock = SimClock::new(0, 3600); // 1 tick = 1 hour
        assert_eq!(clock.elapsed_secs(), 0);
        clock.advance();
        assert_eq!(clock.elapsed_secs(), 3600);
        clock.advance();
        assert_eq!(clock.elapsed_secs(), 7200);
    }

    #[test]
    fn clock_dhm() {
        let mut clock = SimClock::new(0, 3600);
        // Advance 25 hours
        for _ in 0..25 {
            clock.advance();
        }
        let (d, h, m) = clock.elapsed_dhm();
        assert_eq!(d, 1);
        assert_eq!(h, 1);
        assert_eq!(m, 0);
    }

    #[test]
    fn ticks_for_duration() {
        let clock = SimClock::new(0, 3600);
        assert_eq!(clock.ticks_for_hours(24), 24);
        assert_eq!(clock.ticks_for_days(7), 168);
        // partial tick rounds up
        assert_eq!(clock.ticks_for_secs(1), 1);
    }

    #[test]
    fn sim_config_end_tick() {
        let cfg = SimConfig {
            start_unix_secs: 0,
            tick_duration_secs: 3600,
            total_ticks: 8760, // 365 days
            seed: 42,
            num_threads: None,
            output_interval_ticks: 24,
        };
        assert_eq!(cfg.end_tick(), Tick(8760));
    }
}

#[cfg(test)]
mod rng {
    use crate::{AgentId, AgentRng};

    #[test]
    fn deterministic_same_seed() {
        let mut r1 = AgentRng::new(12345, AgentId(0));
        let mut r2 = AgentRng::new(12345, AgentId(0));
        for _ in 0..100 {
            let a: f32 = r1.random();
            let b: f32 = r2.random();
            assert_eq!(a, b);
        }
    }

    #[test]
    fn different_agents_differ() {
        let mut r0 = AgentRng::new(1, AgentId(0));
        let mut r1 = AgentRng::new(1, AgentId(1));
        let a: u64 = r0.random();
        let b: u64 = r1.random();
        assert_ne!(a, b, "seeds for adjacent agents should diverge");
    }

    #[test]
    fn gen_range_in_bounds() {
        let mut rng = AgentRng::new(0, AgentId(0));
        for _ in 0..1000 {
            let v = rng.gen_range(0.0f32..1.0);
            assert!((0.0..1.0).contains(&v));
        }
    }

    #[test]
    fn gen_bool_extremes() {
        let mut rng = AgentRng::new(0, AgentId(0));
        assert!(!rng.gen_bool(0.0));
        assert!(rng.gen_bool(1.0));
    }
}

#[cfg(test)]
mod transport {
    use crate::TransportMode;

    #[test]
    fn is_moving() {
        assert!(!TransportMode::None.is_moving());
        assert!(TransportMode::Car.is_moving());
        assert!(TransportMode::Walk.is_moving());
    }

    #[test]
    fn display() {
        assert_eq!(TransportMode::Car.to_string(), "car");
        assert_eq!(TransportMode::None.to_string(), "none");
    }
}
