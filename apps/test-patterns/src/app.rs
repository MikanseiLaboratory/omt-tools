//! Test Patterns UI and send control.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui;
use omt_media::{AudioToneConfig, SendSession, SendSessionConfig, SendStats};
use pattern_generator::{PatternKind, fill_uyvy, uyvy_from_image_path};
use suite_core::{Language, t};
use vmx::Profile;

pub struct PatternsApp {
    language: Language,
    name: String,
    kind: PatternKind,
    width: i32,
    height: i32,
    fps: i32,
    profile: Profile,
    animate: bool,
    tone_hz: f32,
    image_path: Option<PathBuf>,
    session: Option<SendSession>,
    last_stats: SendStats,
    error: Option<String>,
}

impl PatternsApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, language: Language) -> Self {
        Self {
            language,
            name: "Test Pattern".into(),
            kind: PatternKind::SmpteColorBars,
            width: 1920,
            height: 1080,
            fps: 30,
            profile: Profile::OmtSq,
            animate: true,
            tone_hz: 1000.0,
            image_path: None,
            session: None,
            last_stats: SendStats::default(),
            error: None,
        }
    }

    fn start(&mut self) {
        self.stop();
        self.error = None;

        let width = self.width;
        let height = self.height;
        let kind = self.kind;
        let animate = self.animate;
        let image_uyvy = if kind == PatternKind::Image {
            match self.image_path.as_ref() {
                Some(path) => match uyvy_from_image_path(path, width, height) {
                    Ok(buf) => Some(buf),
                    Err(e) => {
                        self.error = Some(e.to_string());
                        return;
                    }
                },
                None => {
                    self.error = Some("Select an image file first".into());
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
            let phase = if animate {
                ((idx % 300) as f32) / 300.0
            } else {
                0.0
            };
            let mut buf = vec![0u8; (width as usize) * 2 * (height as usize)];
            fill_uyvy(kind, &mut buf, width, height, phase);
            buf
        });

        let config = SendSessionConfig {
            name: self.name.clone(),
            width: self.width,
            height: self.height,
            fps_n: self.fps,
            fps_d: 1,
            profile: self.profile,
            animate: self.animate && self.kind != PatternKind::Image,
            audio: AudioToneConfig {
                tone_hz: self.tone_hz,
                ..Default::default()
            },
        };

        match SendSession::start(config, provider) {
            Ok(session) => self.session = Some(session),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn stop(&mut self) {
        if let Some(mut session) = self.session.take() {
            session.stop();
        }
    }
}

impl eframe::App for PatternsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(session) = &self.session {
            self.last_stats = session.stats();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(t(self.language, "tool.test_patterns"));
            ui.separator();

            ui.horizontal(|ui| {
                ui.label(t(self.language, "patterns.name"));
                ui.text_edit_singleline(&mut self.name);
            });

            ui.horizontal(|ui| {
                ui.label(t(self.language, "patterns.pattern"));
                egui::ComboBox::from_id_salt("pattern")
                    .selected_text(pattern_label(self.language, self.kind))
                    .show_ui(ui, |ui| {
                        for kind in PatternKind::builtins()
                            .iter()
                            .copied()
                            .chain(std::iter::once(PatternKind::Image))
                        {
                            ui.selectable_value(
                                &mut self.kind,
                                kind,
                                pattern_label(self.language, kind),
                            );
                        }
                    });
            });

            if self.kind == PatternKind::Image {
                ui.horizontal(|ui| {
                    ui.label(t(self.language, "patterns.image"));
                    if ui.button("…").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Images", &["png", "jpg", "jpeg", "bmp"])
                            .pick_file()
                        {
                            self.image_path = Some(path);
                        }
                    }
                    if let Some(path) = &self.image_path {
                        ui.monospace(path.display().to_string());
                    }
                });
            }

            ui.horizontal(|ui| {
                ui.label(t(self.language, "patterns.resolution"));
                ui.add(egui::DragValue::new(&mut self.width).range(64..=7680));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut self.height).range(64..=4320));
            });

            ui.horizontal(|ui| {
                ui.label(t(self.language, "patterns.fps"));
                ui.add(egui::DragValue::new(&mut self.fps).range(1..=120));
            });

            ui.horizontal(|ui| {
                ui.label(t(self.language, "patterns.profile"));
                egui::ComboBox::from_id_salt("profile")
                    .selected_text(profile_name(self.profile))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.profile, Profile::OmtLq, "LQ");
                        ui.selectable_value(&mut self.profile, Profile::OmtSq, "SQ");
                        ui.selectable_value(&mut self.profile, Profile::OmtHq, "HQ");
                    });
            });

            ui.checkbox(&mut self.animate, t(self.language, "patterns.animate"));

            ui.horizontal(|ui| {
                ui.label(t(self.language, "patterns.tone"));
                ui.add(egui::DragValue::new(&mut self.tone_hz).range(20.0..=8000.0).speed(10.0));
                for preset in [440.0, 1000.0, 2000.0] {
                    if ui.small_button(format!("{preset:.0}")).clicked() {
                        self.tone_hz = preset;
                    }
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                let sending = self.session.is_some();
                if !sending {
                    if ui.button(t(self.language, "patterns.start")).clicked() {
                        self.start();
                    }
                } else if ui.button(t(self.language, "patterns.stop")).clicked() {
                    self.stop();
                }
                let state = if sending {
                    t(self.language, "patterns.sending")
                } else {
                    t(self.language, "patterns.idle")
                };
                ui.label(state);
            });

            if let Some(err) = &self.error {
                ui.colored_label(egui::Color32::LIGHT_RED, err);
            }

            if self.session.is_some() {
                let st = &self.last_stats;
                ui.label(format!(
                    "port {} | video≈{:.1} fps | encode {:.1} ms | frames {} | dropped {}",
                    st.port, st.video_fps, st.encode_ms, st.frames, st.dropped
                ));
                if st.behind || cfg!(debug_assertions) {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        t(self.language, "patterns.perf_warn"),
                    );
                }
            }
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}

fn pattern_label(lang: Language, kind: PatternKind) -> &'static str {
    match lang {
        Language::Japanese => kind.label_ja(),
        Language::English => kind.label_en(),
    }
}

fn profile_name(profile: Profile) -> &'static str {
    match profile {
        Profile::OmtLq | Profile::Lq => "LQ",
        Profile::OmtHq | Profile::Hq => "HQ",
        _ => "SQ",
    }
}
