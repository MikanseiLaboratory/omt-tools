//! Application chrome colors for dark / light Studio Monitor themes.

use gpui::WindowAppearance;
use suite_core::ThemePreference;

/// Resolved UI palette for the current theme preference.
#[derive(Debug, Clone, Copy)]
pub struct UiChrome {
    /// App / letterbox background.
    pub bg: u32,
    /// Sidebar / stats panel background.
    pub panel: u32,
    /// Slightly elevated surface (rows, buttons).
    pub surface: u32,
    /// Selected / hover surface.
    pub surface_active: u32,
    /// Hairline borders.
    pub border: u32,
    /// Primary text.
    pub text: u32,
    /// Secondary / muted text.
    pub text_muted: u32,
    /// Accent (selection, live affordances).
    pub accent: u32,
    /// Soft accent wash behind selected rows.
    pub accent_soft: u32,
}

impl UiChrome {
    /// Resolve palette from preference + OS window appearance.
    pub fn resolve(pref: ThemePreference, appearance: WindowAppearance) -> Self {
        let dark = match pref {
            ThemePreference::Dark => true,
            ThemePreference::Light => false,
            ThemePreference::System => matches!(
                appearance,
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
        };
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }

    fn dark() -> Self {
        Self {
            bg: 0x0e1116,
            panel: 0x141a22,
            surface: 0x1a222d,
            surface_active: 0x243041,
            border: 0x2a3340,
            text: 0xedf2f7,
            text_muted: 0x8b9aab,
            accent: 0x3d9cf0,
            accent_soft: 0x1a3048,
        }
    }

    fn light() -> Self {
        Self {
            bg: 0xf3f5f8,
            panel: 0xffffff,
            surface: 0xeef1f5,
            surface_active: 0xe2e8f0,
            border: 0xd0d7e0,
            text: 0x1a2330,
            text_muted: 0x5c6b7a,
            accent: 0x1f6feb,
            accent_soft: 0xdbe8f8,
        }
    }
}
