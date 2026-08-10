//! Headless Studio Monitor present-path harness.

#![deny(missing_docs)]

use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use clap::ValueEnum;
use omt_media::{ReceiveWorker, StallState};
use openmediatransport::bgra_to_rgba;
use serde::Serialize;

/// Present-path backend under test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BenchBackend {
    /// GPUI image present simulation.
    Gpui,
    /// Conversion only (baseline, no present simulation).
    Null,
}

impl BenchBackend {
    /// Stable CLI / report id.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Gpui => "gpui",
            Self::Null => "null",
        }
    }
}

/// Options for a headless receive+present run.
#[derive(Debug, Clone)]
pub struct BenchOptions {
    /// Backend label used in the report.
    pub backend: BenchBackend,
    /// Source URL (`omt://…`).
    pub url: String,
    /// How long to run after the first frame (or connect if `allow_zero_frames`).
    pub duration: Duration,
    /// Give up waiting for the first frame after this timeout.
    pub connect_timeout: Duration,
    /// Allow finishing with zero frames (useful for dry CI).
    pub allow_zero_frames: bool,
}

/// One completed headless run.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchReport {
    /// Backend id.
    pub backend: String,
    /// Source URL.
    pub url: String,
    /// Wall duration measured after first frame (or total if none).
    pub duration_ms: u64,
    /// Decoded frames processed.
    pub frames: u64,
    /// Average FPS over the measured window.
    pub fps: f64,
    /// Mean present-path cost in microseconds (convert + backend simulate).
    pub present_us_avg: f64,
    /// p95 inter-frame gap in milliseconds (receive cadence).
    pub inter_frame_p95_ms: f64,
    /// Last observed resolution.
    pub width: u32,
    /// Last observed resolution.
    pub height: u32,
    /// Final stall detector state.
    pub stall: String,
    /// Bytes of the last RGBA buffer produced (present working set proxy).
    pub last_rgba_bytes: usize,
}

/// Run a headless receive loop with a backend-specific present simulation.
pub fn run_headless(opts: BenchOptions) -> Result<BenchReport> {
    let worker = ReceiveWorker::spawn();
    worker.connect(opts.url.clone());

    let deadline_first = Instant::now() + opts.connect_timeout;
    let mut first_at: Option<Instant> = None;
    let mut end_at = Instant::now() + opts.duration;
    let mut frames = 0u64;
    let mut present_us_acc = 0u64;
    let mut gaps_ms: Vec<f64> = Vec::new();
    let mut last_frame_at: Option<Instant> = None;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut last_rgba_bytes = 0usize;
    let mut rgba_scratch = Vec::new();

    loop {
        let now = Instant::now();
        if first_at.is_some() && now >= end_at {
            break;
        }
        if first_at.is_none() && now >= deadline_first {
            if opts.allow_zero_frames {
                break;
            }
            bail!(
                "no video frames within {:?} from {}",
                opts.connect_timeout,
                opts.url
            );
        }

        if let Some(frame) = worker.latest().take() {
            let arrived = Instant::now();
            if let Some(prev) = last_frame_at {
                gaps_ms.push((arrived - prev).as_secs_f64() * 1000.0);
            }
            last_frame_at = Some(arrived);

            let t0 = Instant::now();
            present_frame(opts.backend, &frame.bgra, &mut rgba_scratch);
            present_us_acc += t0.elapsed().as_micros() as u64;
            last_rgba_bytes = rgba_scratch.len();
            width = frame.width;
            height = frame.height;
            frames += 1;

            if first_at.is_none() {
                first_at = Some(arrived);
                end_at = arrived + opts.duration;
            }
        } else {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    let measured = match first_at {
        Some(start) => start.elapsed(),
        None => opts.connect_timeout,
    };
    let fps = if measured.as_secs_f64() > 0.0 {
        frames as f64 / measured.as_secs_f64()
    } else {
        0.0
    };
    let present_us_avg = if frames > 0 {
        present_us_acc as f64 / frames as f64
    } else {
        0.0
    };
    let inter_frame_p95_ms = percentile_p95(&mut gaps_ms);

    let stall = {
        let guard = worker.stall();
        let mut d = guard.lock();
        match d.tick() {
            StallState::Waiting => "waiting",
            StallState::Live => "live",
            StallState::Stalled => "stalled",
        }
    };

    worker.shutdown();

    Ok(BenchReport {
        backend: opts.backend.id().to_string(),
        url: opts.url,
        duration_ms: measured.as_millis() as u64,
        frames,
        fps,
        present_us_avg,
        inter_frame_p95_ms,
        width,
        height,
        stall: stall.to_string(),
        last_rgba_bytes,
    })
}

fn present_frame(backend: BenchBackend, bgra: &[u8], rgba: &mut Vec<u8>) {
    // Shared convert step — UI toolkits ultimately need RGBA/texture-friendly pixels.
    *rgba = bgra_to_rgba(bgra);
    match backend {
        BenchBackend::Null => {}
        BenchBackend::Gpui => {
            // Approximate GPUI image element upload: stride-aware copy into an aligned staging buf.
            let aligned = (rgba.len() + 255) & !255;
            let mut staging = vec![0u8; aligned];
            staging[..rgba.len()].copy_from_slice(rgba);
            std::hint::black_box(staging);
        }
    }
}

fn percentile_p95(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((values.len() as f64) * 0.95).ceil() as usize;
    let idx = idx.saturating_sub(1).min(values.len() - 1);
    values[idx]
}

/// Print a single-line human summary plus JSON on stdout.
pub fn print_report(report: &BenchReport) {
    println!(
        "backend={} frames={} fps={:.2} present_us_avg={:.1} inter_frame_p95_ms={:.2} size={}x{} stall={}",
        report.backend,
        report.frames,
        report.fps,
        report.present_us_avg,
        report.inter_frame_p95_ms,
        report.width,
        report.height,
        report.stall
    );
    if let Ok(json) = serde_json::to_string(report) {
        println!("json={json}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p95_empty() {
        assert_eq!(percentile_p95(&mut []), 0.0);
    }

    #[test]
    fn backend_ids() {
        assert_eq!(BenchBackend::Gpui.id(), "gpui");
        assert_eq!(BenchBackend::Null.id(), "null");
    }
}
