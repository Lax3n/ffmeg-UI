use crate::player::WaveformData;
use crate::utils::format_time;
use eframe::egui;

/// Timeline widget with waveform visualization
pub struct TimelineWidget<'a> {
    pub duration: f64,
    pub current_time: f64,
    pub in_point: Option<f64>,
    pub out_point: Option<f64>,
    pub waveform: Option<&'a WaveformData>,
    pub zoom: f32,
    pub scroll: f32,
}

impl<'a> TimelineWidget<'a> {
    pub fn new(duration: f64, current_time: f64) -> Self {
        Self {
            duration,
            current_time,
            in_point: None,
            out_point: None,
            waveform: None,
            zoom: 1.0,
            scroll: 0.0,
        }
    }

    pub fn in_point(mut self, point: Option<f64>) -> Self {
        self.in_point = point;
        self
    }

    pub fn out_point(mut self, point: Option<f64>) -> Self {
        self.out_point = point;
        self
    }

    pub fn waveform(mut self, waveform: Option<&'a WaveformData>) -> Self {
        self.waveform = waveform;
        self
    }

    pub fn zoom(mut self, zoom: f32) -> Self {
        self.zoom = zoom;
        self
    }

    pub fn scroll(mut self, scroll: f32) -> Self {
        self.scroll = scroll;
        self
    }

    /// Show the timeline widget and return seek position if clicked
    pub fn show(self, ui: &mut egui::Ui) -> TimelineResponse {
        let mut response = TimelineResponse {
            seek_to: None,
            zoom_changed: None,
            scroll_changed: None,
        };

        if self.duration <= 0.0 {
            ui.label("No media loaded");
            return response;
        }

        let available_width = ui.available_width();
        let timeline_height = 120.0;

        // Calculate visible time range based on zoom and scroll
        let visible_duration = self.duration / self.zoom as f64;
        let scroll_time = self.scroll as f64 * (self.duration - visible_duration).max(0.0);

        let (rect, ui_response) = ui.allocate_exact_size(
            egui::vec2(available_width, timeline_height),
            egui::Sense::click_and_drag(),
        );

        if ui.is_rect_visible(rect) {
            let painter = ui.painter_at(rect);

            // Background
            painter.rect_filled(rect, 4.0, egui::Color32::from_gray(30));

            // Draw sections
            let ruler_height = 24.0;
            let waveform_height = 50.0;
            let track_height = timeline_height - ruler_height - waveform_height - 10.0;

            let ruler_rect = egui::Rect::from_min_size(
                rect.min,
                egui::vec2(available_width, ruler_height),
            );
            let waveform_rect = egui::Rect::from_min_size(
                rect.min + egui::vec2(0.0, ruler_height),
                egui::vec2(available_width, waveform_height),
            );
            let track_rect = egui::Rect::from_min_size(
                rect.min + egui::vec2(0.0, ruler_height + waveform_height),
                egui::vec2(available_width, track_height),
            );

            // Draw ruler
            self.draw_ruler(&painter, ruler_rect, scroll_time, visible_duration);

            // Draw waveform
            self.draw_waveform(&painter, waveform_rect, scroll_time, visible_duration);

            // Draw track background
            painter.rect_filled(track_rect, 2.0, egui::Color32::from_gray(40));

            // Draw in/out selection
            self.draw_selection(&painter, track_rect, scroll_time, visible_duration);

            // Draw playhead
            self.draw_playhead(&painter, rect, scroll_time, visible_duration);

            // Handle click to seek
            if ui_response.clicked() {
                if let Some(pos) = ui_response.interact_pointer_pos() {
                    let relative_x = (pos.x - rect.left()) / rect.width();
                    let seek_time = scroll_time + relative_x as f64 * visible_duration;
                    response.seek_to = Some(seek_time.clamp(0.0, self.duration));
                }
            }

            // Handle scroll wheel for zoom
            let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
            if scroll_delta != 0.0 && rect.contains(ui.input(|i| i.pointer.hover_pos().unwrap_or_default())) {
                let new_zoom = (self.zoom + scroll_delta * 0.01).clamp(0.5, 10.0);
                response.zoom_changed = Some(new_zoom);
            }

            // Handle drag for scroll
            if ui_response.dragged() {
                let delta = ui_response.drag_delta().x;
                let scroll_delta = -delta / rect.width() * (self.duration / self.zoom as f64) as f32;
                let new_scroll = (self.scroll + scroll_delta / self.duration as f32).clamp(0.0, 1.0);
                response.scroll_changed = Some(new_scroll);
            }
        }

        response
    }

    fn draw_ruler(&self, painter: &egui::Painter, rect: egui::Rect, scroll_time: f64, visible_duration: f64) {
        let pixels_per_second = rect.width() / visible_duration as f32;

        // Determine step based on zoom level
        let step = self.calculate_ruler_step(pixels_per_second);

        // Draw ruler background
        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(35));

        // Calculate start time (aligned to step)
        let start_time = (scroll_time / step).floor() * step;

        let mut time = start_time;
        while time <= scroll_time + visible_duration {
            let x = rect.left() + ((time - scroll_time) as f32 * pixels_per_second);

            if x >= rect.left() && x <= rect.right() {
                // Draw tick
                let tick_height = if (time / step) as i32 % 5 == 0 { 12.0 } else { 6.0 };
                painter.line_segment(
                    [egui::pos2(x, rect.bottom() - tick_height), egui::pos2(x, rect.bottom())],
                    egui::Stroke::new(1.0, egui::Color32::GRAY),
                );

                // Draw time label for major ticks
                if (time / step) as i32 % 5 == 0 {
                    painter.text(
                        egui::pos2(x + 2.0, rect.top() + 2.0),
                        egui::Align2::LEFT_TOP,
                        format_time(time),
                        egui::FontId::proportional(10.0),
                        egui::Color32::LIGHT_GRAY,
                    );
                }
            }

            time += step;
        }
    }

    fn draw_waveform(&self, painter: &egui::Painter, rect: egui::Rect, scroll_time: f64, visible_duration: f64) {
        // Waveform background
        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(25));

        let Some(waveform) = self.waveform else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Loading waveform...",
                egui::FontId::proportional(12.0),
                egui::Color32::GRAY,
            );
            return;
        };

        if waveform.peaks.is_empty() {
            return;
        }

        let peaks_per_second = waveform.peaks.len() as f64 / waveform.duration;
        let start_peak = ((scroll_time * peaks_per_second) as usize).min(waveform.peaks.len());
        let end_peak = (((scroll_time + visible_duration) * peaks_per_second) as usize).min(waveform.peaks.len());

        if start_peak >= end_peak {
            return;
        }

        let peak_width = rect.width() / (end_peak - start_peak) as f32;
        let center_y = rect.center().y;
        let max_height = rect.height() / 2.0 - 2.0;

        for (i, peak_idx) in (start_peak..end_peak).enumerate() {
            let peak = waveform.peaks[peak_idx];
            let x = rect.left() + i as f32 * peak_width;
            let height = peak * max_height;

            painter.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(x + peak_width / 2.0, center_y),
                    egui::vec2(peak_width.max(1.0), height * 2.0),
                ),
                0.0,
                egui::Color32::from_rgb(100, 149, 237), // Cornflower blue
            );
        }
    }

    fn draw_selection(&self, painter: &egui::Painter, rect: egui::Rect, scroll_time: f64, visible_duration: f64) {
        let pixels_per_second = rect.width() / visible_duration as f32;

        // Draw in/out selection region
        if let (Some(in_pt), Some(out_pt)) = (self.in_point, self.out_point) {
            let in_x = rect.left() + ((in_pt - scroll_time) as f32 * pixels_per_second);
            let out_x = rect.left() + ((out_pt - scroll_time) as f32 * pixels_per_second);

            if in_x < rect.right() && out_x > rect.left() {
                let selection_rect = egui::Rect::from_min_max(
                    egui::pos2(in_x.max(rect.left()), rect.top()),
                    egui::pos2(out_x.min(rect.right()), rect.bottom()),
                );
                painter.rect_filled(
                    selection_rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(100, 200, 100, 60),
                );
            }
        }

        // Draw in point marker
        if let Some(in_pt) = self.in_point {
            let x = rect.left() + ((in_pt - scroll_time) as f32 * pixels_per_second);
            if x >= rect.left() && x <= rect.right() {
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x - 3.0, rect.top()), egui::vec2(6.0, rect.height())),
                    2.0,
                    egui::Color32::GREEN,
                );
                painter.text(
                    egui::pos2(x + 5.0, rect.top() + 2.0),
                    egui::Align2::LEFT_TOP,
                    "IN",
                    egui::FontId::proportional(10.0),
                    egui::Color32::GREEN,
                );
            }
        }

        // Draw out point marker
        if let Some(out_pt) = self.out_point {
            let x = rect.left() + ((out_pt - scroll_time) as f32 * pixels_per_second);
            if x >= rect.left() && x <= rect.right() {
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x - 3.0, rect.top()), egui::vec2(6.0, rect.height())),
                    2.0,
                    egui::Color32::RED,
                );
                painter.text(
                    egui::pos2(x + 5.0, rect.top() + 2.0),
                    egui::Align2::LEFT_TOP,
                    "OUT",
                    egui::FontId::proportional(10.0),
                    egui::Color32::RED,
                );
            }
        }
    }

    fn draw_playhead(&self, painter: &egui::Painter, rect: egui::Rect, scroll_time: f64, visible_duration: f64) {
        let pixels_per_second = rect.width() / visible_duration as f32;
        let x = rect.left() + ((self.current_time - scroll_time) as f32 * pixels_per_second);

        if x >= rect.left() && x <= rect.right() {
            // Playhead line
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(2.0, egui::Color32::WHITE),
            );

            // Playhead triangle at top
            let triangle = vec![
                egui::pos2(x, rect.top() + 10.0),
                egui::pos2(x - 6.0, rect.top()),
                egui::pos2(x + 6.0, rect.top()),
            ];
            painter.add(egui::Shape::convex_polygon(
                triangle,
                egui::Color32::WHITE,
                egui::Stroke::NONE,
            ));
        }
    }

    fn calculate_ruler_step(&self, pixels_per_second: f32) -> f64 {
        let min_pixel_gap = 50.0;
        let steps = [0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0];

        for step in steps {
            if step * pixels_per_second as f64 >= min_pixel_gap as f64 {
                return step;
            }
        }

        600.0
    }
}

pub struct TimelineResponse {
    pub seek_to: Option<f64>,
    pub zoom_changed: Option<f32>,
    pub scroll_changed: Option<f32>,
}
