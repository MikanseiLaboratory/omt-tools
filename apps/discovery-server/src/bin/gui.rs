//! OMT Discovery Server GUI.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[path = "../ui.rs"]
mod ui;

use clap::Parser;
use suite_core::{LaunchOverrides, init_tracing, t};

#[derive(Debug, Parser)]
#[command(name = "omt-discovery-server-gui", about = "OMT Discovery Server")]
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

    let title = t(overrides.language, "tool.discovery_server").to_string();
    ui::run_gpui(title, overrides.language)
}
