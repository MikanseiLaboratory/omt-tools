//! Persistent suite configuration.

use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::i18n::Language;
use crate::theme::ThemePreference;

/// Errors related to config IO.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Config directory could not be resolved.
    #[error("could not resolve config directory")]
    NoConfigDir,
    /// Underlying IO failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// JSON serialize/deserialize failure.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Minimal shared preferences edited only in the launcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SuiteConfig {
    /// UI language.
    pub language: Language,
    /// Theme preference.
    pub theme: ThemePreference,
    /// Schema version for future migrations.
    pub schema_version: u32,
}

impl Default for SuiteConfig {
    fn default() -> Self {
        Self {
            language: Language::default(),
            theme: ThemePreference::default(),
            schema_version: 1,
        }
    }
}

/// Resolve the on-disk config path.
pub fn config_path() -> Result<PathBuf, ConfigError> {
    let dirs = ProjectDirs::from("lab", "Mikansei", "OMT Tools").ok_or(ConfigError::NoConfigDir)?;
    Ok(dirs.config_dir().join("config.json"))
}

/// Load config from disk, migrating unknown fields with defaults.
pub fn load_config() -> Result<SuiteConfig, ConfigError> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(SuiteConfig::default());
    }
    let raw = fs::read_to_string(&path)?;
    let mut cfg: SuiteConfig = serde_json::from_str(&raw)?;
    if cfg.schema_version == 0 {
        cfg.schema_version = 1;
    }
    Ok(cfg)
}

/// Persist config to disk, creating parent directories as needed.
pub fn save_config(cfg: &SuiteConfig) -> Result<(), ConfigError> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cfg)?;
    fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn roundtrip_defaults() {
        let _g = LOCK.lock().unwrap();
        let cfg = SuiteConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: SuiteConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, parsed);
        assert_eq!(parsed.schema_version, 1);
    }

    #[test]
    fn migrate_missing_schema() {
        let json = r#"{"language":"ja","theme":"dark"}"#;
        let cfg: SuiteConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.language, Language::Japanese);
        assert_eq!(cfg.theme, ThemePreference::Dark);
        assert_eq!(cfg.schema_version, 1);
    }
}
