//! Persistent suite and per-app configuration.

use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::de::DeserializeOwned;
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

/// Shared suite preferences (language / theme).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SuiteConfig {
    /// UI language.
    pub language: Language,
    /// Theme preference.
    pub theme: ThemePreference,
    /// Schema version.
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

/// Test Patterns tool preferences (`test-patterns.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TestPatternsConfig {
    /// Schema version.
    pub schema_version: u32,
    /// User-registered still-image paths (loaded at startup).
    pub custom_images: Vec<PathBuf>,
    /// Prefetched video frames for paced OMT send (1..=16, default 3).
    pub frame_buffer_frames: u32,
}

impl Default for TestPatternsConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            custom_images: Vec::new(),
            frame_buffer_frames: 3,
        }
    }
}

/// Studio Monitor tool preferences (`studio-monitor.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StudioMonitorConfig {
    /// Schema version.
    pub schema_version: u32,
}

impl Default for StudioMonitorConfig {
    fn default() -> Self {
        Self { schema_version: 1 }
    }
}

/// Launcher preferences (`launcher.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LauncherConfig {
    /// Schema version.
    pub schema_version: u32,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self { schema_version: 1 }
    }
}

/// Resolve the suite config directory.
pub fn config_dir() -> Result<PathBuf, ConfigError> {
    let dirs = ProjectDirs::from("lab", "Mikansei", "OMT Tools").ok_or(ConfigError::NoConfigDir)?;
    Ok(dirs.config_dir().to_path_buf())
}

/// Path to the shared suite config (`suite.json`).
pub fn config_path() -> Result<PathBuf, ConfigError> {
    Ok(config_dir()?.join("suite.json"))
}

/// Path to a per-app config file (`{app_id}.json`).
pub fn app_config_path(app_id: &str) -> Result<PathBuf, ConfigError> {
    Ok(config_dir()?.join(format!("{app_id}.json")))
}

/// Load shared suite config.
pub fn load_config() -> Result<SuiteConfig, ConfigError> {
    load_json_file(&config_path()?)
}

/// Persist shared suite config.
pub fn save_config(cfg: &SuiteConfig) -> Result<(), ConfigError> {
    save_json_file(&config_path()?, cfg)
}

/// Load Test Patterns preferences.
pub fn load_test_patterns_config() -> Result<TestPatternsConfig, ConfigError> {
    load_json_file(&app_config_path("test-patterns")?)
}

/// Persist Test Patterns preferences.
pub fn save_test_patterns_config(cfg: &TestPatternsConfig) -> Result<(), ConfigError> {
    save_json_file(&app_config_path("test-patterns")?, cfg)
}

/// Load Studio Monitor preferences.
pub fn load_studio_monitor_config() -> Result<StudioMonitorConfig, ConfigError> {
    load_json_file(&app_config_path("studio-monitor")?)
}

/// Persist Studio Monitor preferences.
pub fn save_studio_monitor_config(cfg: &StudioMonitorConfig) -> Result<(), ConfigError> {
    save_json_file(&app_config_path("studio-monitor")?, cfg)
}

/// Load Launcher preferences.
pub fn load_launcher_config() -> Result<LauncherConfig, ConfigError> {
    load_json_file(&app_config_path("launcher")?)
}

/// Persist Launcher preferences.
pub fn save_launcher_config(cfg: &LauncherConfig) -> Result<(), ConfigError> {
    save_json_file(&app_config_path("launcher")?, cfg)
}

fn load_json_file<T: DeserializeOwned + Default>(path: &Path) -> Result<T, ConfigError> {
    if !path.exists() {
        return Ok(T::default());
    }
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

fn save_json_file<T: Serialize>(path: &Path, value: &T) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)?;
    fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_suite_defaults() {
        let cfg = SuiteConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: SuiteConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, parsed);
        assert_eq!(parsed.schema_version, 1);
    }

    #[test]
    fn roundtrip_custom_images() {
        let cfg = TestPatternsConfig {
            schema_version: 1,
            custom_images: vec![PathBuf::from("C:/images/bars.png")],
            frame_buffer_frames: 5,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: TestPatternsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.custom_images.len(), 1);
        assert_eq!(parsed.custom_images[0], PathBuf::from("C:/images/bars.png"));
        assert_eq!(parsed.frame_buffer_frames, 5);
    }

    #[test]
    fn legacy_test_patterns_json_defaults_frame_buffer() {
        let parsed: TestPatternsConfig =
            serde_json::from_str(r#"{"schema_version":1,"custom_images":[]}"#).unwrap();
        assert_eq!(parsed.frame_buffer_frames, 3);
    }

    #[test]
    fn app_config_paths() {
        let dir = config_dir().unwrap();
        assert_eq!(config_path().unwrap(), dir.join("suite.json"));
        assert_eq!(
            app_config_path("test-patterns").unwrap(),
            dir.join("test-patterns.json")
        );
        assert_eq!(
            app_config_path("studio-monitor").unwrap(),
            dir.join("studio-monitor.json")
        );
        assert_eq!(
            app_config_path("launcher").unwrap(),
            dir.join("launcher.json")
        );
    }
}
