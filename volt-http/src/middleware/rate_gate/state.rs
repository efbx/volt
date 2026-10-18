use std::sync::atomic::AtomicU64;

/// The state shared by every clone of a [`RateGate`](crate::middleware::RateGate), in
/// milliseconds on the gate's monotonic clock.
#[derive(Default)]
pub(super) struct State {
    /// When the gate reopens after the last trip.
    pub(super) closed_until: AtomicU64,
    /// The earliest time the next paced caller may go.
    pub(super) next_release: AtomicU64,
    /// The spacing that trips have opened, zero while the gate is not pacing.
    pub(super) trip_gap: AtomicU64,
    /// When the gate was last tripped.
    pub(super) last_trip: AtomicU64,
}
