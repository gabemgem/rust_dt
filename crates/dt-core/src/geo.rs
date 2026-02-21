//! Geographic coordinate type and spatial utilities.
//!
//! `GeoPoint` uses `f32` (single-precision) latitude/longitude.  At the
//! equator this gives ~1 m precision — more than sufficient for city-scale
//! simulation while halving memory consumption vs. `f64`.

/// A WGS-84 geographic coordinate stored as single-precision floats.
#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GeoPoint {
    pub lat: f32,
    pub lon: f32,
}

impl GeoPoint {
    #[inline]
    pub fn new(lat: f32, lon: f32) -> Self {
        Self { lat, lon }
    }

    /// Haversine great-circle distance in metres.
    ///
    /// Accuracy: ±0.5 % (f32 rounding); suitable for routing and contact
    /// detection at city scale.  Use f64 Vincenty if sub-metre fidelity is
    /// ever required.
    pub fn distance_m(self, other: GeoPoint) -> f32 {
        const R: f32 = 6_371_000.0; // mean Earth radius, metres

        let d_lat = (other.lat - self.lat).to_radians();
        let d_lon = (other.lon - self.lon).to_radians();

        let lat1 = self.lat.to_radians();
        let lat2 = other.lat.to_radians();

        let a = (d_lat * 0.5).sin().powi(2)
            + lat1.cos() * lat2.cos() * (d_lon * 0.5).sin().powi(2);

        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
        R * c
    }

    /// Approximate bounding-box check — much cheaper than `distance_m` for
    /// quick rejection before contact detection.
    #[inline]
    pub fn within_bbox(self, center: GeoPoint, half_deg: f32) -> bool {
        (self.lat - center.lat).abs() <= half_deg
            && (self.lon - center.lon).abs() <= half_deg
    }
}

impl std::fmt::Display for GeoPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({:.6}, {:.6})", self.lat, self.lon)
    }
}
