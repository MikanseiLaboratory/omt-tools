//! Windows-style hierarchical context menu for the preview surface.

use gpui::{
    div, prelude::*, px, rgb, Context, InteractiveElement, MouseDownEvent, SharedString, Window,
};
use suite_core::{t, Language};

use omt_media::{BufferUnit, DelaySetting};

use crate::ui::{MonitorSettings, MonitorView, VideoQualityPreset};

/// Menu row height (tight, OS-like).
const ROW_H: f32 = 24.0;
/// Panel width used for flush submenu placement.
const PANEL_W: f32 = 220.0;

/// Open context-menu placement and expansion state.
#[derive(Debug, Clone)]
pub struct ContextMenuState {
    /// Window-space X.
    pub x: f32,
    /// Window-space Y.
    pub y: f32,
    /// Currently expanded submenu path (root → leaf).
    pub path: Vec<MenuNodeId>,
}

/// Submenu identity along the flyout path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuNodeId {
    Settings,
    Audio,
    AudioBoost,
    AvBuffer,
    VideoBuffer,
    AudioBuffer,
    Video,
    VideoQuality,
    Overlay,
}

#[derive(Debug, Clone)]
enum MenuEntry {
    Item {
        id: SharedString,
        label: SharedString,
        checked: bool,
        action: MenuAction,
    },
    Submenu {
        id: MenuNodeId,
        label: SharedString,
    },
    Note {
        label: SharedString,
    },
    Separator,
}

/// Action triggered by a leaf menu item.
#[derive(Debug, Clone)]
pub enum MenuAction {
    Fullscreen,
    SetBoost(i32),
    SetVideoDelay(DelaySetting),
    SetAudioDelay(DelaySetting),
    ToggleBufferLink,
    ToggleAlpha,
    SetQuality(VideoQualityPreset),
    ToggleSafeArea,
    ToggleVu,
    Help,
    Exit,
}

/// Create a fresh menu at window coordinates.
pub fn open_at(x: f32, y: f32) -> ContextMenuState {
    ContextMenuState {
        x,
        y,
        path: Vec::new(),
    }
}

/// Render the full-screen dismiss layer + flyout panels.
pub fn render_overlay(
    settings: &MonitorSettings,
    language: Language,
    menu: &ContextMenuState,
    cx: &mut Context<MonitorView>,
) -> impl IntoElement {
    let panels = build_panels(language, settings, menu);
    let mut root = div()
        .id("ctx-menu-root")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.close_context_menu(cx);
            }),
        )
        .on_mouse_down(
            gpui::MouseButton::Right,
            cx.listener(|this, _, _, cx| {
                this.close_context_menu(cx);
            }),
        );

    for (i, panel) in panels.into_iter().enumerate() {
        root = root.child(render_panel(i, panel, cx));
    }
    root
}

struct MenuPanel {
    x: f32,
    y: f32,
    entries: Vec<MenuEntry>,
}

fn build_panels(
    language: Language,
    settings: &MonitorSettings,
    menu: &ContextMenuState,
) -> Vec<MenuPanel> {
    let mut panels = Vec::new();
    panels.push(MenuPanel {
        x: menu.x,
        y: menu.y,
        entries: root_entries(language),
    });

    // Flush against parent: no horizontal gap between panels.
    let mut anchor_x = menu.x + PANEL_W;
    let mut anchor_y = menu.y;
    for node in &menu.path {
        let y_off = submenu_y_offset(language, node);
        panels.push(MenuPanel {
            x: anchor_x,
            y: anchor_y + y_off,
            entries: submenu_entries(language, settings, node),
        });
        anchor_x += PANEL_W;
        anchor_y += y_off;
    }
    panels
}

fn root_entries(language: Language) -> Vec<MenuEntry> {
    vec![
        MenuEntry::Item {
            id: "fullscreen".into(),
            label: SharedString::from(t(language, "monitor.fullscreen")),
            checked: false,
            action: MenuAction::Fullscreen,
        },
        MenuEntry::Separator,
        MenuEntry::Submenu {
            id: MenuNodeId::Settings,
            label: SharedString::from(t(language, "monitor.settings")),
        },
        MenuEntry::Item {
            id: "help".into(),
            label: SharedString::from(t(language, "monitor.help")),
            checked: false,
            action: MenuAction::Help,
        },
        MenuEntry::Item {
            id: "exit".into(),
            label: SharedString::from(t(language, "monitor.exit")),
            checked: false,
            action: MenuAction::Exit,
        },
    ]
}

fn submenu_entries(
    language: Language,
    settings: &MonitorSettings,
    node: &MenuNodeId,
) -> Vec<MenuEntry> {
    match node {
        MenuNodeId::Settings => vec![
            MenuEntry::Submenu {
                id: MenuNodeId::Audio,
                label: SharedString::from(t(language, "monitor.audio")),
            },
            MenuEntry::Submenu {
                id: MenuNodeId::Video,
                label: SharedString::from(t(language, "monitor.video")),
            },
            MenuEntry::Submenu {
                id: MenuNodeId::Overlay,
                label: SharedString::from(t(language, "monitor.overlay")),
            },
        ],
        MenuNodeId::Audio => vec![
            MenuEntry::Submenu {
                id: MenuNodeId::AudioBoost,
                label: SharedString::from(t(language, "monitor.audio_boost")),
            },
            MenuEntry::Submenu {
                id: MenuNodeId::AvBuffer,
                label: SharedString::from(t(language, "monitor.av_buffer")),
            },
        ],
        MenuNodeId::AudioBoost => [0, 6, 10, 20]
            .into_iter()
            .map(|db| MenuEntry::Item {
                id: SharedString::from(format!("boost-{db}")),
                label: SharedString::from(format!("+{db} dB")),
                checked: settings.audio_boost_db == db,
                action: MenuAction::SetBoost(db),
            })
            .collect(),
        MenuNodeId::AvBuffer => vec![
            MenuEntry::Submenu {
                id: MenuNodeId::VideoBuffer,
                label: SharedString::from(format_delay_submenu_label(
                    language,
                    t(language, "monitor.buffer_video"),
                    settings.buffer.video,
                )),
            },
            MenuEntry::Submenu {
                id: MenuNodeId::AudioBuffer,
                label: SharedString::from(format_delay_submenu_label(
                    language,
                    t(language, "monitor.buffer_audio"),
                    settings.buffer.audio,
                )),
            },
            MenuEntry::Separator,
            MenuEntry::Item {
                id: "buf-link".into(),
                label: SharedString::from(t(language, "monitor.buffer_link")),
                checked: settings.buffer.linked,
                action: MenuAction::ToggleBufferLink,
            },
            MenuEntry::Note {
                label: SharedString::from(t(language, "monitor.buffer_unlink_info")),
            },
        ],
        MenuNodeId::VideoBuffer => delay_preset_entries(
            language,
            "v",
            settings.buffer.video,
            MenuAction::SetVideoDelay,
        ),
        MenuNodeId::AudioBuffer => delay_preset_entries(
            language,
            "a",
            settings.buffer.audio,
            MenuAction::SetAudioDelay,
        ),
        MenuNodeId::Video => vec![
            MenuEntry::Item {
                id: "alpha".into(),
                label: SharedString::from(t(language, "monitor.alpha_mask")),
                checked: settings.show_alpha,
                action: MenuAction::ToggleAlpha,
            },
            MenuEntry::Submenu {
                id: MenuNodeId::VideoQuality,
                label: SharedString::from(t(language, "monitor.quality")),
            },
        ],
        MenuNodeId::VideoQuality => [
            (
                VideoQualityPreset::Default,
                t(language, "monitor.quality_default"),
            ),
            (VideoQualityPreset::Low, t(language, "monitor.quality_low")),
            (
                VideoQualityPreset::Medium,
                t(language, "monitor.quality_medium"),
            ),
            (VideoQualityPreset::High, t(language, "monitor.quality_high")),
            (
                VideoQualityPreset::LowBandwidth,
                t(language, "monitor.quality_low_bw"),
            ),
        ]
        .into_iter()
        .map(|(preset, label)| MenuEntry::Item {
            id: SharedString::from(format!("q-{preset:?}")),
            label: SharedString::from(label),
            checked: settings.quality == preset,
            action: MenuAction::SetQuality(preset),
        })
        .collect(),
        MenuNodeId::Overlay => vec![
            MenuEntry::Item {
                id: "safe".into(),
                label: SharedString::from(t(language, "monitor.safe_area")),
                checked: settings.safe_area,
                action: MenuAction::ToggleSafeArea,
            },
            MenuEntry::Item {
                id: "vu".into(),
                label: SharedString::from(t(language, "monitor.vu_meter")),
                checked: settings.vu_meter,
                action: MenuAction::ToggleVu,
            },
        ],
    }
}

fn format_delay_submenu_label(language: Language, title: &str, delay: DelaySetting) -> String {
    format!("{title} ({})", format_delay_short(language, delay))
}

fn format_delay_short(language: Language, delay: DelaySetting) -> String {
    match delay.unit {
        BufferUnit::Milliseconds => format!("{} ms", delay.amount),
        BufferUnit::Frames if delay.amount == 1 => {
            format!("1 {}", t(language, "monitor.buffer_frame"))
        }
        BufferUnit::Frames => {
            format!(
                "{} {}",
                delay.amount,
                t(language, "monitor.buffer_frames")
            )
        }
    }
}

fn delay_preset_entries(
    language: Language,
    prefix: &str,
    current: DelaySetting,
    wrap: fn(DelaySetting) -> MenuAction,
) -> Vec<MenuEntry> {
    let mut entries = Vec::new();
    for ms in [0u32, 50, 100, 200, 500] {
        let delay = DelaySetting {
            amount: ms,
            unit: BufferUnit::Milliseconds,
        };
        entries.push(MenuEntry::Item {
            id: SharedString::from(format!("buf-{prefix}-ms-{ms}")),
            label: SharedString::from(format!("{ms} ms")),
            checked: current == delay,
            action: wrap(delay),
        });
    }
    entries.push(MenuEntry::Separator);
    for frames in [0u32, 1, 2, 3, 5] {
        let delay = DelaySetting {
            amount: frames,
            unit: BufferUnit::Frames,
        };
        let label = if frames == 1 {
            format!("1 {}", t(language, "monitor.buffer_frame"))
        } else {
            format!("{frames} {}", t(language, "monitor.buffer_frames"))
        };
        entries.push(MenuEntry::Item {
            id: SharedString::from(format!("buf-{prefix}-fr-{frames}")),
            label: SharedString::from(label),
            checked: current == delay,
            action: wrap(delay),
        });
    }
    entries
}

fn submenu_y_offset(language: Language, node: &MenuNodeId) -> f32 {
    let root = root_entries(language);
    let idx = root.iter().position(|e| match (e, node) {
        (MenuEntry::Submenu { id, .. }, n) => id == n,
        _ => false,
    });
    match node {
        MenuNodeId::Settings => idx.map(|i| i as f32 * ROW_H).unwrap_or(0.0),
        MenuNodeId::Audio => 0.0,
        MenuNodeId::Video => ROW_H,
        MenuNodeId::Overlay => ROW_H * 2.0,
        MenuNodeId::AudioBoost => 0.0,
        MenuNodeId::AvBuffer => ROW_H,
        MenuNodeId::VideoBuffer => 0.0,
        MenuNodeId::AudioBuffer => ROW_H,
        MenuNodeId::VideoQuality => ROW_H,
    }
}

fn render_panel(
    panel_idx: usize,
    panel: MenuPanel,
    cx: &mut Context<MonitorView>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("ctx-panel-{panel_idx}")))
        .absolute()
        .left(px(panel.x))
        .top(px(panel.y))
        .w(px(PANEL_W))
        .py_0()
        .border_1()
        .border_color(rgb(0x5a5a5a))
        .bg(rgb(0xf0f0f0))
        .text_color(rgb(0x1a1a1a))
        .shadow_lg()
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| {
            cx.stop_propagation();
        })
        .children(
            panel
                .entries
                .into_iter()
                .enumerate()
                .map(|(i, entry)| render_entry(panel_idx, i, entry, cx).into_any_element()),
        )
}

fn render_entry(
    panel_idx: usize,
    row: usize,
    entry: MenuEntry,
    cx: &mut Context<MonitorView>,
) -> impl IntoElement {
    match entry {
        MenuEntry::Separator => div()
            .id(SharedString::from(format!("sep-{panel_idx}-{row}")))
            .h(px(5.0))
            .px_1()
            .flex()
            .items_center()
            .child(div().h(px(1.0)).w_full().bg(rgb(0xc0c0c0)))
            .into_any_element(),
        MenuEntry::Note { label } => div()
            .id(SharedString::from(format!("note-{panel_idx}-{row}")))
            .px_2()
            .py_1()
            .text_xs()
            .text_color(rgb(0x666666))
            .child(label)
            .into_any_element(),
        MenuEntry::Submenu { id, label } => {
            let node = id.clone();
            div()
                .id(SharedString::from(format!("sub-{panel_idx}-{row}")))
                .px_2()
                .h(px(ROW_H))
                .flex()
                .items_center()
                .justify_between()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(0x0078d4)).text_color(rgb(0xffffff)))
                .child(label)
                .child(SharedString::from("▸"))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.expand_menu_node(node.clone(), cx);
                    }),
                )
                .into_any_element()
        }
        MenuEntry::Item {
            id,
            label,
            checked,
            action,
        } => {
            let check = if checked { "✓ " } else { "   " };
            div()
                .id(id)
                .px_2()
                .h(px(ROW_H))
                .flex()
                .items_center()
                .cursor_pointer()
                .hover(|s| s.bg(rgb(0x0078d4)).text_color(rgb(0xffffff)))
                .child(SharedString::from(format!("{check}{label}")))
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.apply_menu_action(action.clone(), window, cx);
                    }),
                )
                .into_any_element()
        }
    }
}

/// Shared by [`MonitorView`] action dispatch.
pub fn dispatch_action(
    view: &mut MonitorView,
    action: MenuAction,
    window: &mut Window,
    cx: &mut Context<MonitorView>,
) {
    match action {
        MenuAction::Fullscreen => view.enter_fullscreen(window, cx),
        MenuAction::SetBoost(db) => view.set_audio_boost_db(db),
        MenuAction::SetVideoDelay(delay) => view.set_video_delay(delay),
        MenuAction::SetAudioDelay(delay) => view.set_audio_delay(delay),
        MenuAction::ToggleBufferLink => view.toggle_buffer_link(),
        MenuAction::ToggleAlpha => {
            view.settings.show_alpha = !view.settings.show_alpha;
            view.invalidate_texture(cx);
        }
        MenuAction::SetQuality(preset) => {
            view.settings.quality = preset;
            view.reapply_connection(cx);
        }
        MenuAction::ToggleSafeArea => view.settings.safe_area = !view.settings.safe_area,
        MenuAction::ToggleVu => view.settings.vu_meter = !view.settings.vu_meter,
        MenuAction::Help => cx.open_url("https://github.com/MikanseiLaboratory/omt-tools"),
        MenuAction::Exit => cx.quit(),
    }
    view.context_menu = None;
    cx.notify();
}
