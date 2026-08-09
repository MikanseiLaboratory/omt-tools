//! System CJK font loading helpers for egui tools.

use std::path::Path;

/// Load bytes for a system CJK UI font suitable for Japanese (and other CJK) glyphs.
///
/// Returns `None` when no candidate font file is readable.
pub fn load_cjk_font_bytes() -> Option<Vec<u8>> {
    for path in cjk_font_candidates() {
        if let Ok(bytes) = std::fs::read(path) {
            if !bytes.is_empty() {
                return Some(bytes);
            }
        }
    }
    None
}

/// Install a CJK fallback into the egui font stack (Latin defaults stay primary).
#[cfg(feature = "egui-fonts")]
pub fn install_egui_cjk_fonts(ctx: &egui::Context) {
    use std::sync::Arc;

    let Some(bytes) = load_cjk_font_bytes() else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "omt_cjk".into(),
        Arc::new(egui::FontData::from_owned(bytes)),
    );
    if let Some(proportional) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        proportional.push("omt_cjk".into());
    }
    if let Some(monospace) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        monospace.push("omt_cjk".into());
    }
    ctx.set_fonts(fonts);
}

fn cjk_font_candidates() -> Vec<&'static Path> {
    #[cfg(windows)]
    {
        [
            Path::new(r"C:\Windows\Fonts\YuGothM.ttc"),
            Path::new(r"C:\Windows\Fonts\YuGothR.ttc"),
            Path::new(r"C:\Windows\Fonts\meiryo.ttc"),
            Path::new(r"C:\Windows\Fonts\msgothic.ttc"),
            Path::new(r"C:\Windows\Fonts\msyh.ttc"),
        ]
        .to_vec()
    }
    #[cfg(target_os = "macos")]
    {
        [
            Path::new("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc"),
            Path::new("/System/Library/Fonts/Hiragino Sans GB.ttc"),
            Path::new("/Library/Fonts/Arial Unicode.ttf"),
            Path::new("/System/Library/Fonts/Supplemental/Arial Unicode.ttf"),
            Path::new("/System/Library/Fonts/AppleSDGothicNeo.ttc"),
        ]
        .to_vec()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        [
            Path::new("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
            Path::new("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc"),
            Path::new("/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc"),
            Path::new("/usr/share/fonts/truetype/fonts-japanese-gothic.ttf"),
        ]
        .to_vec()
    }
    #[cfg(not(any(windows, unix)))]
    {
        Vec::new()
    }
}
