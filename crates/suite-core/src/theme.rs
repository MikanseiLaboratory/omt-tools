//! Theme preference.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// User theme preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ThemePreference {
    /// Follow OS appearance.
    #[default]
    #[serde(rename = "system")]
    System,
    /// Force light theme.
    #[serde(rename = "light")]
    Light,
    /// Force dark theme.
    #[serde(rename = "dark")]
    Dark,
}

impl ThemePreference {
    /// Wire / env / CLI token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

impl FromStr for ThemePreference {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "system" => Ok(Self::System),
            "light" => Ok(Self::Light),
            "dark" => Ok(Self::Dark),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for ThemePreference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_theme() {
        assert_eq!("dark".parse::<ThemePreference>().unwrap(), ThemePreference::Dark);
        assert_eq!(
            "system".parse::<ThemePreference>().unwrap(),
            ThemePreference::System
        );
    }
}
