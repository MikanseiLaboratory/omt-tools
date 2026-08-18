//! Pattern presets, image helpers, and layout constants.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, RenderImage, SharedString};
use image::{Frame, ImageBuffer, Rgba};
use openmediatransport::{Quality, uyvy_to_rgba};
use pattern_generator::{PatternKind, fill_uyvy};
use smallvec::smallvec;
use suite_core::{Language, t};

pub(crate) const THUMB_W: i32 = 320;
pub(crate) const THUMB_H: i32 = 180;
pub(crate) const PREVIEW_W: i32 = 240;
pub(crate) const PREVIEW_H: i32 = 135;
pub(crate) const TILE_W: f32 = 220.0;
pub(crate) const SIDE_PANEL_W: f32 = 360.0;
pub(crate) const SIDE_PANEL_MIN_W: f32 = 240.0;
pub(crate) const SIDE_PANEL_MAX_W: f32 = 520.0;
pub(crate) const SPLITTER_HIT: f32 = 6.0;
/// Bottom padding under the output preview so it is not flush with the window edge.
pub(crate) const PREVIEW_BOTTOM_MARGIN: f32 = 4.0;

pub(crate) fn clamp_side_panel_w(width: f32) -> f32 {
    if !width.is_finite() || width <= 0.0 {
        return SIDE_PANEL_W;
    }
    width.clamp(SIDE_PANEL_MIN_W, SIDE_PANEL_MAX_W)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameRate {
    pub(crate) n: i32,
    pub(crate) d: i32,
}

impl FrameRate {
    pub(crate) const PRESETS: &[FrameRate] = &[
        FrameRate {
            n: 24_000,
            d: 1_001,
        },
        FrameRate { n: 24, d: 1 },
        FrameRate { n: 25, d: 1 },
        FrameRate {
            n: 30_000,
            d: 1_001,
        },
        FrameRate { n: 30, d: 1 },
        FrameRate { n: 50, d: 1 },
        FrameRate {
            n: 60_000,
            d: 1_001,
        },
        FrameRate { n: 60, d: 1 },
    ];

    pub(crate) fn label(self) -> String {
        let v = self.n as f64 / self.d.max(1) as f64;
        if self.d == 1 {
            format!("{v:.0}")
        } else {
            format!("{v:.2}")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TonePreset {
    Mute,
    Hz(f32),
}

impl TonePreset {
    pub(crate) const PRESETS: &[TonePreset] = &[
        TonePreset::Mute,
        TonePreset::Hz(440.0),
        TonePreset::Hz(1000.0),
        TonePreset::Hz(2000.0),
    ];

    pub(crate) fn hz(self) -> f32 {
        match self {
            Self::Mute => 0.0,
            Self::Hz(v) => v,
        }
    }

    pub(crate) fn label(self, language: Language) -> SharedString {
        match self {
            Self::Mute => SharedString::from(t(language, "patterns.tone_mute")),
            Self::Hz(v) => SharedString::from(format!("{v:.0} Hz")),
        }
    }

    pub(crate) fn matches(self, tone_hz: f32) -> bool {
        match self {
            Self::Mute => tone_hz <= 0.0,
            Self::Hz(v) => (tone_hz - v).abs() < 0.5,
        }
    }
}

/// Discrete tone level presets (dBFS).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LevelPreset(f32);

impl LevelPreset {
    pub(crate) const PRESETS: &[LevelPreset] = &[
        LevelPreset(0.0),
        LevelPreset(-6.0),
        LevelPreset(-10.0),
        LevelPreset(-20.0),
    ];

    pub(crate) fn dbfs(self) -> f32 {
        self.0
    }

    pub(crate) fn label(self) -> SharedString {
        if self.0 == 0.0 {
            SharedString::from("0 dBFS")
        } else {
            SharedString::from(format!("{} dBFS", self.0 as i32))
        }
    }

    pub(crate) fn matches(self, level_dbfs: f32) -> bool {
        (level_dbfs - self.0).abs() < 0.05
    }

    pub(crate) fn nearest(level_dbfs: f32) -> LevelPreset {
        let mut best = Self::PRESETS[3];
        let mut best_dist = (best.0 - level_dbfs).abs();
        for preset in Self::PRESETS {
            let dist = (preset.0 - level_dbfs).abs();
            if dist < best_dist {
                best = *preset;
                best_dist = dist;
            }
        }
        best
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Resolution {
    pub(crate) width: i32,
    pub(crate) height: i32,
}

impl Resolution {
    pub(crate) const PRESETS: &[Resolution] = &[
        Resolution {
            width: 1280,
            height: 720,
        },
        Resolution {
            width: 1920,
            height: 1080,
        },
        Resolution {
            width: 3840,
            height: 2160,
        },
    ];

    pub(crate) fn label(self) -> String {
        format!("{}×{}", self.width, self.height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuKind {
    Resolution,
    Tone,
    Fps,
    Level,
}

pub(crate) fn tone_label(language: Language, tone_hz: f32) -> SharedString {
    if tone_hz <= 0.0 {
        SharedString::from(t(language, "patterns.tone_mute"))
    } else {
        SharedString::from(format!("{tone_hz:.0} Hz"))
    }
}

pub(crate) struct CustomImage {
    pub(crate) path: PathBuf,
    pub(crate) thumb: Option<Arc<RenderImage>>,
}

/// System UI font plus CJK fallbacks (GPUI defaults are Latin-only without this).
pub(crate) fn ui_font() -> Font {
    Font {
        family: ".SystemUIFont".into(),
        features: FontFeatures::default(),
        fallbacks: Some(FontFallbacks::from_fonts(vec![
            "Yu Gothic UI".into(),
            "Yu Gothic".into(),
            "Meiryo UI".into(),
            "Meiryo".into(),
            "MS UI Gothic".into(),
            "Segoe UI".into(),
            "Hiragino Sans".into(),
            "Hiragino Kaku Gothic ProN".into(),
            "Noto Sans CJK JP".into(),
            "Noto Sans JP".into(),
            "Source Han Sans JP".into(),
        ])),
        weight: FontWeight::default(),
        style: FontStyle::default(),
    }
}

pub(crate) fn rgba_to_render_image(
    rgba: Vec<u8>,
    width: u32,
    height: u32,
) -> Option<Arc<RenderImage>> {
    // GPUI `RenderImage` is documented / uploaded as BGRA (see gpui image loader:
    // it always swaps R↔B after decoding to RGBA). Feed BGRA here too.
    let mut bgra = rgba;
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, bgra)?;
    Some(Arc::new(RenderImage::new(smallvec![Frame::new(buffer)])))
}

/// Load a still image file as RGBA for direct UI display (no UYVY round-trip).
pub(crate) fn rgba_image_from_path(
    path: &Path,
    width: u32,
    height: u32,
) -> Result<Arc<RenderImage>, String> {
    let img = image::open(path).map_err(|e| e.to_string())?;
    let resized = image::imageops::resize(
        &img.to_rgba8(),
        width.max(1),
        height.max(1),
        image::imageops::FilterType::Triangle,
    );
    rgba_to_render_image(resized.into_raw(), width.max(1), height.max(1))
        .ok_or_else(|| "invalid image geometry".into())
}

pub(crate) fn pattern_thumb(kind: PatternKind) -> Option<Arc<RenderImage>> {
    let mut uyvy = vec![0u8; (THUMB_W as usize) * 2 * (THUMB_H as usize)];
    fill_uyvy(kind, &mut uyvy, THUMB_W, THUMB_H, 0.0, 0.0);
    let rgba = uyvy_to_rgba(&uyvy, THUMB_W as u32, THUMB_H as u32);
    rgba_to_render_image(rgba, THUMB_W as u32, THUMB_H as u32)
}

pub(crate) fn pattern_label(lang: Language, kind: PatternKind) -> &'static str {
    match lang {
        Language::Japanese => kind.label_ja(),
        Language::English => kind.label_en(),
    }
}

pub(crate) fn quality_label(quality: Quality) -> &'static str {
    match quality {
        Quality::Low => "LQ",
        Quality::High => "HQ",
        Quality::Medium | Quality::Default => "SQ",
    }
}

pub(crate) fn format_bytes(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    if b >= 1_000_000_000.0 {
        format!("{:.2} GB", b / 1_000_000_000.0)
    } else if b >= 1_000_000.0 {
        format!("{:.2} MB", b / 1_000_000.0)
    } else if b >= 1_000.0 {
        format!("{:.1} KB", b / 1_000.0)
    } else {
        format!("{b:.0} B")
    }
}

pub(crate) fn image_display_name(path: &Path) -> SharedString {
    SharedString::from(
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Image")
            .to_string(),
    )
}
