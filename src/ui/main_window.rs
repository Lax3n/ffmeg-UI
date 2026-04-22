use crate::app::FFmpegApp;
use crate::player::PlaybackState;
use crate::ui::{EditingMode, TimelineWidget, TrimMode};
use crate::utils::{format_time, format_size};
use eframe::egui;

pub fn render_main_window(app: &mut FFmpegApp, ctx: &egui::Context) {
    // Top menu bar
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        render_menu_bar(app, ui);
    });

    // Status bar at bottom (rendered first = bottommost)
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        render_status_bar(app, ui);
    });

    // Export bar (above status bar)
    egui::TopBottomPanel::bottom("export_bar").show(ctx, |ui| {
        render_export_bar(app, ui);
    });

    // Timeline (above export bar)
    egui::TopBottomPanel::bottom("timeline_panel")
        .resizable(true)
        .min_height(80.0)
        .default_height(120.0)
        .show(ctx, |ui| {
            render_timeline_panel(app, ui);
        });

    // Mode tabs + content panel (above timeline — always visible!)
    egui::TopBottomPanel::bottom("mode_panel")
        .resizable(true)
        .min_height(120.0)
        .default_height(200.0)
        .show(ctx, |ui| {
            render_mode_tabs(app, ui);
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .id_salt("mode_content_scroll")
                .show(ui, |ui| {
                    match app.editing_mode {
                        EditingMode::Split => {
                            ui.columns(2, |columns| {
                                render_segment_list(app, &mut columns[0]);
                                render_split_settings(app, &mut columns[1]);
                            });
                        }
                        EditingMode::Merge => {
                            render_merge_editor(app, ui, ctx);
                        }
                    }
                });
        });

    // Left panel: file list
    egui::SidePanel::left("file_list_panel")
        .resizable(true)
        .default_width(200.0)
        .min_width(140.0)
        .max_width(350.0)
        .show(ctx, |ui| {
            render_file_list_panel(app, ui);
        });

    // Central panel: preview + controls only
    egui::CentralPanel::default().show(ctx, |ui| {
        render_preview_area(app, ui);
        ui.separator();
        render_playback_controls(app, ui);
    });
}

fn render_menu_bar(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    egui::menu::bar(ui, |ui| {
        ui.menu_button("File", |ui| {
            if ui.button("Open Video... (Ctrl+O)").clicked() {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("Video", &["mp4", "mkv", "avi", "mov", "webm", "ts", "flv"])
                    .add_filter("All Files", &["*"])
                    .pick_files()
                {
                    app.add_files(paths);
                }
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
            if ui.button("Frame Back (,)").clicked() {
                app.frame_step_backward();
                ui.close_menu();
            }
            if ui.button("Frame Forward (.)").clicked() {
                app.frame_step_forward();
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
            if ui.button("Add Segment (S)").clicked() {
                app.add_segment();
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
                .clicked()
            {
                app.open_in_losslesscut();
                ui.close_menu();
            }
            if ui.add_enabled(has_file, egui::Button::new("Open in mpv"))
                .clicked()
            {
                app.open_in_mpv();
                ui.close_menu();
            }
            if ui.add_enabled(has_file, egui::Button::new("Open in Default Player"))
                .clicked()
            {
                app.open_in_default_player();
                ui.close_menu();
            }
        });
    });
}

fn render_status_bar(app: &FFmpegApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(&app.status_message);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let seg_count = app.segments.len();
            let file_count = app.project.files.len();
            ui.label(format!("{} segment(s)", seg_count));
            ui.separator();
            ui.label(format!("{} video(s)", file_count));
        });
    });
}

fn render_preview_area(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    let available_size = ui.available_size();
    let preview_height = (available_size.y * 0.35).min(300.0).max(120.0);

    egui::Frame::canvas(ui.style()).show(ui, |ui| {
        ui.set_min_height(preview_height);
        ui.set_max_height(preview_height);

        if let Some(ref texture) = app.preview_texture {
            let texture_size = texture.size_vec2();
            let aspect_ratio = texture_size.x / texture_size.y;

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
                ui.label("No video loaded\nDrag & drop video files here");
            });
        }

        // Drag & drop overlay
        let is_dragging = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        if is_dragging {
            let rect = ui.max_rect();
            ui.painter().rect_filled(
                rect, 8.0,
                egui::Color32::from_rgba_unmultiplied(80, 140, 220, 60),
            );
            ui.painter().rect_stroke(
                rect, 8.0,
                egui::Stroke::new(3.0, egui::Color32::from_rgb(80, 140, 220)),
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Drop files here",
                egui::FontId::proportional(18.0),
                egui::Color32::WHITE,
            );
        }
    });
}

fn render_playback_controls(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    let state = app.get_playback_state();
    let duration = app.get_duration();

    // Row 1: Transport + time + speed + volume
    ui.horizontal(|ui| {
        if ui.button("|<").on_hover_text("Start (Home)").clicked() {
            app.seek(0.0);
        }
        if ui.button("<<").on_hover_text("-10s (J)").clicked() {
            app.seek_relative(-10.0);
        }
        if ui.button("<").on_hover_text("Frame back (,)").clicked() {
            if state == PlaybackState::Playing { app.pause(); }
            app.frame_step_backward();
        }

        let play_btn = match state {
            PlaybackState::Playing => egui::RichText::new("||").strong(),
            _ => egui::RichText::new("\u{25B6}").strong(),
        };
        if ui.button(play_btn).on_hover_text("Play/Pause (Space)").clicked() {
            app.toggle_play_pause();
        }

        if ui.button(">").on_hover_text("Frame forward (.)").clicked() {
            if state == PlaybackState::Playing { app.pause(); }
            app.frame_step_forward();
        }
        if ui.button(">>").on_hover_text("+10s (L)").clicked() {
            app.seek_relative(10.0);
        }
        if ui.button(">|").on_hover_text("End (End)").clicked() {
            app.seek(duration);
        }

        ui.separator();

        // Time display
        ui.monospace(format!(
            "{} / {}",
            format_time(app.current_time),
            format_time(duration)
        ));

        ui.separator();

        // Speed control
        let speed_text = if (app.playback_speed - 1.0).abs() < 0.01 {
            "1x".to_string()
        } else {
            format!("{:.2}x", app.playback_speed)
        };
        let speed_color = if (app.playback_speed - 1.0).abs() < 0.01 {
            egui::Color32::GRAY
        } else {
            egui::Color32::from_rgb(255, 180, 50)
        };
        if ui.button(egui::RichText::new(&speed_text).color(speed_color).small())
            .on_hover_text("Click to reset to 1x (Backspace)\nCtrl+Up/Down to change")
            .clicked()
        {
            app.set_speed(1.0);
        }
        if ui.small_button("-").on_hover_text("Slower (Ctrl+Down)").clicked() {
            app.speed_down();
        }
        if ui.small_button("+").on_hover_text("Faster (Ctrl+Up)").clicked() {
            app.speed_up();
        }

        // Volume
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut volume = app.volume;
            ui.style_mut().spacing.slider_width = 60.0;
            if ui.add(egui::Slider::new(&mut volume, 0.0..=2.0)
                .show_value(false)
            ).changed() {
                app.set_volume(volume);
            }
            ui.small("Vol");
        });
    });

    // Row 2: I/O + Add segment
    ui.horizontal(|ui| {
        if ui.button("[I] In").on_hover_text("Set In point (I)").clicked() {
            app.set_in_point();
        }
        if ui.button("[O] Out").on_hover_text("Set Out point (O)").clicked() {
            app.set_out_point();
        }

        if let Some(in_pt) = app.in_point {
            ui.small(format!("IN: {}", format_time(in_pt)));
        }
        if let Some(out_pt) = app.out_point {
            ui.small(format!("OUT: {}", format_time(out_pt)));
        }

        ui.separator();

        let can_add = app.in_point.is_some() && app.out_point.is_some();
        if ui.add_enabled(can_add, egui::Button::new("+ Segment (S)"))
            .on_hover_text("Create segment from IN/OUT points")
            .clicked()
        {
            app.add_segment();
        }

        if (app.in_point.is_some() || app.out_point.is_some())
            && ui.small_button("Clear").on_hover_text("Clear IN/OUT markers").clicked()
        {
            app.clear_in_out_points();
        }

        // Segment navigation
        if !app.segments.is_empty() {
            ui.separator();
            if ui.small_button("< Seg").on_hover_text("Previous segment (PgUp)").clicked() {
                app.select_prev_segment();
            }
            if ui.small_button("Seg >").on_hover_text("Next segment (PgDn)").clicked() {
                app.select_next_segment();
            }
        }
    });

    // Seek slider
    ui.horizontal(|ui| {
        let mut current = app.current_time;
        ui.style_mut().spacing.slider_width = ui.available_width() - 20.0;

        let slider_response = ui.add(
            egui::Slider::new(&mut current, 0.0..=duration.max(0.001))
                .show_value(false)
                .trailing_fill(true)
        );

        if slider_response.changed() {
            app.seek(current);
        }
        if slider_response.dragged() || slider_response.changed() {
            ui.ctx().request_repaint();
        }
    });
}

fn render_mode_tabs(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let split_selected = app.editing_mode == EditingMode::Split;
        let merge_selected = app.editing_mode == EditingMode::Merge;
        let file_count = app.project.files.len();

        // Split tab
        let split_text = egui::RichText::new("SPLIT / SEGMENTS")
            .strong()
            .size(13.0);
        let split_btn = egui::Button::new(if split_selected {
            split_text.color(egui::Color32::WHITE)
        } else {
            split_text.color(egui::Color32::GRAY)
        });
        if ui.add(split_btn).clicked() {
            app.editing_mode = EditingMode::Split;
        }

        ui.add_space(8.0);
        ui.label("|");
        ui.add_space(8.0);

        // Merge tab — highlighted when 2+ files
        let merge_label = if file_count >= 2 {
            format!("MERGE ({} files)", file_count)
        } else {
            "MERGE".to_string()
        };
        let merge_color = if merge_selected {
            egui::Color32::WHITE
        } else if file_count >= 2 {
            egui::Color32::from_rgb(100, 200, 100) // green hint when available
        } else {
            egui::Color32::DARK_GRAY
        };
        let merge_text = egui::RichText::new(&merge_label)
            .strong()
            .size(13.0)
            .color(merge_color);
        if ui.add(egui::Button::new(merge_text)).clicked() {
            app.editing_mode = EditingMode::Merge;
        }

        // Keyboard hint
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.small(egui::RichText::new("Tab to switch").color(egui::Color32::from_gray(80)));
        });
    });
}

fn render_timeline_panel(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    let duration = app.get_duration();

    let response = TimelineWidget::new(duration, app.current_time)
        .in_point(app.in_point)
        .out_point(app.out_point)
        .zoom(app.timeline_zoom)
        .scroll(app.timeline_scroll)
        .segments(&app.segments)
        .selected_segment(app.selected_segment)
        .waveform_data(&app.current_waveform)
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
    if let Some(idx) = response.segment_clicked {
        app.selected_segment = Some(idx);
    }
    if response.is_scrubbing {
        ui.ctx().request_repaint();
    }
    if response.zoom_to_fit {
        app.timeline_zoom = 1.0;
        app.timeline_scroll = 0.0;
    }
}

fn render_file_list_panel(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Files");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("+").on_hover_text("Open Video... (Ctrl+O)").clicked() {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("Video", &["mp4", "mkv", "avi", "mov", "webm", "ts", "flv"])
                    .add_filter("All Files", &["*"])
                    .pick_files()
                {
                    app.add_files(paths);
                }
            }
        });
    });

    ui.separator();

    if app.project.files.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label("No files loaded");
            ui.small("Drag & drop or File > Open");
        });
        return;
    }

    let mut select_idx: Option<usize> = None;
    let mut remove_idx: Option<usize> = None;

    // Limit file list height so merge section below is always visible
    let max_file_list_height = if app.project.files.len() >= 2 { 200.0 } else { 400.0 };

    egui::ScrollArea::vertical()
        .auto_shrink([false, true])
        .max_height(max_file_list_height)
        .id_salt("file_list_scroll")
        .show(ui, |ui| {
            for (i, file) in app.project.files.iter().enumerate() {
                let is_selected = app.selected_file_index == Some(i);

                ui.horizontal(|ui| {
                    let label_text = format!("{} ({})", file.filename(), file.duration_string());
                    let label = egui::RichText::new(label_text).small();
                    let response = ui.selectable_label(is_selected, label);
                    if response.clicked() {
                        select_idx = Some(i);
                    }
                    response.on_hover_text(format!(
                        "{}\n{} | {} | {}",
                        file.path.display(),
                        file.resolution_string(),
                        file.duration_string(),
                        format_size(file.info.file_size),
                    ));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("x").on_hover_text("Remove file").clicked() {
                            remove_idx = Some(i);
                        }
                    });
                });
            }
        });

    if let Some(idx) = select_idx {
        app.save_current_segments();
        app.select_file(idx);
    }
    if let Some(idx) = remove_idx {
        app.remove_file_at(idx);
    }

    ui.separator();

    ui.horizontal(|ui| {
        if !app.project.files.is_empty() {
            if ui.button("Remove All").clicked() {
                app.remove_all_files();
            }
        }
    });

    ui.small(format!("{} file(s)", app.project.files.len()));

    // ---- MERGE SECTION directly in file list panel (always visible) ----
    if app.project.files.len() >= 2 {
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);

        ui.label(egui::RichText::new("MERGE").strong().size(14.0));

        // Output folder
        ui.horizontal(|ui| {
            let folder_text = app.split_settings.output_folder
                .as_ref()
                .map(|p| {
                    let s = p.to_string_lossy().to_string();
                    if s.len() > 25 { format!("...{}", &s[s.len()-22..]) } else { s }
                })
                .unwrap_or_else(|| "(source folder)".to_string());
            ui.small(&folder_text);
        });
        if ui.button("Browse output...").on_hover_text("Choose where to save the merged file").clicked() {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                app.split_settings.output_folder = Some(folder);
            }
        }
        if app.split_settings.output_folder.is_some() {
            if ui.small_button("Reset folder").clicked() {
                app.split_settings.output_folder = None;
            }
        }

        ui.add_space(4.0);

        // Big merge button
        let is_busy = app.export_queue.lock().ok().map(|q| q.is_processing).unwrap_or(false);
        let can_merge = !is_busy;

        let merge_text = format!("MERGE {} files", app.project.files.len());
        let btn = egui::Button::new(
            egui::RichText::new(&merge_text)
                .strong()
                .size(13.0)
                .color(egui::Color32::WHITE)
        ).fill(egui::Color32::from_rgb(40, 120, 40));

        if ui.add_enabled(can_merge, btn)
            .on_hover_text("Merge all files into one video\nUse Tab to switch to merge editor for reordering")
            .clicked()
        {
            app.start_merge();
        }

        // Export progress
        if is_busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.small("Merging...");
            });
        }

        // Show export results
        let (completed, total) = app.export_queue.lock().ok()
            .map(|q| q.total_progress())
            .unwrap_or((0, 0));
        if total > 0 && completed == total && !is_busy && app.show_export_progress {
            ui.colored_label(egui::Color32::GREEN, "Done!");
            if ui.small_button("OK").clicked() {
                app.clear_finished_jobs();
                app.show_export_progress = false;
            }
        }
    }
}

fn render_segment_list(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    ui.heading("Segments");

    if app.segments.is_empty() {
        ui.label("No segments defined.");
        ui.label("Use I/O keys then press S to add segments.");
        return;
    }

    let mut to_remove: Option<usize> = None;
    let mut to_select: Option<usize> = None;
    let mut toggle_enable: Option<usize> = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(200.0)
        .show(ui, |ui| {
            for (i, seg) in app.segments.iter().enumerate() {
                let is_selected = app.selected_segment == Some(i);

                ui.horizontal(|ui| {
                    // Checkbox
                    let mut enabled = seg.enabled;
                    if ui.checkbox(&mut enabled, "").changed() {
                        toggle_enable = Some(i);
                    }

                    // Segment info (clickable)
                    let label = format!(
                        "#{} {} - {} (~{})",
                        i + 1,
                        format_time(seg.start_time),
                        format_time(seg.end_time),
                        format_size(seg.estimated_size_bytes),
                    );

                    let response = ui.selectable_label(is_selected, &label);
                    if response.clicked() {
                        to_select = Some(i);
                    }

                    // Delete button
                    if ui.small_button("x").on_hover_text("Remove segment (Del)").clicked() {
                        to_remove = Some(i);
                    }
                });
            }
        });

    if let Some(idx) = toggle_enable {
        if let Some(seg) = app.segments.get_mut(idx) {
            seg.enabled = !seg.enabled;
        }
    }
    if let Some(idx) = to_select {
        app.selected_segment = Some(idx);
    }
    if let Some(idx) = to_remove {
        app.remove_segment(idx);
    }

    ui.separator();

    ui.horizontal(|ui| {
        if ui.small_button("All").on_hover_text("Select all").clicked() {
            for seg in &mut app.segments {
                seg.enabled = true;
            }
        }
        if ui.small_button("None").on_hover_text("Deselect all").clicked() {
            for seg in &mut app.segments {
                seg.enabled = false;
            }
        }

        ui.separator();

        let can_split = app.selected_segment.map_or(false, |idx| {
            app.segments.get(idx).map_or(false, |seg| {
                app.current_time > seg.start_time && app.current_time < seg.end_time
            })
        });
        if ui.add_enabled(can_split, egui::Button::new("Split").small())
            .on_hover_text("Split at playhead")
            .clicked()
        {
            if let Some(idx) = app.selected_segment {
                app.split_segment_at(idx, app.current_time);
            }
        }

        let has_selected = app.selected_segment.is_some();
        if ui.add_enabled(has_selected, egui::Button::new("Dup").small())
            .on_hover_text("Duplicate segment (D)")
            .clicked()
        {
            app.duplicate_segment();
        }

        if ui.small_button("Clear All")
            .on_hover_text("Remove all segments (Ctrl+Shift+Del)")
            .clicked()
        {
            app.clear_all_segments();
        }
    });
}

fn render_split_settings(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    ui.heading("Settings");

    // Trim mode
    ui.label("Mode:");
    ui.indent("trim_mode_indent", |ui| {
        for mode in TrimMode::all() {
            let is_selected = app.split_settings.trim_mode == *mode;
            if ui.radio(is_selected, mode.name()).clicked() {
                app.split_settings.trim_mode = *mode;
            }
            if is_selected {
                ui.indent("mode_desc", |ui| {
                    ui.small(mode.description());
                });
            }
        }
    });

    ui.separator();

    // Max size
    ui.horizontal(|ui| {
        ui.label("Max size:");
        let mut max_mb = app.split_settings.max_size_mb;
        if ui.add(
            egui::DragValue::new(&mut max_mb)
                .range(0.0..=10000.0)
                .speed(1.0)
                .suffix(" MB")
        ).changed() {
            app.split_settings.max_size_mb = max_mb;
        }
    });
    if app.split_settings.max_size_mb > 0.0 {
        ui.small("Segments exceeding this size will be auto-split.");
    } else {
        ui.small("0 = no size limit");
    }

    ui.separator();

    // Auto-Cut button
    ui.horizontal(|ui| {
        let has_file = app.selected_file().is_some();
        let has_max_size = app.split_settings.max_size_mb > 0.0;
        let can_auto_cut = has_file && has_max_size && !app.auto_cut_running;

        if ui
            .add_enabled(can_auto_cut, egui::Button::new("Auto-Cut (silence-aware)"))
            .on_hover_text("Detect silences and split at quiet moments")
            .clicked()
        {
            app.start_auto_cut();
        }

        if app.auto_cut_running {
            ui.spinner();
            ui.label(&app.auto_cut_status);
        }
    });
    if !app.auto_cut_running && !app.auto_cut_status.is_empty() {
        ui.small(&app.auto_cut_status);
    }

    ui.separator();

    // Batch processing section
    let file_count = app.project.files.len();
    let has_multiple_files = file_count > 1;
    let has_max_size = app.split_settings.max_size_mb > 0.0;
    let is_busy = app.batch_running || app.auto_cut_running;

    if has_multiple_files {
        ui.horizontal(|ui| {
            // Combined: detect + export in one click
            let can_process = has_max_size && !is_busy;
            let process_label = format!("Process & Export All ({} files)", file_count);
            if ui
                .add_enabled(can_process, egui::Button::new(&process_label))
                .on_hover_text("Detect silences on all files in parallel, then export all segments into per-file folders")
                .clicked()
            {
                app.batch_process_and_export();
            }

            if app.batch_running {
                ui.spinner();
                ui.label(&app.batch_status);
            }
        });

        // Separate buttons for more control
        ui.horizontal(|ui| {
            let can_batch = has_max_size && !is_busy;
            if ui
                .add_enabled(can_batch, egui::Button::new("Batch Auto-Cut only"))
                .on_hover_text("Detect silences on all files (without exporting)")
                .clicked()
            {
                app.start_batch_auto_cut();
            }

            // Export All button (only if segments exist)
            let files_with_segs = app.files_with_segments_count();
            let total_segs = app.total_segments_all_files();
            if files_with_segs > 0 && !is_busy {
                let export_label = format!("Export All ({} segs, {} files)", total_segs, files_with_segs);
                if ui.button(&export_label)
                    .on_hover_text("Export all files' segments into per-file subfolders")
                    .clicked()
                {
                    app.export_all_files();
                }
            }
        });

        if !app.batch_running && !app.batch_status.is_empty() {
            ui.small(&app.batch_status);
        }

        // Show per-file segment summary
        {
            let files_with_segs = app.files_with_segments_count();
            let total_segs = app.total_segments_all_files();
            if total_segs > 0 {
                ui.small(format!("{} segments across {} file(s)", total_segs, files_with_segs));
            }
        }
    }

    ui.separator();

    // Output folder
    ui.horizontal(|ui| {
        ui.label("Output:");
        let folder_text = app.split_settings.output_folder
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "Same as source".to_string());
        ui.label(&folder_text);
    });

    ui.horizontal(|ui| {
        if ui.button("Browse...").clicked() {
            if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                app.split_settings.output_folder = Some(folder);
            }
        }
        if app.split_settings.output_folder.is_some() {
            if ui.button("Reset").clicked() {
                app.split_settings.output_folder = None;
            }
        }
    });
}

/// Dedicated merge editor — full-width panel with file cards, reordering, and merge controls
fn render_merge_editor(app: &mut FFmpegApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    if app.project.files.len() < 2 {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.heading("Merge / Assemble");
            ui.add_space(10.0);
            ui.label("Add at least 2 videos to use the merge feature.");
            ui.add_space(10.0);
            if ui.button("+ Add Videos").clicked() {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("Video", &["mp4", "mkv", "avi", "mov", "webm", "ts", "flv"])
                    .add_filter("All Files", &["*"])
                    .pick_files()
                {
                    app.add_files(paths);
                }
            }
        });
        return;
    }

    ui.heading("Merge / Assemble");
    ui.small("Arrange your videos in the desired order, then merge them into a single file.");

    ui.add_space(4.0);

    // Sync order with loaded files
    app.sync_merge_order();
    let order = app.merge_file_order.clone();

    let mut move_up: Option<usize> = None;
    let mut move_down: Option<usize> = None;
    let mut preview_file: Option<usize> = None;
    let mut remove_file: Option<usize> = None;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .max_height(250.0)
        .id_salt("merge_editor_scroll")
        .show(ui, |ui| {
            for (pos, &file_idx) in order.iter().enumerate() {
                let Some(file) = app.project.files.get(file_idx) else { continue };
                let is_current = app.selected_file_index == Some(file_idx);

                let frame = egui::Frame::none()
                    .inner_margin(egui::Margin::same(6.0))
                    .rounding(4.0)
                    .fill(if is_current {
                        egui::Color32::from_rgba_unmultiplied(80, 140, 220, 40)
                    } else {
                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 8)
                    })
                    .stroke(if is_current {
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 140, 220))
                    } else {
                        egui::Stroke::new(0.5, egui::Color32::from_gray(60))
                    });

                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Position number (large)
                        ui.label(
                            egui::RichText::new(format!("{}", pos + 1))
                                .size(20.0)
                                .strong()
                                .color(egui::Color32::from_gray(180))
                        );

                        ui.add_space(4.0);

                        // Thumbnail
                        let thumb_key = file.path.to_string_lossy().to_string();
                        if let Some((data, w, h)) = app.thumbnails.get(&file.path) {
                            // Create texture if not cached
                            if !app.thumbnail_textures.contains_key(&thumb_key) {
                                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                    [*w as usize, *h as usize],
                                    data,
                                );
                                let tex = ctx.load_texture(
                                    &thumb_key,
                                    color_image,
                                    egui::TextureOptions::LINEAR,
                                );
                                app.thumbnail_textures.insert(thumb_key.clone(), tex);
                            }
                            if let Some(tex) = app.thumbnail_textures.get(&thumb_key) {
                                let thumb_display = egui::vec2(80.0, 45.0);
                                ui.image((tex.id(), thumb_display));
                            }
                        } else {
                            // Placeholder
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(80.0, 45.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                rect,
                                2.0,
                                egui::Color32::from_gray(40),
                            );
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "...",
                                egui::FontId::proportional(10.0),
                                egui::Color32::GRAY,
                            );
                        }

                        ui.add_space(6.0);

                        // File info
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(file.filename())
                                    .strong()
                            );
                            ui.horizontal(|ui| {
                                ui.small(file.duration_string());
                                ui.small("|");
                                ui.small(file.resolution_string());
                                ui.small("|");
                                ui.small(format_size(file.info.file_size));
                            });
                            if let Some(ref codec) = file.info.video_codec {
                                ui.small(format!("Codec: {}", codec));
                            }
                        });

                        // Spacer
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Move buttons
                            let can_down = pos + 1 < order.len();
                            let can_up = pos > 0;

                            if ui.add_enabled(can_down, egui::Button::new("\u{2193}").small())
                                .on_hover_text("Move down")
                                .clicked()
                            {
                                move_down = Some(pos);
                            }
                            if ui.add_enabled(can_up, egui::Button::new("\u{2191}").small())
                                .on_hover_text("Move up")
                                .clicked()
                            {
                                move_up = Some(pos);
                            }

                            // Remove button
                            if ui.small_button("x")
                                .on_hover_text("Remove this clip")
                                .clicked()
                            {
                                remove_file = Some(file_idx);
                            }

                            // Preview button
                            if ui.small_button("Preview")
                                .on_hover_text("Load this video in the preview player")
                                .clicked()
                            {
                                preview_file = Some(file_idx);
                            }
                        });
                    });
                });

                ui.add_space(2.0);
            }
        });

    // Apply actions (remove first to avoid stale indices)
    if let Some(idx) = remove_file {
        app.remove_file_at(idx);
    } else {
        // Only apply reorder/preview if no removal happened
        if let Some(pos) = move_up {
            app.merge_move_up(pos);
        }
        if let Some(pos) = move_down {
            app.merge_move_down(pos);
        }
        if let Some(idx) = preview_file {
            app.save_current_segments();
            app.select_file(idx);
        }
    }

    ui.add_space(6.0);

    // Summary line
    let total_dur: f64 = app.merge_file_order.iter()
        .filter_map(|&i| app.project.files.get(i))
        .map(|f| f.info.duration)
        .sum();
    let total_size: u64 = app.merge_file_order.iter()
        .filter_map(|&i| app.project.files.get(i))
        .map(|f| f.info.file_size)
        .sum();

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "{} videos  |  Total: {}  |  Size: {}",
                app.merge_file_order.len(),
                format_time(total_dur),
                format_size(total_size),
            ))
            .strong()
        );
    });

    ui.add_space(4.0);

    // Output folder — prominent
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Output folder:").strong());

        let folder_text = app.split_settings.output_folder
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "(same as source video)".to_string());

        ui.label(&folder_text);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if app.split_settings.output_folder.is_some() {
                if ui.button("Reset").on_hover_text("Use source folder").clicked() {
                    app.split_settings.output_folder = None;
                }
            }
            if ui.button("Browse...").on_hover_text("Choose output folder for merged file").clicked() {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    app.split_settings.output_folder = Some(folder);
                }
            }
        });
    });

    ui.add_space(4.0);

    // Merge button (prominent)
    let is_busy = app.export_queue.lock().ok().map(|q| q.is_processing).unwrap_or(false);
    let can_merge = app.merge_file_order.len() >= 2 && !is_busy;

    ui.horizontal(|ui| {
        let merge_label = format!("MERGE {} videos into one file", app.merge_file_order.len());
        let button = egui::Button::new(
            egui::RichText::new(&merge_label)
                .strong()
                .size(14.0)
        );
        if ui.add_enabled(can_merge, button)
            .on_hover_text("Concatenate all files in the order shown above")
            .clicked()
        {
            app.start_merge();
        }

        if is_busy && app.show_export_progress {
            ui.spinner();
        }
    });
}

fn render_export_bar(app: &mut FFmpegApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        let enabled_count = app.segments.iter().filter(|s| s.enabled).count();
        let total_size: u64 = app.segments.iter()
            .filter(|s| s.enabled)
            .map(|s| s.estimated_size_bytes)
            .sum();

        let has_segments = enabled_count > 0;

        // Export button
        let button_text = format!(
            "EXPORT ({} segment(s), ~{})",
            enabled_count,
            format_size(total_size)
        );

        if ui.add_enabled(has_segments, egui::Button::new(&button_text)).clicked() {
            app.save_current_segments();
            app.export_all();
        }

        // Progress bar if processing
        let (completed, total, is_processing) = app.export_queue.lock()
            .map(|q| { let (c, t) = q.total_progress(); (c, t, q.is_processing) })
            .unwrap_or((0, 0, false));

        if total > 0 && (is_processing || app.show_export_progress) {
            ui.separator();
            if is_processing {
                ui.spinner();
            }
            let progress = if total > 0 { completed as f32 / total as f32 } else { 0.0 };
            ui.add(egui::ProgressBar::new(progress)
                .text(format!("{}/{}", completed, total))
                .desired_width(150.0));

            // Stop All button — cancel pending exports
            if completed < total && ui.button("Stop All").on_hover_text("Cancel all pending exports").clicked() {
                app.cancel_exports();
            }

            if completed == total && !is_processing {
                if ui.button("Clear").clicked() {
                    app.clear_finished_jobs();
                    app.show_export_progress = false;
                }
            }
        }

        // Quick open new file (for chained workflow)
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Open Next Video...").clicked() {
                if let Some(paths) = rfd::FileDialog::new()
                    .add_filter("Video", &["mp4", "mkv", "avi", "mov", "webm", "ts", "flv"])
                    .add_filter("All Files", &["*"])
                    .pick_files()
                {
                    app.add_files(paths);
                    // Select the last added file
                    let last = app.project.files.len().saturating_sub(1);
                    app.select_file(last);
                }
            }
        });
    });
}
