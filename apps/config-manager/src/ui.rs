//! GPUI Config Manager window.

use anyhow::Result;
use gpui::{
    App, Application, Bounds, Context, FocusHandle, Focusable, Font, FontFallbacks, FontFeatures,
    FontStyle, FontWeight, InteractiveElement, KeyDownEvent, SharedString, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, rgb, size,
};
use suite_core::{Language, reveal_in_file_manager, t};

use crate::model::SettingsEditor;

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditTarget {
    None,
    Discovery,
    PortStart,
    PortEnd,
    ExtraKey(usize),
    ExtraValue(usize),
    NewKey,
    NewValue,
}

pub fn run_gpui(title: String, language: Language) -> Result<()> {
    Application::new().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(760.0), px(720.0)), cx);
        let title = SharedString::from(title);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some(title.clone()),
                    ..Default::default()
                }),
                app_id: Some(suite_core::ToolId::ConfigManager.binary_name().into()),
                ..Default::default()
            },
            move |_, cx| cx.new(|cx| ConfigView::new(cx, language)),
        )
        .expect("open GPUI Config Manager window");
        cx.activate(true);
    });
    Ok(())
}

struct ConfigView {
    language: Language,
    editor: SettingsEditor,
    editing: EditTarget,
    focus_handle: FocusHandle,
}

impl ConfigView {
    fn new(cx: &mut Context<Self>, language: Language) -> Self {
        Self {
            language,
            editor: SettingsEditor::load(),
            editing: EditTarget::None,
            focus_handle: cx.focus_handle(),
        }
    }

    fn begin_edit(&mut self, target: EditTarget, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = target;
        self.focus_handle.focus(window);
        cx.notify();
    }

    fn apply_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if self.editing == EditTarget::None {
            return;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.editing = EditTarget::None;
            cx.notify();
            return;
        }
        if key == "enter" {
            self.editing = EditTarget::None;
            cx.notify();
            return;
        }
        let Some(field) = self.field_mut() else {
            return;
        };
        if key == "backspace" {
            field.pop();
            cx.notify();
            return;
        }
        if let Some(ch) = event.keystroke.key_char.as_deref()
            && ch.chars().all(|c| !c.is_control())
            && field.len() + ch.len() <= 256
        {
            field.push_str(ch);
            cx.notify();
        }
    }

    fn field_mut(&mut self) -> Option<&mut String> {
        match self.editing {
            EditTarget::None => None,
            EditTarget::Discovery => Some(&mut self.editor.discovery_server),
            EditTarget::PortStart => Some(&mut self.editor.port_start),
            EditTarget::PortEnd => Some(&mut self.editor.port_end),
            EditTarget::NewKey => Some(&mut self.editor.new_key),
            EditTarget::NewValue => Some(&mut self.editor.new_value),
            EditTarget::ExtraKey(i) => self.editor.extras.get_mut(i).map(|(k, _)| k),
            EditTarget::ExtraValue(i) => self.editor.extras.get_mut(i).map(|(_, v)| v),
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        match self.editor.save() {
            Ok(()) => {}
            Err(e) => {
                self.editor.status = None;
                self.editor.error = Some(e);
            }
        }
        cx.notify();
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        self.editor.reload();
        self.editing = EditTarget::None;
        cx.notify();
    }
}

impl Focusable for ConfigView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ConfigView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lang = self.language;
        let path = self.editor.path.display().to_string();
        let can_save = !self.editor.unreadable;
        let extras = self.editor.extras.clone();
        let editing = self.editing;
        let mut extra_rows = Vec::new();
        for (i, (k, v)) in extras.iter().enumerate() {
            extra_rows.push(extra_row(cx, lang, i, k, v, editing).into_any_element());
        }

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
                this.apply_key(event, cx);
            }))
            .child(
                div()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(0x2a3340))
                    .font_weight(FontWeight::BOLD)
                    .child(t(lang, "tool.config_manager")),
            )
            .child(
                div()
                    .id("config-body")
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_4()
                    .overflow_y_scroll()
                    .flex_1()
                    .child(label_row(t(lang, "config.path"), path.clone()))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(action_button(
                                cx,
                                "reload".into(),
                                t(lang, "config.reload"),
                                rgb(0x243041),
                                |this, _, cx| this.reload(cx),
                            ))
                            .child(action_button(
                                cx,
                                "reveal".into(),
                                t(lang, "config.reveal"),
                                rgb(0x243041),
                                move |this, _, cx| {
                                    let _ = reveal_in_file_manager(&this.editor.path);
                                    cx.notify();
                                },
                            ))
                            .child(action_button(
                                cx,
                                "save".into(),
                                t(lang, "save"),
                                if can_save {
                                    rgb(0x2f6fed)
                                } else {
                                    rgb(0x243041)
                                },
                                |this, _, cx| this.save(cx),
                            )),
                    )
                    .child(status_line(&self.editor))
                    .child(section(t(lang, "config.discovery")))
                    .child(field(
                        cx,
                        "discovery".into(),
                        t(lang, "config.discovery"),
                        &self.editor.discovery_server,
                        self.editing == EditTarget::Discovery,
                        EditTarget::Discovery,
                    ))
                    .child(
                        div()
                            .text_xs()
                            .opacity(0.65)
                            .child(t(lang, "config.discovery_hint")),
                    )
                    .child(action_button(
                        cx,
                        "dns-sd".into(),
                        t(lang, "config.clear_discovery"),
                        rgb(0x243041),
                        |this, _, cx| {
                            this.editor.clear_discovery();
                            cx.notify();
                        },
                    ))
                    .child(section(t(lang, "config.port_start")))
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(field(
                                cx,
                                "port-start".into(),
                                t(lang, "config.port_start"),
                                &self.editor.port_start,
                                self.editing == EditTarget::PortStart,
                                EditTarget::PortStart,
                            ))
                            .child(field(
                                cx,
                                "port-end".into(),
                                t(lang, "config.port_end"),
                                &self.editor.port_end,
                                self.editing == EditTarget::PortEnd,
                                EditTarget::PortEnd,
                            )),
                    )
                    .child(section(t(lang, "config.extra")))
                    .children(extra_rows)
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .items_end()
                            .child(field(
                                cx,
                                "new-key".into(),
                                t(lang, "config.key"),
                                &self.editor.new_key,
                                self.editing == EditTarget::NewKey,
                                EditTarget::NewKey,
                            ))
                            .child(field(
                                cx,
                                "new-value".into(),
                                t(lang, "config.value"),
                                &self.editor.new_value,
                                self.editing == EditTarget::NewValue,
                                EditTarget::NewValue,
                            ))
                            .child(action_button(
                                cx,
                                "add".into(),
                                t(lang, "config.add"),
                                rgb(0x243041),
                                |this, _, cx| {
                                    if let Err(e) = this.editor.add_extra() {
                                        this.editor.error = Some(e);
                                        this.editor.status = None;
                                    }
                                    cx.notify();
                                },
                            )),
                    ),
            )
    }
}

fn extra_row(
    cx: &mut Context<ConfigView>,
    lang: Language,
    index: usize,
    key: &str,
    value: &str,
    editing: EditTarget,
) -> impl IntoElement + use<> {
    div()
        .flex()
        .gap_2()
        .items_end()
        .child(field(
            cx,
            SharedString::from(format!("ek-{index}")),
            t(lang, "config.key"),
            key,
            editing == EditTarget::ExtraKey(index),
            EditTarget::ExtraKey(index),
        ))
        .child(field(
            cx,
            SharedString::from(format!("ev-{index}")),
            t(lang, "config.value"),
            value,
            editing == EditTarget::ExtraValue(index),
            EditTarget::ExtraValue(index),
        ))
        .child(action_button(
            cx,
            SharedString::from(format!("del-{index}")),
            t(lang, "config.delete"),
            rgb(0xb33a3a),
            move |this, _, cx| {
                this.editor.remove_extra(index);
                this.editing = EditTarget::None;
                cx.notify();
            },
        ))
}

fn field(
    cx: &mut Context<ConfigView>,
    id: SharedString,
    caption: &str,
    value: &str,
    editing: bool,
    target: EditTarget,
) -> impl IntoElement + use<> {
    let display = if editing {
        format!("{value}|")
    } else {
        value.to_string()
    };
    let caption = SharedString::from(caption.to_string());
    div()
        .flex()
        .flex_col()
        .gap_1()
        .flex_1()
        .min_w_0()
        .child(div().text_xs().opacity(0.65).child(caption))
        .child(
            div()
                .id(id)
                .w_full()
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
                .cursor_text()
                .text_xs()
                .child(display)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.begin_edit(target, window, cx);
                })),
        )
}

fn action_button<F>(
    cx: &mut Context<ConfigView>,
    id: SharedString,
    label: &str,
    color: gpui::Rgba,
    on_click: F,
) -> impl IntoElement + use<F>
where
    F: Fn(&mut ConfigView, &gpui::ClickEvent, &mut Context<ConfigView>) + 'static + Clone,
{
    let label = SharedString::from(label.to_string());
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_md()
        .bg(color)
        .cursor_pointer()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .child(label)
        .on_click(cx.listener(move |this, event, _, cx| on_click(this, event, cx)))
}

fn section(title: &str) -> impl IntoElement + use<> {
    div()
        .mt_2()
        .font_weight(FontWeight::BOLD)
        .child(SharedString::from(title.to_string()))
}

fn label_row(caption: &str, value: String) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_xs().opacity(0.65).child(caption.to_string()))
        .child(div().text_xs().child(value))
}

fn status_line(editor: &SettingsEditor) -> impl IntoElement + use<> {
    if let Some(err) = &editor.error {
        div().text_xs().text_color(rgb(0xff8a8a)).child(err.clone())
    } else if let Some(ok) = &editor.status {
        div().text_xs().text_color(rgb(0x8ad7a0)).child(ok.clone())
    } else {
        div()
    }
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
