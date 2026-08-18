//! OMT Config Manager (GPUI).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod model;
mod ui;

use clap::Parser;
use suite_core::{LaunchOverrides, init_tracing, t};

#[derive(Debug, Parser)]
#[command(name = "omt-config-manager", about = "OMT Config Manager")]
struct Args {
    /// UI language (`en` / `ja`).
    #[arg(long)]
    language: Option<String>,
    /// Theme (`light` / `dark` / `system`).
    #[arg(long)]
    theme: Option<String>,
}

fn main() -> anyhow::Result<()> {
    init_tracing();

    let args = Args::parse();
    let overrides = LaunchOverrides::resolve(
        args.language.as_deref().and_then(|s| s.parse().ok()),
        args.theme.as_deref().and_then(|s| s.parse().ok()),
        None,
    );

    let title = t(overrides.language, "tool.config_manager").to_string();
    ui::run_gpui(title, overrides.language)
}
