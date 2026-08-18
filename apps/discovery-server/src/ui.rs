//! GPUI Discovery Server console.

use std::time::Duration;

use anyhow::Result;
use gpui::{
    App, Application, Bounds, Context, FocusHandle, Focusable, Font, FontFallbacks, FontFeatures,
    FontStyle, FontWeight, InteractiveElement, KeyDownEvent, SharedString, Timer, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use omt_discovery_server::{ServerController, ServerSettings};
use suite_core::{
    DiscoveryServerConfig, Language, load_discovery_server_config, save_discovery_server_config, t,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditTarget {
    None,
    Bind,
    Port,
}

pub fn run_gpui(title: String, language: Language) -> Result<()> {
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(820.0), px(640.0)), cx);
        let title = SharedString::from(title);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(title.clone()),
                    ..Default::default()
                }),
                app_id: Some(suite_core::ToolId::DiscoveryServer.binary_name().into()),
                ..Default::default()
            },
            move |_, cx| cx.new(|cx| ServerView::new(cx, language)),
        )
        .expect("open GPUI Discovery Server window");
        cx.activate(true);
    });
    Ok(())
}

struct ServerView {
    language: Language,
    bind: String,
    port: String,
    controller: ServerController,
    editing: EditTarget,
    error: Option<String>,
    focus_handle: FocusHandle,
}

impl ServerView {
    fn new(cx: &mut Context<Self>, language: Language) -> Self {
        let cfg = load_discovery_server_config()
            .unwrap_or_default()
            .sanitized();
        let settings = ServerSettings {
            bind: cfg.bind.clone(),
            port: cfg.port,
        };
        let view = Self {
            language,
            bind: cfg.bind,
            port: cfg.port.to_string(),
            controller: ServerController::new(settings),
            editing: EditTarget::None,
            error: None,
            focus_handle: cx.focus_handle(),
        };
        view.schedule_tick(cx);
        view
    }

    fn schedule_tick(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            Timer::after(Duration::from_millis(200)).await;
            this.update(cx, |this, cx| {
                this.controller.poll();
                this.schedule_tick(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn persist(&self) {
        let port = self.port.parse().unwrap_or(6399);
        let cfg = DiscoveryServerConfig {
            schema_version: 1,
            bind: self.bind.clone(),
            port,
        }
        .sanitized();
        let _ = save_discovery_server_config(&cfg);
    }

    fn apply_settings(&mut self) -> Result<(), String> {
        let port: u16 = self
            .port
            .trim()
            .parse()
            .map_err(|_| "port must be an integer from 1 to 65535".to_string())?;
        if port == 0 {
            return Err("port must be an integer from 1 to 65535".into());
        }
        let settings = ServerSettings {
            bind: self.bind.clone(),
            port,
        };
        self.controller.set_settings(settings)?;
        self.persist();
        Ok(())
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        match self.apply_settings().and_then(|()| self.controller.start()) {
            Ok(()) => self.error = None,
            Err(e) => self.error = Some(e),
        }
        cx.notify();
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        match self.controller.stop() {
            Ok(()) => self.error = None,
            Err(e) => self.error = Some(e),
        }
        cx.notify();
    }

    fn apply_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.editing == EditTarget::None || self.controller.is_running() {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" || key == "enter" {
            self.editing = EditTarget::None;
            cx.notify();
            return;
        }
        let field = match self.editing {
            EditTarget::Bind => &mut self.bind,
            EditTarget::Port => &mut self.port,
            EditTarget::None => return,
        };
        if key == "backspace" {
            field.pop();
            cx.notify();
            return;
        }
        if let Some(ch) = event.keystroke.key_char.as_deref()
            && ch.chars().all(|c| !c.is_control())
            && field.len() + ch.len() <= 64
        {
            field.push_str(ch);
            cx.notify();
        }
    }
}

impl Focusable for ServerView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ServerView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = self.language;
        let running = self.controller.is_running();
        let snap = self.controller.snapshot();
        let status = if running {
            t(lang, "discovery.running")
        } else {
            t(lang, "discovery.stopped")
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .font(ui_font())
            .bg(rgb(0x1a1d23))
            .text_color(rgb(0xedf2f7))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.apply_key(event, cx);
            }))
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(0x2a3340))
                    .font_weight(FontWeight::BOLD)
                    .child(t(lang, "tool.discovery_server")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(field(
                                cx,
                                "bind",
                                t(lang, "discovery.bind"),
                                &self.bind,
                                self.editing == EditTarget::Bind,
                                !running,
                                EditTarget::Bind,
                            ))
                            .child(field(
                                cx,
                                "port",
                                t(lang, "discovery.port"),
                                &self.port,
                                self.editing == EditTarget::Port,
                                !running,
                                EditTarget::Port,
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .opacity(0.65)
                            .child(t(lang, "discovery.bind_hint")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(action_button(
                                cx,
                                "start",
                                t(lang, "discovery.start"),
                                rgb(0x2f6fed),
                                running,
                                |this, _, cx| this.start(cx),
                            ))
                            .child(action_button(
                                cx,
                                "stop",
                                t(lang, "discovery.stop"),
                                rgb(0xb33a3a),
                                !running,
                                |this, _, cx| this.stop(cx),
                            ))
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(if running {
                                        rgb(0x8ad7a0)
                                    } else {
                                        rgb(0xa0a8b4)
                                    })
                                    .child(status.to_string()),
                            )
                            .child(div().text_xs().opacity(0.8).child(format!(
                                "{}: {}",
                                t(lang, "discovery.peers"),
                                snap.peer_count()
                            ))),
                    )
                    .children(
                        self.error.as_ref().map(|err| {
                            div().text_xs().text_color(rgb(0xff8a8a)).child(err.clone())
                        }),
                    )
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .child(t(lang, "discovery.sources")),
                    )
                    .child(sources_list(lang, &snap.sources))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .child(t(lang, "discovery.log")),
                            )
                            .child(action_button(
                                cx,
                                "clear-log",
                                t(lang, "discovery.clear_log"),
                                rgb(0x243041),
                                false,
                                |this, _, cx| {
                                    this.controller.clear_events();
                                    cx.notify();
                                },
                            )),
                    )
                    .child(event_log(self.controller.events())),
            )
    }
}

fn sources_list(lang: Language, sources: &[openmediatransport::OmtAddress]) -> impl IntoElement {
    if sources.is_empty() {
        return div()
            .text_xs()
            .opacity(0.65)
            .child(t(lang, "discovery.none"));
    }
    div()
        .flex()
        .flex_col()
        .gap_1()
        .children(sources.iter().map(|src| {
            let ips = src.addresses.join(", ");
            div()
                .text_xs()
                .child(format!("{}  :{}  {ips}", src.instance_name(), src.port))
        }))
}

fn event_log(events: &[String]) -> impl IntoElement {
    div()
        .id("event-log")
        .flex()
        .flex_col()
        .flex_1()
        .min_h(px(160.0))
        .overflow_y_scroll()
        .p_2()
        .rounded_md()
        .bg(rgb(0x0c1016))
        .border_1()
        .border_color(rgb(0x2a3340))
        .children(
            events
                .iter()
                .rev()
                .take(80)
                .map(|line| div().text_xs().opacity(0.9).child(line.clone())),
        )
}

fn field(
    cx: &mut Context<ServerView>,
    id: &'static str,
    caption: &str,
    value: &str,
    editing: bool,
    enabled: bool,
    target: EditTarget,
) -> impl IntoElement {
    let display = if editing {
        format!("{value}|")
    } else {
        value.to_string()
    };
    div()
        .flex()
        .flex_col()
        .gap_1()
        .flex_1()
        .child(div().text_xs().opacity(0.65).child(caption.to_string()))
        .child(
            div()
                .id(id)
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
                .opacity(if enabled { 1.0 } else { 0.55 })
                .cursor_text()
                .text_xs()
                .child(display)
                .on_click(cx.listener(move |this, _, window, cx| {
                    if enabled {
                        this.editing = target;
                        this.focus_handle.focus(window);
                        cx.notify();
                    }
                })),
        )
}

fn action_button<F>(
    cx: &mut Context<ServerView>,
    id: &'static str,
    label: &str,
    color: gpui::Rgba,
    disabled: bool,
    on_click: F,
) -> impl IntoElement
where
    F: Fn(&mut ServerView, &gpui::ClickEvent, &mut Context<ServerView>) + 'static + Clone,
{
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_md()
        .bg(color)
        .opacity(if disabled { 0.45 } else { 1.0 })
        .cursor_pointer()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .child(label.to_string())
        .on_click(cx.listener(move |this, event, _, cx| {
            if !disabled {
                on_click(this, event, cx);
            }
        }))
}

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
