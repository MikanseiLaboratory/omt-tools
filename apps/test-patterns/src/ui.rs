//! GPUI Test Patterns UI — pattern grid, send settings, preview, host stats.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use gpui::{
    App, Application, Bounds, ClickEvent, Context, FocusHandle, Focusable, Font, FontFallbacks,
    FontFeatures, FontStyle, FontWeight, InteractiveElement, KeyDownEvent, MouseButton,
    MouseDownEvent, ObjectFit, RenderImage, SharedString, Timer, Window, WindowBounds,
    WindowOptions, div, img, prelude::*, px, rgb, size,
};
use image::{Frame, ImageBuffer, Rgba};
use omt_media::{
    AudioToneConfig, MAX_VIDEO_FRAME_BUFFER_FRAMES, MIN_VIDEO_FRAME_BUFFER_FRAMES, SendSession,
    SendSessionConfig, SendStats, clamp_video_frame_buffer_frames,
};
use openmediatransport::{Quality, uyvy_to_rgba};
use parking_lot::Mutex;
use pattern_generator::{PatternKind, fill_uyvy, scroll_uyvy, uyvy_from_image_path};
use smallvec::smallvec;
use suite_core::{
    Language, SimdCapabilities, TestPatternsConfig, load_test_patterns_config,
    reveal_in_file_manager, save_test_patterns_config, t,
};

/// Frames for one full scroll cycle at ±100% animation speed.
const ANIM_BASE_CYCLE_FRAMES: f32 = 300.0;

/// Advance an independent scroll phase without resetting on speed changes.
fn advance_scroll_phase(phase: f32, speed_pct: f32) -> f32 {
    let step = speed_pct.clamp(-200.0, 200.0) / 100.0 / ANIM_BASE_CYCLE_FRAMES;
    (phase + step).rem_euclid(1.0)
}

/// Live pattern pixels / animation, shared with the send provider.
struct LivePattern {
    kind: PatternKind,
    image_uyvy: Option<Arc<Vec<u8>>>,
    animate: bool,
    speed_h: f32,
    speed_v: f32,
    phase_x: f32,
    phase_y: f32,
}

impl LivePattern {
    fn from_view(
        kind: PatternKind,
        animate: bool,
        speed_h_pct: i32,
        speed_v_pct: i32,
        image_uyvy: Option<Arc<Vec<u8>>>,
    ) -> Self {
        Self {
            kind,
            image_uyvy,
            animate,
            speed_h: speed_h_pct.clamp(-200, 200) as f32 / 100.0,
            speed_v: speed_v_pct.clamp(-200, 200) as f32 / 100.0,
            phase_x: 0.0,
            phase_y: 0.0,
        }
    }
}
const THUMB_W: i32 = 320;
const THUMB_H: i32 = 180;
const PREVIEW_W: i32 = 240;
const PREVIEW_H: i32 = 135;
const TILE_W: f32 = 220.0;
const SIDE_PANEL_W: f32 = 360.0;
const SIDE_PANEL_MIN_W: f32 = 240.0;
/// Bottom padding under the output preview so it is not flush with the window edge.
const PREVIEW_BOTTOM_MARGIN: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FrameRate {
    n: i32,
    d: i32,
}

impl FrameRate {
    const PRESETS: &[FrameRate] = &[
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

    fn matches(self, tone_hz: f32) -> bool {
        match self {
            Self::Mute => tone_hz <= 0.0,
            Self::Hz(v) => (tone_hz - v).abs() < 0.5,
        }
    }
}

/// Discrete tone level presets (dBFS).
#[derive(Debug, Clone, Copy, PartialEq)]
struct LevelPreset(f32);

impl LevelPreset {
    const PRESETS: &[LevelPreset] = &[
        LevelPreset(0.0),
        LevelPreset(-6.0),
        LevelPreset(-10.0),
        LevelPreset(-20.0),
    ];

    fn dbfs(self) -> f32 {
        self.0
    }

    fn label(self) -> SharedString {
        if self.0 == 0.0 {
            SharedString::from("0 dBFS")
        } else {
            SharedString::from(format!("{} dBFS", self.0 as i32))
        }
    }

    fn matches(self, level_dbfs: f32) -> bool {
        (level_dbfs - self.0).abs() < 0.05
    }

    fn nearest(level_dbfs: f32) -> LevelPreset {
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
struct Resolution {
    width: i32,
    height: i32,
}

impl Resolution {
    const PRESETS: &[Resolution] = &[
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

    fn label(self) -> String {
        format!("{}×{}", self.width, self.height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuKind {
    Resolution,
    Tone,
    Fps,
    Level,
}

fn tone_label(language: Language, tone_hz: f32) -> SharedString {
    if tone_hz <= 0.0 {
        SharedString::from(t(language, "patterns.tone_mute"))
    } else {
        SharedString::from(format!("{tone_hz:.0} Hz"))
    }
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

fn quality_label(quality: Quality) -> &'static str {
    match quality {
        Quality::Low => "LQ",
        Quality::High => "HQ",
        Quality::Medium | Quality::Default => "SQ",
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

fn persist_test_patterns_prefs(paths: &[PathBuf], frame_buffer_frames: u32) {
    let cfg = TestPatternsConfig {
        schema_version: 1,
        custom_images: paths.to_vec(),
        frame_buffer_frames: clamp_video_frame_buffer_frames(frame_buffer_frames) as u32,
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
                app_id: Some(suite_core::ToolId::TestPatterns.binary_name().into()),
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
    quality: Quality,
    animate: bool,
    anim_speed_h_pct: i32,
    anim_speed_v_pct: i32,
    /// Tone frequency in Hz; `0` means mute.
    tone_hz: f32,
    level_dbfs: f32,
    sample_rate: i32,
    channels: i32,
    samples: i32,
    /// Prefetch depth for paced video sends (persisted).
    frame_buffer_frames: u32,
    custom_images: Vec<CustomImage>,
    selected_custom: Option<usize>,
    session: Option<SendSession>,
    live: Arc<Mutex<LivePattern>>,
    last_stats: SendStats,
    error: Option<SharedString>,
    thumbs: Vec<(PatternKind, Option<Arc<RenderImage>>)>,
    preview: Option<Arc<RenderImage>>,
    /// Preview-sized UYVY cache for the selected custom image (scroll source).
    preview_image_uyvy: Option<Arc<Vec<u8>>>,
    preview_phase_x: f32,
    preview_phase_y: f32,
    last_preview_at: Instant,
    window_title: SharedString,
    /// Open dropdown and its window-relative left anchor (survives control-bar wrap).
    open_menu: Option<(MenuKind, f32)>,
    /// Right-click menu: image index and window-relative anchor.
    custom_menu: Option<(usize, f32, f32)>,
    /// Pending native file dialog result (must not block the GPUI UI thread).
    image_pick_rx: Option<Receiver<Option<PathBuf>>>,
    focus_handle: FocusHandle,
    name_editing: bool,
}

impl PatternsView {
    fn new(cx: &mut Context<Self>, language: Language) -> Self {
        let thumbs = PatternKind::builtins()
            .iter()
            .copied()
            .map(|kind| (kind, pattern_thumb(kind)))
            .collect();

        let saved_cfg = load_test_patterns_config().unwrap_or_default();
        let frame_buffer_frames =
            clamp_video_frame_buffer_frames(saved_cfg.frame_buffer_frames) as u32;
        let saved = saved_cfg.custom_images;
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
            persist_test_patterns_prefs(&kept_paths, frame_buffer_frames);
        }

        let mut view = Self {
            language,
            name: "Test Pattern".into(),
            kind: PatternKind::SmpteColorBars,
            width: 1920,
            height: 1080,
            frame_rate: FrameRate {
                n: 30_000,
                d: 1_001,
            },
            quality: Quality::Medium,
            animate: true,
            anim_speed_h_pct: 100,
            anim_speed_v_pct: 100,
            tone_hz: 1000.0,
            level_dbfs: -20.0,
            sample_rate: 48_000,
            channels: 2,
            samples: 480,
            frame_buffer_frames,
            custom_images,
            selected_custom: None,
            session: None,
            live: Arc::new(Mutex::new(LivePattern::from_view(
                PatternKind::SmpteColorBars,
                true,
                100,
                100,
                None,
            ))),
            last_stats: SendStats::default(),
            error: None,
            thumbs,
            preview: None,
            preview_image_uyvy: None,
            preview_phase_x: 0.0,
            preview_phase_y: 0.0,
            last_preview_at: Instant::now() - Duration::from_secs(1),
            window_title: SharedString::from(""),
            open_menu: None,
            custom_menu: None,
            image_pick_rx: None,
            focus_handle: cx.focus_handle(),
            name_editing: false,
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
        // Animate generated patterns and custom images at the selected output frame rate.
        // Static image stills skip the tick until Animate is enabled.
        if self.kind != PatternKind::Image || self.animate {
            let frame_interval = Duration::from_secs_f64(
                self.frame_rate.d.max(1) as f64 / self.frame_rate.n.max(1) as f64,
            );
            if self.last_preview_at.elapsed() >= frame_interval {
                if self.animate {
                    self.preview_phase_x =
                        advance_scroll_phase(self.preview_phase_x, self.anim_speed_h_pct as f32);
                    self.preview_phase_y =
                        advance_scroll_phase(self.preview_phase_y, self.anim_speed_v_pct as f32);
                }
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
        self.window_title =
            SharedString::from(format!("OMT Test Patterns - {pattern} ({res}{fps})"));
    }

    fn select_pattern(&mut self, kind: PatternKind, cx: &mut Context<Self>) {
        self.custom_menu = None;
        self.open_menu = None;
        if kind == PatternKind::Image || self.kind == kind {
            return;
        }
        self.kind = kind;
        self.selected_custom = None;
        self.preview_image_uyvy = None;
        self.push_live_content(!self.animate);
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
        let preview_uyvy = match uyvy_from_image_path(&path, PREVIEW_W, PREVIEW_H) {
            Ok(buf) => Arc::new(buf),
            Err(e) => {
                self.error = Some(SharedString::from(e.to_string()));
                cx.notify();
                return;
            }
        };
        self.preview_image_uyvy = Some(preview_uyvy);
        self.selected_custom = Some(index);
        self.kind = PatternKind::Image;
        self.error = None;
        self.push_live_content(true);
        self.refresh_preview(cx);
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

        let was_selected =
            self.kind == PatternKind::Image && self.selected_custom.is_some_and(|i| i == index);
        if let Some(sel) = self.selected_custom.as_mut()
            && *sel > index
        {
            *sel -= 1;
        }
        if was_selected {
            self.selected_custom = None;
            self.preview_image_uyvy = None;
            self.kind = PatternKind::SmpteColorBars;
            self.push_live_content(!self.animate);
            self.refresh_preview(cx);
        }
        cx.notify();
    }

    fn persist_images(&self) {
        let paths: Vec<_> = self
            .custom_images
            .iter()
            .map(|img| img.path.clone())
            .collect();
        persist_test_patterns_prefs(&paths, self.frame_buffer_frames);
    }

    fn nudge_frame_buffer(&mut self, delta: i32, cx: &mut Context<Self>) {
        let next = (self.frame_buffer_frames as i32 + delta).clamp(
            MIN_VIDEO_FRAME_BUFFER_FRAMES as i32,
            MAX_VIDEO_FRAME_BUFFER_FRAMES as i32,
        ) as u32;
        if next == self.frame_buffer_frames {
            cx.notify();
            return;
        }
        self.frame_buffer_frames = next;
        self.persist_images();
        if let Some(session) = self.session.as_ref() {
            session.set_frame_buffer_frames(next);
        }
        cx.notify();
    }

    fn open_custom_menu(&mut self, index: usize, x: f32, y: f32, cx: &mut Context<Self>) {
        self.open_menu = None;
        self.custom_menu = Some((index, x, y));
        cx.notify();
    }

    fn reveal_custom_image(&mut self, index: usize, cx: &mut Context<Self>) {
        self.custom_menu = None;
        if let Some(entry) = self.custom_images.get(index)
            && let Err(e) = reveal_in_file_manager(&entry.path)
        {
            self.error = Some(SharedString::from(e.to_string()));
        }
        cx.notify();
    }

    fn close_overlays(&mut self, cx: &mut Context<Self>) {
        self.open_menu = None;
        self.custom_menu = None;
        self.blur_name(cx);
    }

    fn blur_name(&mut self, cx: &mut Context<Self>) {
        if !self.name_editing {
            cx.notify();
            return;
        }
        self.name_editing = false;
        if self.name.trim().is_empty() {
            self.name = "Test Pattern".into();
        }
        cx.notify();
    }

    fn begin_edit_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.is_some() {
            cx.notify();
            return;
        }
        self.open_menu = None;
        self.custom_menu = None;
        self.name_editing = true;
        self.focus_handle.focus(window);
        cx.notify();
    }

    fn on_name_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if !self.name_editing {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "backspace" {
            self.name.pop();
            cx.notify();
            return;
        }
        if key == "enter" || key == "escape" {
            self.blur_name(cx);
            return;
        }
        if let Some(ch) = event.keystroke.key_char.as_deref()
            && ch.chars().all(|c| !c.is_control())
            && self.name.len() + ch.len() <= 64
        {
            self.name.push_str(ch);
            cx.notify();
        }
    }

    fn set_tone(&mut self, tone: TonePreset, cx: &mut Context<Self>) {
        self.open_menu = None;
        let hz = tone.hz();
        if (self.tone_hz - hz).abs() < f32::EPSILON {
            cx.notify();
            return;
        }
        self.tone_hz = hz;
        self.push_live_audio();
        cx.notify();
    }

    fn set_frame_rate(&mut self, frame_rate: FrameRate, cx: &mut Context<Self>) {
        self.open_menu = None;
        if self.session.is_some() {
            cx.notify();
            return;
        }
        if self.frame_rate == frame_rate {
            cx.notify();
            return;
        }
        self.frame_rate = frame_rate;
        cx.notify();
    }

    fn set_resolution(&mut self, resolution: Resolution, cx: &mut Context<Self>) {
        self.open_menu = None;
        if self.session.is_some() {
            cx.notify();
            return;
        }
        if self.width == resolution.width && self.height == resolution.height {
            cx.notify();
            return;
        }
        self.width = resolution.width;
        self.height = resolution.height;
        self.refresh_title();
        cx.notify();
    }

    fn nudge_width(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.session.is_some() {
            cx.notify();
            return;
        }
        let next = (self.width + delta).clamp(64, 7680);
        // Keep even width for UYVY.
        let next = next - (next % 2);
        if next == self.width {
            cx.notify();
            return;
        }
        self.width = next;
        self.refresh_title();
        cx.notify();
    }

    fn nudge_height(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.session.is_some() {
            cx.notify();
            return;
        }
        let next = (self.height + delta).clamp(64, 4320);
        if next == self.height {
            cx.notify();
            return;
        }
        self.height = next;
        self.refresh_title();
        cx.notify();
    }

    fn toggle_animate(&mut self, cx: &mut Context<Self>) {
        self.animate = !self.animate;
        if !self.animate {
            self.preview_phase_x = 0.0;
            self.preview_phase_y = 0.0;
        }
        self.push_live_content(true);
        self.refresh_preview(cx);
        cx.notify();
    }

    fn nudge_anim_speed_h(&mut self, delta: i32, cx: &mut Context<Self>) {
        let next = (self.anim_speed_h_pct + delta).clamp(-200, 200);
        if next == self.anim_speed_h_pct {
            cx.notify();
            return;
        }
        self.anim_speed_h_pct = next;
        self.push_live_speeds();
        cx.notify();
    }

    fn nudge_anim_speed_v(&mut self, delta: i32, cx: &mut Context<Self>) {
        let next = (self.anim_speed_v_pct + delta).clamp(-200, 200);
        if next == self.anim_speed_v_pct {
            cx.notify();
            return;
        }
        self.anim_speed_v_pct = next;
        self.push_live_speeds();
        cx.notify();
    }

    fn set_level(&mut self, level: LevelPreset, cx: &mut Context<Self>) {
        self.open_menu = None;
        let dbfs = level.dbfs();
        if (self.level_dbfs - dbfs).abs() < f32::EPSILON {
            cx.notify();
            return;
        }
        self.level_dbfs = dbfs;
        self.push_live_audio();
        cx.notify();
    }

    fn toggle_menu(&mut self, menu: MenuKind, anchor_x: f32, cx: &mut Context<Self>) {
        self.custom_menu = None;
        self.name_editing = false;
        if self.session.is_some() && matches!(menu, MenuKind::Resolution | MenuKind::Fps) {
            if self.open_menu.map(|(kind, _)| kind) == Some(menu) {
                self.open_menu = None;
            }
            cx.notify();
            return;
        }
        self.open_menu = if self.open_menu.map(|(kind, _)| kind) == Some(menu) {
            None
        } else {
            Some((menu, anchor_x))
        };
        cx.notify();
    }

    fn set_quality(&mut self, quality: Quality, cx: &mut Context<Self>) {
        if self.quality == quality {
            return;
        }
        self.quality = quality;
        if let Some(session) = self.session.as_ref() {
            session.set_quality(quality);
        }
        cx.notify();
    }

    fn start_sending(&mut self, cx: &mut Context<Self>) {
        self.open_menu = None;
        self.custom_menu = None;
        self.name_editing = false;
        if self.name.trim().is_empty() {
            self.name = "Test Pattern".into();
        }
        self.restart_session();
        cx.notify();
    }

    fn stop_sending(&mut self, cx: &mut Context<Self>) {
        self.open_menu = None;
        self.custom_menu = None;
        self.stop();
        cx.notify();
    }

    fn current_image_path(&self) -> Option<&Path> {
        self.selected_custom
            .and_then(|i| self.custom_images.get(i))
            .map(|img| img.path.as_path())
    }

    fn audio_config(&self) -> AudioToneConfig {
        AudioToneConfig {
            sample_rate: self.sample_rate,
            channels: self.channels,
            tone_hz: self.tone_hz,
            level_dbfs: self.level_dbfs,
            samples: self.samples,
        }
    }

    fn load_image_uyvy(&self) -> std::result::Result<Arc<Vec<u8>>, SharedString> {
        match self.current_image_path() {
            Some(path) => match uyvy_from_image_path(path, self.width, self.height) {
                Ok(buf) => Ok(Arc::new(buf)),
                Err(e) => Err(SharedString::from(e.to_string())),
            },
            None => Err(SharedString::from("Select an image file first")),
        }
    }

    /// Push pattern / image / animation into the shared provider state.
    fn push_live_content(&mut self, invalidate_still: bool) -> bool {
        let image_uyvy = if self.kind == PatternKind::Image {
            match self.load_image_uyvy() {
                Ok(buf) => Some(buf),
                Err(e) => {
                    self.error = Some(e);
                    return false;
                }
            }
        } else {
            None
        };
        let animate = self.animate;
        {
            let mut live = self.live.lock();
            live.kind = self.kind;
            live.image_uyvy = image_uyvy;
            live.animate = animate;
            live.speed_h = self.anim_speed_h_pct.clamp(-200, 200) as f32 / 100.0;
            live.speed_v = self.anim_speed_v_pct.clamp(-200, 200) as f32 / 100.0;
            if !animate {
                live.phase_x = 0.0;
                live.phase_y = 0.0;
            }
        }
        if let Some(session) = self.session.as_ref() {
            session.set_animate(animate);
            if invalidate_still || !animate {
                session.invalidate_content();
            }
        }
        true
    }

    fn push_live_speeds(&self) {
        let mut live = self.live.lock();
        live.speed_h = self.anim_speed_h_pct.clamp(-200, 200) as f32 / 100.0;
        live.speed_v = self.anim_speed_v_pct.clamp(-200, 200) as f32 / 100.0;
    }

    fn push_live_audio(&self) {
        if let Some(session) = self.session.as_ref() {
            session.update_audio(self.audio_config());
        }
    }

    fn restart_session(&mut self) {
        self.stop();
        self.error = None;
        {
            let mut live = self.live.lock();
            live.phase_x = 0.0;
            live.phase_y = 0.0;
        }
        if !self.push_live_content(true) {
            return;
        }

        let width = self.width;
        let height = self.height;
        let live = Arc::clone(&self.live);
        let provider: Arc<dyn Fn(u64) -> Vec<u8> + Send + Sync> = Arc::new(move |_idx| {
            let (kind, phase_x, phase_y, image) = {
                let mut state = live.lock();
                let kind = state.kind;
                let image = state.image_uyvy.clone();
                let (phase_x, phase_y) = if state.animate {
                    let px = state.phase_x;
                    let py = state.phase_y;
                    state.phase_x =
                        (state.phase_x + state.speed_h / ANIM_BASE_CYCLE_FRAMES).rem_euclid(1.0);
                    state.phase_y =
                        (state.phase_y + state.speed_v / ANIM_BASE_CYCLE_FRAMES).rem_euclid(1.0);
                    (px, py)
                } else {
                    (0.0, 0.0)
                };
                (kind, phase_x, phase_y, image)
            };
            if let Some(still) = image {
                if phase_x == 0.0 && phase_y == 0.0 {
                    return still.as_ref().clone();
                }
                let mut buf = vec![0u8; (width as usize) * 2 * (height as usize)];
                scroll_uyvy(still.as_ref(), &mut buf, width, height, phase_x, phase_y);
                return buf;
            }
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
            quality: self.quality,
            animate: self.animate,
            frame_buffer_frames: self.frame_buffer_frames,
            audio: self.audio_config(),
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
        let (phase_x, phase_y) = if self.animate {
            (self.preview_phase_x, self.preview_phase_y)
        } else {
            (0.0, 0.0)
        };
        let uyvy = if self.kind == PatternKind::Image {
            let Some(src) = self.preview_image_uyvy.as_ref() else {
                return;
            };
            if phase_x == 0.0 && phase_y == 0.0 {
                src.as_ref().clone()
            } else {
                let mut buf = vec![0u8; (PREVIEW_W as usize) * 2 * (PREVIEW_H as usize)];
                scroll_uyvy(
                    src.as_ref(),
                    &mut buf,
                    PREVIEW_W,
                    PREVIEW_H,
                    phase_x,
                    phase_y,
                );
                buf
            }
        } else {
            let mut buf = vec![0u8; (PREVIEW_W as usize) * 2 * (PREVIEW_H as usize)];
            fill_uyvy(self.kind, &mut buf, PREVIEW_W, PREVIEW_H, phase_x, phase_y);
            buf
        };
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

impl Focusable for PatternsView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PatternsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language;
        let kind = self.kind;
        let tone_hz = self.tone_hz;
        let frame_rate = self.frame_rate;
        let quality = self.quality;
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
        let name = self.name.clone();
        let name_editing = self.name_editing;
        let width = self.width;
        let height = self.height;
        let animate = self.animate;
        let speed_h = self.anim_speed_h_pct;
        let speed_v = self.anim_speed_v_pct;
        let frame_buffer_frames = self.frame_buffer_frames;
        let level_dbfs = self.level_dbfs;

        let mut root =
            div()
                .relative()
                .flex()
                .flex_col()
                .size_full()
                .font(ui_font())
                .bg(rgb(0x1a1d23))
                .text_color(rgb(0xedf2f7))
                .track_focus(&self.focus_handle)
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    this.on_name_key(event, cx);
                }))
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
                        .gap_3()
                        .min_w_0()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_sm()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .child(title),
                        )
                        .child(
                            div()
                                .text_xs()
                                .opacity(0.7)
                                .flex_shrink_0()
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
                        .min_w_0()
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
                        // Stats + settings side panel
                        .child(
                            div()
                                .w(px(SIDE_PANEL_W))
                                .min_w(px(SIDE_PANEL_MIN_W))
                                .max_w_full()
                                .flex_shrink()
                                .h_full()
                                .min_h_0()
                                .id("side-panel")
                                .overflow_y_scroll()
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
                                .child(div().h(px(8.0)))
                                .child(stat_row(
                                    t(language, "simd"),
                                    SimdCapabilities::detect().summary(),
                                ))
                                .child(
                                    div()
                                        .mt_2()
                                        .min_h(px(36.0))
                                        .text_xs()
                                        .text_color(rgb(0xf6c344))
                                        .opacity(if stats.behind { 1.0 } else { 0.0 })
                                        .child(t(language, "patterns.perf_warn")),
                                )
                                .children(error.map(|e| {
                                    div().mt_2().text_xs().text_color(rgb(0xff6b6b)).child(e)
                                }))
                                .child(div().mt_3().mb_1().h(px(1.0)).bg(rgb(0x2a3340)))
                                .child(
                                    div()
                                        .font_weight(FontWeight::BOLD)
                                        .child(t(language, "patterns.settings")),
                                )
                                .child(name_field(cx, language, &name, name_editing, sending))
                                .children(sending.then(|| {
                                    div()
                                        .text_xs()
                                        .text_color(rgb(0xf6c344))
                                        .child(t(language, "patterns.restart_required"))
                                }))
                                .child(toggle_row(
                                    cx,
                                    "animate-toggle",
                                    t(language, "patterns.animate"),
                                    animate,
                                    |this, cx| this.toggle_animate(cx),
                                ))
                                .child(stepper_row(
                                    cx,
                                    "speed-h",
                                    t(language, "patterns.anim_speed_h"),
                                    format!("{speed_h}%"),
                                    |this, cx| this.nudge_anim_speed_h(-10, cx),
                                    |this, cx| this.nudge_anim_speed_h(10, cx),
                                ))
                                .child(stepper_row(
                                    cx,
                                    "speed-v",
                                    t(language, "patterns.anim_speed_v"),
                                    format!("{speed_v}%"),
                                    |this, cx| this.nudge_anim_speed_v(-10, cx),
                                    |this, cx| this.nudge_anim_speed_v(10, cx),
                                )),
                        ),
                )
                // Bottom control bar — wrap to the viewport; scroll horizontally if still tight.
                .child(
                    div()
                        .id("bottom-bar")
                        .px_4()
                        .pt_3()
                        .pb(px(PREVIEW_BOTTOM_MARGIN))
                        .gap_4()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_end()
                        .min_w_0()
                        .overflow_x_scroll()
                        .border_t_1()
                        .border_color(rgb(0x2a3340))
                        .bg(rgb(0x12161c))
                        .child(resolution_control(
                            cx,
                            language,
                            width,
                            height,
                            open_menu.map(|(kind, _)| kind) == Some(MenuKind::Resolution),
                            sending,
                        ))
                        .child(tone_control(
                            cx,
                            language,
                            tone_hz,
                            open_menu.map(|(kind, _)| kind) == Some(MenuKind::Tone),
                        ))
                        .child(fps_control(
                            cx,
                            language,
                            frame_rate,
                            open_menu.map(|(kind, _)| kind) == Some(MenuKind::Fps),
                            sending,
                        ))
                        .child(frame_buffer_control(cx, language, frame_buffer_frames))
                        .child(quality_control(cx, language, quality))
                        .child(level_control(
                            cx,
                            language,
                            level_dbfs,
                            open_menu.map(|(kind, _)| kind) == Some(MenuKind::Level),
                        ))
                        .child(transport_controls(cx, language, sending))
                        .child(div().flex_1().min_w(px(0.0)))
                        .child(output_preview(language, preview)),
                );

        // Root-level overlays so dropdowns / context menus paint above the main grid.
        if open_menu.is_some() || custom_menu.is_some() {
            root = root.child(overlay_layer(
                cx,
                language,
                open_menu,
                tone_hz,
                frame_rate,
                level_dbfs,
                width,
                height,
                custom_menu,
            ));
        }

        root
    }
}

#[allow(clippy::too_many_arguments)]
fn overlay_layer(
    cx: &mut Context<PatternsView>,
    language: Language,
    open_menu: Option<(MenuKind, f32)>,
    tone_hz: f32,
    frame_rate: FrameRate,
    level_dbfs: f32,
    width: i32,
    height: i32,
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

    if let Some((menu, anchor_x)) = open_menu {
        let menu_width = match menu {
            MenuKind::Resolution => 168.0,
            MenuKind::Tone => 160.0,
            MenuKind::Fps => 100.0,
            MenuKind::Level => 120.0,
        };
        // Keep the menu near the control that opened it, even when the footer wraps.
        let left = (anchor_x - 8.0).max(8.0);
        let menu_div = div()
            .absolute()
            .w(px(menu_width))
            .bottom(px(72.0))
            .left(px(left))
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(0x2a3340))
            .bg(rgb(0x1b222c))
            .text_color(rgb(0xedf2f7))
            .shadow_md()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        layer = layer.child(
            menu_div.children(match menu {
                MenuKind::Tone => TonePreset::PRESETS
                    .iter()
                    .map(|preset| {
                        let preset = *preset;
                        let active = preset.matches(tone_hz);
                        dropdown_item(
                            cx,
                            SharedString::from(format!("tone-{}", preset.hz())),
                            preset.label(language),
                            active,
                            move |this, cx| this.set_tone(preset, cx),
                        )
                    })
                    .collect::<Vec<_>>(),
                MenuKind::Fps => FrameRate::PRESETS
                    .iter()
                    .map(|preset| {
                        let preset = *preset;
                        let active = frame_rate == preset;
                        dropdown_item(
                            cx,
                            SharedString::from(format!("fps-{}-{}", preset.n, preset.d)),
                            SharedString::from(preset.label()),
                            active,
                            move |this, cx| this.set_frame_rate(preset, cx),
                        )
                    })
                    .collect::<Vec<_>>(),
                MenuKind::Level => LevelPreset::PRESETS
                    .iter()
                    .map(|preset| {
                        let preset = *preset;
                        let active = preset.matches(level_dbfs);
                        dropdown_item(
                            cx,
                            SharedString::from(format!("level-{}", preset.dbfs() as i32)),
                            preset.label(),
                            active,
                            move |this, cx| this.set_level(preset, cx),
                        )
                    })
                    .collect::<Vec<_>>(),
                MenuKind::Resolution => {
                    let mut items: Vec<gpui::AnyElement> = Resolution::PRESETS
                        .iter()
                        .map(|preset| {
                            let preset = *preset;
                            let active = width == preset.width && height == preset.height;
                            dropdown_item(
                                cx,
                                SharedString::from(format!(
                                    "res-{}-{}",
                                    preset.width, preset.height
                                )),
                                SharedString::from(preset.label()),
                                active,
                                move |this, cx| this.set_resolution(preset, cx),
                            )
                        })
                        .collect();
                    items.push(
                        div()
                            .mt_1()
                            .pt_1()
                            .border_t_1()
                            .border_color(rgb(0x2a3340))
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .px_1()
                                    .text_xs()
                                    .text_color(rgb(0xa0aec0))
                                    .child(format!("W {width}")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .child(step_btn(cx, "res-w-dec", "−", |this, cx| {
                                        this.nudge_width(-2, cx);
                                    }))
                                    .child(step_btn(cx, "res-w-inc", "+", |this, cx| {
                                        this.nudge_width(2, cx);
                                    })),
                            )
                            .child(
                                div()
                                    .px_1()
                                    .text_xs()
                                    .text_color(rgb(0xa0aec0))
                                    .child(format!("H {height}")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_1()
                                    .child(step_btn(cx, "res-h-dec", "−", |this, cx| {
                                        this.nudge_height(-2, cx);
                                    }))
                                    .child(step_btn(cx, "res-h-inc", "+", |this, cx| {
                                        this.nudge_height(2, cx);
                                    })),
                            )
                            .into_any_element(),
                    );
                    items
                }
            }),
        );
    }

    if let Some((index, x, y)) = custom_menu {
        layer = layer.child(
            div()
                .absolute()
                .top(px(y))
                .left(px(x))
                .min_w(px(168.0))
                .p_1()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x2a3340))
                .bg(rgb(0x1b222c))
                .text_color(rgb(0xedf2f7))
                .shadow_md()
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(dropdown_item(
                    cx,
                    SharedString::from("custom-image-reveal"),
                    SharedString::from(t(language, "patterns.image_reveal")),
                    false,
                    move |this, cx| this.reveal_custom_image(index, cx),
                ))
                .child(dropdown_item(
                    cx,
                    SharedString::from("custom-image-remove"),
                    SharedString::from(t(language, "patterns.image_remove")),
                    false,
                    move |this, cx| this.remove_custom_image(index, cx),
                )),
        );
    }

    layer
}

fn dropdown_item<F>(
    cx: &mut Context<PatternsView>,
    id: SharedString,
    label: SharedString,
    active: bool,
    on_click: F,
) -> gpui::AnyElement
where
    F: Fn(&mut PatternsView, &mut Context<PatternsView>) + 'static + Clone,
{
    let handler = on_click.clone();
    let hover_bg = if active { rgb(0x3d7cff) } else { rgb(0x2f3b4d) };
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_sm()
        .bg(if active { rgb(0x2f6fed) } else { rgb(0x1b222c) })
        .text_color(rgb(0xedf2f7))
        .hover(move |s| s.bg(hover_bg).text_color(rgb(0xedf2f7)).opacity(1.0))
        .cursor_pointer()
        .text_xs()
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            handler(this, cx);
        }))
        .into_any_element()
}

fn pattern_grid(
    cx: &mut Context<PatternsView>,
    language: Language,
    selected: PatternKind,
    selected_custom: Option<usize>,
    thumbs: Vec<(PatternKind, Option<Arc<RenderImage>>)>,
    custom_images: Vec<(usize, PathBuf, Option<Arc<RenderImage>>)>,
) -> impl IntoElement {
    let pattern_tiles: Vec<_> = thumbs
        .into_iter()
        .map(|(kind, thumb)| {
            let is_selected = selected != PatternKind::Image && kind == selected;
            pattern_tile(
                cx,
                SharedString::from(kind.id()),
                SharedString::from(pattern_label(language, kind)),
                thumb,
                is_selected,
                move |this, cx| this.select_pattern(kind, cx),
                None::<fn(&mut PatternsView, &MouseDownEvent, &mut Context<PatternsView>)>,
            )
        })
        .collect();

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

    div()
        .flex()
        .flex_col()
        .min_w_0()
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_4()
                .children(pattern_tiles),
        )
        .child(div().mt_2().mb_3().h(px(1.0)).bg(rgb(0x2a3340)))
        .child(
            div()
                .mb_3()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .opacity(0.7)
                .child(t(language, "patterns.image")),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_4()
                .children(custom_cells),
        )
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
        .flex_shrink_0()
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
        .child(
            div()
                .text_sm()
                .text_center()
                .w_full()
                .truncate()
                .child(label),
        )
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
        .flex_shrink_0()
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
    tone_hz: f32,
    open: bool,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .flex_shrink_0()
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
                .child(tone_label(language, tone_hz))
                .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                    let x: f32 = event.position().x.into();
                    this.toggle_menu(MenuKind::Tone, x, cx);
                })),
        )
}

fn transport_controls(
    cx: &mut Context<PatternsView>,
    language: Language,
    sending: bool,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .flex_shrink_0()
        .child(div().text_xs().opacity(0.65).child(" "))
        .child(
            div()
                .flex()
                .gap_2()
                .child(
                    div()
                        .id("patterns-start")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .bg(if sending {
                            rgb(0x1a3a2a)
                        } else {
                            rgb(0x2f6fed)
                        })
                        .opacity(if sending { 0.55 } else { 1.0 })
                        .cursor_pointer()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(t(language, "patterns.start"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !sending {
                                this.start_sending(cx);
                            }
                        })),
                )
                .child(
                    div()
                        .id("patterns-stop")
                        .px_3()
                        .py_1()
                        .rounded_md()
                        .bg(if sending {
                            rgb(0xb33a3a)
                        } else {
                            rgb(0x243041)
                        })
                        .opacity(if sending { 1.0 } else { 0.55 })
                        .cursor_pointer()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(t(language, "patterns.stop"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if sending {
                                this.stop_sending(cx);
                            }
                        })),
                ),
        )
}

fn name_field(
    cx: &mut Context<PatternsView>,
    language: Language,
    name: &str,
    editing: bool,
    locked: bool,
) -> impl IntoElement {
    let display = if editing {
        format!("{name}|")
    } else {
        name.to_string()
    };
    div()
        .flex()
        .flex_col()
        .gap_1()
        .min_w_0()
        .child(
            div()
                .text_xs()
                .opacity(0.65)
                .child(t(language, "patterns.name")),
        )
        .child(
            div()
                .id("source-name")
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(if editing {
                    rgb(0x2f6fed)
                } else {
                    rgb(0x2a3340)
                })
                .bg(rgb(0x0c1016))
                .opacity(if locked { 0.55 } else { 1.0 })
                .cursor_text()
                .text_xs()
                .min_w_0()
                .truncate()
                .child(display)
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !locked {
                        this.begin_edit_name(window, cx);
                    }
                })),
        )
}

fn resolution_control(
    cx: &mut Context<PatternsView>,
    language: Language,
    width: i32,
    height: i32,
    open: bool,
    locked: bool,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .flex_shrink_0()
        .child(
            div()
                .text_xs()
                .opacity(0.65)
                .child(t(language, "patterns.resolution")),
        )
        .child(
            div()
                .id("resolution-toggle")
                .w(px(110.0))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(if open { rgb(0x2f6fed) } else { rgb(0x243041) })
                .opacity(if locked { 0.55 } else { 1.0 })
                .cursor_pointer()
                .text_xs()
                .child(format!("{width}×{height}"))
                .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                    if !locked {
                        let x: f32 = event.position().x.into();
                        this.toggle_menu(MenuKind::Resolution, x, cx);
                    }
                })),
        )
}

fn level_control(
    cx: &mut Context<PatternsView>,
    language: Language,
    level_dbfs: f32,
    open: bool,
) -> impl IntoElement {
    let display = LevelPreset::nearest(level_dbfs).label();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .flex_shrink_0()
        .child(
            div()
                .text_xs()
                .opacity(0.65)
                .child(t(language, "patterns.tone_level")),
        )
        .child(
            div()
                .id("level-toggle")
                .w(px(100.0))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(if open { rgb(0x2f6fed) } else { rgb(0x243041) })
                .cursor_pointer()
                .text_xs()
                .child(display)
                .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                    let x: f32 = event.position().x.into();
                    this.toggle_menu(MenuKind::Level, x, cx);
                })),
        )
}

fn toggle_row<F>(
    cx: &mut Context<PatternsView>,
    id: &'static str,
    label: &str,
    active: bool,
    on_toggle: F,
) -> impl IntoElement
where
    F: Fn(&mut PatternsView, &mut Context<PatternsView>) + 'static + Clone,
{
    let toggle = on_toggle.clone();
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .min_w_0()
        .child(
            div()
                .text_xs()
                .opacity(0.65)
                .min_w_0()
                .flex_1()
                .truncate()
                .child(label.to_string()),
        )
        .child(
            div()
                .id(SharedString::from(id))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(if active { rgb(0x2f6fed) } else { rgb(0x243041) })
                .cursor_pointer()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .flex_shrink_0()
                .child(if active { "ON" } else { "OFF" })
                .on_click(cx.listener(move |this, _, _, cx| {
                    toggle(this, cx);
                })),
        )
}

fn stepper_row<FDec, FInc>(
    cx: &mut Context<PatternsView>,
    id: &'static str,
    label: &str,
    value: String,
    on_dec: FDec,
    on_inc: FInc,
) -> impl IntoElement
where
    FDec: Fn(&mut PatternsView, &mut Context<PatternsView>) + 'static + Clone,
    FInc: Fn(&mut PatternsView, &mut Context<PatternsView>) + 'static + Clone,
{
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .min_w_0()
        .child(
            div()
                .text_xs()
                .opacity(0.65)
                .min_w_0()
                .flex_1()
                .truncate()
                .child(label.to_string()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .flex_shrink_0()
                .child(step_btn(
                    cx,
                    SharedString::from(format!("{id}-dec")),
                    "−",
                    on_dec,
                ))
                .child(
                    div()
                        .min_w(px(64.0))
                        .text_xs()
                        .text_center()
                        .font_weight(FontWeight::MEDIUM)
                        .child(value),
                )
                .child(step_btn(
                    cx,
                    SharedString::from(format!("{id}-inc")),
                    "+",
                    on_inc,
                )),
        )
}

fn step_btn<F>(
    cx: &mut Context<PatternsView>,
    id: impl Into<SharedString>,
    label: &'static str,
    on_click: F,
) -> impl IntoElement
where
    F: Fn(&mut PatternsView, &mut Context<PatternsView>) + 'static + Clone,
{
    let handler = on_click.clone();
    div()
        .id(id.into())
        .w(px(22.0))
        .h(px(22.0))
        .rounded_sm()
        .bg(rgb(0x243041))
        .hover(|s| s.bg(rgb(0x2f3b4d)))
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| {
            handler(this, cx);
        }))
}

fn frame_buffer_control(
    cx: &mut Context<PatternsView>,
    language: Language,
    frames: u32,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .flex_shrink_0()
        .child(
            div()
                .text_xs()
                .opacity(0.65)
                .child(t(language, "patterns.frame_buffer")),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .h(px(24.0))
                .child(step_btn(cx, "frame-buffer-dec", "−", |this, cx| {
                    this.nudge_frame_buffer(-1, cx);
                }))
                .child(
                    div()
                        .w(px(24.0))
                        .text_xs()
                        .text_center()
                        .font_weight(FontWeight::MEDIUM)
                        .child(format!("{frames}")),
                )
                .child(step_btn(cx, "frame-buffer-inc", "+", |this, cx| {
                    this.nudge_frame_buffer(1, cx);
                })),
        )
}

fn fps_control(
    cx: &mut Context<PatternsView>,
    language: Language,
    selected: FrameRate,
    open: bool,
    locked: bool,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .flex_shrink_0()
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
                .opacity(if locked { 0.55 } else { 1.0 })
                .cursor_pointer()
                .text_xs()
                .child(selected.label())
                .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                    if !locked {
                        let x: f32 = event.position().x.into();
                        this.toggle_menu(MenuKind::Fps, x, cx);
                    }
                })),
        )
}

fn quality_control(
    cx: &mut Context<PatternsView>,
    language: Language,
    selected: Quality,
) -> impl IntoElement {
    let qualities = [Quality::Low, Quality::Medium, Quality::High];
    div()
        .flex()
        .flex_col()
        .gap_1()
        .flex_shrink_0()
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
                .children(qualities.into_iter().map(|quality| {
                    let active = selected == quality;
                    div()
                        .id(SharedString::from(quality_label(quality)))
                        .px_3()
                        .py_1()
                        .bg(if active { rgb(0x2f6fed) } else { rgb(0x243041) })
                        .cursor_pointer()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(quality_label(quality))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_quality(quality, cx);
                        }))
                        .into_any_element()
                })),
        )
}

fn output_preview(language: Language, preview: Option<Arc<RenderImage>>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_end()
        .gap_1()
        .flex_shrink_0()
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
        .gap_3()
        .text_xs()
        .child(div().opacity(0.65).flex_shrink_0().child(label.to_string()))
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .min_w_0()
                .flex_1()
                .text_right()
                .child(value),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anim_speed_change_does_not_jump_phase() {
        let start = 0.42;
        let next_slow = advance_scroll_phase(start, 50.0);
        let next_fast = advance_scroll_phase(start, 200.0);
        let delta_slow = (next_slow - start).rem_euclid(1.0);
        let delta_fast = (next_fast - start).rem_euclid(1.0);
        assert!(delta_fast > delta_slow);
        assert!(delta_fast < 0.02);
        assert!((next_slow - start).abs() < 0.01);
    }
}
