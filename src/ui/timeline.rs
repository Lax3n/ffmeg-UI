// Timeline UI widget for visual editing
// Currently using simple sliders in main_window.rs
// This module is reserved for a more advanced visual timeline

use eframe::egui;

/// Draw a simple timeline ruler
pub fn draw_timeline_ruler(ui: &mut egui::Ui, duration: f64, zoom: f32) {
    let width = ui.available_width();
    let painter = ui.painter();
    let rect = ui.available_rect_before_wrap();

    let pixels_per_second = (width / duration as f32) * zoom;

    // Draw ruler marks
    let step = determine_step(duration, pixels_per_second);

    let mut time = 0.0;
    while time <= duration {
        let x = rect.left() + (time as f32 * pixels_per_second);

        if x <= rect.right() {
            // Draw tick
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.top() + 10.0)],
                egui::Stroke::new(1.0, egui::Color32::GRAY),
            );

            // Draw time label
            let label = crate::utils::format_time(time);
            painter.text(
                egui::pos2(x + 2.0, rect.top() + 12.0),
                egui::Align2::LEFT_TOP,
                label,
                egui::FontId::proportional(10.0),
                egui::Color32::GRAY,
            );
        }

        time += step;
    }

    ui.allocate_rect(
        egui::Rect::from_min_size(rect.min, egui::vec2(width, 24.0)),
        egui::Sense::hover(),
    );
}

/// Draw a timeline track with start/end markers
pub fn draw_timeline_track(
    ui: &mut egui::Ui,
    duration: f64,
    start: f64,
    end: f64,
    zoom: f32,
) -> (f64, f64) {
    let width = ui.available_width();
    let rect = ui.available_rect_before_wrap();
    let track_height = 40.0;

    let painter = ui.painter();
    let pixels_per_second = (width / duration as f32) * zoom;

    // Draw track background
    let track_rect = egui::Rect::from_min_size(rect.min, egui::vec2(width, track_height));
    painter.rect_filled(track_rect, 4.0, egui::Color32::from_gray(40));

    // Draw selected region
    let start_x = rect.left() + (start as f32 * pixels_per_second);
    let end_x = rect.left() + (end as f32 * pixels_per_second);

    let selection_rect = egui::Rect::from_min_max(
        egui::pos2(start_x, rect.top()),
        egui::pos2(end_x, rect.top() + track_height),
    );
    painter.rect_filled(selection_rect, 0.0, egui::Color32::from_rgba_unmultiplied(100, 149, 237, 100));

    // Draw markers
    painter.rect_filled(
        egui::Rect::from_min_size(egui::pos2(start_x - 4.0, rect.top()), egui::vec2(8.0, track_height)),
        2.0,
        egui::Color32::GREEN,
    );
    painter.rect_filled(
        egui::Rect::from_min_size(egui::pos2(end_x - 4.0, rect.top()), egui::vec2(8.0, track_height)),
        2.0,
        egui::Color32::RED,
    );

    ui.allocate_rect(track_rect, egui::Sense::hover());

    (start, end)
}

/// Determine appropriate step size for timeline ruler based on zoom level
fn determine_step(duration: f64, pixels_per_second: f32) -> f64 {
    let min_pixel_gap = 60.0; // Minimum pixels between labels

    let steps = [0.1, 0.5, 1.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0];

    for step in steps {
        if step * pixels_per_second as f64 >= min_pixel_gap as f64 {
            return step;
        }
    }

    duration / 10.0
}
