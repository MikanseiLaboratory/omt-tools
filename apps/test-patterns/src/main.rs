//! OMT Test Patterns sender application.

mod app;

use clap::Parser;
use suite_core::{LaunchOverrides, ThemePreference, t};

#[derive(Debug, Parser)]
#[command(name = "omt-test-patterns", about = "OMT Test Patterns")]
struct Args {
    #[arg(long)]
    language: Option<String>,
    #[arg(long)]
    theme: Option<String>,
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

    let title = t(overrides.language, "tool.test_patterns");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([720.0, 640.0])
            .with_title(title),
        ..Default::default()
    };

    eframe::run_native(
        title,
        options,
        Box::new(move |cc| {
            apply_theme(&cc.egui_ctx, overrides.theme);
            Ok(Box::new(app::PatternsApp::new(cc, overrides.language)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe error: {e}"))
}

fn apply_theme(ctx: &egui::Context, theme: ThemePreference) {
    match theme {
        ThemePreference::Light => ctx.set_visuals(egui::Visuals::light()),
        ThemePreference::Dark => ctx.set_visuals(egui::Visuals::dark()),
        ThemePreference::System => {}
    }
}
