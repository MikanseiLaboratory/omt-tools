//! Localization helpers (Japanese / English).

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Supported UI languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Language {
    /// English.
    #[default]
    #[serde(rename = "en", alias = "english")]
    English,
    /// Japanese.
    #[serde(rename = "ja", alias = "japanese", alias = "jp")]
    Japanese,
}

impl Language {
    /// Wire / env / CLI token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Japanese => "ja",
        }
    }

    /// Human-readable label in the language itself.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Japanese => "日本語",
        }
    }
}

impl FromStr for Language {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "en" | "english" => Ok(Self::English),
            "ja" | "jp" | "japanese" => Ok(Self::Japanese),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Translate a message key for the given language.
pub fn t(lang: Language, key: &str) -> &'static str {
    match (lang, key) {
        // Launcher
        (Language::English, "app.title") => "OMT Tools",
        (Language::Japanese, "app.title") => "OMT Tools",
        (Language::English, "launcher.subtitle") => "Open Media Transport production utilities",
        (Language::Japanese, "launcher.subtitle") => "Open Media Transport 配信用ユーティリティ",
        (Language::English, "docs") => "Docs & Guides",
        (Language::Japanese, "docs") => "ドキュメント",
        (Language::English, "save") => "Save",
        (Language::Japanese, "save") => "保存",
        (Language::English, "settings") => "Settings",
        (Language::Japanese, "settings") => "設定",
        (Language::English, "language") => "Language",
        (Language::Japanese, "language") => "言語",
        (Language::English, "theme") => "Theme",
        (Language::Japanese, "theme") => "テーマ",
        (Language::English, "version") => "Version",
        (Language::Japanese, "version") => "バージョン",
        (Language::English, "launch") => "Launch",
        (Language::Japanese, "launch") => "起動",
        (Language::English, "back") => "Back",
        (Language::Japanese, "back") => "戻る",
        (Language::English, "theme.light") => "Light",
        (Language::Japanese, "theme.light") => "ライト",
        (Language::English, "theme.dark") => "Dark",
        (Language::Japanese, "theme.dark") => "ダーク",
        (Language::English, "theme.system") => "System",
        (Language::Japanese, "theme.system") => "システム",

        // Tools
        (Language::English, "tool.studio_monitor") => "Studio Monitor",
        (Language::Japanese, "tool.studio_monitor") => "Studio Monitor",
        (Language::English, "tool.studio_monitor.desc") => {
            "Browse and view OMT sources on the LAN"
        }
        (Language::Japanese, "tool.studio_monitor.desc") => {
            "LAN上のOMTソースを一覧表示・プレビュー"
        }
        (Language::English, "tool.test_patterns") => "Test Patterns",
        (Language::Japanese, "tool.test_patterns") => "Test Patterns",
        (Language::English, "tool.test_patterns.desc") => {
            "Send SMPTE-style video and tone over OMT"
        }
        (Language::Japanese, "tool.test_patterns.desc") => {
            "SMPTE系テスト映像とトーンをOMTで送出"
        }
        (Language::English, "tool.screen_capture") => "Screen Capture",
        (Language::Japanese, "tool.screen_capture") => "Screen Capture",
        (Language::English, "tool.screen_capture.desc") => {
            "Capture the desktop and send over OMT (preview)"
        }
        (Language::Japanese, "tool.screen_capture.desc") => {
            "デスクトップをキャプチャしてOMT送出（プレビュー）"
        }
        (Language::English, "tool.unavailable") => "Not installed",
        (Language::Japanese, "tool.unavailable") => "未インストール",

        // Monitor
        (Language::English, "monitor.sources") => "Sources",
        (Language::Japanese, "monitor.sources") => "ソース",
        (Language::English, "monitor.refresh") => "Refresh",
        (Language::Japanese, "monitor.refresh") => "更新",
        (Language::English, "monitor.no_sources") => "No OMT sources discovered",
        (Language::Japanese, "monitor.no_sources") => "OMTソースが見つかりません",
        (Language::English, "monitor.waiting") => "Waiting for video…",
        (Language::Japanese, "monitor.waiting") => "映像待機中…",
        (Language::English, "monitor.stalled") => "SIGNAL LOST",
        (Language::Japanese, "monitor.stalled") => "信号途絶",
        (Language::English, "monitor.alpha_mask") => "Alpha mask",
        (Language::Japanese, "monitor.alpha_mask") => "アルファマスク",
        (Language::English, "monitor.checkerboard") => "Checkerboard",
        (Language::Japanese, "monitor.checkerboard") => "チェッカーボード",
        (Language::English, "monitor.fit") => "Fit",
        (Language::Japanese, "monitor.fit") => "フィット",
        (Language::English, "monitor.fill") => "Fill",
        (Language::Japanese, "monitor.fill") => "フィル",

        // Patterns
        (Language::English, "patterns.start") => "Start",
        (Language::Japanese, "patterns.start") => "開始",
        (Language::English, "patterns.stop") => "Stop",
        (Language::Japanese, "patterns.stop") => "停止",
        (Language::English, "patterns.name") => "Source name",
        (Language::Japanese, "patterns.name") => "ソース名",
        (Language::English, "patterns.pattern") => "Pattern",
        (Language::Japanese, "patterns.pattern") => "パターン",
        (Language::English, "patterns.animate") => "Animate",
        (Language::Japanese, "patterns.animate") => "アニメーション",
        (Language::English, "patterns.tone") => "Tone (Hz)",
        (Language::Japanese, "patterns.tone") => "トーン (Hz)",
        (Language::English, "patterns.resolution") => "Resolution",
        (Language::Japanese, "patterns.resolution") => "解像度",
        (Language::English, "patterns.fps") => "Frame rate",
        (Language::Japanese, "patterns.fps") => "フレームレート",
        (Language::English, "patterns.profile") => "VMX profile",
        (Language::Japanese, "patterns.profile") => "VMXプロファイル",
        (Language::English, "patterns.image") => "Image file",
        (Language::Japanese, "patterns.image") => "画像ファイル",
        (Language::English, "patterns.sending") => "Sending",
        (Language::Japanese, "patterns.sending") => "送出中",
        (Language::English, "patterns.idle") => "Idle",
        (Language::Japanese, "patterns.idle") => "停止中",
        (Language::English, "patterns.perf_warn") => {
            "Encode cannot sustain target FPS — use Release build or lower quality"
        }
        (Language::Japanese, "patterns.perf_warn") => {
            "目標FPSを維持できません — Releaseビルドまたは低品質設定を使用してください"
        }

        _ => "???",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_language() {
        assert_eq!("ja".parse::<Language>().unwrap(), Language::Japanese);
        assert_eq!("en".parse::<Language>().unwrap(), Language::English);
    }

    #[test]
    fn required_keys_present() {
        let keys = [
            "app.title",
            "settings",
            "docs",
            "save",
            "language",
            "theme",
            "version",
            "tool.studio_monitor",
            "tool.test_patterns",
            "monitor.stalled",
            "patterns.start",
        ];
        for key in keys {
            assert_ne!(t(Language::English, key), key, "missing en key {key}");
            assert_ne!(t(Language::Japanese, key), key, "missing ja key {key}");
        }
    }
}
