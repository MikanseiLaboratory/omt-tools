//! Shared settings, localization, and suite version metadata for OMT Tools.

#![deny(missing_docs)]

mod config;
mod fonts;
mod i18n;
mod theme;
mod version;

pub use config::{SuiteConfig, config_path, load_config, save_config};
#[cfg(feature = "egui-fonts")]
pub use fonts::install_egui_cjk_fonts;
pub use fonts::load_cjk_font_bytes;
pub use i18n::{Language, t};
pub use theme::ThemePreference;
pub use version::{SUITE_VERSION, SuiteManifest, ToolId, ToolInfo, suite_manifest};

/// Environment variables used when the launcher starts a tool.
pub mod env {
    /// Language override (`ja` / `en`).
    pub const LANGUAGE: &str = "OMT_TOOLS_LANGUAGE";
    /// Theme override (`light` / `dark` / `system`).
    pub const THEME: &str = "OMT_TOOLS_THEME";
    /// Suite version string passed to child tools.
    pub const SUITE_VERSION: &str = "OMT_TOOLS_SUITE_VERSION";
}

/// CLI / process launch overrides resolved from args then environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchOverrides {
    /// Selected UI language.
    pub language: Language,
    /// Selected theme preference.
    pub theme: ThemePreference,
    /// Suite version string.
    pub suite_version: String,
}

impl LaunchOverrides {
    /// Resolve overrides from process environment and optional CLI values.
    pub fn resolve(
        language: Option<Language>,
        theme: Option<ThemePreference>,
        suite_version: Option<String>,
    ) -> Self {
        let cfg = load_config().unwrap_or_default();
        let language = language
            .or_else(|| std::env::var(env::LANGUAGE).ok().and_then(|s| s.parse().ok()))
            .unwrap_or(cfg.language);
        let theme = theme
            .or_else(|| std::env::var(env::THEME).ok().and_then(|s| s.parse().ok()))
            .unwrap_or(cfg.theme);
        let suite_version = suite_version
            .or_else(|| std::env::var(env::SUITE_VERSION).ok())
            .unwrap_or_else(|| SUITE_VERSION.to_string());
        Self {
            language,
            theme,
            suite_version,
        }
    }

    /// Apply overrides as environment variables for a child process.
    pub fn apply_to_command(&self, cmd: &mut std::process::Command) {
        cmd.env(env::LANGUAGE, self.language.as_str());
        cmd.env(env::THEME, self.theme.as_str());
        cmd.env(env::SUITE_VERSION, &self.suite_version);
    }
}
