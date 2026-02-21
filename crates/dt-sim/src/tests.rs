//! Integration tests for dt-sim.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use dt_agent::AgentStoreBuilder;
use dt_behavior::{BehaviorModel, ContactEvent, Intent, NoopBehavior, SimContext};
use dt_core::{AgentId, AgentRng, GeoPoint, NodeId, SimConfig, Tick, TransportMode};
use dt_schedule::{ActivityPlan, ScheduledActivity, Destination};
use dt_spatial::{DijkstraRouter, RoadNetworkBuilder};

use crate::{NoopObserver, SimBuilder, SimObserver};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn test_config(total_ticks: u64) -> SimConfig {
    SimConfig {
        start_unix_secs:       0,
        tick_duration_secs:    3600,
        total_ticks,
        seed:                  42,
        num_threads:           Some(1),
        output_interval_ticks: total_ticks,
    }
}

fn small_store(n: usize) -> (dt_agent::AgentStore, dt_agent::AgentRngs) {
    AgentStoreBuilder::new(n, 42).build()
}

/// Network with 3 nodes in a line: 0 ↔ 1 ↔ 2.
fn line_network() -> dt_spatial::RoadNetwork {
    let mut b = RoadNetworkBuilder::new();
    let n0 = b.add_node(GeoPoint { lat: 0.0,   lon: 0.0 });
    let n1 = b.add_node(GeoPoint { lat: 0.005, lon: 0.0 });
    let n2 = b.add_node(GeoPoint { lat: 0.01,  lon: 0.0 });
    b.add_road(n0, n1, 500.0, 60_000); // 500 m, 60 s → travel_ticks = ceil(60/3600) = 1
    b.add_road(n1, n2, 500.0, 60_000);
    b.build()
}

// ── SimBuilder validation ─────────────────────────────────────────────────────

#[cfg(test)]
mod builder_tests {
    use super::*;

    #[test]
    fn builds_successfully_with_defaults() {
        let (store, rngs) = small_store(3);
        let sim = SimBuilder::new(test_config(10), store, rngs, NoopBehavior, DijkstraRouter)
            .build()
            .unwrap();
        assert_eq!(sim.agents.count, 3);
        assert_eq!(sim.plans.len(), 3);
    }

    #[test]
    fn plan_count_mismatch_errors() {
        let (store, rngs) = small_store(3);
        let plans = vec![ActivityPlan::empty(); 2]; // wrong length
        let result = SimBuilder::new(test_config(10), store, rngs, NoopBehavior, DijkstraRouter)
            .plans(plans)
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn position_count_mismatch_errors() {
        let (store, rngs) = small_store(3);
        let positions = vec![NodeId(0); 2]; // wrong length
        let result = SimBuilder::new(test_config(10), store, rngs, NoopBehavior, DijkstraRouter)
            .initial_positions(positions)
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn initial_positions_placed_in_mobility() {
        let (store, rngs) = small_store(2);
        let positions = vec![NodeId(0), NodeId(0)];
        let sim = SimBuilder::new(test_config(10), store, rngs, NoopBehavior, DijkstraRouter)
            .initial_positions(positions)
            .build()
            .unwrap();
        assert_eq!(sim.mobility.store.states[0].departure_node, NodeId(0));
        assert_eq!(sim.mobility.store.states[1].departure_node, NodeId(0));
    }

    #[test]
    fn plans_seed_wake_queue() {
        // Activity that starts at tick 0 and lasts 8 ticks (offset=0, dur=8).
        // next_wake_tick(0) = tick 8 (when next activity would start — but there
        // is only one activity so it wraps to tick 24).
        let act = ScheduledActivity {
            start_offset_ticks: 0,
            duration_ticks:     8,
            activity_id:        dt_core::ActivityId(0),
            destination:        Destination::Home,
        };
        let plan = ActivityPlan::new(vec![act], 24);
        let (store, rngs) = small_store(1);
        let sim = SimBuilder::new(test_config(100), store, rngs, NoopBehavior, DijkstraRouter)
            .plans(vec![plan])
            .build()
            .unwrap();
        // Agent 0 should be woken at tick 24 (single activity wraps to next cycle).
        assert_eq!(sim.wake_queue.next_tick(), Some(Tick(24)));
    }
}

// ── Basic run ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod run_tests {
    use super::*;

    #[test]
    fn noop_runs_to_end_tick() {
        let (store, rngs) = small_store(5);
        let mut sim = SimBuilder::new(test_config(10), store, rngs, NoopBehavior, DijkstraRouter)
            .build()
            .unwrap();
        sim.run(&mut NoopObserver).unwrap();
        assert_eq!(sim.clock.current_tick, Tick(10));
    }

    #[test]
    fn run_ticks_advances_clock() {
        let (store, rngs) = small_store(2);
        let mut sim = SimBuilder::new(test_config(100), store, rngs, NoopBehavior, DijkstraRouter)
            .build()
            .unwrap();
        sim.run_ticks(5, &mut NoopObserver).unwrap();
        assert_eq!(sim.clock.current_tick, Tick(5));
        sim.run_ticks(3, &mut NoopObserver).unwrap();
        assert_eq!(sim.clock.current_tick, Tick(8));
    }

    /// Observer that counts ticks.
    struct TickCounter {
        starts: usize,
        ends:   usize,
    }
    impl SimObserver for TickCounter {
        fn on_tick_start(&mut self, _t: Tick) { self.starts += 1; }
        fn on_tick_end(&mut self, _t: Tick, _w: usize) { self.ends += 1; }
    }

    #[test]
    fn observer_called_correct_number_of_times() {
        let (store, rngs) = small_store(1);
        let mut sim = SimBuilder::new(test_config(7), store, rngs, NoopBehavior, DijkstraRouter)
            .build()
            .unwrap();
        let mut obs = TickCounter { starts: 0, ends: 0 };
        sim.run(&mut obs).unwrap();
        assert_eq!(obs.starts, 7);
        assert_eq!(obs.ends, 7);
    }

    #[test]
    fn observer_woken_count_reported() {
        // A behavior that re-schedules the agent every tick.
        struct WakeEveryTick;
        impl BehaviorModel for WakeEveryTick {
            fn replan(&self, _a: AgentId, ctx: &SimContext<'_>, _r: &mut AgentRng) -> Vec<Intent> {
                vec![Intent::WakeAt(ctx.tick + 1)]
            }
        }

        // Give agent 0 a plan so it gets an initial wake at tick 24.
        let act = ScheduledActivity {
            start_offset_ticks: 0,
            duration_ticks:     1,
            activity_id:        dt_core::ActivityId(0),
            destination:        Destination::Home,
        };
        let plan = ActivityPlan::new(vec![act], 1); // 1-tick cycle → wakes every tick
        let (store, rngs) = small_store(1);
        let mut sim = SimBuilder::new(test_config(5), store, rngs, WakeEveryTick, DijkstraRouter)
            .plans(vec![plan])
            .build()
            .unwrap();

        let woken_counts = Arc::new(Mutex::new(Vec::new()));
        struct CountWoken(Arc<Mutex<Vec<usize>>>);
        impl SimObserver for CountWoken {
            fn on_tick_end(&mut self, _t: Tick, w: usize) {
                self.0.lock().unwrap().push(w);
            }
        }

        sim.run(&mut CountWoken(Arc::clone(&woken_counts))).unwrap();
        // For a 1-tick cycle, next_wake_tick(Tick(0)) = Tick(1) (cycle wraps by 1).
        // So tick 0 has 0 woken agents, and ticks 1-4 have 1 woken each.
        let counts = woken_counts.lock().unwrap();
        assert_eq!(counts[0], 0, "tick 0: agent not yet in queue");
        assert!(counts[1..].iter().all(|&c| c == 1), "ticks 1-4: expect 1 woken each: {counts:?}");
    }
}

// ── Intent processing ─────────────────────────────────────────────────────────

#[cfg(test)]
mod intent_tests {
    use super::*;

    #[test]
    fn wake_at_reschedules_agent() {
        // Behavior: on first call return WakeAt(tick+3), then return nothing.
        struct WakeOnce(Mutex<bool>);
        impl BehaviorModel for WakeOnce {
            fn replan(&self, _a: AgentId, ctx: &SimContext<'_>, _r: &mut AgentRng) -> Vec<Intent> {
                let mut fired = self.0.lock().unwrap();
                if !*fired {
                    *fired = true;
                    vec![Intent::WakeAt(ctx.tick + 3)]
                } else {
                    vec![]
                }
            }
        }

        // Use a 1-tick cycle plan so agent starts in the queue at tick 1.
        let act = ScheduledActivity {
            start_offset_ticks: 0,
            duration_ticks:     1,
            activity_id:        dt_core::ActivityId(0),
            destination:        Destination::Home,
        };
        let plan = ActivityPlan::new(vec![act], 1);
        let (store, rngs) = small_store(1);
        let mut sim = SimBuilder::new(
                test_config(20),
                store, rngs,
                WakeOnce(Mutex::new(false)),
                DijkstraRouter,
            )
            .plans(vec![plan])
            .build()
            .unwrap();

        // Record every tick the agent was woken.
        let woken_ticks = Arc::new(Mutex::new(Vec::new()));
        struct RecordWoken(Arc<Mutex<Vec<Tick>>>);
        impl SimObserver for RecordWoken {
            fn on_tick_end(&mut self, t: Tick, w: usize) {
                if w > 0 { self.0.lock().unwrap().push(t); }
            }
        }

        sim.run(&mut RecordWoken(Arc::clone(&woken_ticks))).unwrap();
        let woken = woken_ticks.lock().unwrap();
        // For a 1-tick cycle, next_wake_tick(Tick(0)) = Tick(1).
        // WakeOnce fires at its first wake (tick 1) and returns WakeAt(tick + 3) = WakeAt(4).
        assert!(woken.contains(&Tick(1)), "expected first wake at tick 1, got {woken:?}");
        assert!(woken.contains(&Tick(4)), "expected rescheduled wake at tick 4, got {woken:?}");
    }

    #[test]
    fn wake_at_in_past_ignored() {
        // Behavior returns WakeAt(tick - 1) on first call (in the past).
        struct WakeInPast;
        impl BehaviorModel for WakeInPast {
            fn replan(&self, _a: AgentId, ctx: &SimContext<'_>, _r: &mut AgentRng) -> Vec<Intent> {
                if ctx.tick == Tick(0) {
                    vec![Intent::WakeAt(Tick(0))] // same tick — should be ignored
                } else {
                    vec![]
                }
            }
        }
        let act = ScheduledActivity {
            start_offset_ticks: 0,
            duration_ticks:     1,
            activity_id:        dt_core::ActivityId(0),
            destination:        Destination::Home,
        };
        let plan = ActivityPlan::new(vec![act], 1);
        let (store, rngs) = small_store(1);
        let mut sim = SimBuilder::new(test_config(5), store, rngs, WakeInPast, DijkstraRouter)
            .plans(vec![plan])
            .build()
            .unwrap();
        // Should complete without hanging (no infinite re-schedule).
        sim.run(&mut NoopObserver).unwrap();
        assert_eq!(sim.clock.current_tick, Tick(5));
    }

    #[test]
    fn travel_to_initiates_transit() {
        // Agent at node 0 requests travel to node 2 on its first wake.
        struct TravelOnce(Mutex<bool>);
        impl BehaviorModel for TravelOnce {
            fn replan(&self, _a: AgentId, _ctx: &SimContext<'_>, _r: &mut AgentRng) -> Vec<Intent> {
                let mut done = self.0.lock().unwrap();
                if !*done {
                    *done = true;
                    vec![Intent::TravelTo {
                        destination: NodeId(2),
                        mode:        TransportMode::Car,
                    }]
                } else {
                    vec![]
                }
            }
        }

        let net = line_network();
        let (store, rngs) = small_store(1);
        // Give agent a 1-tick cycle so it wakes at tick 0.
        let act = ScheduledActivity {
            start_offset_ticks: 0,
            duration_ticks:     1,
            activity_id:        dt_core::ActivityId(0),
            destination:        Destination::Home,
        };
        let plan = ActivityPlan::new(vec![act], 1);
        let mut sim = SimBuilder::new(
                test_config(10),
                store, rngs,
                TravelOnce(Mutex::new(false)),
                DijkstraRouter,
            )
            .plans(vec![plan])
            .network(net)
            .initial_positions(vec![NodeId(0)])
            .build()
            .unwrap();

        // After tick 1 (agent's first wake), the agent should be in transit.
        // run_ticks(2) processes ticks 0 and 1; arrival is at tick 2 so the
        // agent is still mid-journey when we check.
        sim.run_ticks(2, &mut NoopObserver).unwrap();
        assert!(
            sim.mobility.store.in_transit(AgentId(0)),
            "agent should be in transit after TravelTo intent"
        );
        assert_eq!(sim.mobility.store.states[0].destination_node, NodeId(2));
    }

    #[test]
    fn agent_arrives_after_travel_ticks() {
        // Agent travels from 0 to 2; each leg is 60 s = 1 tick at 3600 s/tick?
        // travel_ticks = ceil(total_travel_secs / tick_duration_secs)
        // For node 0→1→2 via Dijkstra: 60s + 60s = 120s → ceil(120/3600) = 1 tick.
        struct TravelToNode2(Mutex<bool>);
        impl BehaviorModel for TravelToNode2 {
            fn replan(&self, _a: AgentId, _ctx: &SimContext<'_>, _r: &mut AgentRng) -> Vec<Intent> {
                let mut done = self.0.lock().unwrap();
                if !*done {
                    *done = true;
                    vec![Intent::TravelTo {
                        destination: NodeId(2),
                        mode:        TransportMode::Car,
                    }]
                } else {
                    vec![]
                }
            }
        }

        let net = line_network();
        let (store, rngs) = small_store(1);
        let act = ScheduledActivity {
            start_offset_ticks: 0,
            duration_ticks:     1,
            activity_id:        dt_core::ActivityId(0),
            destination:        Destination::Home,
        };
        let plan = ActivityPlan::new(vec![act], 1);
        let mut sim = SimBuilder::new(
                test_config(10),
                store, rngs,
                TravelToNode2(Mutex::new(false)),
                DijkstraRouter,
            )
            .plans(vec![plan])
            .network(net)
            .initial_positions(vec![NodeId(0)])
            .build()
            .unwrap();

        sim.run(&mut NoopObserver).unwrap();
        // After the sim completes, the agent should be at node 2 (arrived).
        assert!(
            !sim.mobility.store.in_transit(AgentId(0)),
            "agent should have arrived"
        );
        assert_eq!(
            sim.mobility.store.states[0].departure_node,
            NodeId(2),
            "agent should be at destination node"
        );
    }
}

// ── Message queue ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod message_tests {
    use super::*;

    /// One-tick-cycle helper plan used throughout these tests.
    fn tick1_plan() -> ActivityPlan {
        let act = ScheduledActivity {
            start_offset_ticks: 0,
            duration_ticks:     1,
            activity_id:        dt_core::ActivityId(0),
            destination:        Destination::Home,
        };
        ActivityPlan::new(vec![act], 1)
    }

    #[test]
    fn message_delivered_on_next_wake() {
        // Two agents with 1-tick cycle plans: both first wake at tick 1.
        // Agent 0 sends a message to agent 1 on its first wake (tick 1).
        // Agent 1 pre-collects its messages BEFORE apply — so it sees the
        // message at tick 2 (its next wake).

        let received = Arc::new(AtomicBool::new(false));

        struct PingPong {
            sent:     AtomicBool,
            received: Arc<AtomicBool>,
        }

        impl BehaviorModel for PingPong {
            fn replan(
                &self,
                agent: AgentId,
                ctx:   &SimContext<'_>,
                _rng:  &mut AgentRng,
            ) -> Vec<Intent> {
                // Always reschedule so both agents keep waking.
                let mut v = vec![Intent::WakeAt(ctx.tick + 1)];
                // Agent 0 sends exactly once.
                if agent == AgentId(0)
                    && !self.sent.swap(true, Ordering::SeqCst)
                {
                    v.push(Intent::SendMessage {
                        to:      AgentId(1),
                        payload: b"ping".to_vec(),
                    });
                }
                v
            }

            fn on_message(
                &self,
                agent:   AgentId,
                from:    AgentId,
                payload: &[u8],
                _ctx:    &SimContext<'_>,
                _rng:    &mut AgentRng,
            ) -> Vec<Intent> {
                if agent == AgentId(1) && from == AgentId(0) && payload == b"ping" {
                    self.received.store(true, Ordering::SeqCst);
                }
                vec![]
            }
        }

        let plan = tick1_plan();
        let (store, rngs) = small_store(2);
        let mut sim = SimBuilder::new(
                test_config(5),
                store, rngs,
                PingPong { sent: AtomicBool::new(false), received: Arc::clone(&received) },
                DijkstraRouter,
            )
            .plans(vec![plan.clone(), plan])
            .build()
            .unwrap();

        sim.run(&mut NoopObserver).unwrap();
        assert!(received.load(Ordering::SeqCst), "agent 1 should have received the ping");
    }

    #[test]
    fn message_queued_in_sim_state() {
        // After a tick that sends a message, the message should be visible in
        // sim.message_queue until the recipient next wakes.

        struct OneSender;
        impl BehaviorModel for OneSender {
            fn replan(
                &self,
                agent: AgentId,
                _ctx:  &SimContext<'_>,
                _rng:  &mut AgentRng,
            ) -> Vec<Intent> {
                if agent == AgentId(0) {
                    vec![Intent::SendMessage {
                        to:      AgentId(1),
                        payload: b"hello".to_vec(),
                    }]
                } else {
                    vec![]
                }
            }
        }

        // Only agent 0 wakes (1-tick cycle); agent 1 has empty plan.
        let plan = tick1_plan();
        let (store, rngs) = small_store(2);
        let mut sim = SimBuilder::new(
                test_config(10),
                store, rngs,
                OneSender,
                DijkstraRouter,
            )
            .plans(vec![plan, ActivityPlan::empty()])
            .build()
            .unwrap();

        // Run 2 ticks: tick 0 (nothing), tick 1 (agent 0 wakes and sends).
        sim.run_ticks(2, &mut NoopObserver).unwrap();

        // Agent 1 has never woken, so the message should still be queued.
        assert!(
            sim.message_queue.contains_key(&AgentId(1)),
            "message should be in queue for agent 1"
        );
        let msgs = sim.message_queue.get(&AgentId(1)).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].0, AgentId(0));
        assert_eq!(msgs[0].1, b"hello");
    }

    #[test]
    fn multiple_senders_all_delivered() {
        // Agents 0 and 2 both send to agent 1; agent 1 should receive both.
        let received = Arc::new(AtomicUsize::new(0));

        struct MultiSend {
            received: Arc<AtomicUsize>,
        }

        impl BehaviorModel for MultiSend {
            fn replan(
                &self,
                agent: AgentId,
                ctx:   &SimContext<'_>,
                _rng:  &mut AgentRng,
            ) -> Vec<Intent> {
                let mut v = vec![Intent::WakeAt(ctx.tick + 1)];
                // Send exactly once: on the first wake (tick 1), agents 0 and 2 both send.
                // Tick-based guard avoids the shared-flag race where one sender's swap
                // prevents the other from firing.
                if agent != AgentId(1) && ctx.tick == Tick(1) {
                    v.push(Intent::SendMessage {
                        to:      AgentId(1),
                        payload: vec![agent.0 as u8],
                    });
                }
                v
            }

            fn on_message(
                &self,
                agent: AgentId,
                _from: AgentId,
                _payload: &[u8],
                _ctx: &SimContext<'_>,
                _rng: &mut AgentRng,
            ) -> Vec<Intent> {
                if agent == AgentId(1) {
                    self.received.fetch_add(1, Ordering::SeqCst);
                }
                vec![]
            }
        }

        let plan = tick1_plan();
        let (store, rngs) = small_store(3);
        let mut sim = SimBuilder::new(
                test_config(5),
                store, rngs,
                MultiSend { received: Arc::clone(&received) },
                DijkstraRouter,
            )
            .plans(vec![plan.clone(), plan.clone(), plan])
            .build()
            .unwrap();

        sim.run(&mut NoopObserver).unwrap();
        // Agents 0 and 2 each send exactly one message → 2 deliveries.
        assert_eq!(received.load(Ordering::SeqCst), 2);
    }
}

// ── Contact detection ─────────────────────────────────────────────────────────

#[cfg(test)]
mod contact_tests {
    use super::*;

    fn tick1_plan() -> ActivityPlan {
        let act = ScheduledActivity {
            start_offset_ticks: 0,
            duration_ticks:     1,
            activity_id:        dt_core::ActivityId(0),
            destination:        Destination::Home,
        };
        ActivityPlan::new(vec![act], 1)
    }

    #[test]
    fn colocated_agents_see_each_other() {
        // Two agents placed at node 0.  Each time they wake they should each
        // see the other as a contact.
        let contact_count = Arc::new(AtomicUsize::new(0));

        struct CountContacts(Arc<AtomicUsize>);
        impl BehaviorModel for CountContacts {
            fn replan(
                &self,
                _a:   AgentId,
                ctx:  &SimContext<'_>,
                _rng: &mut AgentRng,
            ) -> Vec<Intent> {
                vec![Intent::WakeAt(ctx.tick + 1)]
            }

            fn on_contacts(
                &self,
                _a:        AgentId,
                contacts:  &[ContactEvent],
                _ctx:      &SimContext<'_>,
                _rng:      &mut AgentRng,
            ) -> Vec<Intent> {
                self.0.fetch_add(contacts.len(), Ordering::SeqCst);
                vec![]
            }
        }

        let plan = tick1_plan();
        let (store, rngs) = small_store(2);
        let mut sim = SimBuilder::new(
                test_config(4),
                store, rngs,
                CountContacts(Arc::clone(&contact_count)),
                DijkstraRouter,
            )
            .plans(vec![plan.clone(), plan])
            .initial_positions(vec![NodeId(0), NodeId(0)])
            .build()
            .unwrap();

        sim.run(&mut NoopObserver).unwrap();

        // Both agents wake at ticks 1, 2, 3 (first wake is at tick 1 for
        // 1-tick cycle; WakeAt(tick+1) keeps them waking through tick 3).
        // Each tick both agents see 1 contact → 3 ticks × 2 agents = 6.
        assert_eq!(
            contact_count.load(Ordering::SeqCst),
            6,
            "expected 6 contact observations (3 ticks × 2 agents)"
        );
    }

    #[test]
    fn separated_agents_see_no_contacts() {
        let contact_count = Arc::new(AtomicUsize::new(0));

        struct CountContacts(Arc<AtomicUsize>);
        impl BehaviorModel for CountContacts {
            fn replan(
                &self,
                _a:   AgentId,
                ctx:  &SimContext<'_>,
                _rng: &mut AgentRng,
            ) -> Vec<Intent> {
                vec![Intent::WakeAt(ctx.tick + 1)]
            }

            fn on_contacts(
                &self,
                _a:       AgentId,
                contacts: &[ContactEvent],
                _ctx:     &SimContext<'_>,
                _rng:     &mut AgentRng,
            ) -> Vec<Intent> {
                self.0.fetch_add(contacts.len(), Ordering::SeqCst);
                vec![]
            }
        }

        let net = line_network(); // has nodes 0, 1, 2
        let plan = tick1_plan();
        let (store, rngs) = small_store(2);
        let mut sim = SimBuilder::new(
                test_config(4),
                store, rngs,
                CountContacts(Arc::clone(&contact_count)),
                DijkstraRouter,
            )
            .plans(vec![plan.clone(), plan])
            .network(net)
            // Agent 0 at node 0, agent 1 at node 2 — never co-located.
            .initial_positions(vec![NodeId(0), NodeId(2)])
            .build()
            .unwrap();

        sim.run(&mut NoopObserver).unwrap();
        assert_eq!(contact_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn in_transit_agent_not_in_contact_index() {
        // Agent 0 is at node 0; agent 1 starts in transit (placed, then manually
        // set in-transit so it is excluded from the contact index).
        // We verify agent 0 sees 0 contacts even though agent 1's departure_node
        // is also node 0.
        let contact_count = Arc::new(AtomicUsize::new(0));

        struct CountContacts(Arc<AtomicUsize>);
        impl BehaviorModel for CountContacts {
            fn replan(
                &self,
                _a:   AgentId,
                ctx:  &SimContext<'_>,
                _rng: &mut AgentRng,
            ) -> Vec<Intent> {
                vec![Intent::WakeAt(ctx.tick + 1)]
            }
            fn on_contacts(
                &self,
                _a:       AgentId,
                contacts: &[ContactEvent],
                _ctx:     &SimContext<'_>,
                _rng:     &mut AgentRng,
            ) -> Vec<Intent> {
                self.0.fetch_add(contacts.len(), Ordering::SeqCst);
                vec![]
            }
        }

        let net = line_network();
        let plan = tick1_plan();
        let (store, rngs) = small_store(2);
        let mut sim = SimBuilder::new(
                test_config(4),
                store, rngs,
                CountContacts(Arc::clone(&contact_count)),
                DijkstraRouter,
            )
            .plans(vec![plan, ActivityPlan::empty()])
            .network(net)
            .initial_positions(vec![NodeId(0), NodeId(0)])
            .build()
            .unwrap();

        // Manually place agent 1 in transit (departure_node = 0, in_transit = true).
        // It shares departure_node with agent 0 but should be excluded from the
        // contact index because in_transit = true.
        use dt_mobility::MovementState;
        sim.mobility.store.states[1] = MovementState {
            in_transit:       true,
            departure_node:   NodeId(0),
            destination_node: NodeId(2),
            departure_tick:   Tick(0),
            arrival_tick:     Tick(100), // won't arrive during this run
        };

        sim.run(&mut NoopObserver).unwrap();
        assert_eq!(contact_count.load(Ordering::SeqCst), 0,
            "in-transit agent should not appear in contact index");
    }
}
