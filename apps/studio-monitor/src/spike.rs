//! Studio Monitor egui spike acceptance notes.
//!
//! Decision: keep egui/eframe for the first product release unless measured
//! 1080p30 decode+texture upload fails the thresholds below on target hardware.
//!
//! Acceptance thresholds (manual / release profile):
//! - Sustained display of a local Test Patterns 1080p30 source
//! - UI remains interactive while receiving
//! - Latest-frame replacement (no unbounded queue growth)
//! - Stall overlay appears within ~2s after sender stop
//!
//! GPUI evaluation is only warranted if egui cannot meet these on
//! Windows x64 and macOS Apple Silicon.

/// Marker so the spike notes module is intentionally linked.
#[allow(dead_code)]
pub const EGUI_SPIKE_ACCEPTED_FOR_MVP: bool = true;
