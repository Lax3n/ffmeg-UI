use crate::app::FFmpegApp;
use crate::player::PlaybackState;
use crate::project::{ExportPreset, SUPPORTED_AUDIO_FORMATS, SUPPORTED_VIDEO_FORMATS};
use crate::ui::{ActiveTool, CropPreset, TimelineWidget, TrimMode};
use crate::utils::format_time;
use eframe::egui;

pub fn render_main_window(app: &mut FFmpegApp, ctx: &egui::Context) {
    // Top menu bar
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        render_menu_bar(app, ui);
    });

    // Status bar at bottom
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        render_status_bar(app, ui);
    });

    // Timeline at bottom (above status bar)
    egui::TopBottomPanel::bottom("timeline_panel")
        .resizable(true)
        .min_height(100.0)
        .default_height(140.0)
        .show(ctx, |ui| {
            render_timeline_panel(app, ui);
        });

    // Left panel - File browser
    egui::SidePanel::left("file_panel")
        .default_width(250.0)
        .min_width(200.0)
        .show(ctx, |ui| {
            render_file_panel(app, ui);
        });

    // Right panel - Export queue (when visible)
    if app.show_queue_panel {
        egui::SidePanel::right("queue_panel")
            .default_width(300.0)
            .min_width(200.0)
            .show(ctx, |ui| {
                render_queue_panel(app, ui);
            });
    }

    // Central panel
    egui::CentralPanel::default().show(ctx, |ui| {
        // Preview area
        render_preview_area(app, ui);

        ui.separator();

        // Playback controls
        render_playback_controls(app, ui);

        ui.separator();

        // Tool tabs
        ui.horizontal(|ui| {
            for tool in ActiveTool::all() {
                if ui
                    .selectable_label(app.active_tool == *tool, tool.name())
                    .clicked()
                {
                    app.active_tool = *tool;
                }
            }
        });

        ui.separator();

        // Tool panel
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                render_tool_panel(app, ui);
            });
    });
}

fn render_menu_bar(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    egui::menu::bar(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("Open Files...").clicked() {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("Video", &["mp4", "mkv", "avi", "mov", "webm"])
                    .add_filter("Audio", &["mp3", "wav", "aac", "flac", "ogg"])
                    .add_filter("All Files", &["*"])
                    .pick_files()
                {
                    app.add_files(paths);
                }
                ui.close_menu();
            }
            if ui.button("Clear All").clicked() {
                app.project.clear();
                app.selected_file_index = None;
                app.player = None;
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Exit").clicked() {
                std::process::exit(0);
            }
        });

        ui.menu_button("Playback", |ui| {
            if ui.button("Play/Pause (Space)").clicked() {
                app.toggle_play_pause();
                ui.close_menu();
            }
            if ui.button("Stop").clicked() {
                app.stop_player();
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Set In Point (I)").clicked() {
                app.set_in_point();
                ui.close_menu();
            }
            if ui.button("Set Out Point (O)").clicked() {
                app.set_out_point();
                ui.close_menu();
            }
            if ui.button("Clear In/Out Points").clicked() {
                app.clear_in_out_points();
                ui.close_menu();
            }
        });

        ui.menu_button("Tools", |ui| {
            let has_file = app.selected_file().is_some();

            if ui.add_enabled(has_file, egui::Button::new("Open in LosslessCut"))
                .on_hover_text("Open in LosslessCut for precise cutting")
                .clicked()
            {
                app.open_in_losslesscut();
                ui.close_menu();
            }
            if ui.add_enabled(has_file, egui::Button::new("Open in mpv"))
                .on_hover_text("Open in mpv for smooth preview")
                .clicked()
            {
                app.open_in_mpv();
                ui.close_menu();
            }
            if ui.add_enabled(has_file, egui::Button::new("Open in Default Player"))
                .on_hover_text("Open with system default application")
                .clicked()
            {
                app.open_in_default_player();
                ui.close_menu();
            }
        });

        ui.menu_button("Help", |ui| {
            if ui.button("Keyboard Shortcuts").clicked() {
                // TODO: Show shortcuts dialog
                ui.close_menu();
            }
            if ui.button("About").clicked() {
                ui.close_menu();
            }
        });
    });
}

fn render_status_bar(app: &FFmpegApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(&app.status_message);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Ok(progress) = app.current_task.lock() {
                if let Some(ref p) = *progress {
                    if !p.is_complete {
                        ui.add(egui::ProgressBar::new(p.progress).show_percentage());
                    }
                }
            }

            ui.label(format!("{} files", app.project.files.len()));
        });
    });
}

fn render_file_panel(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    ui.heading("Source Files");

    if ui.button("+ Add Files").clicked() {
        if let Some(paths) = rfd::FileDialog::new()
            .add_filter("Video", &["mp4", "mkv", "avi", "mov", "webm"])
            .add_filter("Audio", &["mp3", "wav", "aac", "flac", "ogg"])
            .add_filter("All Files", &["*"])
            .pick_files()
        {
            app.add_files(paths);
        }
    }

    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(200.0)
        .show(ui, |ui| {
            let mut to_select: Option<usize> = None;

            for (i, file) in app.project.files.iter().enumerate() {
                let is_selected = app.selected_file_index == Some(i);
                let response = ui.selectable_label(is_selected, file.filename());

                if response.clicked() {
                    to_select = Some(i);
                }

                if response.hovered() {
                    egui::show_tooltip(ui.ctx(), ui.layer_id(), egui::Id::new(format!("file_tooltip_{}", i)), |ui| {
                        ui.label(format!("Duration: {}", file.duration_string()));
                        ui.label(format!("Resolution: {}", file.resolution_string()));
                        if let Some(ref codec) = file.info.video_codec {
                            ui.label(format!("Video: {}", codec));
                        }
                        if let Some(ref codec) = file.info.audio_codec {
                            ui.label(format!("Audio: {}", codec));
                        }
                    });
                }
            }

            if let Some(idx) = to_select {
                app.select_file(idx);
            }
        });

    ui.separator();

    // File properties panel
    if let Some(file) = app.selected_file() {
        ui.heading("Properties");
        egui::Grid::new("file_properties").show(ui, |ui| {
            ui.label("Duration:");
            ui.label(file.duration_string());
            ui.end_row();

            ui.label("Resolution:");
            ui.label(file.resolution_string());
            ui.end_row();

            if let Some(ref codec) = file.info.video_codec {
                ui.label("Video Codec:");
                ui.label(codec);
                ui.end_row();
            }

            if let Some(ref codec) = file.info.audio_codec {
                ui.label("Audio Codec:");
                ui.label(codec);
                ui.end_row();
            }

            if let Some(fps) = file.info.framerate {
                ui.label("Frame Rate:");
                ui.label(format!("{:.2} fps", fps));
                ui.end_row();
            }

            ui.label("Size:");
            ui.label(crate::utils::format_size(file.info.file_size));
            ui.end_row();
        });

        ui.separator();

        if ui.button("Remove File").clicked() {
            app.remove_selected_file();
        }
    }
}

fn render_preview_area(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    ui.heading("Preview");

    let available_size = ui.available_size();
    let preview_height = (available_size.y * 0.5).min(400.0).max(200.0);

    egui::Frame::canvas(ui.style()).show(ui, |ui| {
        ui.set_min_height(preview_height);
        ui.set_max_height(preview_height);

        // Render video frame if available
        if let Some(ref texture) = app.preview_texture {
            let texture_size = texture.size_vec2();
            let aspect_ratio = texture_size.x / texture_size.y;

            // Calculate display size maintaining aspect ratio
            let available = ui.available_size();
            let display_size = if available.x / available.y > aspect_ratio {
                egui::vec2(available.y * aspect_ratio, available.y)
            } else {
                egui::vec2(available.x, available.x / aspect_ratio)
            };

            ui.centered_and_justified(|ui| {
                ui.image((texture.id(), display_size));
            });
        } else if let Some(file) = app.selected_file() {
            ui.centered_and_justified(|ui| {
                ui.label(format!(
                    "{}\n{} | {}\n\nPress Play to start",
                    file.filename(),
                    file.resolution_string(),
                    file.duration_string()
                ));
            });
        } else {
            ui.centered_and_justified(|ui| {
                ui.label("No file selected\nDrag & drop files or click 'Add Files'");
            });
        }
    });
}

fn render_playback_controls(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let state = app.get_playback_state();
        let duration = app.get_duration();

        // Playback buttons
        if ui.button("|<").on_hover_text("Go to start (Home)").clicked() {
            app.seek(0.0);
        }

        if ui.button("<<").on_hover_text("Rewind 10s (J)").clicked() {
            app.seek_relative(-10.0);
        }

        let play_pause_text = match state {
            PlaybackState::Playing => "||",
            _ => ">",
        };
        if ui.button(play_pause_text).on_hover_text("Play/Pause (Space)").clicked() {
            app.toggle_play_pause();
        }

        if ui.button(">>").on_hover_text("Forward 10s (L)").clicked() {
            app.seek_relative(10.0);
        }

        if ui.button(">|").on_hover_text("Go to end (End)").clicked() {
            app.seek(duration);
        }

        if ui.button("[]").on_hover_text("Stop").clicked() {
            app.stop_player();
        }

        ui.separator();

        // Time display
        ui.label(format!(
            "{} / {}",
            format_time(app.current_time),
            format_time(duration)
        ));

        ui.separator();

        // Volume control
        ui.label("Vol:");
        let mut volume = app.volume;
        if ui.add(egui::Slider::new(&mut volume, 0.0..=2.0).show_value(false)).changed() {
            app.set_volume(volume);
        }

        ui.separator();

        // In/Out point buttons
        if ui.button("[I").on_hover_text("Set In point (I)").clicked() {
            app.set_in_point();
        }
        if ui.button("O]").on_hover_text("Set Out point (O)").clicked() {
            app.set_out_point();
        }

        // Show current in/out points
        if let Some(in_pt) = app.in_point {
            ui.label(format!("IN: {}", format_time(in_pt)));
        }
        if let Some(out_pt) = app.out_point {
            ui.label(format!("OUT: {}", format_time(out_pt)));
        }
    });

    // Seek slider
    ui.horizontal(|ui| {
        let duration = app.get_duration();
        let mut current = app.current_time;

        ui.style_mut().spacing.slider_width = ui.available_width() - 20.0;

        if ui.add(
            egui::Slider::new(&mut current, 0.0..=duration.max(0.001))
                .show_value(false)
                .trailing_fill(true)
        ).changed() {
            app.seek(current);
        }
    });
}

fn render_timeline_panel(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    let duration = app.get_duration();

    let response = TimelineWidget::new(duration, app.current_time)
        .in_point(app.in_point)
        .out_point(app.out_point)
        .waveform(app.waveform.as_ref())
        .zoom(app.timeline_zoom)
        .scroll(app.timeline_scroll)
        .show(ui);

    if let Some(time) = response.seek_to {
        app.seek(time);
    }
    if let Some(zoom) = response.zoom_changed {
        app.timeline_zoom = zoom;
    }
    if let Some(scroll) = response.scroll_changed {
        app.timeline_scroll = scroll;
    }
}

fn render_tool_panel(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    match app.active_tool {
        ActiveTool::Convert => render_convert_tool(app, ui),
        ActiveTool::Trim => render_trim_tool(app, ui),
        ActiveTool::Crop => render_crop_tool(app, ui),
        ActiveTool::Concat => render_concat_tool(app, ui),
        ActiveTool::Filters => render_filters_tool(app, ui),
    }
}

fn render_convert_tool(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    ui.heading("Convert");
    ui.label("Convert video/audio to different formats");

    ui.separator();

    egui::Grid::new("convert_settings").show(ui, |ui| {
        ui.label("Output Format:");
        egui::ComboBox::from_id_salt("format_select")
            .selected_text(&app.export_settings.format)
            .show_ui(ui, |ui| {
                for format in SUPPORTED_VIDEO_FORMATS {
                    if ui
                        .selectable_label(app.export_settings.format == *format, *format)
                        .clicked()
                    {
                        app.export_settings.set_format(format);
                    }
                }
                ui.separator();
                for format in SUPPORTED_AUDIO_FORMATS {
                    if ui
                        .selectable_label(app.export_settings.format == *format, *format)
                        .clicked()
                    {
                        app.export_settings.set_format(format);
                    }
                }
            });
        ui.end_row();

        ui.label("Quality Preset:");
        egui::ComboBox::from_id_salt("preset_select")
            .selected_text(app.export_settings.preset.name())
            .show_ui(ui, |ui| {
                for preset in ExportPreset::all() {
                    if ui
                        .selectable_label(app.export_settings.preset == *preset, preset.name())
                        .clicked()
                    {
                        app.export_settings.apply_preset(*preset);
                    }
                }
            });
        ui.end_row();

        if app.export_settings.preset == ExportPreset::Custom {
            if let Some(ref mut crf) = app.export_settings.crf {
                ui.label("CRF (Quality):");
                ui.add(egui::Slider::new(crf, 0..=51).suffix(""));
                ui.end_row();
            }

            ui.label("Audio Bitrate:");
            let mut abitrate = app.export_settings.audio_bitrate.unwrap_or(192);
            if ui
                .add(egui::Slider::new(&mut abitrate, 64..=320).suffix(" kbps"))
                .changed()
            {
                app.export_settings.audio_bitrate = Some(abitrate);
            }
            ui.end_row();
        }
    });

    ui.separator();

    ui.horizontal(|ui| {
        if ui.button("Convert").clicked() {
            app.execute_current_tool();
        }
    });
}

fn render_trim_tool(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    ui.heading("Trim / Cut");
    ui.label("Extract a segment from the video");

    // Show tip about using in/out points
    ui.colored_label(egui::Color32::LIGHT_BLUE, "Tip: Use I/O keys or the timeline to set In/Out points");

    ui.separator();

    let duration = app
        .selected_file()
        .map(|f| f.info.duration)
        .unwrap_or(100.0);

    egui::Grid::new("trim_settings").show(ui, |ui| {
        ui.label("Start Time:");
        ui.horizontal(|ui| {
            let response = ui.text_edit_singleline(&mut app.trim_settings.start_time_str);
            if response.lost_focus() {
                if let Some(t) = crate::utils::parse_time(&app.trim_settings.start_time_str) {
                    app.trim_settings.start_time = t.min(duration);
                }
                app.trim_settings.start_time_str = format_time(app.trim_settings.start_time);
            }
            if ui.button("Use IN").clicked() {
                if let Some(in_pt) = app.in_point {
                    app.trim_settings.start_time = in_pt;
                    app.trim_settings.start_time_str = format_time(in_pt);
                }
            }
        });
        ui.end_row();

        ui.label("End Time:");
        ui.horizontal(|ui| {
            let response = ui.text_edit_singleline(&mut app.trim_settings.end_time_str);
            if response.lost_focus() {
                if let Some(t) = crate::utils::parse_time(&app.trim_settings.end_time_str) {
                    app.trim_settings.end_time = t.min(duration);
                }
                app.trim_settings.end_time_str = format_time(app.trim_settings.end_time);
            }
            if ui.button("Use OUT").clicked() {
                if let Some(out_pt) = app.out_point {
                    app.trim_settings.end_time = out_pt;
                    app.trim_settings.end_time_str = format_time(out_pt);
                }
            }
        });
        ui.end_row();

        ui.label("Duration:");
        let trim_duration = app.trim_settings.end_time - app.trim_settings.start_time;
        ui.label(format_time(trim_duration.max(0.0)));
        ui.end_row();
    });

    ui.separator();

    // Mode de trim avec radio buttons
    ui.label("Export Mode:");
    ui.indent("trim_mode_indent", |ui| {
        for mode in TrimMode::all() {
            let is_selected = app.trim_settings.mode == *mode;
            if ui.radio(is_selected, mode.name()).clicked() {
                app.trim_settings.mode = *mode;
            }
            if is_selected {
                ui.indent("mode_desc", |ui| {
                    ui.small(mode.description());
                });
            }
        }
    });

    ui.separator();

    ui.horizontal(|ui| {
        let button_text = match app.trim_settings.mode {
            TrimMode::Lossless => "Cut (instant)",
            TrimMode::Precise => "Cut (fast)",
            TrimMode::HighQuality => "Cut (quality)",
        };
        if ui.button(button_text).clicked() {
            app.execute_current_tool();
        }

        if ui.button("+ Add to Queue")
            .on_hover_text("Add this cut to the export queue")
            .clicked()
        {
            app.add_trim_to_queue();
        }

        // Show queue status
        let queue = app.export_queue.lock().unwrap();
        let pending = queue.pending_count();
        let completed = queue.completed_count();
        drop(queue);

        if pending > 0 || completed > 0 {
            ui.separator();
            let queue_text = format!("Queue: {} pending, {} done", pending, completed);
            if ui.button(&queue_text).clicked() {
                app.show_queue_panel = !app.show_queue_panel;
            }
        }
    });

    ui.separator();

    ui.horizontal(|ui| {
        // Quick access to external tools
        if ui.button("LosslessCut")
            .on_hover_text("Open in LosslessCut (requires installation)")
            .clicked()
        {
            app.open_in_losslesscut();
        }
        if ui.button("mpv")
            .on_hover_text("Preview in mpv (requires installation)")
            .clicked()
        {
            app.open_in_mpv();
        }
    });
}

fn render_crop_tool(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    ui.heading("Crop");
    ui.label("Crop video to a specific region");

    ui.separator();

    let (source_w, source_h) = app
        .selected_file()
        .map(|f| (f.info.width, f.info.height))
        .unwrap_or((1920, 1080));

    egui::Grid::new("crop_settings").show(ui, |ui| {
        ui.label("Aspect Ratio:");
        egui::ComboBox::from_id_salt("crop_preset")
            .selected_text(app.crop_settings.preset.name())
            .show_ui(ui, |ui| {
                for preset in CropPreset::all() {
                    if ui
                        .selectable_label(app.crop_settings.preset == *preset, preset.name())
                        .clicked()
                    {
                        app.crop_settings.apply_preset(*preset, source_w, source_h);
                    }
                }
            });
        ui.end_row();

        ui.label("X Offset:");
        ui.add(egui::DragValue::new(&mut app.crop_settings.x).range(0..=source_w));
        ui.end_row();

        ui.label("Y Offset:");
        ui.add(egui::DragValue::new(&mut app.crop_settings.y).range(0..=source_h));
        ui.end_row();

        ui.label("Width:");
        ui.add(egui::DragValue::new(&mut app.crop_settings.width).range(1..=source_w));
        ui.end_row();

        ui.label("Height:");
        ui.add(egui::DragValue::new(&mut app.crop_settings.height).range(1..=source_h));
        ui.end_row();
    });

    ui.separator();

    ui.horizontal(|ui| {
        if ui.button("Crop").clicked() {
            app.execute_current_tool();
        }
    });
}

fn render_concat_tool(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    ui.heading("Concatenate");
    ui.label("Join multiple files together in order");

    ui.separator();

    ui.label(format!(
        "{} files selected for concatenation",
        app.project.files.len()
    ));

    if app.project.files.len() < 2 {
        ui.colored_label(egui::Color32::YELLOW, "Add at least 2 files to concatenate");
    } else {
        ui.label("Files will be joined in the order shown in the file list.");

        let total_duration: f64 = app.project.files.iter().map(|f| f.info.duration).sum();
        ui.label(format!("Total duration: {}", format_time(total_duration)));
    }

    ui.separator();

    ui.horizontal(|ui| {
        if ui
            .add_enabled(app.project.files.len() >= 2, egui::Button::new("Concatenate"))
            .clicked()
        {
            app.execute_current_tool();
        }
    });
}

fn render_filters_tool(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    ui.heading("Filters");
    ui.label("Apply video and audio filters");

    ui.separator();

    let (source_w, source_h) = app
        .selected_file()
        .map(|f| (f.info.width, f.info.height))
        .unwrap_or((1920, 1080));

    ui.collapsing("Resize", |ui| {
        let mut enable_resize = app.filter_settings.resize.is_some();
        if ui.checkbox(&mut enable_resize, "Enable resize").changed() {
            app.filter_settings.resize = if enable_resize {
                Some((source_w, source_h))
            } else {
                None
            };
        }

        if let Some(ref mut size) = app.filter_settings.resize {
            egui::Grid::new("resize_grid").show(ui, |ui| {
                ui.label("Width:");
                ui.add(egui::DragValue::new(&mut size.0).range(1..=7680));
                ui.end_row();

                ui.label("Height:");
                ui.add(egui::DragValue::new(&mut size.1).range(1..=4320));
                ui.end_row();
            });
        }
    });

    ui.collapsing("Rotation", |ui| {
        let mut rotation = app.filter_settings.rotation.unwrap_or(0);
        ui.horizontal(|ui| {
            if ui.radio_value(&mut rotation, 0, "None").clicked() {
                app.filter_settings.rotation = None;
            }
            if ui.radio_value(&mut rotation, 90, "90").clicked() {
                app.filter_settings.rotation = Some(90);
            }
            if ui.radio_value(&mut rotation, 180, "180").clicked() {
                app.filter_settings.rotation = Some(180);
            }
            if ui.radio_value(&mut rotation, 270, "270").clicked() {
                app.filter_settings.rotation = Some(270);
            }
        });
    });

    ui.collapsing("Flip", |ui| {
        ui.checkbox(&mut app.filter_settings.flip_horizontal, "Horizontal flip");
        ui.checkbox(&mut app.filter_settings.flip_vertical, "Vertical flip");
    });

    ui.collapsing("Audio", |ui| {
        let mut volume = app.filter_settings.volume.unwrap_or(1.0);
        ui.horizontal(|ui| {
            ui.label("Volume:");
            if ui
                .add(egui::Slider::new(&mut volume, 0.0..=3.0).suffix("x"))
                .changed()
            {
                app.filter_settings.volume = Some(volume);
            }
        });

        ui.checkbox(&mut app.filter_settings.normalize_audio, "Normalize audio");
    });

    ui.separator();

    ui.horizontal(|ui| {
        if ui.button("Apply Filters").clicked() {
            app.execute_current_tool();
        }
    });
}

fn render_queue_panel(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Export Queue");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("X").clicked() {
                app.show_queue_panel = false;
            }
        });
    });

    ui.separator();

    // Queue stats
    let (pending, completed, is_processing) = {
        let queue = app.export_queue.lock().unwrap();
        (queue.pending_count(), queue.completed_count(), queue.is_processing)
    };

    ui.horizontal(|ui| {
        if is_processing {
            ui.spinner();
            ui.label("Processing...");
        } else if pending > 0 {
            ui.label(format!("{} jobs pending", pending));
        } else {
            ui.label("Queue empty");
        }
    });

    if completed > 0 {
        ui.horizontal(|ui| {
            ui.label(format!("{} completed", completed));
            if ui.small_button("Clear done").clicked() {
                app.clear_finished_jobs();
            }
        });
    }

    ui.separator();

    // Job list
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let jobs: Vec<_> = {
                let queue = app.export_queue.lock().unwrap();
                queue.jobs.iter().map(|j| (j.id, j.description(), j.status_text().to_string(), j.status.clone())).collect()
            };

            for (id, desc, status_text, status) in jobs {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        // Status indicator
                        let color = match &status {
                            crate::export_queue::JobStatus::Pending => egui::Color32::GRAY,
                            crate::export_queue::JobStatus::Running => egui::Color32::YELLOW,
                            crate::export_queue::JobStatus::Completed => egui::Color32::GREEN,
                            crate::export_queue::JobStatus::Failed(_) => egui::Color32::RED,
                        };
                        ui.colored_label(color, "●");
                        ui.label(&status_text);

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("x").clicked() {
                                let mut queue = app.export_queue.lock().unwrap();
                                queue.remove_job(id);
                            }
                        });
                    });
                    ui.small(&desc);
                });
            }
        });
}
