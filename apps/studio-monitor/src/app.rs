//! Studio Monitor application state.

use std::time::{Duration, Instant};

use eframe::egui;
use omt_media::{
    FpsCounter, ReceiveWorker, SourceBrowser, StallState, VideoFrame, bgra_alpha_mask,
    bgra_over_checkerboard, bgra_to_rgba,
};
use suite_core::{Language, ThemePreference, t};

use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitMode {
    Fit,
    Fill,
}

/// Main Studio Monitor app.
pub struct MonitorApp {
    pub language: Language,
    #[allow(dead_code)]
    pub theme: ThemePreference,
    pub browser: SourceBrowser,
    pub sources: Vec<omt_media::DiscoveredSource>,
    pub selected: Option<String>,
    pub worker: ReceiveWorker,
    pub texture: Option<egui::TextureHandle>,
    pub last_frame: Option<VideoFrame>,
    pub fit: FitMode,
    pub alpha_mask: bool,
    pub checkerboard: bool,
    pub fps: FpsCounter,
    pub last_refresh: Instant,
    pub status: String,
}

impl MonitorApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        language: Language,
        theme: ThemePreference,
        initial_url: Option<String>,
    ) -> Self {
        let worker = ReceiveWorker::spawn();
        if let Some(url) = initial_url {
            worker.connect(url);
        }
        let mut app = Self {
            language,
            theme,
            browser: SourceBrowser::new(),
            sources: Vec::new(),
            selected: None,
            worker,
            texture: None,
            last_frame: None,
            fit: FitMode::Fit,
            alpha_mask: false,
            checkerboard: true,
            fps: FpsCounter::default(),
            last_refresh: Instant::now() - Duration::from_secs(10),
            status: String::new(),
        };
        app.refresh_sources();
        let _ = cc;
        app
    }

    pub fn refresh_sources(&mut self) {
        match self.browser.refresh(Duration::from_millis(800)) {
            Ok(list) => {
                self.sources = list.to_vec();
                self.status = format!("{} source(s)", self.sources.len());
            }
            Err(e) => {
                self.status = e.to_string();
            }
        }
        self.last_refresh = Instant::now();
    }

    pub fn select_source(&mut self, url: String) {
        self.selected = Some(url.clone());
        self.worker.connect(url);
        self.last_frame = None;
        self.texture = None;
    }

    fn upload_frame(&mut self, ctx: &egui::Context, frame: VideoFrame) {
        let rgba = if self.alpha_mask {
            bgra_alpha_mask(&frame.bgra)
        } else if self.checkerboard {
            bgra_over_checkerboard(&frame.bgra, frame.width, frame.height, 16)
        } else {
            bgra_to_rgba(&frame.bgra)
        };
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [frame.width as usize, frame.height as usize],
            &rgba,
        );
        match &mut self.texture {
            Some(tex) => tex.set(image, egui::TextureOptions::LINEAR),
            None => {
                self.texture = Some(ctx.load_texture(
                    "omt-monitor-frame",
                    image,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
        self.fps.tick();
        self.last_frame = Some(frame);
    }
}

impl eframe::App for MonitorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.last_refresh.elapsed() > Duration::from_secs(3) {
            // Non-blocking soft refresh only when sidebar is idle-ish; keep cadence light.
            self.refresh_sources();
        }

        if let Some(frame) = self.worker.latest().take() {
            self.upload_frame(ctx, frame);
        }

        let stall = {
            let stall = self.worker.stall();
            let mut detector = stall.lock();
            detector.tick()
        };

        ui::draw(self, ctx, stall);
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

pub fn stall_label(lang: Language, state: StallState) -> &'static str {
    match state {
        StallState::Waiting => t(lang, "monitor.waiting"),
        StallState::Live => "",
        StallState::Stalled => t(lang, "monitor.stalled"),
    }
}
