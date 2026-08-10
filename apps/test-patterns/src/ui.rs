//! GPUI Test Patterns UI — pattern grid, tone/fps/quality bar, preview, host stats.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use gpui::{
    div, img, prelude::*, px, rgb, size, App, Application, Bounds, Context, Font, FontFallbacks,
    FontFeatures, FontStyle, FontWeight, InteractiveElement, MouseButton, MouseDownEvent,
    ObjectFit, RenderImage, SharedString, Timer, Window, WindowBounds, WindowOptions,
};
use image::{Frame, ImageBuffer, Rgba};
use omt_media::{AudioToneConfig, SendSession, SendSessionConfig, SendStats};
use openmediatransport::uyvy_to_rgba;
use pattern_generator::{PatternKind, fill_uyvy, uyvy_from_image_path};
use smallvec::smallvec;
use suite_core::{
    load_test_patterns_config, save_test_patterns_config, Language, TestPatternsConfig, t,
};
use vmx::Profile;

/// Frames for one full scroll cycle at ±100% animation speed.
const ANIM_BASE_CYCLE_FRAMES: f32 = 300.0;
const THUMB_W: i32 = 320;
const THUMB_H: i32 = 180;
const PREVIEW_W: i32 = 240;
const PREVIEW_H: i32 = 135;
const GRID_COLS: usize = 4;
const TILE_W: f32 = 220.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameRate {
    n: i32,
    d: i32,
}

impl FrameRate {
    const PRESETS: &[FrameRate] = &[
        FrameRate { n: 24_000, d: 1_001 },
        FrameRate { n: 24, d: 1 },
        FrameRate { n: 25, d: 1 },
        FrameRate { n: 30_000, d: 1_001 },
        FrameRate { n: 30, d: 1 },
        FrameRate { n: 50, d: 1 },
        FrameRate { n: 60_000, d: 1_001 },
        FrameRate { n: 60, d: 1 },
    ];

    fn label(self) -> String {
        let v = self.n as f64 / self.d.max(1) as f64;
        if self.d == 1 {
            format!("{v:.0}")
        } else {
            format!("{v:.2}")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TonePreset {
    Mute,
    Hz(f32),
}

impl TonePreset {
    const PRESETS: &[TonePreset] = &[
        TonePreset::Mute,
        TonePreset::Hz(440.0),
        TonePreset::Hz(1000.0),
        TonePreset::Hz(2000.0),
    ];

    fn hz(self) -> f32 {
        match self {
            Self::Mute => 0.0,
            Self::Hz(v) => v,
        }
    }

    fn label(self, language: Language) -> SharedString {
        match self {
            Self::Mute => SharedString::from(t(language, "patterns.tone_mute")),
            Self::Hz(v) => SharedString::from(format!("{v:.0} Hz")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuKind {
    Tone,
    Fps,
}

struct CustomImage {
    path: PathBuf,
    thumb: Option<Arc<RenderImage>>,
}

/// System UI font plus CJK fallbacks (GPUI defaults are Latin-only without this).
fn ui_font() -> Font {
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

fn rgba_to_render_image(rgba: Vec<u8>, width: u32, height: u32) -> Option<Arc<RenderImage>> {
    let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, rgba)?;
    Some(Arc::new(RenderImage::new(smallvec![Frame::new(buffer)])))
}

/// Load a still image file as RGBA for direct UI display (no UYVY round-trip).
fn rgba_image_from_path(path: &Path, width: u32, height: u32) -> Result<Arc<RenderImage>, String> {
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

fn pattern_thumb(kind: PatternKind) -> Option<Arc<RenderImage>> {
    let mut uyvy = vec![0u8; (THUMB_W as usize) * 2 * (THUMB_H as usize)];
    fill_uyvy(kind, &mut uyvy, THUMB_W, THUMB_H, 0.0, 0.0);
    let rgba = uyvy_to_rgba(&uyvy, THUMB_W as u32, THUMB_H as u32);
    rgba_to_render_image(rgba, THUMB_W as u32, THUMB_H as u32)
}

fn pattern_label(lang: Language, kind: PatternKind) -> &'static str {
    match lang {
        Language::Japanese => kind.label_ja(),
        Language::English => kind.label_en(),
    }
}

fn profile_label(profile: Profile) -> &'static str {
    match profile {
        Profile::OmtLq | Profile::Lq => "LQ",
        Profile::OmtHq | Profile::Hq => "HQ",
        _ => "SQ",
    }
}

fn format_bytes(bytes: i64) -> String {
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

fn image_display_name(path: &Path) -> SharedString {
    SharedString::from(
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Image")
            .to_string(),
    )
}

fn persist_custom_images(paths: &[PathBuf]) {
    let cfg = TestPatternsConfig {
        schema_version: 1,
        custom_images: paths.to_vec(),
    };
    let _ = save_test_patterns_config(&cfg);
}

pub fn run_gpui(title: String, language: Language) -> Result<()> {
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.0), px(780.0)), cx);
        let title = SharedString::from(title);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(title.clone()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            move |_, cx| cx.new(|cx| PatternsView::new(cx, language)),
        )
        .expect("open GPUI Test Patterns window");
        cx.activate(true);
    });
    Ok(())
}

struct PatternsView {
    language: Language,
    name: String,
    kind: PatternKind,
    width: i32,
    height: i32,
    frame_rate: FrameRate,
    profile: Profile,
    animate: bool,
    anim_speed_h_pct: i32,
    anim_speed_v_pct: i32,
    tone: TonePreset,
    custom_images: Vec<CustomImage>,
    selected_custom: Option<usize>,
    session: Option<SendSession>,
    last_stats: SendStats,
    error: Option<SharedString>,
    thumbs: Vec<(PatternKind, Option<Arc<RenderImage>>)>,
    preview: Option<Arc<RenderImage>>,
    preview_phase: f32,
    last_preview_at: Instant,
    window_title: SharedString,
    open_menu: Option<MenuKind>,
    /// Right-click menu: image index and window-relative anchor.
    custom_menu: Option<(usize, f32, f32)>,
    /// Pending native file dialog result (must not block the GPUI UI thread).
    image_pick_rx: Option<Receiver<Option<PathBuf>>>,
}

impl PatternsView {
    fn new(cx: &mut Context<Self>, language: Language) -> Self {
        let thumbs = PatternKind::builtins()
            .iter()
            .copied()
            .map(|kind| (kind, pattern_thumb(kind)))
            .collect();

        let saved = load_test_patterns_config()
            .unwrap_or_default()
            .custom_images;
        let saved_count = saved.len();
        let mut custom_images = Vec::new();
        let mut kept_paths = Vec::new();
        for path in saved {
            if !path.is_file() {
                continue;
            }
            let thumb = rgba_image_from_path(&path, THUMB_W as u32, THUMB_H as u32).ok();
            kept_paths.push(path.clone());
            custom_images.push(CustomImage { path, thumb });
        }
        if kept_paths.len() != saved_count {
            persist_custom_images(&kept_paths);
        }

        let mut view = Self {
            language,
            name: "Test Pattern".into(),
            kind: PatternKind::SmpteColorBars,
            width: 1920,
            height: 1080,
            frame_rate: FrameRate { n: 30_000, d: 1_001 },
            profile: Profile::OmtSq,
            animate: true,
            anim_speed_h_pct: 100,
            anim_speed_v_pct: 100,
            tone: TonePreset::Hz(1000.0),
            custom_images,
            selected_custom: None,
            session: None,
            last_stats: SendStats::default(),
            error: None,
            thumbs,
            preview: None,
            preview_phase: 0.0,
            last_preview_at: Instant::now() - Duration::from_secs(1),
            window_title: SharedString::from(""),
            open_menu: None,
            custom_menu: None,
            image_pick_rx: None,
        };
        view.refresh_title();
        view.restart_session();
        view.refresh_preview(cx);
        view.schedule_tick(cx);
        view
    }

    fn schedule_tick(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            Timer::after(Duration::from_millis(16)).await;
            this.update(cx, |this, cx| {
                this.on_tick(cx);
                this.schedule_tick(cx);
            })
            .ok();
        })
        .detach();
    }

    fn on_tick(&mut self, cx: &mut Context<Self>) {
        self.poll_image_pick(cx);
        if let Some(session) = &self.session {
            self.last_stats = session.stats();
        }
        // Still-image preview is loaded once at pick time; only animate generated patterns.
        if self.kind != PatternKind::Image {
            if self.animate {
                self.preview_phase = (self.preview_phase + 0.02).rem_euclid(1.0);
            }
            if self.last_preview_at.elapsed() >= Duration::from_millis(200) {
                self.refresh_preview(cx);
            }
        }
        self.refresh_title();
        cx.notify();
    }

    fn refresh_title(&mut self) {
        let pattern = if self.kind == PatternKind::Image {
            self.selected_custom
                .and_then(|i| self.custom_images.get(i))
                .map(|img| {
                    img.path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("Image")
                        .to_string()
                })
                .unwrap_or_else(|| "Image".into())
        } else {
            pattern_label(self.language, self.kind).to_string()
        };
        let fps = self.frame_rate.label();
        let res = if self.height >= 2160 {
            "2160p"
        } else if self.height >= 1080 {
            "1080p"
        } else if self.height >= 720 {
            "720p"
        } else {
            "SD"
        };
        self.window_title = SharedString::from(format!(
            "OMT Test Patterns - {pattern} ({res}{fps})"
        ));
    }

    fn select_pattern(&mut self, kind: PatternKind, cx: &mut Context<Self>) {
        self.custom_menu = None;
        self.open_menu = None;
        if kind == PatternKind::Image || self.kind == kind {
            return;
        }
        self.kind = kind;
        self.selected_custom = None;
        self.restart_session();
        self.refresh_preview(cx);
        cx.notify();
    }

    fn select_custom_image(&mut self, index: usize, cx: &mut Context<Self>) {
        self.custom_menu = None;
        self.open_menu = None;
        if self.kind == PatternKind::Image && self.selected_custom == Some(index) {
            return;
        }
        let Some(entry) = self.custom_images.get(index) else {
            return;
        };
        let path = entry.path.clone();
        let preview = match rgba_image_from_path(&path, PREVIEW_W as u32, PREVIEW_H as u32) {
            Ok(img) => img,
            Err(e) => {
                self.error = Some(SharedString::from(e));
                cx.notify();
                return;
            }
        };
        if let Some(old) = self.preview.take() {
            cx.drop_image(old, None);
        }
        self.preview = Some(preview);
        self.last_preview_at = Instant::now();
        self.selected_custom = Some(index);
        self.kind = PatternKind::Image;
        self.error = None;
        self.restart_session();
        cx.notify();
    }

    fn request_pick_image(&mut self, _cx: &mut Context<Self>) {
        if self.image_pick_rx.is_some() {
            return;
        }
        self.custom_menu = None;
        let (tx, rx) = mpsc::channel();
        self.image_pick_rx = Some(rx);
        thread::Builder::new()
            .name("omt-pick-image".into())
            .spawn(move || {
                let path = rfd::FileDialog::new()
                    .add_filter("Images", &["png", "jpg", "jpeg", "bmp"])
                    .pick_file();
                let _ = tx.send(path);
            })
            .ok();
    }

    fn poll_image_pick(&mut self, cx: &mut Context<Self>) {
        let result = match self.image_pick_rx.as_ref() {
            Some(rx) => match rx.try_recv() {
                Ok(path) => Some(Ok(path)),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(())),
            },
            None => None,
        };
        match result {
            Some(Ok(Some(path))) => {
                self.image_pick_rx = None;
                self.add_custom_image(path, cx);
            }
            Some(Ok(None)) => {
                // Cancelled — leave the previous pattern running.
                self.image_pick_rx = None;
                self.error = None;
            }
            Some(Err(())) => {
                self.image_pick_rx = None;
            }
            None => {}
        }
    }

    fn add_custom_image(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.error = None;
        if let Some(existing) = self.custom_images.iter().position(|img| img.path == path) {
            self.select_custom_image(existing, cx);
            return;
        }

        let thumb = match rgba_image_from_path(&path, THUMB_W as u32, THUMB_H as u32) {
            Ok(img) => img,
            Err(e) => {
                self.error = Some(SharedString::from(e));
                return;
            }
        };

        self.custom_images.push(CustomImage {
            path,
            thumb: Some(thumb),
        });
        self.persist_images();
        let index = self.custom_images.len() - 1;
        self.select_custom_image(index, cx);
    }

    fn remove_custom_image(&mut self, index: usize, cx: &mut Context<Self>) {
        self.custom_menu = None;
        if index >= self.custom_images.len() {
            return;
        }
        let removed = self.custom_images.remove(index);
        if let Some(thumb) = removed.thumb {
            cx.drop_image(thumb, None);
        }
        self.persist_images();

        let was_selected = self.kind == PatternKind::Image
            && self.selected_custom.is_some_and(|i| i == index);
        if let Some(sel) = self.selected_custom.as_mut() {
            if *sel > index {
                *sel -= 1;
            }
        }
        if was_selected {
            self.selected_custom = None;
            self.kind = PatternKind::SmpteColorBars;
            self.restart_session();
            self.refresh_preview(cx);
        }
        cx.notify();
    }

    fn persist_images(&self) {
        let paths: Vec<_> = self.custom_images.iter().map(|img| img.path.clone()).collect();
        persist_custom_images(&paths);
    }

    fn open_custom_menu(&mut self, index: usize, x: f32, y: f32, cx: &mut Context<Self>) {
        self.open_menu = None;
        self.custom_menu = Some((index, x, y));
        cx.notify();
    }

    fn close_overlays(&mut self, cx: &mut Context<Self>) {
        self.open_menu = None;
        self.custom_menu = None;
        cx.notify();
    }

    fn set_tone(&mut self, tone: TonePreset, cx: &mut Context<Self>) {
        self.open_menu = None;
        if self.tone == tone {
            cx.notify();
            return;
        }
        self.tone = tone;
        self.restart_session();
        cx.notify();
    }

    fn set_frame_rate(&mut self, frame_rate: FrameRate, cx: &mut Context<Self>) {
        self.open_menu = None;
        if self.frame_rate == frame_rate {
            cx.notify();
            return;
        }
        self.frame_rate = frame_rate;
        self.restart_session();
        cx.notify();
    }

    fn toggle_menu(&mut self, menu: MenuKind, cx: &mut Context<Self>) {
        self.custom_menu = None;
        self.open_menu = if self.open_menu == Some(menu) {
            None
        } else {
            Some(menu)
        };
        cx.notify();
    }

    fn set_profile(&mut self, profile: Profile, cx: &mut Context<Self>) {
        if self.profile == profile {
            return;
        }
        self.profile = profile;
        self.restart_session();
        cx.notify();
    }

    fn current_image_path(&self) -> Option<&Path> {
        self.selected_custom
            .and_then(|i| self.custom_images.get(i))
            .map(|img| img.path.as_path())
    }

    fn restart_session(&mut self) {
        self.stop();
        self.error = None;

        let width = self.width;
        let height = self.height;
        let kind = self.kind;
        let animate = self.animate;
        let speed_h = self.anim_speed_h_pct.clamp(-200, 200) as f32 / 100.0;
        let speed_v = self.anim_speed_v_pct.clamp(-200, 200) as f32 / 100.0;
        let image_uyvy = if kind == PatternKind::Image {
            match self.current_image_path() {
                Some(path) => match uyvy_from_image_path(path, width, height) {
                    Ok(buf) => Some(buf),
                    Err(e) => {
                        self.error = Some(SharedString::from(e.to_string()));
                        return;
                    }
                },
                None => {
                    self.error = Some(SharedString::from("Select an image file first"));
                    return;
                }
            }
        } else {
            None
        };

        let provider: Arc<dyn Fn(u64) -> Vec<u8> + Send + Sync> = Arc::new(move |idx| {
            if let Some(ref still) = image_uyvy {
                return still.clone();
            }
            let (phase_x, phase_y) = if animate {
                let t = idx as f32;
                (
                    (t * speed_h / ANIM_BASE_CYCLE_FRAMES).rem_euclid(1.0),
                    (t * speed_v / ANIM_BASE_CYCLE_FRAMES).rem_euclid(1.0),
                )
            } else {
                (0.0, 0.0)
            };
            let mut buf = vec![0u8; (width as usize) * 2 * (height as usize)];
            fill_uyvy(kind, &mut buf, width, height, phase_x, phase_y);
            buf
        });

        let config = SendSessionConfig {
            name: self.name.clone(),
            width: self.width,
            height: self.height,
            fps_n: self.frame_rate.n,
            fps_d: self.frame_rate.d,
            profile: self.profile,
            animate: self.animate && self.kind != PatternKind::Image,
            audio: AudioToneConfig {
                tone_hz: self.tone.hz(),
                ..Default::default()
            },
        };

        match SendSession::start(config, provider) {
            Ok(session) => self.session = Some(session),
            Err(e) => self.error = Some(SharedString::from(e.to_string())),
        }
    }

    fn stop(&mut self) {
        if let Some(mut session) = self.session.take() {
            session.stop();
        }
    }

    fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        self.last_preview_at = Instant::now();
        // Image stills are assigned once when selecting a custom image.
        if self.kind == PatternKind::Image {
            return;
        }
        let phase = if self.animate { self.preview_phase } else { 0.0 };
        let mut uyvy = vec![0u8; (PREVIEW_W as usize) * 2 * (PREVIEW_H as usize)];
        fill_uyvy(
            self.kind,
            &mut uyvy,
            PREVIEW_W,
            PREVIEW_H,
            phase,
            phase,
        );
        let rgba = uyvy_to_rgba(&uyvy, PREVIEW_W as u32, PREVIEW_H as u32);
        if let Some(image) = rgba_to_render_image(rgba, PREVIEW_W as u32, PREVIEW_H as u32) {
            if let Some(old) = self.preview.take() {
                cx.drop_image(old, None);
            }
            self.preview = Some(image);
        }
    }
}

impl Drop for PatternsView {
    fn drop(&mut self) {
        self.stop();
    }
}

impl Render for PatternsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language;
        let kind = self.kind;
        let tone = self.tone;
        let frame_rate = self.frame_rate;
        let profile = self.profile;
        let open_menu = self.open_menu;
        let custom_menu = self.custom_menu;
        let thumbs = self.thumbs.clone();
        let custom_images: Vec<(usize, PathBuf, Option<Arc<RenderImage>>)> = self
            .custom_images
            .iter()
            .enumerate()
            .map(|(i, img)| (i, img.path.clone(), img.thumb.clone()))
            .collect();
        let selected_custom = self.selected_custom;
        let preview = self.preview.clone();
        let stats = self.last_stats.clone();
        let error = self.error.clone();
        let sending = self.session.is_some();
        let title = self.window_title.clone();

        let mut root = div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .font(ui_font())
            .bg(rgb(0x1a1d23))
            .text_color(rgb(0xedf2f7))
            // Title strip
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(0x2a3340))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_sm()
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .opacity(0.7)
                            .child(if sending {
                                SharedString::from(t(language, "patterns.sending"))
                            } else {
                                SharedString::from(t(language, "patterns.idle"))
                            }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    // Pattern grid
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .id("pattern-grid")
                            .overflow_y_scroll()
                            .p_4()
                            .child(pattern_grid(
                                cx,
                                language,
                                kind,
                                selected_custom,
                                thumbs,
                                custom_images,
                            )),
                    )
                    // Stats side panel
                    .child(
                        div()
                            .w(px(240.0))
                            .h_full()
                            .p_3()
                            .gap_2()
                            .flex()
                            .flex_col()
                            .border_l_1()
                            .border_color(rgb(0x2a3340))
                            .bg(rgb(0x14181f))
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .child(t(language, "patterns.stats")),
                            )
                            .child(stat_row(
                                t(language, "patterns.clients"),
                                format!("{}", stats.clients),
                            ))
                            .child(stat_row(
                                t(language, "patterns.connections"),
                                format!("{}", stats.connections),
                            ))
                            .child(stat_row(
                                t(language, "patterns.video_subs"),
                                format!("{}", stats.video_subscribers),
                            ))
                            .child(stat_row(
                                t(language, "patterns.audio_subs"),
                                format!("{}", stats.audio_subscribers),
                            ))
                            .child(stat_row("Port", format!("{}", stats.port)))
                            .child(stat_row("Video FPS", format!("{:.1}", stats.video_fps)))
                            .child(stat_row("Encode", format!("{:.1} ms", stats.encode_ms)))
                            .child(stat_row("Frames", format!("{}", stats.frames)))
                            .child(stat_row("Dropped", format!("{}", stats.dropped)))
                            .child(stat_row("Bytes TX", format_bytes(stats.bytes_sent)))
                            .child(stat_row(
                                t(language, "patterns.name"),
                                self.name.clone(),
                            ))
                            .children(if stats.behind {
                                Some(
                                    div()
                                        .mt_2()
                                        .text_xs()
                                        .text_color(rgb(0xf6c344))
                                        .child(t(language, "patterns.perf_warn")),
                                )
                            } else {
                                None
                            })
                            .children(error.map(|e| {
                                div()
                                    .mt_2()
                                    .text_xs()
                                    .text_color(rgb(0xff6b6b))
                                    .child(e)
                            })),
                    ),
            )
            // Bottom control bar
            .child(
                div()
                    .px_4()
                    .py_3()
                    .gap_4()
                    .flex()
                    .items_end()
                    .border_t_1()
                    .border_color(rgb(0x2a3340))
                    .bg(rgb(0x12161c))
                    .child(tone_control(cx, language, tone, open_menu == Some(MenuKind::Tone)))
                    .child(fps_control(
                        cx,
                        language,
                        frame_rate,
                        open_menu == Some(MenuKind::Fps),
                    ))
                    .child(quality_control(cx, language, profile))
                    .child(div().flex_1())
                    .child(output_preview(language, preview)),
            );

        // Root-level overlays so dropdowns / context menus paint above the main grid.
        if open_menu.is_some() || custom_menu.is_some() {
            root = root.child(overlay_layer(
                cx,
                language,
                open_menu,
                tone,
                frame_rate,
                custom_menu,
            ));
        }

        root
    }
}

fn overlay_layer(
    cx: &mut Context<PatternsView>,
    language: Language,
    open_menu: Option<MenuKind>,
    tone: TonePreset,
    frame_rate: FrameRate,
    custom_menu: Option<(usize, f32, f32)>,
) -> impl IntoElement {
    let mut layer = div()
        .id("overlay-root")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .occlude()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.close_overlays(cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|this, _, _, cx| {
                this.close_overlays(cx);
            }),
        );

    if let Some(menu) = open_menu {
        let left = match menu {
            MenuKind::Tone => px(16.0),
            MenuKind::Fps => px(172.0),
        };
        let width = match menu {
            MenuKind::Tone => px(160.0),
            MenuKind::Fps => px(100.0),
        };
        layer = layer.child(
            div()
                .absolute()
                .bottom(px(72.0))
                .left(left)
                .w(width)
                .p_1()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x2a3340))
                .bg(rgb(0x1b222c))
                .shadow_md()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .children(match menu {
                    MenuKind::Tone => TonePreset::PRESETS
                        .iter()
                        .map(|preset| {
                            let preset = *preset;
                            let active = tone == preset;
                            div()
                                .id(SharedString::from(format!("tone-{}", preset.hz())))
                                .px_2()
                                .py_1()
                                .rounded_sm()
                                .bg(if active {
                                    rgb(0x2f6fed)
                                } else {
                                    rgb(0x1b222c)
                                })
                                .hover(|s| s.bg(rgb(0x243041)))
                                .cursor_pointer()
                                .text_xs()
                                .child(preset.label(language))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.set_tone(preset, cx);
                                }))
                                .into_any_element()
                        })
                        .collect::<Vec<_>>(),
                    MenuKind::Fps => FrameRate::PRESETS
                        .iter()
                        .map(|preset| {
                            let preset = *preset;
                            let active = frame_rate == preset;
                            div()
                                .id(SharedString::from(format!(
                                    "fps-{}-{}",
                                    preset.n, preset.d
                                )))
                                .px_2()
                                .py_1()
                                .rounded_sm()
                                .bg(if active {
                                    rgb(0x2f6fed)
                                } else {
                                    rgb(0x1b222c)
                                })
                                .hover(|s| s.bg(rgb(0x243041)))
                                .cursor_pointer()
                                .text_xs()
                                .child(preset.label())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.set_frame_rate(preset, cx);
                                }))
                                .into_any_element()
                        })
                        .collect::<Vec<_>>(),
                }),
        );
    }

    if let Some((index, x, y)) = custom_menu {
        layer = layer.child(
            div()
                .absolute()
                .top(px(y))
                .left(px(x))
                .min_w(px(120.0))
                .p_1()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x2a3340))
                .bg(rgb(0x1b222c))
                .shadow_md()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .id("custom-image-remove")
                        .px_3()
                        .py_1()
                        .rounded_sm()
                        .hover(|s| s.bg(rgb(0x243041)))
                        .cursor_pointer()
                        .text_xs()
                        .child(t(language, "patterns.image_remove"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.remove_custom_image(index, cx);
                        })),
                ),
        );
    }

    layer
}

fn pattern_grid(
    cx: &mut Context<PatternsView>,
    language: Language,
    selected: PatternKind,
    selected_custom: Option<usize>,
    thumbs: Vec<(PatternKind, Option<Arc<RenderImage>>)>,
    custom_images: Vec<(usize, PathBuf, Option<Arc<RenderImage>>)>,
) -> impl IntoElement {
    let mut children: Vec<gpui::AnyElement> = Vec::new();

    for chunk in thumbs.chunks(GRID_COLS) {
        let cells: Vec<_> = chunk
            .iter()
            .map(|(kind, thumb)| {
                let kind = *kind;
                let is_selected = selected != PatternKind::Image && kind == selected;
                pattern_tile(
                    cx,
                    SharedString::from(kind.id()),
                    SharedString::from(pattern_label(language, kind)),
                    thumb.clone(),
                    is_selected,
                    move |this, cx| this.select_pattern(kind, cx),
                    None::<fn(&mut PatternsView, &MouseDownEvent, &mut Context<PatternsView>)>,
                )
            })
            .collect();
        children.push(
            div()
                .flex()
                .flex_row()
                .gap_4()
                .mb_4()
                .children(cells)
                .into_any_element(),
        );
    }

    children.push(
        div()
            .mt_2()
            .mb_3()
            .h(px(1.0))
            .bg(rgb(0x2a3340))
            .into_any_element(),
    );
    children.push(
        div()
            .mb_3()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .opacity(0.7)
            .child(t(language, "patterns.image"))
            .into_any_element(),
    );

    let mut custom_cells: Vec<gpui::AnyElement> = custom_images
        .into_iter()
        .map(|(index, path, thumb)| {
            let is_selected = selected == PatternKind::Image && selected_custom == Some(index);
            pattern_tile(
                cx,
                SharedString::from(format!("custom-image-{index}")),
                image_display_name(&path),
                thumb,
                is_selected,
                move |this, cx| this.select_custom_image(index, cx),
                Some(
                    move |this: &mut PatternsView,
                          event: &MouseDownEvent,
                          cx: &mut Context<PatternsView>| {
                        let x: f32 = event.position.x.into();
                        let y: f32 = event.position.y.into();
                        this.open_custom_menu(index, x, y, cx);
                    },
                ),
            )
        })
        .collect();

    custom_cells.push(add_image_tile(cx, language));

    while !custom_cells.is_empty() {
        let take = custom_cells.len().min(GRID_COLS);
        let row: Vec<_> = custom_cells.drain(..take).collect();
        children.push(
            div()
                .flex()
                .flex_row()
                .gap_4()
                .mb_4()
                .children(row)
                .into_any_element(),
        );
    }

    div().flex().flex_col().children(children)
}

fn pattern_tile<FSelect, FMenu>(
    cx: &mut Context<PatternsView>,
    id: SharedString,
    label: SharedString,
    thumb: Option<Arc<RenderImage>>,
    is_selected: bool,
    on_select: FSelect,
    on_menu: Option<FMenu>,
) -> gpui::AnyElement
where
    FSelect: Fn(&mut PatternsView, &mut Context<PatternsView>) + 'static + Clone,
    FMenu: Fn(&mut PatternsView, &MouseDownEvent, &mut Context<PatternsView>) + 'static + Clone,
{
    let select = on_select.clone();
    let mut tile = div()
        .id(id)
        .w(px(TILE_W))
        .flex()
        .flex_col()
        .gap_1()
        .cursor_pointer()
        .child(
            div()
                .rounded_sm()
                .border_2()
                .border_color(if is_selected {
                    rgb(0x2f6fed)
                } else {
                    rgb(0x2a3340)
                })
                .bg(rgb(0x0c1016))
                .overflow_hidden()
                .child(if let Some(tex) = thumb {
                    img(tex)
                        .object_fit(ObjectFit::Fill)
                        .w(px(216.0))
                        .h(px(122.0))
                        .into_any_element()
                } else {
                    div()
                        .w(px(216.0))
                        .h(px(122.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .opacity(0.5)
                        .child("?")
                        .into_any_element()
                }),
        )
        .child(div().text_sm().text_center().child(label))
        .on_click(cx.listener(move |this, _, _, cx| {
            select(this, cx);
        }));

    if let Some(on_menu) = on_menu {
        tile = tile.on_mouse_down(
            MouseButton::Right,
            cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                on_menu(this, event, cx);
                cx.stop_propagation();
            }),
        );
    }

    tile.into_any_element()
}

fn add_image_tile(cx: &mut Context<PatternsView>, language: Language) -> gpui::AnyElement {
    div()
        .id("add-custom-image")
        .w(px(TILE_W))
        .flex()
        .flex_col()
        .gap_1()
        .cursor_pointer()
        .child(
            div()
                .rounded_sm()
                .border_2()
                .border_color(rgb(0x2a3340))
                .border_dashed()
                .bg(rgb(0x0c1016))
                .w(px(216.0))
                .h(px(122.0))
                .flex()
                .items_center()
                .justify_center()
                .text_2xl()
                .opacity(0.7)
                .child("+"),
        )
        .child(
            div()
                .text_sm()
                .text_center()
                .opacity(0.7)
                .child(t(language, "patterns.image_add")),
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.request_pick_image(cx);
            cx.notify();
        }))
        .into_any_element()
}

fn tone_control(
    cx: &mut Context<PatternsView>,
    language: Language,
    selected: TonePreset,
    open: bool,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .opacity(0.65)
                .child(t(language, "patterns.tone")),
        )
        .child(
            div()
                .id("tone-toggle")
                .w(px(140.0))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(if open { rgb(0x2f6fed) } else { rgb(0x243041) })
                .cursor_pointer()
                .text_xs()
                .child(selected.label(language))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_menu(MenuKind::Tone, cx);
                })),
        )
}

fn fps_control(
    cx: &mut Context<PatternsView>,
    language: Language,
    selected: FrameRate,
    open: bool,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .opacity(0.65)
                .child(t(language, "patterns.fps")),
        )
        .child(
            div()
                .id("fps-toggle")
                .w(px(88.0))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(if open { rgb(0x2f6fed) } else { rgb(0x243041) })
                .cursor_pointer()
                .text_xs()
                .child(selected.label())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_menu(MenuKind::Fps, cx);
                })),
        )
}

fn quality_control(
    cx: &mut Context<PatternsView>,
    language: Language,
    selected: Profile,
) -> impl IntoElement {
    let profiles = [Profile::OmtLq, Profile::OmtSq, Profile::OmtHq];
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .opacity(0.65)
                .child(t(language, "patterns.profile")),
        )
        .child(
            div()
                .flex()
                .rounded_md()
                .overflow_hidden()
                .children(profiles.into_iter().map(|profile| {
                    let active = selected == profile;
                    div()
                        .id(SharedString::from(profile_label(profile)))
                        .px_3()
                        .py_1()
                        .bg(if active { rgb(0x2f6fed) } else { rgb(0x243041) })
                        .cursor_pointer()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(profile_label(profile))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_profile(profile, cx);
                        }))
                        .into_any_element()
                })),
        )
}

fn output_preview(
    language: Language,
    preview: Option<Arc<RenderImage>>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .text_xs()
                .opacity(0.65)
                .child(t(language, "patterns.output")),
        )
        .child(
            div()
                .w(px(160.0))
                .h(px(90.0))
                .rounded_sm()
                .border_1()
                .border_color(rgb(0x2a3340))
                .bg(rgb(0x000000))
                .overflow_hidden()
                .child(if let Some(tex) = preview {
                    img(tex)
                        .object_fit(ObjectFit::Fill)
                        .w(px(160.0))
                        .h(px(90.0))
                        .into_any_element()
                } else {
                    div().into_any_element()
                }),
        )
}

fn stat_row(label: &str, value: String) -> impl IntoElement {
    div()
        .flex()
        .justify_between()
        .gap_2()
        .text_xs()
        .child(div().opacity(0.65).child(label.to_string()))
        .child(div().font_weight(FontWeight::MEDIUM).child(value))
}
