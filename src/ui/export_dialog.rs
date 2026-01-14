// Export dialog for advanced export settings
// Basic export settings are in main_window.rs
// This module is reserved for a modal export dialog

use crate::project::{
    ExportPreset, ExportSettings, AUDIO_CODECS, RESOLUTION_PRESETS, SUPPORTED_AUDIO_FORMATS,
    SUPPORTED_VIDEO_FORMATS, VIDEO_CODECS,
};
use eframe::egui;

/// Render advanced export settings dialog
pub fn render_export_dialog(
    ui: &mut egui::Ui,
    settings: &mut ExportSettings,
    show: &mut bool,
) {
    egui::Window::new("Export Settings")
        .open(show)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            egui::Grid::new("export_dialog_grid").show(ui, |ui| {
                // Format selection
                ui.label("Format:");
                egui::ComboBox::from_id_salt("export_format")
                    .selected_text(&settings.format)
                    .show_ui(ui, |ui| {
                        ui.heading("Video");
                        for format in SUPPORTED_VIDEO_FORMATS {
                            if ui
                                .selectable_label(settings.format == *format, *format)
                                .clicked()
                            {
                                settings.set_format(format);
                            }
                        }
                        ui.separator();
                        ui.heading("Audio");
                        for format in SUPPORTED_AUDIO_FORMATS {
                            if ui
                                .selectable_label(settings.format == *format, *format)
                                .clicked()
                            {
                                settings.set_format(format);
                            }
                        }
                    });
                ui.end_row();

                // Quality preset
                ui.label("Quality:");
                egui::ComboBox::from_id_salt("export_preset")
                    .selected_text(settings.preset.name())
                    .show_ui(ui, |ui| {
                        for preset in ExportPreset::all() {
                            if ui
                                .selectable_label(settings.preset == *preset, preset.name())
                                .clicked()
                            {
                                settings.apply_preset(*preset);
                            }
                        }
                    });
                ui.end_row();

                // Video codec
                if let Some(ref mut vcodec) = settings.video_codec {
                    ui.label("Video Codec:");
                    egui::ComboBox::from_id_salt("video_codec")
                        .selected_text(vcodec.as_str())
                        .show_ui(ui, |ui| {
                            for (codec, name) in VIDEO_CODECS {
                                if ui.selectable_label(vcodec == *codec, *name).clicked() {
                                    *vcodec = codec.to_string();
                                }
                            }
                        });
                    ui.end_row();
                }

                // Audio codec
                if let Some(ref mut acodec) = settings.audio_codec {
                    ui.label("Audio Codec:");
                    egui::ComboBox::from_id_salt("audio_codec")
                        .selected_text(acodec.as_str())
                        .show_ui(ui, |ui| {
                            for (codec, name) in AUDIO_CODECS {
                                if ui.selectable_label(acodec == *codec, *name).clicked() {
                                    *acodec = codec.to_string();
                                }
                            }
                        });
                    ui.end_row();
                }

                // Resolution
                ui.label("Resolution:");
                let current_res = settings
                    .resolution
                    .map(|(w, h)| format!("{}x{}", w, h))
                    .unwrap_or_else(|| "Original".to_string());

                egui::ComboBox::from_id_salt("resolution")
                    .selected_text(current_res)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(settings.resolution.is_none(), "Original")
                            .clicked()
                        {
                            settings.resolution = None;
                        }
                        for (name, res) in RESOLUTION_PRESETS {
                            if ui
                                .selectable_label(settings.resolution == Some(*res), *name)
                                .clicked()
                            {
                                settings.resolution = Some(*res);
                            }
                        }
                    });
                ui.end_row();

                // CRF for custom preset
                if settings.preset == ExportPreset::Custom {
                    if let Some(ref mut crf) = settings.crf {
                        ui.label("CRF:");
                        ui.add(egui::Slider::new(crf, 0..=51));
                        ui.end_row();
                    }

                    if let Some(ref mut vbitrate) = settings.video_bitrate {
                        ui.label("Video Bitrate:");
                        ui.add(egui::Slider::new(vbitrate, 100..=50000).suffix(" kbps"));
                        ui.end_row();
                    }

                    if let Some(ref mut abitrate) = settings.audio_bitrate {
                        ui.label("Audio Bitrate:");
                        ui.add(egui::Slider::new(abitrate, 32..=512).suffix(" kbps"));
                        ui.end_row();
                    }
                }
            });

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Reset to Defaults").clicked() {
                    *settings = ExportSettings::default();
                }
            });
        });
}
