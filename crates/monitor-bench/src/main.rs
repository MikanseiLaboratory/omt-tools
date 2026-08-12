//! Headless Studio Monitor present-path CLI.
//!
//! Example:
//!   cargo run --release -p monitor-bench -- --url omt://127.0.0.1:1234/source --duration 10 --backend null

use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use monitor_bench::{BenchBackend, BenchOptions, print_report, run_headless};

#[derive(Debug, Parser)]
#[command(
    name = "monitor-bench",
    about = "Headless OMT receive + present harness"
)]
struct Args {
    /// Source URL (`omt://…`).
    #[arg(long)]
    url: String,

    /// Present backend simulation.
    #[arg(long, value_enum, default_value_t = BenchBackend::Null)]
    backend: BenchBackend,

    /// Measurement window after the first frame, in seconds.
    #[arg(long, default_value_t = 10.0)]
    duration: f64,

    /// Timeout waiting for the first frame, in seconds.
    #[arg(long, default_value_t = 15.0)]
    connect_timeout: f64,

    /// Allow finishing with zero frames (CI dry-run).
    #[arg(long, default_value_t = false)]
    allow_zero_frames: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let report = run_headless(BenchOptions {
        backend: args.backend,
        url: args.url,
        duration: Duration::from_secs_f64(args.duration.max(0.1)),
        connect_timeout: Duration::from_secs_f64(args.connect_timeout.max(0.1)),
        allow_zero_frames: args.allow_zero_frames,
    })?;
    print_report(&report);
    Ok(())
}
