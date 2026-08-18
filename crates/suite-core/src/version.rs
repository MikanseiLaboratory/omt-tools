//! Suite and tool version metadata.

use serde::{Deserialize, Serialize};

/// Current suite version (aligned with workspace package version).
pub const SUITE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Identifiers for bundled tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolId {
    /// Studio Monitor viewer.
    StudioMonitor,
    /// Test Patterns sender.
    TestPatterns,
    /// Screen Capture sender (optional / preview).
    ScreenCapture,
}

impl ToolId {
    /// Stable sidecar / binary stem name.
    pub const fn binary_name(self) -> &'static str {
        match self {
            Self::StudioMonitor => "omt-studio-monitor",
            Self::TestPatterns => "omt-test-patterns",
            Self::ScreenCapture => "omt-screen-capture",
        }
    }

    /// i18n key for the display title.
    pub const fn title_key(self) -> &'static str {
        match self {
            Self::StudioMonitor => "tool.studio_monitor",
            Self::TestPatterns => "tool.test_patterns",
            Self::ScreenCapture => "tool.screen_capture",
        }
    }

    /// i18n key for the short description.
    pub const fn description_key(self) -> &'static str {
        match self {
            Self::StudioMonitor => "tool.studio_monitor.desc",
            Self::TestPatterns => "tool.test_patterns.desc",
            Self::ScreenCapture => "tool.screen_capture.desc",
        }
    }

    /// All known tools in launcher order.
    pub const fn all() -> &'static [ToolId] {
        &[Self::StudioMonitor, Self::TestPatterns, Self::ScreenCapture]
    }

    /// Start Menu / Applications / `.desktop` display name (English, OS chrome).
    pub const fn os_entry_name(self) -> &'static str {
        match self {
            Self::StudioMonitor => "Studio Monitor",
            Self::TestPatterns => "Test Patterns",
            Self::ScreenCapture => "Screen Capture",
        }
    }

    /// Prefixed name used where apps share a global namespace (Launchpad, GNOME).
    pub const fn os_entry_name_qualified(self) -> &'static str {
        match self {
            Self::StudioMonitor => "OMT Studio Monitor",
            Self::TestPatterns => "OMT Test Patterns",
            Self::ScreenCapture => "OMT Screen Capture",
        }
    }

    /// Stable OS integration id (`lab.mikansei.omt-tools.<id>`).
    pub const fn os_entry_id(self) -> &'static str {
        match self {
            Self::StudioMonitor => "studio-monitor",
            Self::TestPatterns => "test-patterns",
            Self::ScreenCapture => "screen-capture",
        }
    }
}

/// Per-tool version entry in the suite manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInfo {
    /// Tool id.
    pub id: ToolId,
    /// Tool version string.
    pub version: String,
    /// Relative binary name.
    pub binary: String,
    /// Whether the tool is considered release-ready.
    pub enabled: bool,
}

/// Suite-wide manifest shown in settings and used by the updater.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuiteManifest {
    /// Suite product version.
    pub suite_version: String,
    /// Target triple used for this build when known.
    pub target: String,
    /// Bundled tools.
    pub tools: Vec<ToolInfo>,
}

/// Build the default in-tree suite manifest.
pub fn suite_manifest() -> SuiteManifest {
    SuiteManifest {
        suite_version: SUITE_VERSION.to_string(),
        target: std::env::consts::ARCH.to_string(),
        tools: vec![
            ToolInfo {
                id: ToolId::StudioMonitor,
                version: SUITE_VERSION.to_string(),
                binary: ToolId::StudioMonitor.binary_name().to_string(),
                enabled: true,
            },
            ToolInfo {
                id: ToolId::TestPatterns,
                version: SUITE_VERSION.to_string(),
                binary: ToolId::TestPatterns.binary_name().to_string(),
                enabled: true,
            },
            ToolInfo {
                id: ToolId::ScreenCapture,
                version: SUITE_VERSION.to_string(),
                binary: ToolId::ScreenCapture.binary_name().to_string(),
                enabled: false,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_contains_core_tools() {
        let m = suite_manifest();
        assert_eq!(m.suite_version, SUITE_VERSION);
        assert!(
            m.tools
                .iter()
                .any(|t| t.id == ToolId::StudioMonitor && t.enabled)
        );
        assert!(
            m.tools
                .iter()
                .any(|t| t.id == ToolId::TestPatterns && t.enabled)
        );
    }

    #[test]
    fn os_entry_names_are_stable() {
        assert_eq!(ToolId::StudioMonitor.os_entry_id(), "studio-monitor");
        assert_eq!(ToolId::TestPatterns.os_entry_name(), "Test Patterns");
        assert_eq!(
            ToolId::StudioMonitor.os_entry_name_qualified(),
            "OMT Studio Monitor"
        );
    }
}
