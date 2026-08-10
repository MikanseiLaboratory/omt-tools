//! GPUI Studio Monitor UI — sources, zoom preview, stats, overlays, context menu.

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use gpui::{
    actions, canvas, div, img, prelude::*, px, rgb, size, App, Application, Bounds, Context,
    FocusHandle, Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, InteractiveElement,
    KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, Pixels, Point,
    RenderImage, ScrollDelta, ScrollWheelEvent, SharedString, Timer, Window, WindowBounds,
    WindowOptions,
};
use image::{Frame, ImageBuffer, Rgba};
use omt_media::{
    list_output_devices, AudioLevels, AudioOutputDevice, BufferSettings, BufferUnit, DelaySetting,
    ConnectOptions, DiscoveredSource, Quality, ReceiveWorker, StallState, spawn_discover,
};
use openmediatransport::{bgra_alpha_mask, bgra_to_rgba};
use smallvec::smallvec;
use suite_core::{t, Language, ThemePreference, SUITE_VERSION, load_config, save_config};

use crate::chrome::UiChrome;
use crate::menu::{self, ContextMenuState, MenuAction, MenuNodeId};
use crate::preferences;

type DiscoveryResult = Result<Vec<DiscoveredSource>, String>;

const ZOOM_MIN: f32 = 0.1;
const ZOOM_MAX: f32 = 8.0;
const SIDEBAR_W: f32 = 280.0;
const STATS_W: f32 = 280.0;
const LOG_H: f32 = 180.0;
const TOOLBAR_H: f32 = 40.0;

actions!(monitor, [ExitFullscreen]);

/// Receive quality preset (maps to OMT quality + optional preview).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoQualityPreset {
    Default,
    Low,
    Medium,
    High,
    LowBandwidth,
}

impl VideoQualityPreset {
    fn to_connect_parts(self) -> (Quality, bool) {
        match self {
            Self::Default => (Quality::Default, false),
            Self::Low => (Quality::Low, false),
            Self::Medium => (Quality::Medium, false),
            Self::High => (Quality::High, false),
            Self::LowBandwidth => (Quality::Low, true),
        }
    }
}

/// Viewer settings toggled from the context menu.
#[derive(Debug, Clone)]
pub struct MonitorSettings {
    pub show_alpha: bool,
    /// SMPTE ST 2046-1 action/title safe guides over the picture.
    pub safe_area: bool,
    pub vu_meter: bool,
    pub quality: VideoQualityPreset,
    pub audio_boost_db: i32,
    /// Linked or independent A/V playout buffers (PTS gate).
    pub buffer: BufferSettings,
}

impl Default for MonitorSettings {
    fn default() -> Self {
        Self {
            show_alpha: false,
            safe_area: false,
            vu_meter: true,
            quality: VideoQualityPreset::Default,
            audio_boost_db: 0,
            buffer: BufferSettings::default(),
        }
    }
}

/// Windows-installed UI fonts only (avoids DirectWrite “No matching font” spam).
fn ui_font() -> Font {
    Font {
        family: "Segoe UI".into(),
        features: FontFeatures::default(),
        fallbacks: Some(FontFallbacks::from_fonts(vec![
            "Yu Gothic UI".into(),
            "Meiryo UI".into(),
            "MS UI Gothic".into(),
            "Segoe UI".into(),
        ])),
        weight: FontWeight::default(),
        style: FontStyle::default(),
    }
}

fn rgba_to_render_image(rgba: Vec<u8>, width: u32, height: u32) -> Option<Arc<RenderImage>> {
    let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, rgba)?;
    Some(Arc::new(RenderImage::new(smallvec![Frame::new(buffer)])))
}

fn format_bitrate(bps: f64) -> String {
    if bps >= 1_000_000.0 {
        format!("{:.2} Mbps", bps / 1_000_000.0)
    } else if bps >= 1_000.0 {
        format!("{:.1} kbps", bps / 1_000.0)
    } else {
        format!("{bps:.0} bps")
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

pub fn run_gpui(
    title: String,
    language: Language,
    theme: ThemePreference,
    initial_url: Option<String>,
) -> Result<()> {
    Application::new().run(move |cx: &mut App| {
        cx.bind_keys([KeyBinding::new("escape", ExitFullscreen, None)]);
        let bounds = Bounds::centered(None, size(px(1440.0), px(860.0)), cx);
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
            move |window, cx| {
                cx.new(|cx| {
                    let view = MonitorView::new(cx, language, theme, initial_url.clone());
                    view.focus_handle.focus(window);
                    view
                })
            },
        )
        .expect("open GPUI Studio Monitor window");
        cx.activate(true);
    });
    Ok(())
}

pub struct MonitorView {
    language: Language,
    theme: ThemePreference,
    suite_version: String,
    preferences_open: bool,
    audio_devices: Vec<AudioOutputDevice>,
    audio_output_device: Option<String>,
    worker: ReceiveWorker,
    /// Raw discovered sources (host-grouped in the context menu).
    discovered: Vec<DiscoveredSource>,
    selected: Option<SharedString>,
    status: SharedString,
    frame_w: u32,
    frame_h: u32,
    fps_n: i32,
    fps_d: i32,
    display_fps: f32,
    frames_presented: u64,
    frames_render_skipped: u64,
    last_refresh: Instant,
    last_frame_at: Option<Instant>,
    window_fps_count: u32,
    window_fps_start: Instant,
    texture: Option<Arc<RenderImage>>,
    discovering: bool,
    discovery_rx: Option<Receiver<DiscoveryResult>>,
    refresh_silent: bool,
    /// Zoom relative to panel-fit (1.0 = fit preview panel).
    zoom: f32,
    /// Measured preview viewport size in CSS pixels.
    preview_w: f32,
    preview_h: f32,
    bitrate_bps: f64,
    last_bytes_received: i64,
    last_bitrate_at: Instant,
    source_dropped: u64,
    net_dropped: i64,
    frames_decoded: u64,
    audio_frames: u64,
    audio_levels: AudioLevels,
    video_buffer_delay_ms: u32,
    audio_buffer_delay_ms: u32,
    bytes_received: i64,
    /// Pan offset in CSS pixels (drag to move when zoomed).
    pan_x: f32,
    pan_y: f32,
    /// Last mouse position while dragging to pan.
    pan_drag: Option<(f32, f32)>,
    log_lines: VecDeque<SharedString>,
    log_last_unix_ms: u64,
    stall_text: SharedString,
    pub(crate) settings: MonitorSettings,
    pub(crate) context_menu: Option<ContextMenuState>,
    /// Last raw BGRA retained so overlay toggles can rebuild without a new frame.
    last_bgra: Option<Vec<u8>>,
    fullscreen: bool,
    focus_handle: FocusHandle,
}

impl MonitorView {
    fn new(
        cx: &mut Context<Self>,
        language: Language,
        theme: ThemePreference,
        initial_url: Option<String>,
    ) -> Self {
        let worker = ReceiveWorker::spawn();
        let settings = MonitorSettings::default();
        worker.set_buffer(settings.buffer);
        worker.set_audio_boost_db(settings.audio_boost_db);
        if let Some(url) = &initial_url {
            worker.connect(url.clone());
        }
        let suite_version = std::env::var(suite_core::env::SUITE_VERSION)
            .unwrap_or_else(|_| SUITE_VERSION.to_string());
        let mut view = Self {
            language,
            theme,
            suite_version,
            preferences_open: false,
            audio_devices: list_output_devices(),
            audio_output_device: None,
            worker,
            discovered: Vec::new(),
            selected: initial_url.map(SharedString::from),
            status: SharedString::from(""),
            frame_w: 0,
            frame_h: 0,
            fps_n: 0,
            fps_d: 1,
            display_fps: 0.0,
            frames_presented: 0,
            frames_render_skipped: 0,
            last_refresh: Instant::now() - Duration::from_secs(10),
            last_frame_at: None,
            window_fps_count: 0,
            window_fps_start: Instant::now(),
            texture: None,
            discovering: false,
            discovery_rx: None,
            refresh_silent: true,
            zoom: 1.0,
            preview_w: 0.0,
            preview_h: 0.0,
            bitrate_bps: 0.0,
            last_bytes_received: 0,
            last_bitrate_at: Instant::now(),
            source_dropped: 0,
            net_dropped: 0,
            frames_decoded: 0,
            audio_frames: 0,
            audio_levels: AudioLevels::default(),
            video_buffer_delay_ms: settings.buffer.video_delay_ms(30, 1),
            audio_buffer_delay_ms: settings.buffer.audio_delay_ms(30, 1),
            bytes_received: 0,
            pan_x: 0.0,
            pan_y: 0.0,
            pan_drag: None,
            log_lines: VecDeque::new(),
            log_last_unix_ms: 0,
            stall_text: SharedString::from(t(language, "monitor.waiting")),
            settings,
            context_menu: None,
            last_bgra: None,
            fullscreen: false,
            focus_handle: cx.focus_handle(),
        };
        view.request_refresh(true, cx);
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

    fn request_refresh(&mut self, silent: bool, _cx: &mut Context<Self>) {
        if self.discovering {
            return;
        }
        self.discovering = true;
        self.refresh_silent = silent;
        if !silent {
            self.status = SharedString::from(t(self.language, "monitor.refresh"));
        }
        self.discovery_rx = Some(spawn_discover(Duration::from_millis(1500)));
        self.last_refresh = Instant::now();
    }

    fn poll_discovery(&mut self) {
        let Some(rx) = self.discovery_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(list)) => {
                self.discovered = list;
                if !self.refresh_silent {
                    self.status = if self.discovered.is_empty() {
                        SharedString::from(t(self.language, "monitor.no_sources"))
                    } else {
                        SharedString::from(format!("{} source(s)", self.discovered.len()))
                    };
                }
                self.discovering = false;
                self.discovery_rx = None;
            }
            Ok(Err(err)) => {
                if !self.refresh_silent {
                    self.status = SharedString::from(err);
                }
                self.discovering = false;
                self.discovery_rx = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                if !self.refresh_silent {
                    self.status = SharedString::from("discovery task ended");
                }
                self.discovering = false;
                self.discovery_rx = None;
            }
        }
    }

    fn ingest_logs(&mut self) {
        let entries = self.worker.latest().metadata_since(self.log_last_unix_ms);
        for entry in entries {
            self.log_last_unix_ms = self.log_last_unix_ms.max(entry.unix_ms);
            let line = SharedString::from(format!(
                "[{}] {}: {}",
                entry.unix_ms % 100_000,
                entry.kind,
                entry.text
            ));
            // Realtime: keep only the latest line.
            self.log_lines.clear();
            self.log_lines.push_back(line);
        }
    }

    fn present_bgra(&mut self, bgra: Vec<u8>, width: u32, height: u32, cx: &mut Context<Self>) {
        let rgba = if self.settings.show_alpha {
            bgra_alpha_mask(&bgra)
        } else {
            bgra_to_rgba(&bgra)
        };
        self.last_bgra = Some(bgra);
        match rgba_to_render_image(rgba, width, height) {
            Some(image) => {
                if let Some(old) = self.texture.take() {
                    cx.drop_image(old, None);
                }
                self.texture = Some(image);
                self.frames_presented += 1;
            }
            None => {
                self.frames_render_skipped += 1;
            }
        }
    }

    pub(crate) fn invalidate_texture(&mut self, cx: &mut Context<Self>) {
        if let (Some(bgra), w, h) = (self.last_bgra.clone(), self.frame_w, self.frame_h) {
            if w > 0 && h > 0 {
                self.present_bgra(bgra, w, h, cx);
            }
        }
    }

    pub(crate) fn reapply_connection(&mut self, cx: &mut Context<Self>) {
        if let Some(url) = self.selected.clone() {
            self.connect_url(url.to_string(), cx);
        }
    }

    pub(crate) fn set_audio_boost_db(&mut self, db: i32) {
        self.settings.audio_boost_db = db;
        self.worker.set_audio_boost_db(db);
    }

    fn buffer_fps(&self) -> (i32, i32) {
        if self.fps_n > 0 {
            (self.fps_n, self.fps_d.max(1))
        } else {
            (30, 1)
        }
    }

    pub(crate) fn set_video_delay(&mut self, delay: DelaySetting) {
        let (fps_n, fps_d) = self.buffer_fps();
        self.settings.buffer.set_video(delay, fps_n, fps_d);
        self.worker.set_buffer(self.settings.buffer);
    }

    pub(crate) fn set_audio_delay(&mut self, delay: DelaySetting) {
        let (fps_n, fps_d) = self.buffer_fps();
        self.settings.buffer.set_audio(delay, fps_n, fps_d);
        self.worker.set_buffer(self.settings.buffer);
    }

    pub(crate) fn toggle_buffer_link(&mut self) {
        let (fps_n, fps_d) = self.buffer_fps();
        let linked = !self.settings.buffer.linked;
        self.settings.buffer.set_linked(linked, fps_n, fps_d);
        self.worker.set_buffer(self.settings.buffer);
    }

    pub(crate) fn set_buffer_link(&mut self, linked: bool) {
        let (fps_n, fps_d) = self.buffer_fps();
        self.settings.buffer.set_linked(linked, fps_n, fps_d);
        self.worker.set_buffer(self.settings.buffer);
    }

    fn connect_url(&mut self, url: String, _cx: &mut Context<Self>) {
        let (quality, low_bandwidth) = self.settings.quality.to_connect_parts();
        self.worker.connect_with(ConnectOptions {
            url,
            quality,
            low_bandwidth,
        });
    }

    pub(crate) fn disconnect_source(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.worker.disconnect();
        self.last_bgra = None;
        self.frame_w = 0;
        self.frame_h = 0;
        if let Some(old) = self.texture.take() {
            cx.drop_image(old, None);
        }
        self.status = SharedString::from(t(self.language, "monitor.none"));
    }

    pub(crate) fn close_context_menu(&mut self, cx: &mut Context<Self>) {
        self.context_menu = None;
        cx.notify();
    }

    pub(crate) fn expand_menu_node(&mut self, node: MenuNodeId, cx: &mut Context<Self>) {
        let Some(menu) = self.context_menu.as_mut() else {
            return;
        };
        let depth = match &node {
            MenuNodeId::Settings => 0,
            MenuNodeId::Audio | MenuNodeId::Video | MenuNodeId::Overlay => 1,
            MenuNodeId::AudioBoost | MenuNodeId::AvBuffer | MenuNodeId::VideoQuality => 2,
            MenuNodeId::VideoBuffer | MenuNodeId::AudioBuffer => 3,
        };
        menu.path.truncate(depth);
        menu.path.push(node);
        cx.notify();
    }

    pub(crate) fn apply_menu_action(
        &mut self,
        action: MenuAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        menu::dispatch_action(self, action, window, cx);
    }

    pub(crate) fn enter_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fullscreen = true;
        self.context_menu = None;
        self.preferences_open = false;
        let vp = window.viewport_size();
        let vw: f32 = vp.width.into();
        let vh: f32 = vp.height.into();
        if vw > 1.0 && vh > 1.0 {
            self.preview_w = vw;
            self.preview_h = vh;
        }
        if !window.is_fullscreen() {
            window.toggle_fullscreen();
        }
        cx.notify();
    }

    pub(crate) fn exit_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.fullscreen {
            self.fullscreen = false;
            if window.is_fullscreen() {
                window.toggle_fullscreen();
            }
            cx.notify();
        }
    }

    pub(crate) fn open_preferences(&mut self, cx: &mut Context<Self>) {
        self.preferences_open = true;
        self.context_menu = None;
        self.audio_devices = list_output_devices();
        self.audio_output_device = self.worker.audio_output_device();
        cx.notify();
    }

    pub(crate) fn close_preferences(&mut self, cx: &mut Context<Self>) {
        self.preferences_open = false;
        cx.notify();
    }

    pub(crate) fn set_audio_output_device(
        &mut self,
        name: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.audio_output_device = name.clone();
        self.worker.set_audio_output_device(name);
        cx.notify();
    }

    pub(crate) fn set_language(&mut self, language: Language, cx: &mut Context<Self>) {
        if self.language != language {
            self.language = language;
            self.persist_suite_prefs();
            cx.notify();
        }
    }

    pub(crate) fn set_theme(&mut self, theme: ThemePreference, cx: &mut Context<Self>) {
        if self.theme != theme {
            self.theme = theme;
            self.persist_suite_prefs();
            cx.notify();
        }
    }

    fn persist_suite_prefs(&self) {
        let mut cfg = load_config().unwrap_or_default();
        cfg.language = self.language;
        cfg.theme = self.theme;
        let _ = save_config(&cfg);
    }

    /// Fit video exactly to one axis of the viewport (true contain, no shrink slack).
    fn fit_display_in_viewport(&self, vw: f32, vh: f32) -> (f32, f32) {
        if self.frame_w == 0 || self.frame_h == 0 {
            return (vw.max(1.0), vh.max(1.0));
        }
        let fw = self.frame_w as f32;
        let fh = self.frame_h as f32;
        if vw / vh.max(1.0) > fw / fh {
            let dh = vh;
            let dw = dh * fw / fh;
            (dw.max(1.0), dh.max(1.0))
        } else {
            let dw = vw;
            let dh = dw * fh / fw;
            (dw.max(1.0), dh.max(1.0))
        }
    }

    fn set_preview_size(&mut self, w: f32, h: f32, cx: &mut Context<Self>) {
        if w <= 1.0 || h <= 1.0 {
            return;
        }
        if (self.preview_w - w).abs() > 0.5 || (self.preview_h - h).abs() > 0.5 {
            self.preview_w = w;
            self.preview_h = h;
            cx.notify();
        }
    }

    /// Scale that fits the frame inside the current preview viewport.
    fn fit_scale(&self) -> f32 {
        let (pw, ph) = if self.fullscreen {
            // Prefer measured size; fall back to last known preview size.
            (self.preview_w.max(1.0), self.preview_h.max(1.0))
        } else if self.preview_w > 1.0 && self.preview_h > 1.0 {
            (self.preview_w, self.preview_h)
        } else {
            // Rough fallback until canvas measures the pane.
            (900.0, 500.0)
        };
        if self.frame_w == 0 || self.frame_h == 0 {
            return 1.0;
        }
        (pw / self.frame_w as f32)
            .min(ph / self.frame_h as f32)
            .max(0.01)
    }

    fn display_size(&self) -> (f32, f32) {
        let fit = self.fit_scale();
        let scale = fit * self.zoom;
        (
            (self.frame_w as f32 * scale).max(1.0),
            (self.frame_h as f32 * scale).max(1.0),
        )
    }

    fn on_tick(&mut self, cx: &mut Context<Self>) {
        self.poll_discovery();
        if !self.discovering && self.last_refresh.elapsed() > Duration::from_secs(3) {
            self.request_refresh(true, cx);
        }

        self.ingest_logs();

        {
            let counters = *self.worker.latest().counters.lock();
            self.frames_decoded = counters.frames_decoded;
            self.source_dropped = counters.frames_replaced;
            self.audio_frames = counters.audio_frames;
            self.audio_levels = *self.worker.latest().audio_levels.lock();
            self.video_buffer_delay_ms = *self.worker.latest().video_buffer_delay_ms.lock();
            self.audio_buffer_delay_ms = *self.worker.latest().audio_buffer_delay_ms.lock();
            // Keep linked pair labels in sync when source FPS drifts.
            if self.settings.buffer.linked {
                let (fps_n, fps_d) = self.buffer_fps();
                let before = self.settings.buffer;
                self.settings.buffer.resync_linked(fps_n, fps_d);
                if self.settings.buffer != before {
                    self.worker.set_buffer(self.settings.buffer);
                }
            }
            let stats = *self.worker.latest().stats.lock();
            self.net_dropped = stats.frames_dropped;
            self.bytes_received = stats.bytes_received;
            let elapsed = self.last_bitrate_at.elapsed().as_secs_f64().max(0.001);
            if elapsed >= 0.5 {
                let delta = (stats.bytes_received - self.last_bytes_received).max(0) as f64;
                self.bitrate_bps = delta * 8.0 / elapsed;
                self.last_bytes_received = stats.bytes_received;
                self.last_bitrate_at = Instant::now();
            }
        }

        if let Some(frame) = self.worker.latest().take() {
            self.frame_w = frame.width;
            self.frame_h = frame.height;
            self.fps_n = frame.fps_n;
            self.fps_d = frame.fps_d.max(1);
            self.window_fps_count += 1;
            self.last_frame_at = Some(Instant::now());
            self.present_bgra(frame.bgra, frame.width, frame.height, cx);
        }

        if let Some(err) = self.worker.latest().error.lock().clone() {
            self.status = SharedString::from(err);
        }

        {
            let guard = self.worker.stall();
            let mut d = guard.lock();
            self.stall_text = SharedString::from(match d.tick() {
                StallState::Waiting => t(self.language, "monitor.waiting"),
                StallState::Live => "LIVE",
                StallState::Stalled => t(self.language, "monitor.stalled"),
            });
        }

        if self.window_fps_start.elapsed() >= Duration::from_secs(1) {
            self.display_fps =
                self.window_fps_count as f32 / self.window_fps_start.elapsed().as_secs_f32();
            self.window_fps_count = 0;
            self.window_fps_start = Instant::now();
        }
        cx.notify();
    }

    pub(crate) fn select(&mut self, url: SharedString, cx: &mut Context<Self>) {
        self.selected = Some(url.clone());
        self.connect_url(url.to_string(), cx);
        self.frames_presented = 0;
        self.frames_render_skipped = 0;
        self.frame_w = 0;
        self.frame_h = 0;
        self.last_frame_at = None;
        self.last_bgra = None;
        self.audio_frames = 0;
        self.audio_levels = AudioLevels::default();
        self.log_lines.clear();
        self.log_last_unix_ms = 0;
        self.last_bytes_received = 0;
        self.bitrate_bps = 0.0;
        self.zoom = 1.0;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
        self.pan_drag = None;
        if let Some(old) = self.texture.take() {
            cx.drop_image(old, None);
        }
        self.status = SharedString::from("");
    }

    fn adjust_zoom(&mut self, delta_y: f32) {
        let factor = if delta_y > 0.0 { 1.1 } else { 1.0 / 1.1 };
        self.zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        if (self.zoom - 1.0).abs() < 0.01 {
            self.pan_x = 0.0;
            self.pan_y = 0.0;
        }
    }

    fn zoom_reset(&mut self) {
        self.zoom = 1.0;
        self.pan_x = 0.0;
        self.pan_y = 0.0;
    }

    fn open_context_menu(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.context_menu = Some(menu::open_at(position.x.into(), position.y.into()));
        cx.notify();
    }
}

impl gpui::Focusable for MonitorView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MonitorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let language = self.language;
        let theme = self.theme;
        let chrome = UiChrome::resolve(theme, window.appearance());
        let discovered = self.discovered.clone();
        let selected = self.selected.clone();
        let texture = self.texture.clone();
        let zoom = self.zoom;
        let frame_w = self.frame_w;
        let frame_h = self.frame_h;
        let log_lines: Vec<SharedString> = self.log_lines.iter().cloned().collect();
        let source_fps = if self.fps_d > 0 {
            self.fps_n as f32 / self.fps_d as f32
        } else {
            0.0
        };
        let settings = self.settings.clone();
        let context_menu = self.context_menu.clone();
        let safe_area = settings.safe_area;
        let vu_meter = settings.vu_meter;
        let fullscreen = self.fullscreen;
        let preferences_open = self.preferences_open;
        let suite_version = self.suite_version.clone();
        let audio_devices = self.audio_devices.clone();
        let audio_output_device = self.audio_output_device.clone();
        let entity = cx.entity();
        let audio_levels = self.audio_levels;

        if fullscreen {
            let vp = window.viewport_size();
            let vw: f32 = vp.width.into();
            let vh: f32 = vp.height.into();
            if vw > 1.0 && vh > 1.0 {
                self.preview_w = vw;
                self.preview_h = vh;
            }
            let (display_w, display_h) = self.fit_display_in_viewport(vw.max(1.0), vh.max(1.0));

            let mut root = div()
                .id("fullscreen-root")
                .size_full()
                .bg(rgb(0x000000))
                .font(ui_font())
                .track_focus(&self.focus_handle)
                .on_action(cx.listener(|this, _: &ExitFullscreen, window, cx| {
                    if this.preferences_open {
                        this.close_preferences(cx);
                    } else {
                        this.exit_fullscreen(window, cx);
                    }
                }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.exit_fullscreen(window, cx);
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _, window, cx| {
                        this.exit_fullscreen(window, cx);
                    }),
                )
                .child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(video_stack(
                            texture,
                            frame_w,
                            frame_h,
                            display_w,
                            display_h,
                            safe_area,
                            vu_meter,
                            audio_levels,
                            language,
                        )),
                );

            if preferences_open {
                root = root.child(preferences::render_overlay(
                    language,
                    theme,
                    &suite_version,
                    chrome,
                    &audio_devices,
                    audio_output_device.as_deref(),
                    settings.buffer,
                    self.video_buffer_delay_ms,
                    self.audio_buffer_delay_ms,
                    cx,
                ));
            }
            return root.into_any_element();
        }

        let (display_w, display_h) = self.display_size();

        let mut root = div()
            .flex()
            .flex_col()
            .size_full()
            .font(ui_font())
            .bg(rgb(chrome.bg))
            .text_color(rgb(chrome.text))
            .relative()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &ExitFullscreen, window, cx| {
                if this.preferences_open {
                    this.close_preferences(cx);
                } else {
                    this.exit_fullscreen(window, cx);
                }
            }))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(source_sidebar(
                        cx,
                        language,
                        chrome,
                        &discovered,
                        selected.as_ref(),
                    ))
                    .child(preview_pane(
                        cx,
                        language,
                        chrome,
                        entity,
                        texture,
                        zoom,
                        self.pan_x,
                        self.pan_y,
                        frame_w,
                        frame_h,
                        display_w,
                        display_h,
                        safe_area,
                        vu_meter,
                        audio_levels,
                        &self.stall_text,
                    ))
                    .child(stats_panel(
                        language,
                        chrome,
                        self.display_fps,
                        source_fps,
                        self.frames_presented,
                        self.source_dropped,
                        self.frames_render_skipped,
                        self.net_dropped,
                        self.frames_decoded,
                        self.audio_frames,
                        self.audio_levels,
                        self.video_buffer_delay_ms,
                        self.audio_buffer_delay_ms,
                        &self.settings.buffer,
                        self.frame_w,
                        self.frame_h,
                        self.bitrate_bps,
                        self.bytes_received,
                        selected.as_ref(),
                    )),
            )
            .child(log_panel(cx, language, chrome, &log_lines));

        if let Some(menu) = context_menu.as_ref() {
            root = root.child(menu::render_overlay(&settings, language, menu, cx));
        }
        if preferences_open {
            root = root.child(preferences::render_overlay(
                language,
                theme,
                &suite_version,
                chrome,
                &audio_devices,
                audio_output_device.as_deref(),
                settings.buffer,
                self.video_buffer_delay_ms,
                self.audio_buffer_delay_ms,
                cx,
            ));
        }

        root.into_any_element()
    }
}


fn source_sidebar(
    cx: &mut Context<MonitorView>,
    language: Language,
    chrome: UiChrome,
    discovered: &[DiscoveredSource],
    selected: Option<&SharedString>,
) -> impl IntoElement {
    let none_selected = selected.is_none();
    let mut items: Vec<gpui::AnyElement> = Vec::new();

    items.push(
        div()
            .id("source-none")
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(if none_selected {
                chrome.accent
            } else {
                chrome.border
            }))
            .bg(rgb(if none_selected {
                chrome.accent_soft
            } else {
                chrome.surface
            }))
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(6.0))
                            .h(px(6.0))
                            .rounded_full()
                            .bg(rgb(if none_selected {
                                chrome.accent
                            } else {
                                chrome.text_muted
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(if none_selected {
                                chrome.accent
                            } else {
                                chrome.text
                            }))
                            .child(SharedString::from(t(language, "monitor.none"))),
                    ),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.disconnect_source(cx);
                cx.notify();
            }))
            .into_any_element(),
    );

    if discovered.is_empty() {
        items.push(
            div()
                .px_3()
                .py_3()
                .text_xs()
                .text_color(rgb(chrome.text_muted))
                .child(SharedString::from(t(language, "monitor.no_sources")))
                .into_any_element(),
        );
    } else {
        let mut hosts: Vec<(String, Vec<&DiscoveredSource>)> = Vec::new();
        for s in discovered {
            if let Some((_, list)) = hosts.iter_mut().find(|(h, _)| h == &s.host) {
                list.push(s);
            } else {
                hosts.push((s.host.clone(), vec![s]));
            }
        }

        for (host, sources) in hosts {
            items.push(
                div()
                    .mt_2()
                    .px_2()
                    .py_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(chrome.text_muted))
                            .child(SharedString::from(host.to_uppercase())),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(chrome.text_muted))
                            .child(SharedString::from(format!("{}", sources.len()))),
                    )
                    .into_any_element(),
            );

            for s in sources {
                let url = SharedString::from(s.url.clone());
                let is_selected = selected == Some(&url);
                let url_click = url.clone();
                let source_label = if s.source.is_empty() {
                    s.name.clone()
                } else {
                    s.source.clone()
                };
                items.push(
                    div()
                        .id(url.clone())
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(rgb(if is_selected {
                            chrome.accent_soft
                        } else {
                            chrome.surface
                        }))
                        .border_l_2()
                        .border_color(rgb(if is_selected {
                            chrome.accent
                        } else {
                            chrome.border
                        }))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(chrome.surface_active)))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_0p5()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgb(if is_selected {
                                            chrome.accent
                                        } else {
                                            chrome.text
                                        }))
                                        .child(SharedString::from(source_label)),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(chrome.text_muted))
                                        .child(SharedString::from(format!(":{}", s.port))),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select(url_click.clone(), cx);
                        }))
                        .into_any_element(),
                );
            }
        }
    }

    div()
        .w(px(SIDEBAR_W))
        .h_full()
        .bg(rgb(chrome.panel))
        .border_r_1()
        .border_color(rgb(chrome.border))
        .flex()
        .flex_col()
        .child(
            div()
                .px_3()
                .py_3()
                .border_b_1()
                .border_color(rgb(chrome.border))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_sm()
                        .child(t(language, "monitor.sources")),
                )
                .child(
                    div()
                        .id("refresh")
                        .px_2()
                        .py_0p5()
                        .rounded_md()
                        .bg(rgb(chrome.surface))
                        .text_xs()
                        .text_color(rgb(chrome.text_muted))
                        .cursor_pointer()
                        .hover(|s| s.text_color(rgb(chrome.text)))
                        .child(t(language, "monitor.refresh"))
                        .on_click(cx.listener(|this, _, _, cx| this.request_refresh(false, cx))),
                ),
        )
        .child(
            div()
                .id("source-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px_2()
                .py_2()
                .gap_1()
                .flex()
                .flex_col()
                .children(items),
        )
        .child(
            div()
                .px_3()
                .py_2()
                .border_t_1()
                .border_color(rgb(chrome.border))
                .child(
                    div()
                        .id("open-preferences")
                        .w_full()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .bg(rgb(chrome.surface))
                        .text_sm()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(chrome.surface_active)))
                        .child(t(language, "monitor.preferences"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_preferences(cx);
                        })),
                ),
        )
}


#[allow(clippy::too_many_arguments)]
fn preview_pane(
    cx: &mut Context<MonitorView>,
    language: Language,
    chrome: UiChrome,
    entity: gpui::Entity<MonitorView>,
    texture: Option<Arc<RenderImage>>,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
    frame_w: u32,
    frame_h: u32,
    display_w: f32,
    display_h: f32,
    safe_area: bool,
    vu_meter: bool,
    audio_levels: AudioLevels,
    stall_text: &SharedString,
) -> impl IntoElement {
    div()
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .flex_col()
        .child(
            div()
                .px_3()
                .py_2()
                .h(px(TOOLBAR_H))
                .gap_3()
                .flex()
                .items_center()
                .border_b_1()
                .border_color(rgb(chrome.border))
                .bg(rgb(chrome.panel))
                .child(stall_text.clone())
                .child(format!("{:.0}%", zoom * 100.0))
                .child(
                    div()
                        .id("zoom-reset")
                        .px_2()
                        .py_0p5()
                        .rounded_md()
                        .bg(rgb(chrome.surface))
                        .cursor_pointer()
                        .child(t(language, "monitor.zoom_reset"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.zoom_reset();
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id("toolbar-preferences")
                        .px_2()
                        .py_0p5()
                        .rounded_md()
                        .bg(rgb(chrome.surface))
                        .cursor_pointer()
                        .child(t(language, "monitor.preferences"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open_preferences(cx);
                        })),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(chrome.text_muted))
                        .child("wheel = zoom · middle-drag = pan · right-click = menu"),
                ),
        )
        .child(
            div()
                .id("preview")
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .bg(rgb(chrome.bg))
                .relative()
                .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                    let dy = match event.delta {
                        ScrollDelta::Pixels(p) => f32::from(p.y),
                        ScrollDelta::Lines(l) => l.y * 20.0,
                    };
                    if dy.abs() > f32::EPSILON {
                        this.adjust_zoom(dy);
                        cx.notify();
                    }
                    cx.stop_propagation();
                }))
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        let x: f32 = event.position.x.into();
                        let y: f32 = event.position.y.into();
                        this.pan_drag = Some((x, y));
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_up(
                    MouseButton::Middle,
                    cx.listener(|this, _: &MouseUpEvent, _, cx| {
                        this.pan_drag = None;
                        cx.notify();
                    }),
                )
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                    if let Some((lx, ly)) = this.pan_drag {
                        let x: f32 = event.position.x.into();
                        let y: f32 = event.position.y.into();
                        this.pan_x += x - lx;
                        this.pan_y += y - ly;
                        this.pan_drag = Some((x, y));
                        cx.notify();
                    }
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        this.open_context_menu(event.position, cx);
                    }),
                )
                .child(
                    canvas(
                        move |bounds, _, app| {
                            let w: f32 = bounds.size.width.into();
                            let h: f32 = bounds.size.height.into();
                            entity.update(app, |this, cx| {
                                this.set_preview_size(w, h, cx);
                            });
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(pan_x))
                        .top(px(pan_y))
                        .child(video_stack(
                            texture,
                            frame_w,
                            frame_h,
                            display_w,
                            display_h,
                            safe_area,
                            vu_meter,
                            audio_levels,
                            language,
                        )),
                ),
        )
}

#[allow(clippy::too_many_arguments)]
fn video_stack(
    texture: Option<Arc<RenderImage>>,
    frame_w: u32,
    frame_h: u32,
    display_w: f32,
    display_h: f32,
    safe_area: bool,
    vu_meter: bool,
    audio_levels: AudioLevels,
    language: Language,
) -> impl IntoElement {
    if let Some(tex) = texture {
        if frame_w > 0 && frame_h > 0 {
            let mut stack = div()
                .relative()
                .w(px(display_w))
                .h(px(display_h))
                .child(
                    img(tex)
                        .object_fit(ObjectFit::Fill)
                        .w(px(display_w))
                        .h(px(display_h)),
                );
            if let Some(overlay) = safe_area_overlay(safe_area, display_w, display_h) {
                stack = stack.child(overlay);
            }
            if vu_meter {
                stack = stack.child(vu_overlay(display_w, display_h, audio_levels));
            }
            return stack.into_any_element();
        }
    }
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(SharedString::from(t(language, "monitor.waiting")))
        .into_any_element()
}

/// SMPTE ST 2046-1: Safe Action = 93%, Safe Title = 90% of production aperture.
const ACTION_SAFE_FRAC: f32 = 0.93;
const TITLE_SAFE_FRAC: f32 = 0.90;

fn safe_area_overlay(enabled: bool, display_w: f32, display_h: f32) -> Option<impl IntoElement> {
    if !enabled {
        return None;
    }
    Some(
        div()
            .absolute()
            .left_0()
            .top_0()
            .w(px(display_w))
            .h(px(display_h))
            // Outer: action safe — keep critical motion inside this box.
            .child(safe_guide_rect(
                display_w,
                display_h,
                ACTION_SAFE_FRAC,
                0xffffff,
            ))
            // Inner: title safe — keep graphics/text inside this box.
            .child(safe_guide_rect(
                display_w,
                display_h,
                TITLE_SAFE_FRAC,
                0xffeb3b,
            )),
    )
}

fn safe_guide_rect(
    display_w: f32,
    display_h: f32,
    fraction: f32,
    color: u32,
) -> impl IntoElement {
    let box_w = display_w * fraction;
    let box_h = display_h * fraction;
    let left = (display_w - box_w) * 0.5;
    let top = (display_h - box_h) * 0.5;
    div()
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(box_w))
        .h(px(box_h))
        .border_1()
        .border_color(rgb(color))
        .opacity(0.9)
}

fn vu_overlay(display_w: f32, display_h: f32, levels: AudioLevels) -> impl IntoElement {
    let bar_w = 10.0f32;
    let gap = 4.0f32;
    let height = display_h * 0.6;
    let top = (display_h - height) * 0.5;
    let left = display_w - bar_w * 2.0 - gap - 12.0;
    let l = levels.peak_l.clamp(0.0, 1.0);
    let r = levels.peak_r.clamp(0.0, 1.0);
    div()
        .absolute()
        .left(px(left))
        .top(px(top))
        .h(px(height))
        .flex()
        .gap_1()
        .items_end()
        .child(vu_bar(bar_w, height, l))
        .child(vu_bar(bar_w, height, r))
}

fn vu_bar(width: f32, full_h: f32, level: f32) -> impl IntoElement {
    let h = (full_h * level).max(2.0);
    let color = if level > 0.89 {
        rgb(0xf44336)
    } else if level > 0.7 {
        rgb(0xffeb3b)
    } else {
        rgb(0x4caf50)
    };
    div()
        .w(px(width))
        .h(px(full_h))
        .bg(rgb(0x1b222c))
        .rounded_sm()
        .flex()
        .items_end()
        .child(div().w(px(width)).h(px(h)).bg(color).rounded_sm())
}

fn format_dbfs(peak: f32) -> String {
    if peak <= 1e-6 {
        "-∞ dBFS".into()
    } else {
        format!("{:.1} dBFS", 20.0 * peak.log10())
    }
}

fn format_delay_setting(language: Language, delay: DelaySetting, delay_ms: u32) -> String {
    match delay.unit {
        BufferUnit::Milliseconds => format!("{delay_ms} ms"),
        BufferUnit::Frames if delay.amount == 1 => {
            format!(
                "1 {} ({}{} ms)",
                t(language, "monitor.buffer_frame"),
                t(language, "monitor.buffer_equiv"),
                delay_ms
            )
        }
        BufferUnit::Frames => {
            format!(
                "{} {} ({}{} ms)",
                delay.amount,
                t(language, "monitor.buffer_frames"),
                t(language, "monitor.buffer_equiv"),
                delay_ms
            )
        }
    }
}

fn format_buffer_stats(
    language: Language,
    buffer: &BufferSettings,
    video_ms: u32,
    audio_ms: u32,
) -> String {
    let v = format_delay_setting(language, buffer.video, video_ms);
    let a = format_delay_setting(language, buffer.audio, audio_ms);
    if buffer.linked {
        format!("link · V {v} / A {a}")
    } else {
        format!("indep · V {v} / A {a}")
    }
}

#[allow(clippy::too_many_arguments)]
fn stats_panel(
    language: Language,
    chrome: UiChrome,
    display_fps: f32,
    source_fps: f32,
    frames_presented: u64,
    source_dropped: u64,
    frames_render_skipped: u64,
    net_dropped: i64,
    frames_decoded: u64,
    audio_frames: u64,
    audio_levels: AudioLevels,
    video_buffer_delay_ms: u32,
    audio_buffer_delay_ms: u32,
    buffer: &BufferSettings,
    frame_w: u32,
    frame_h: u32,
    bitrate_bps: f64,
    bytes_received: i64,
    selected: Option<&SharedString>,
) -> impl IntoElement {
    let buffer_label = format_buffer_stats(language, buffer, video_buffer_delay_ms, audio_buffer_delay_ms);
    div()
        .w(px(STATS_W))
        .h_full()
        .p_3()
        .gap_2()
        .flex()
        .flex_col()
        .bg(rgb(chrome.panel))
        .border_l_1()
        .border_color(rgb(chrome.border))
        .id("stats-panel")
        .overflow_y_scroll()
        .child(
            div()
                .font_weight(FontWeight::BOLD)
                .child(t(language, "monitor.stats")),
        )
        .child(stat_row("Display FPS", format!("{display_fps:.1}")))
        .child(stat_row("Source FPS", format!("{source_fps:.2}")))
        .child(stat_row("Presented", format!("{frames_presented}")))
        .child(stat_row("Source dropped", format!("{source_dropped}")))
        .child(stat_row(
            "Render skipped",
            format!("{frames_render_skipped}"),
        ))
        .child(stat_row("Net dropped", format!("{net_dropped}")))
        .child(stat_row("Decoded", format!("{frames_decoded}")))
        .child(div().h(px(8.0)))
        .child(
            div()
                .font_weight(FontWeight::BOLD)
                .child(t(language, "monitor.audio")),
        )
        .child(stat_row("Audio packets", format!("{audio_frames}")))
        .child(stat_row("L", format_dbfs(audio_levels.peak_l)))
        .child(stat_row("R", format_dbfs(audio_levels.peak_r)))
        .child(stat_row(
            "Format",
            if audio_levels.sample_rate > 0 {
                format!(
                    "{} Hz / {} ch",
                    audio_levels.sample_rate, audio_levels.channels
                )
            } else {
                "-".into()
            },
        ))
        .child(stat_row(t(language, "monitor.av_buffer"), buffer_label))
        .child(div().h(px(8.0)))
        .child(
            div()
                .font_weight(FontWeight::BOLD)
                .child(t(language, "monitor.source_info")),
        )
        .child(stat_row(
            "Resolution",
            if frame_w > 0 {
                format!("{frame_w}×{frame_h}")
            } else {
                "-".into()
            },
        ))
        .child(stat_row("Bitrate", format_bitrate(bitrate_bps)))
        .child(stat_row("Bytes RX", format_bytes(bytes_received)))
        .child(stat_row(
            "URL",
            selected
                .map(|s| s.to_string())
                .unwrap_or_else(|| "-".into()),
        ))
}

fn log_panel(
    cx: &mut Context<MonitorView>,
    language: Language,
    chrome: UiChrome,
    log_lines: &[SharedString],
) -> impl IntoElement {
    div()
        .h(px(LOG_H))
        .border_t_1()
        .border_color(rgb(chrome.border))
        .bg(rgb(chrome.panel))
        .flex()
        .flex_col()
        .child(
            div()
                .px_3()
                .py_1()
                .gap_2()
                .flex()
                .items_center()
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .child(t(language, "monitor.log")),
                )
                .child(
                    div()
                        .id("log-clear")
                        .px_2()
                        .py_0p5()
                        .rounded_md()
                        .bg(rgb(chrome.surface))
                        .cursor_pointer()
                        .child(t(language, "monitor.clear_log"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.log_lines.clear();
                            cx.notify();
                        })),
                ),
        )
        .child(
            div()
                .id("log-body")
                .flex_1()
                .min_h_0()
                .px_3()
                .pb_2()
                .overflow_y_scroll()
                .bg(rgb(chrome.bg))
                .text_xs()
                .text_color(rgb(chrome.text_muted))
                .font_family("Consolas")
                .children(if log_lines.is_empty() {
                    vec![div()
                        .opacity(0.5)
                        .child("XML / metadata will appear here")
                        .into_any_element()]
                } else {
                    log_lines
                        .iter()
                        .cloned()
                        .enumerate()
                        .map(|(i, line)| {
                            div()
                                .id(SharedString::from(format!("log-{i}")))
                                .py_0p5()
                                .child(line)
                                .into_any_element()
                        })
                        .collect()
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
