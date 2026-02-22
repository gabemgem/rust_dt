/** Domain types matching the Python backend JSON shapes. */

export interface AgentSnapshot {
  agent_id: number;
  tick: number;
  departure_node: number;
  in_transit: boolean;
  destination_node: number;
}

export interface TickSummary {
  tick: number;
  unix_time_secs: number;
  woken_agents: number;
}

export interface NodeCoord {
  node_id: number;
  lat: number;
  lon: number;
}

export interface NetworkEdge {
  from_node: number;
  to_node: number;
}

export interface Manifest {
  available_ticks: number[];
  agent_count: number;
  tick_duration_secs: number;
}

export type PlaybackMode = 'load' | 'live';
