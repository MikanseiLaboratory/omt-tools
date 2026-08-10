//! Preferences overlay — language, theme, audio output, A/V buffer, version, license.

use gpui::{div, prelude::*, px, rgb, Context, FontWeight, ScrollWheelEvent, SharedString};
use omt_media::{AudioOutputDevice, BufferSettings, BufferUnit, DelaySetting};
use suite_core::{t, Language, ThemePreference, SUITE_VERSION};

use crate::chrome::UiChrome;
use crate::ui::MonitorView;

const PANEL_W: f32 = 460.0;
const DEVICE_LIST_H: f32 = 140.0;

/// Full-window preferences overlay.
pub fn render_overlay(
    language: Language,
    theme: ThemePreference,
    suite_version: &str,
    chrome: UiChrome,
    audio_devices: &[AudioOutputDevice],
    selected_audio: Option<&str>,
    buffer: BufferSettings,
    video_delay_ms: u32,
    audio_delay_ms: u32,
    cx: &mut Context<MonitorView>,
) -> impl IntoElement {
    div()
        .id("prefs-root")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(gpui::rgba(0x00000080))
        .overflow_hidden()
        .on_scroll_wheel(cx.listener(|_, _: &ScrollWheelEvent, _, cx| {
            cx.stop_propagation();
        }))
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.close_preferences(cx);
            }),
        )
        .on_mouse_down(
            gpui::MouseButton::Middle,
            cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
            }),
        )
        .on_mouse_down(
            gpui::MouseButton::Right,
            cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
            }),
        )
        .child(
            div()
                .id("prefs-panel")
                .w(px(PANEL_W))
                .rounded_lg()
                .border_1()
                .border_color(rgb(chrome.border))
                .bg(rgb(chrome.panel))
                .text_color(rgb(chrome.text))
                .overflow_hidden()
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                    cx.stop_propagation();
                })
                .on_scroll_wheel(|_, _, cx| {
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(rgb(chrome.border))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(t(language, "monitor.preferences")),
                        )
                        .child(
                            div()
                                .id("prefs-close")
                                .px_2()
                                .py_0p5()
                                .rounded_md()
                                .bg(rgb(chrome.surface))
                                .cursor_pointer()
                                .child(t(language, "back"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.close_preferences(cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .p_4()
                        .gap_3()
                        .flex()
                        .flex_col()
                        .child(section_title(chrome, t(language, "language")))
                        .child(choice_row(
                            cx,
                            chrome,
                            &[
                                (Language::English.display_name(), Language::English),
                                (Language::Japanese.display_name(), Language::Japanese),
                            ],
                            language,
                            |this, lang, cx| this.set_language(lang, cx),
                        ))
                        .child(section_title(chrome, t(language, "theme")))
                        .child(theme_row(cx, chrome, language, theme))
                        .child(section_title(chrome, t(language, "monitor.audio_output")))
                        .child(audio_device_list(
                            cx,
                            chrome,
                            language,
                            audio_devices,
                            selected_audio,
                        ))
                        .child(divider(chrome))
                        .child(section_title(chrome, t(language, "monitor.av_buffer")))
                        .child(buffer_section(
                            cx,
                            chrome,
                            language,
                            buffer,
                            video_delay_ms,
                            audio_delay_ms,
                        ))
                        .child(divider(chrome))
                        .child(section_title(chrome, t(language, "version")))
                        .child(info_block(
                            chrome,
                            format!("OMT Tools / Studio Monitor  v{suite_version}"),
                        ))
                        .child(section_title(chrome, t(language, "license")))
                        .child(info_block(
                            chrome,
                            format!(
                                "{}\n\n{}",
                                t(language, "monitor.license_spdx"),
                                t(language, "monitor.license_body")
                            ),
                        ))
                        .child(
                            div()
                                .id("prefs-license-link")
                                .mt_1()
                                .text_sm()
                                .text_color(rgb(chrome.accent))
                                .cursor_pointer()
                                .child(t(language, "monitor.license_link"))
                                .on_click(cx.listener(|_, _, _, cx| {
                                    cx.open_url(
                                        "https://github.com/MikanseiLaboratory/omt-tools/blob/main/LICENSE",
                                    );
                                })),
                        )
                        .child(
                            div()
                                .mt_1()
                                .text_xs()
                                .text_color(rgb(chrome.text_muted))
                                .child(SharedString::from(format!(
                                    "{} · suite {}",
                                    t(language, "tool.studio_monitor"),
                                    SUITE_VERSION
                                ))),
                        ),
                ),
        )
}

fn buffer_section(
    cx: &mut Context<MonitorView>,
    chrome: UiChrome,
    language: Language,
    buffer: BufferSettings,
    video_delay_ms: u32,
    audio_delay_ms: u32,
) -> impl IntoElement {
    div()
        .gap_2()
        .flex()
        .flex_col()
        .child(
            div()
                .text_sm()
                .text_color(rgb(chrome.text_muted))
                .child(SharedString::from(format!(
                    "{} · {}",
                    t(language, "monitor.buffer_video"),
                    format_delay_label(language, buffer.video, video_delay_ms)
                ))),
        )
        .child(delay_choice_row(cx, chrome, language, buffer.video, true))
        .child(
            div()
                .mt_1()
                .text_sm()
                .text_color(rgb(chrome.text_muted))
                .child(SharedString::from(format!(
                    "{} · {}",
                    t(language, "monitor.buffer_audio"),
                    format_delay_label(language, buffer.audio, audio_delay_ms)
                ))),
        )
        .child(delay_choice_row(cx, chrome, language, buffer.audio, false))
        .child(
            div()
                .mt_2()
                .gap_2()
                .flex()
                .flex_row()
                .items_start()
                .child(
                    div()
                        .id("buf-link-toggle")
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(if buffer.linked {
                            chrome.accent
                        } else {
                            chrome.border
                        }))
                        .bg(rgb(if buffer.linked {
                            chrome.accent_soft
                        } else {
                            chrome.surface
                        }))
                        .text_sm()
                        .text_color(rgb(if buffer.linked {
                            chrome.accent
                        } else {
                            chrome.text
                        }))
                        .cursor_pointer()
                        .child(SharedString::from(format!(
                            "{} {}",
                            if buffer.linked { "✓" } else { "○" },
                            t(language, "monitor.buffer_link")
                        )))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_buffer_link(!buffer.linked);
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(rgb(chrome.text_muted))
                        .child(SharedString::from(t(
                            language,
                            "monitor.buffer_unlink_info",
                        ))),
                ),
        )
}

fn format_delay_label(language: Language, delay: DelaySetting, delay_ms: u32) -> String {
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

fn delay_choice_row(
    cx: &mut Context<MonitorView>,
    chrome: UiChrome,
    language: Language,
    current: DelaySetting,
    is_video: bool,
) -> impl IntoElement {
    let mut chips: Vec<(String, DelaySetting)> = Vec::new();
    for ms in [0u32, 50, 100, 200, 500] {
        chips.push((
            format!("{ms} ms"),
            DelaySetting {
                amount: ms,
                unit: BufferUnit::Milliseconds,
            },
        ));
    }
    for frames in [1u32, 2, 3, 5] {
        let label = if frames == 1 {
            format!("1 {}", t(language, "monitor.buffer_frame"))
        } else {
            format!("{frames} {}", t(language, "monitor.buffer_frames"))
        };
        chips.push((
            label,
            DelaySetting {
                amount: frames,
                unit: BufferUnit::Frames,
            },
        ));
    }

    div()
        .gap_1()
        .flex()
        .flex_row()
        .flex_wrap()
        .children(chips.into_iter().enumerate().map(|(i, (label, delay))| {
            let active = current == delay;
            let id_prefix = if is_video { "vdelay" } else { "adelay" };
            div()
                .id(SharedString::from(format!("{id_prefix}-{i}")))
                .px_2()
                .py_1()
                .rounded_md()
                .border_1()
                .border_color(rgb(if active { chrome.accent } else { chrome.border }))
                .bg(rgb(if active {
                    chrome.accent_soft
                } else {
                    chrome.surface
                }))
                .text_xs()
                .text_color(rgb(if active { chrome.accent } else { chrome.text }))
                .cursor_pointer()
                .child(SharedString::from(label))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if is_video {
                        this.set_video_delay(delay);
                    } else {
                        this.set_audio_delay(delay);
                    }
                    cx.notify();
                }))
                .into_any_element()
        }))
}

fn audio_device_list(
    cx: &mut Context<MonitorView>,
    chrome: UiChrome,
    language: Language,
    devices: &[AudioOutputDevice],
    selected: Option<&str>,
) -> impl IntoElement {
    let mut rows: Vec<gpui::AnyElement> = Vec::new();

    let default_active = selected.is_none();
    rows.push(device_row(
        cx,
        chrome,
        "audio-default",
        t(language, "monitor.audio_default"),
        default_active,
        None,
    ));

    if devices.is_empty() {
        rows.push(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(rgb(chrome.text_muted))
                .child(SharedString::from(t(language, "monitor.audio_none")))
                .into_any_element(),
        );
    } else {
        for (i, device) in devices.iter().enumerate() {
            let active = selected == Some(device.name.as_str());
            let label = if device.is_default {
                format!("{} ({})", device.name, t(language, "monitor.audio_system_default"))
            } else {
                device.name.clone()
            };
            rows.push(device_row(
                cx,
                chrome,
                &format!("audio-dev-{i}"),
                &label,
                active,
                Some(device.name.clone()),
            ));
        }
    }

    div()
        .id("audio-device-list")
        .h(px(DEVICE_LIST_H))
        .rounded_md()
        .border_1()
        .border_color(rgb(chrome.border))
        .bg(rgb(chrome.surface))
        .overflow_y_scroll()
        .on_scroll_wheel(|_, _, cx| {
            // Keep wheel inside the device list; do not scroll the app behind.
            cx.stop_propagation();
        })
        .children(rows)
}

fn device_row(
    cx: &mut Context<MonitorView>,
    chrome: UiChrome,
    id: &str,
    label: &str,
    active: bool,
    device_name: Option<String>,
) -> gpui::AnyElement {
    div()
        .id(SharedString::from(id.to_string()))
        .px_2()
        .py_1p5()
        .border_b_1()
        .border_color(rgb(chrome.border))
        .bg(rgb(if active {
            chrome.accent_soft
        } else {
            chrome.surface
        }))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(chrome.surface_active)))
        .child(
            div()
                .text_sm()
                .text_color(rgb(if active { chrome.accent } else { chrome.text }))
                .child(SharedString::from(label.to_string())),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            this.set_audio_output_device(device_name.clone(), cx);
        }))
        .into_any_element()
}

fn section_title(chrome: UiChrome, label: &str) -> impl IntoElement {
    div()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(chrome.text_muted))
        .child(SharedString::from(label.to_uppercase()))
}

fn divider(chrome: UiChrome) -> impl IntoElement {
    div().h(px(1.0)).bg(rgb(chrome.border)).my_1()
}

fn info_block(chrome: UiChrome, body: String) -> impl IntoElement {
    div()
        .px_3()
        .py_2()
        .rounded_md()
        .bg(rgb(chrome.surface))
        .text_sm()
        .text_color(rgb(chrome.text))
        .child(SharedString::from(body))
}

fn theme_row(
    cx: &mut Context<MonitorView>,
    chrome: UiChrome,
    language: Language,
    selected: ThemePreference,
) -> impl IntoElement {
    let options = [
        (t(language, "theme.light"), ThemePreference::Light),
        (t(language, "theme.dark"), ThemePreference::Dark),
        (t(language, "theme.system"), ThemePreference::System),
    ];
    div()
        .gap_2()
        .flex()
        .flex_row()
        .children(options.into_iter().map(|(label, value)| {
            let active = selected == value;
            div()
                .id(SharedString::from(format!("theme-{value}")))
                .flex_1()
                .px_2()
                .py_1p5()
                .rounded_md()
                .border_1()
                .border_color(rgb(if active { chrome.accent } else { chrome.border }))
                .bg(rgb(if active {
                    chrome.accent_soft
                } else {
                    chrome.surface
                }))
                .text_sm()
                .text_color(rgb(if active { chrome.accent } else { chrome.text }))
                .cursor_pointer()
                .child(SharedString::from(label))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_theme(value, cx);
                }))
                .into_any_element()
        }))
}

fn choice_row<T: Copy + PartialEq + 'static>(
    cx: &mut Context<MonitorView>,
    chrome: UiChrome,
    options: &[(&str, T)],
    selected: T,
    on_pick: impl Fn(&mut MonitorView, T, &mut Context<MonitorView>) + Clone + 'static,
) -> impl IntoElement {
    div()
        .gap_2()
        .flex()
        .flex_row()
        .children(options.iter().enumerate().map(|(i, (label, value))| {
            let active = selected == *value;
            let value = *value;
            let on_pick = on_pick.clone();
            let label = (*label).to_string();
            div()
                .id(SharedString::from(format!("choice-{i}")))
                .flex_1()
                .px_2()
                .py_1p5()
                .rounded_md()
                .border_1()
                .border_color(rgb(if active { chrome.accent } else { chrome.border }))
                .bg(rgb(if active {
                    chrome.accent_soft
                } else {
                    chrome.surface
                }))
                .text_sm()
                .text_color(rgb(if active { chrome.accent } else { chrome.text }))
                .cursor_pointer()
                .child(SharedString::from(label))
                .on_click(cx.listener(move |this, _, _, cx| {
                    on_pick(this, value, cx);
                }))
                .into_any_element()
        }))
}
