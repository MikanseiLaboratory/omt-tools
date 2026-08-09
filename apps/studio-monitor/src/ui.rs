//! Studio Monitor UI layout.

use eframe::egui;
use omt_media::StallState;
use suite_core::t;

use crate::app::{FitMode, MonitorApp, stall_label};

pub fn draw(app: &mut MonitorApp, ctx: &egui::Context, stall: StallState) {
    egui::SidePanel::left("sources")
        .default_width(280.0)
        .show(ctx, |ui| {
            ui.heading(t(app.language, "monitor.sources"));
            if ui.button(t(app.language, "monitor.refresh")).clicked() {
                app.refresh_sources();
            }
            ui.separator();
            if app.sources.is_empty() {
                ui.label(t(app.language, "monitor.no_sources"));
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for src in app.sources.clone() {
                    let selected = app.selected.as_deref() == Some(src.url.as_str());
                    if ui.selectable_label(selected, &src.name).clicked() {
                        app.select_source(src.url.clone());
                    }
                    ui.weak(&src.url);
                }
            });
            ui.separator();
            ui.checkbox(
                &mut app.alpha_mask,
                t(app.language, "monitor.alpha_mask"),
            );
            ui.checkbox(
                &mut app.checkerboard,
                t(app.language, "monitor.checkerboard"),
            );
            ui.horizontal(|ui| {
                ui.selectable_value(&mut app.fit, FitMode::Fit, t(app.language, "monitor.fit"));
                ui.selectable_value(&mut app.fit, FitMode::Fill, t(app.language, "monitor.fill"));
            });
            ui.separator();
            ui.small(&app.status);
            if let Some(err) = app.worker.latest().error.lock().clone() {
                ui.colored_label(egui::Color32::LIGHT_RED, err);
            }
        });

    egui::TopBottomPanel::top("hud").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if let Some(frame) = &app.last_frame {
                ui.label(format!("{}×{}", frame.width, frame.height));
                ui.separator();
                ui.label(format!("{:.1} fps", app.fps.fps()));
                ui.separator();
                ui.label(format!(
                    "{}/{} fps src",
                    frame.fps_n,
                    frame.fps_d.max(1)
                ));
            } else {
                ui.label(t(app.language, "monitor.waiting"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("v{}", suite_core::SUITE_VERSION));
            });
        });
    });

    egui::CentralPanel::default()
        .frame(egui::Frame::central_panel(&ctx.style()).fill(egui::Color32::BLACK))
        .show(ctx, |ui| {
            let avail = ui.available_size();
            if let Some(texture) = &app.texture {
                let size = texture.size_vec2();
                let display = match app.fit {
                    FitMode::Fit => {
                        let scale = (avail.x / size.x).min(avail.y / size.y);
                        size * scale
                    }
                    FitMode::Fill => avail,
                };
                ui.centered_and_justified(|ui| {
                    ui.image((texture.id(), display));
                });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(t(app.language, "monitor.waiting"))
                            .color(egui::Color32::WHITE)
                            .size(28.0),
                    );
                });
            }

            let overlay = stall_label(app.language, stall);
            if !overlay.is_empty() {
                let painter = ui.painter();
                let rect = ui.max_rect();
                painter.rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(180, 0, 0, 90),
                );
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    overlay,
                    egui::FontId::proportional(42.0),
                    egui::Color32::WHITE,
                );
            }
        });
}
