//! GPUI Studio Monitor UI — source list, receive, and live frame display.

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use gpui::{
    div, img, prelude::*, px, rgb, size, App, Application, Bounds, Context, Font, FontFallbacks,
    FontFeatures, FontStyle, FontWeight, InteractiveElement, ObjectFit, RenderImage, SharedString,
    Timer, Window, WindowBounds, WindowOptions,
};
use image::{Frame, ImageBuffer, Rgba};
use omt_media::{ReceiveWorker, SourceBrowser, StallState};
use smallvec::smallvec;
use suite_core::{t, Language};

type SourcePair = (SharedString, SharedString);
type DiscoveryResult = Result<Vec<(String, String)>, String>;

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

/// Build a GPUI [`RenderImage`] from tightly packed BGRA8 pixels.
///
/// GPUI treats [`Frame`] buffers as BGRA; OMT already delivers BGRA so no channel swap.
fn bgra_to_render_image(bgra: Vec<u8>, width: u32, height: u32) -> Option<Arc<RenderImage>> {
    let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, bgra)?;
    Some(Arc::new(RenderImage::new(smallvec![Frame::new(buffer)])))
}

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
            move |_, cx| cx.new(|cx| MonitorView::new(cx, language, initial_url.clone())),
        )
        .expect("open GPUI Studio Monitor window");
        cx.activate(true);
    });
    Ok(())
}

struct MonitorView {
    language: Language,
    worker: ReceiveWorker,
    sources: Vec<SourcePair>,
    selected: Option<SharedString>,
    status: SharedString,
    resolution: SharedString,
    fps: f32,
    frames: u64,
    last_refresh: Instant,
    last_frame_at: Option<Instant>,
    window_fps_count: u32,
    window_fps_start: Instant,
    pixel_bytes: usize,
    texture: Option<Arc<RenderImage>>,
    discovering: bool,
    discovery_rx: Option<Receiver<DiscoveryResult>>,
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
            pixel_bytes: 0,
            texture: None,
            discovering: false,
            discovery_rx: None,
        };
        view.request_refresh(cx);
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

    fn request_refresh(&mut self, _cx: &mut Context<Self>) {
        if self.discovering {
            return;
        }
        self.discovering = true;
        self.status = SharedString::from(t(self.language, "monitor.refresh"));
        let (tx, rx) = mpsc::channel();
        self.discovery_rx = Some(rx);
        thread::Builder::new()
            .name("omt-discover".into())
            .spawn(move || {
                let mut browser = SourceBrowser::new();
                let result = match browser.refresh(Duration::from_millis(1500)) {
                    Ok(list) => Ok(list
                        .iter()
                        .map(|s| (s.name.clone(), s.url.clone()))
                        .collect()),
                    Err(e) => Err(e.to_string()),
                };
                let _ = tx.send(result);
            })
            .expect("spawn discovery thread");
        self.last_refresh = Instant::now();
    }

    fn poll_discovery(&mut self) {
        let Some(rx) = self.discovery_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(list)) => {
                self.sources = list
                    .into_iter()
                    .map(|(name, url)| (SharedString::from(name), SharedString::from(url)))
                    .collect();
                self.status = if self.sources.is_empty() {
                    SharedString::from(t(self.language, "monitor.no_sources"))
                } else {
                    SharedString::from(format!("{} source(s)", self.sources.len()))
                };
                self.discovering = false;
                self.discovery_rx = None;
            }
            Ok(Err(err)) => {
                self.status = SharedString::from(err);
                self.discovering = false;
                self.discovery_rx = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.status = SharedString::from("discovery thread ended");
                self.discovering = false;
                self.discovery_rx = None;
            }
        }
    }

    fn on_tick(&mut self, cx: &mut Context<Self>) {
        self.poll_discovery();
        if !self.discovering && self.last_refresh.elapsed() > Duration::from_secs(3) {
            self.request_refresh(cx);
        }

        if let Some(frame) = self.worker.latest().take() {
            self.pixel_bytes = frame.bgra.len();
            self.resolution = SharedString::from(format!("{}×{}", frame.width, frame.height));
            self.frames += 1;
            self.window_fps_count += 1;
            self.last_frame_at = Some(Instant::now());
            if let Some(image) = bgra_to_render_image(frame.bgra, frame.width, frame.height) {
                if let Some(old) = self.texture.take() {
                    cx.drop_image(old, None);
                }
                self.texture = Some(image);
            }
        }

        if let Some(err) = self.worker.latest().error.lock().clone() {
            self.status = SharedString::from(err);
        }

        if self.window_fps_start.elapsed() >= Duration::from_secs(1) {
            self.fps = self.window_fps_count as f32 / self.window_fps_start.elapsed().as_secs_f32();
            self.window_fps_count = 0;
            self.window_fps_start = Instant::now();
        }
        cx.notify();
    }

    fn select(&mut self, url: SharedString, cx: &mut Context<Self>) {
        self.selected = Some(url.clone());
        self.worker.connect(url.to_string());
        self.frames = 0;
        self.pixel_bytes = 0;
        self.last_frame_at = None;
        if let Some(old) = self.texture.take() {
            cx.drop_image(old, None);
        }
        self.status = SharedString::from(t(self.language, "monitor.waiting"));
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
        let language = self.language;
        let sources = self.sources.clone();
        let selected = self.selected.clone();
        let texture = self.texture.clone();

        div()
            .flex()
            .flex_row()
            .size_full()
            .font(ui_font())
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
                            .font_weight(FontWeight::BOLD)
                            .child(t(language, "monitor.sources")),
                    )
                    .child(
                        div()
                            .id("refresh")
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(rgb(0x243041))
                            .cursor_pointer()
                            .child(t(language, "monitor.refresh"))
                            .on_click(cx.listener(|this, _, _, cx| this.request_refresh(cx))),
                    )
                    .child(
                        div()
                            .id("source-list")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .gap_1()
                            .flex()
                            .flex_col()
                            .children(if sources.is_empty() {
                                vec![div()
                                    .px_2()
                                    .py_1()
                                    .text_xs()
                                    .opacity(0.7)
                                    .child(if self.discovering {
                                        SharedString::from(t(language, "monitor.refresh"))
                                    } else {
                                        SharedString::from(t(language, "monitor.no_sources"))
                                    })
                                    .into_any_element()]
                            } else {
                                sources
                                    .into_iter()
                                    .map(|(name, url)| {
                                        let is_selected = selected.as_ref() == Some(&url);
                                        let url_click = url.clone();
                                        div()
                                            .id(url.clone())
                                            .px_2()
                                            .py_1()
                                            .rounded_md()
                                            .bg(if is_selected {
                                                rgb(0x2f6fed)
                                            } else {
                                                rgb(0x1b222c)
                                            })
                                            .cursor_pointer()
                                            .child(name)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.select(url_click.clone(), cx);
                                            }))
                                            .into_any_element()
                                    })
                                    .collect()
                            }),
                    )
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
                            .child(format!("bytes {}", self.pixel_bytes))
                            .child(stall),
                    )
                    .child(
                        div()
                            .flex_1()
                            .rounded_lg()
                            .bg(rgb(0x000000))
                            .overflow_hidden()
                            .justify_center()
                            .items_center()
                            .child(if let Some(tex) = texture {
                                img(tex)
                                    .object_fit(ObjectFit::Contain)
                                    .size_full()
                                    .into_any_element()
                            } else {
                                div()
                                    .child(SharedString::from(t(language, "monitor.waiting")))
                                    .into_any_element()
                            }),
                    ),
            )
    }
}
