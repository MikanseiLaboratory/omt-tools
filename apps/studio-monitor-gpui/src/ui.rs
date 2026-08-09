//! Minimal GPUI Studio Monitor UI (source list + live frame stats).

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use gpui::{
    App, Application, Bounds, Context, SharedString, Timer, Window, WindowBounds, WindowOptions,
    div, prelude::*, px, rgb, size,
};
use omt_media::{ReceiveWorker, SourceBrowser, StallState, bgra_to_rgba};
use suite_core::{Language, t};

pub fn run_gpui(title: String, language: Language, initial_url: Option<String>) -> Result<()> {
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.0), px(720.0)), cx);
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
            move |_, cx| {
                cx.new(|cx| MonitorView::new(cx, language, initial_url.clone()))
            },
        )
        .expect("open GPUI Studio Monitor window");
        cx.activate(true);
    });
    Ok(())
}

struct MonitorView {
    language: Language,
    worker: ReceiveWorker,
    browser: SourceBrowser,
    sources: Vec<(SharedString, SharedString)>,
    selected: Option<SharedString>,
    status: SharedString,
    resolution: SharedString,
    fps: f32,
    frames: u64,
    last_refresh: Instant,
    last_frame_at: Option<Instant>,
    window_fps_count: u32,
    window_fps_start: Instant,
    rgba_bytes: usize,
}

impl MonitorView {
    fn new(cx: &mut Context<Self>, language: Language, initial_url: Option<String>) -> Self {
        let worker = ReceiveWorker::spawn();
        if let Some(url) = initial_url {
            worker.connect(url);
        }
        let mut view = Self {
            language,
            worker,
            browser: SourceBrowser::new(),
            sources: Vec::new(),
            selected: None,
            status: SharedString::from(t(language, "monitor.waiting")),
            resolution: SharedString::from("-"),
            fps: 0.0,
            frames: 0,
            last_refresh: Instant::now() - Duration::from_secs(10),
            last_frame_at: None,
            window_fps_count: 0,
            window_fps_start: Instant::now(),
            rgba_bytes: 0,
        };
        view.refresh_sources();
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

    fn refresh_sources(&mut self) {
        match self.browser.refresh(Duration::from_millis(600)) {
            Ok(list) => {
                self.sources = list
                    .iter()
                    .map(|s| {
                        (
                            SharedString::from(s.name.clone()),
                            SharedString::from(s.url.clone()),
                        )
                    })
                    .collect();
                self.status = SharedString::from(format!("{} source(s)", self.sources.len()));
            }
            Err(e) => {
                self.status = SharedString::from(e.to_string());
            }
        }
        self.last_refresh = Instant::now();
    }

    fn on_tick(&mut self, cx: &mut Context<Self>) {
        if self.last_refresh.elapsed() > Duration::from_secs(3) {
            self.refresh_sources();
        }
        if let Some(frame) = self.worker.latest().take() {
            // Present-path proxy: convert BGRA→RGBA as GPUI image upload would need.
            let rgba = bgra_to_rgba(&frame.bgra);
            self.rgba_bytes = rgba.len();
            self.resolution = SharedString::from(format!("{}×{}", frame.width, frame.height));
            self.frames += 1;
            self.window_fps_count += 1;
            self.last_frame_at = Some(Instant::now());
            let _ = Arc::new(rgba);
        }
        if self.window_fps_start.elapsed() >= Duration::from_secs(1) {
            self.fps = self.window_fps_count as f32 / self.window_fps_start.elapsed().as_secs_f32();
            self.window_fps_count = 0;
            self.window_fps_start = Instant::now();
        }
        cx.notify();
    }

    fn select(&mut self, url: SharedString) {
        self.selected = Some(url.clone());
        self.worker.connect(url.to_string());
        self.frames = 0;
        self.rgba_bytes = 0;
    }

    fn stall_label(&self) -> SharedString {
        let guard = self.worker.stall();
        let mut d = guard.lock();
        let label = match d.tick() {
            StallState::Waiting => t(self.language, "monitor.waiting"),
            StallState::Live => "LIVE",
            StallState::Stalled => t(self.language, "monitor.stalled"),
        };
        SharedString::from(label)
    }
}

impl Render for MonitorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let stall = self.stall_label();
        div()
            .flex()
            .flex_row()
            .size_full()
            .bg(rgb(0x12161c))
            .text_color(rgb(0xedf2f7))
            .child(
                div()
                    .w(px(300.0))
                    .h_full()
                    .p_3()
                    .gap_2()
                    .flex()
                    .flex_col()
                    .border_r_1()
                    .border_color(rgb(0x2a3340))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(t(self.language, "monitor.sources")),
                    )
                    .child(
                        div()
                            .id("refresh")
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x243041))
                            .cursor_pointer()
                            .child(t(self.language, "monitor.refresh"))
                            .on_click(cx.listener(|this, _, _, _| this.refresh_sources())),
                    )
                    .children(self.sources.iter().cloned().map(|(name, url)| {
                        let selected = self.selected.as_ref() == Some(&url);
                        let url_click = url.clone();
                        div()
                            .id(url.clone())
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(if selected {
                                rgb(0x2f6fed)
                            } else {
                                rgb(0x1b222c)
                            })
                            .cursor_pointer()
                            .child(name)
                            .on_click(cx.listener(move |this, _, _, _| {
                                this.select(url_click.clone());
                            }))
                    }))
                    .child(div().text_xs().opacity(0.7).child(self.status.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .p_4()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .gap_4()
                            .child(self.resolution.clone())
                            .child(format!("{:.1} fps", self.fps))
                            .child(format!("frames {}", self.frames))
                            .child(format!("rgba {}", self.rgba_bytes))
                            .child(stall),
                    )
                    .child(
                        div()
                            .flex_1()
                            .rounded_lg()
                            .bg(rgb(0x000000))
                            .justify_center()
                            .items_center()
                            .child(if self.last_frame_at.is_some() {
                                SharedString::from("Receiving (GPUI prototype — texture path in headless A/B)")
                            } else {
                                SharedString::from(t(self.language, "monitor.waiting"))
                            }),
                    ),
            )
    }
}
