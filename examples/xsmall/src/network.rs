//! Shared Mobile, AL road network definition.
//!
//! A 5-node synthetic network inspired by the geography of Mobile, Alabama.
//! Both `mobile_al` (the sim) and `export_nodes` (the sidecar) call this.

use dt_core::{GeoPoint, NodeId};
use dt_spatial::{RoadNetwork, RoadNetworkBuilder};

/// Build the 5-node Mobile, AL–inspired road network.
///
/// Returns `(network, [north_residential, south_residential, downtown,
/// commerce_park, connector])`.
pub fn build_network() -> (RoadNetwork, [NodeId; 5]) {
    let mut b = RoadNetworkBuilder::new();

    let north_residential = b.add_node(GeoPoint::new(30.710, -88.070));
    let south_residential = b.add_node(GeoPoint::new(30.670, -88.030));
    let downtown          = b.add_node(GeoPoint::new(30.695, -88.050));
    let commerce_park     = b.add_node(GeoPoint::new(30.700, -88.030));
    let connector         = b.add_node(GeoPoint::new(30.680, -88.060));

    // Bidirectional roads, ~45 km/h urban speed.
    b.add_road(north_residential, downtown,      2_500.0, 200_000);
    b.add_road(north_residential, connector,     1_500.0, 120_000);
    b.add_road(connector,         downtown,      1_000.0,  80_000);
    b.add_road(south_residential, connector,     1_500.0, 120_000);
    b.add_road(south_residential, commerce_park, 2_000.0, 160_000);
    b.add_road(downtown,          commerce_park, 2_000.0, 160_000);

    let net = b.build();
    (net, [north_residential, south_residential, downtown, commerce_park, connector])
}
