//! System Japanese UI font loading helpers for egui tools.

use std::path::Path;

#[derive(Clone, Copy)]
struct FontCandidate {
    path: &'static Path,
    /// Face index inside a TTC (use proportional / UI faces when available).
    index: u32,
}

/// Load bytes + face index for a modern Japanese UI font.
///
/// Prefers business / UI gothic faces (BIZ UDPGothic, Yu Gothic) and never
/// falls back to Chinese-primary fonts (e.g. Microsoft YaHei / pan-CJK CJK).
pub fn load_cjk_font() -> Option<(Vec<u8>, u32)> {
    for cand in japanese_ui_font_candidates() {
        if let Ok(bytes) = std::fs::read(cand.path)
            && !bytes.is_empty()
        {
            return Some((bytes, cand.index));
        }
    }
    None
}

/// Load bytes for a system Japanese UI font.
pub fn load_cjk_font_bytes() -> Option<Vec<u8>> {
    load_cjk_font().map(|(bytes, _)| bytes)
}

/// Install Latin + Japanese UI fonts for egui.
///
/// Segoe UI (or platform UI sans) is primary so English stays clean; Japanese
/// faces are registered as fallbacks for kana/kanji.
#[cfg(feature = "egui-fonts")]
pub fn install_egui_cjk_fonts(ctx: &egui::Context) {
    use std::sync::Arc;

    let mut fonts = egui::FontDefinitions::default();

    // Latin: Segoe UI (clean modern Windows UI).
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf")
        && !bytes.is_empty()
    {
        fonts.font_data.insert(
            "omt_latin".into(),
            Arc::new(egui::FontData::from_owned(bytes)),
        );
    }

    if let Some((bytes, index)) = load_cjk_font() {
        let mut data = egui::FontData::from_owned(bytes);
        data.index = index;
        fonts.font_data.insert("omt_jp".into(), Arc::new(data));
    }

    if let Some(proportional) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        // Latin first so English uses Segoe; JP second for kana/kanji fallback.
        if fonts.font_data.contains_key("omt_latin") {
            proportional.insert(0, "omt_latin".into());
        }
        if fonts.font_data.contains_key("omt_jp") {
            let insert_at = if fonts.font_data.contains_key("omt_latin") {
                1
            } else {
                0
            };
            proportional.insert(insert_at, "omt_jp".into());
        }
    }
    if let Some(monospace) = fonts.families.get_mut(&egui::FontFamily::Monospace)
        && fonts.font_data.contains_key("omt_jp")
    {
        monospace.push("omt_jp".into());
    }

    ctx.set_fonts(fonts);
}

fn japanese_ui_font_candidates() -> Vec<FontCandidate> {
    #[cfg(windows)]
    {
        [
            // BIZ UDPGothic = proportional business UD gothic (index 1 in the Regular TTC).
            FontCandidate {
                path: Path::new(r"C:\Windows\Fonts\BIZ-UDGothicR.ttc"),
                index: 1,
            },
            FontCandidate {
                path: Path::new(r"C:\Windows\Fonts\BIZ-UDGothicR.ttc"),
                index: 0,
            },
            // Yu Gothic Regular / Medium — clean modern JP sans.
            FontCandidate {
                path: Path::new(r"C:\Windows\Fonts\YuGothR.ttc"),
                index: 0,
            },
            FontCandidate {
                path: Path::new(r"C:\Windows\Fonts\YuGothM.ttc"),
                index: 0,
            },
            FontCandidate {
                path: Path::new(r"C:\Windows\Fonts\meiryo.ttc"),
                index: 0,
            },
            // JP-only Noto (not pan-CJK).
            FontCandidate {
                path: Path::new(r"C:\Windows\Fonts\NotoSansJP-VF.ttf"),
                index: 0,
            },
        ]
        .to_vec()
    }
    #[cfg(target_os = "macos")]
    {
        [
            FontCandidate {
                path: Path::new("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc"),
                index: 0,
            },
            FontCandidate {
                path: Path::new("/System/Library/Fonts/Hiragino Sans W3.ttc"),
                index: 0,
            },
            FontCandidate {
                path: Path::new("/System/Library/Fonts/Supplemental/Arial Unicode.ttf"),
                index: 0,
            },
        ]
        .to_vec()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        [
            FontCandidate {
                path: Path::new("/usr/share/fonts/opentype/noto/NotoSansJP-Regular.otf"),
                index: 0,
            },
            FontCandidate {
                path: Path::new("/usr/share/fonts/truetype/fonts-japanese-gothic.ttf"),
                index: 0,
            },
            FontCandidate {
                path: Path::new("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
                index: 0,
            },
        ]
        .to_vec()
    }
    #[cfg(not(any(windows, unix)))]
    {
        Vec::new()
    }
}
