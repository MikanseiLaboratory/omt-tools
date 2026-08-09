//! OMT Studio Monitor (GPUI).
//!
//! Headless performance run:
//! ```text
//! cargo run --release -p omt-studio-monitor -- --headless --url omt://... --seconds 10
//! ```

mod ui;

use std::time::Duration;

use clap::Parser;
use monitor_bench::{BenchBackend, BenchOptions, print_report, run_headless};
use suite_core::{LaunchOverrides, t};

#[derive(Debug, Parser)]
#[command(name = "omt-studio-monitor", about = "OMT Studio Monitor")]
struct Args {
    /// UI language (`en` / `ja`).
    #[arg(long)]
    language: Option<String>,
    /// Theme (`light` / `dark` / `system`).
    #[arg(long)]
    theme: Option<String>,
    /// Optional initial `omt://` URL.
    #[arg(long)]
    url: Option<String>,
    /// Run without opening a window (performance harness).
    #[arg(long, default_value_t = false)]
    headless: bool,
    /// Headless duration in seconds after the first frame.
    #[arg(long, default_value_t = 10)]
    seconds: u64,
    /// Seconds to wait for the first video frame in headless mode.
    #[arg(long, default_value_t = 15)]
    connect_timeout: u64,
    /// Allow zero frames (CI dry-run).
    #[arg(long, default_value_t = false)]
    allow_zero_frames: bool,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let overrides = LaunchOverrides::resolve(
        args.language.as_deref().and_then(|s| s.parse().ok()),
        args.theme.as_deref().and_then(|s| s.parse().ok()),
        None,
    );

    if args.headless {
        let url = args
            .url
            .clone()
            .ok_or_else(|| anyhow::anyhow!("--headless requires --url omt://..."))?;
        let report = run_headless(BenchOptions {
            backend: BenchBackend::Gpui,
            url,
            duration: Duration::from_secs(args.seconds),
            connect_timeout: Duration::from_secs(args.connect_timeout),
            allow_zero_frames: args.allow_zero_frames,
        })?;
        print_report(&report);
        return Ok(());
    }

    let title = t(overrides.language, "tool.studio_monitor").to_string();
    ui::run_gpui(title, overrides.language, args.url)
}
