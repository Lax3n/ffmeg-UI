use crate::ui::SplitSegment;
use crate::utils::format_time;
use eframe::egui;

/// Palette de couleurs pour les segments
const SEGMENT_COLORS: [(u8, u8, u8); 8] = [
    (100, 200, 100),  // vert
    (100, 149, 237),  // bleu
    (237, 149, 100),  // orange
    (200, 100, 200),  // violet
    (237, 237, 100),  // jaune
    (100, 237, 237),  // cyan
    (237, 100, 149),  // rose
    (149, 237, 100),  // lime
];

/// Timeline widget with waveform visualization and multi-segment support
pub struct TimelineWidget<'a> {
    pub duration: f64,
    pub current_time: f64,
    pub in_point: Option<f64>,
    pub out_point: Option<f64>,
    pub zoom: f32,
    pub scroll: f32,
    pub segments: &'a [SplitSegment],
    pub selected_segment: Option<usize>,
    pub waveform_data: &'a [f32],
}

impl<'a> TimelineWidget<'a> {
    pub fn new(duration: f64, current_time: f64) -> Self {
        Self {
            duration,
            current_time,
            in_point: None,
            out_point: None,
            zoom: 1.0,
            scroll: 0.0,
            segments: &[],
            selected_segment: None,
            waveform_data: &[],
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

    pub fn zoom(mut self, zoom: f32) -> Self {
        self.zoom = zoom;
        self
    }

    pub fn scroll(mut self, scroll: f32) -> Self {
        self.scroll = scroll;
        self
    }

    pub fn segments(mut self, segments: &'a [SplitSegment]) -> Self {
        self.segments = segments;
        self
    }

    pub fn selected_segment(mut self, selected: Option<usize>) -> Self {
        self.selected_segment = selected;
        self
    }

    pub fn waveform_data(mut self, data: &'a [f32]) -> Self {
        self.waveform_data = data;
        self
    }

    /// Show the timeline widget and return seek position if clicked
    pub fn show(self, ui: &mut egui::Ui) -> TimelineResponse {
        let mut response = TimelineResponse {
            seek_to: None,
            zoom_changed: None,
            scroll_changed: None,
            segment_clicked: None,
            is_scrubbing: false,
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

            // Draw all segments
            self.draw_segments(&painter, track_rect, scroll_time, visible_duration);

            // Draw in/out working markers (dashed style)
            self.draw_working_markers(&painter, track_rect, scroll_time, visible_duration);

            // Draw playhead
            self.draw_playhead(&painter, rect, scroll_time, visible_duration);

            // Handle click — always seek, and also detect segment clicks
            if ui_response.clicked() {
                if let Some(pos) = ui_response.interact_pointer_pos() {
                    let relative_x = (pos.x - rect.left()) / rect.width();
                    let click_time = scroll_time + relative_x as f64 * visible_duration;

                    // Check if a segment was clicked
                    let pixels_per_second = track_rect.width() / visible_duration as f32;
                    if pos.y >= track_rect.top() && pos.y <= track_rect.bottom() {
                        for (i, seg) in self.segments.iter().enumerate() {
                            let seg_start_x = track_rect.left() + ((seg.start_time - scroll_time) as f32 * pixels_per_second);
                            let seg_end_x = track_rect.left() + ((seg.end_time - scroll_time) as f32 * pixels_per_second);
                            if pos.x >= seg_start_x && pos.x <= seg_end_x {
                                response.segment_clicked = Some(i);
                                break;
                            }
                        }
                    }

                    // Always seek on click
                    response.seek_to = Some(click_time.clamp(0.0, self.duration));
                }
            }

            // Handle scroll wheel for zoom
            let scroll_delta = ui.input(|i| i.raw_scroll_delta.y);
            if scroll_delta != 0.0 && rect.contains(ui.input(|i| i.pointer.hover_pos().unwrap_or_default())) {
                let new_zoom = (self.zoom + scroll_delta * 0.01).clamp(0.5, 10.0);
                response.zoom_changed = Some(new_zoom);
            }

            // Handle drag: normal drag = scrub (seek), Ctrl+drag = pan
            if ui_response.dragged() {
                if ui.input(|i| i.modifiers.ctrl) {
                    // Ctrl+drag = pan (old behavior)
                    let delta = ui_response.drag_delta().x;
                    let scroll_delta = -delta / rect.width() * (self.duration / self.zoom as f64) as f32;
                    let new_scroll = (self.scroll + scroll_delta / self.duration as f32).clamp(0.0, 1.0);
                    response.scroll_changed = Some(new_scroll);
                } else if let Some(pos) = ui_response.interact_pointer_pos() {
                    // Normal drag = scrub (continuous seek)
                    let relative_x = (pos.x - rect.left()) / rect.width();
                    let drag_time = scroll_time + relative_x as f64 * visible_duration;
                    response.seek_to = Some(drag_time.clamp(0.0, self.duration));
                    response.is_scrubbing = true;
                }
            }
        }

        response
    }

    fn draw_ruler(&self, painter: &egui::Painter, rect: egui::Rect, scroll_time: f64, visible_duration: f64) {
        let pixels_per_second = rect.width() / visible_duration as f32;

        let step = self.calculate_ruler_step(pixels_per_second);

        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(35));

        let start_time = (scroll_time / step).floor() * step;

        let mut time = start_time;
        while time <= scroll_time + visible_duration {
            let x = rect.left() + ((time - scroll_time) as f32 * pixels_per_second);

            if x >= rect.left() && x <= rect.right() {
                let tick_height = if (time / step) as i32 % 5 == 0 { 12.0 } else { 6.0 };
                painter.line_segment(
                    [egui::pos2(x, rect.bottom() - tick_height), egui::pos2(x, rect.bottom())],
                    egui::Stroke::new(1.0, egui::Color32::GRAY),
                );

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
        painter.rect_filled(rect, 0.0, egui::Color32::from_gray(25));

        if self.waveform_data.is_empty() || self.duration <= 0.0 {
            return;
        }

        let samples_per_second = self.waveform_data.len() as f64 / self.duration;
        let width_pixels = rect.width() as usize;
        let center_y = rect.center().y;
        let half_height = rect.height() / 2.0 - 2.0;

        let bar_color = egui::Color32::from_rgb(80, 180, 80);
        let bar_color_dim = egui::Color32::from_rgba_unmultiplied(80, 180, 80, 100);

        for px in 0..width_pixels {
            let t_start = scroll_time + (px as f64 / width_pixels as f64) * visible_duration;
            let t_end = scroll_time + ((px + 1) as f64 / width_pixels as f64) * visible_duration;

            let idx_start = (t_start * samples_per_second) as usize;
            let idx_end = ((t_end * samples_per_second) as usize + 1).min(self.waveform_data.len());

            if idx_start >= self.waveform_data.len() || idx_start >= idx_end {
                continue;
            }

            let peak = self.waveform_data[idx_start..idx_end]
                .iter()
                .copied()
                .fold(0.0f32, f32::max);

            if peak < 0.005 {
                continue;
            }

            let bar_height = peak * half_height;
            let x = rect.left() + px as f32;

            // Dim background bar for depth
            painter.line_segment(
                [egui::pos2(x, center_y - bar_height * 1.1), egui::pos2(x, center_y + bar_height * 1.1)],
                egui::Stroke::new(1.0, bar_color_dim),
            );
            // Main bar
            painter.line_segment(
                [egui::pos2(x, center_y - bar_height), egui::pos2(x, center_y + bar_height)],
                egui::Stroke::new(1.0, bar_color),
            );
        }

        // Center line
        painter.line_segment(
            [egui::pos2(rect.left(), center_y), egui::pos2(rect.right(), center_y)],
            egui::Stroke::new(0.5, egui::Color32::from_gray(60)),
        );
    }

    fn draw_segments(&self, painter: &egui::Painter, rect: egui::Rect, scroll_time: f64, visible_duration: f64) {
        let pixels_per_second = rect.width() / visible_duration as f32;

        for (i, seg) in self.segments.iter().enumerate() {
            if !seg.enabled {
                continue;
            }

            let start_x = rect.left() + ((seg.start_time - scroll_time) as f32 * pixels_per_second);
            let end_x = rect.left() + ((seg.end_time - scroll_time) as f32 * pixels_per_second);

            if start_x > rect.right() || end_x < rect.left() {
                continue;
            }

            let (r, g, b) = SEGMENT_COLORS[i % SEGMENT_COLORS.len()];
            let is_selected = self.selected_segment == Some(i);
            let alpha = if is_selected { 120 } else { 60 };

            let seg_rect = egui::Rect::from_min_max(
                egui::pos2(start_x.max(rect.left()), rect.top()),
                egui::pos2(end_x.min(rect.right()), rect.bottom()),
            );

            // Fill
            painter.rect_filled(seg_rect, 0.0, egui::Color32::from_rgba_unmultiplied(r, g, b, alpha));

            // Border for selected segment
            if is_selected {
                painter.rect_stroke(seg_rect, 0.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(r, g, b)));
            }

            // Label
            let label_width = seg_rect.width();
            if label_width > 30.0 {
                painter.text(
                    egui::pos2(seg_rect.left() + 4.0, seg_rect.top() + 2.0),
                    egui::Align2::LEFT_TOP,
                    &seg.label,
                    egui::FontId::proportional(10.0),
                    egui::Color32::WHITE,
                );
            }
        }
    }

    fn draw_working_markers(&self, painter: &egui::Painter, rect: egui::Rect, scroll_time: f64, visible_duration: f64) {
        let pixels_per_second = rect.width() / visible_duration as f32;

        // Draw in/out working selection (dashed)
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
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, 25),
                );
                // Dashed border
                painter.rect_stroke(
                    selection_rect,
                    0.0,
                    egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 100)),
                );
            }
        }

        // In point marker
        if let Some(in_pt) = self.in_point {
            let x = rect.left() + ((in_pt - scroll_time) as f32 * pixels_per_second);
            if x >= rect.left() && x <= rect.right() {
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x - 2.0, rect.top()), egui::vec2(4.0, rect.height())),
                    1.0,
                    egui::Color32::GREEN,
                );
                painter.text(
                    egui::pos2(x + 4.0, rect.bottom() - 12.0),
                    egui::Align2::LEFT_TOP,
                    "IN",
                    egui::FontId::proportional(9.0),
                    egui::Color32::GREEN,
                );
            }
        }

        // Out point marker
        if let Some(out_pt) = self.out_point {
            let x = rect.left() + ((out_pt - scroll_time) as f32 * pixels_per_second);
            if x >= rect.left() && x <= rect.right() {
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x - 2.0, rect.top()), egui::vec2(4.0, rect.height())),
                    1.0,
                    egui::Color32::RED,
                );
                painter.text(
                    egui::pos2(x + 4.0, rect.bottom() - 12.0),
                    egui::Align2::LEFT_TOP,
                    "OUT",
                    egui::FontId::proportional(9.0),
                    egui::Color32::RED,
                );
            }
        }
    }

    fn draw_playhead(&self, painter: &egui::Painter, rect: egui::Rect, scroll_time: f64, visible_duration: f64) {
        let pixels_per_second = rect.width() / visible_duration as f32;
        let x = rect.left() + ((self.current_time - scroll_time) as f32 * pixels_per_second);

        if x >= rect.left() && x <= rect.right() {
            let playhead_color = egui::Color32::from_rgb(255, 80, 80);

            // Halo / shadow behind the line (4px wide, semi-transparent)
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(4.0, egui::Color32::from_rgba_unmultiplied(255, 80, 80, 60)),
            );

            // Main playhead line (2px, red)
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(2.0, playhead_color),
            );

            // Triangle (8px wide, 14px tall) with white border
            let triangle = vec![
                egui::pos2(x, rect.top() + 14.0),
                egui::pos2(x - 8.0, rect.top()),
                egui::pos2(x + 8.0, rect.top()),
            ];
            painter.add(egui::Shape::convex_polygon(
                triangle.clone(),
                playhead_color,
                egui::Stroke::new(1.0, egui::Color32::WHITE),
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
    pub segment_clicked: Option<usize>,
    pub is_scrubbing: bool,
}
