use crate::ffmpeg::{FFmpegWrapper, TaskProgress};
use crate::player::{MediaPlayer, PlaybackState, WaveformData};
use crate::project::{ExportSettings, MediaFile, Project};
use crate::ui::{ActiveTool, CropSettings, FilterSettings, TrimSettings};
use eframe::egui;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

pub struct FFmpegApp {
    pub project: Project,
    pub ffmpeg: FFmpegWrapper,
    pub runtime: Runtime,
    pub active_tool: ActiveTool,
    pub selected_file_index: Option<usize>,
    pub trim_settings: TrimSettings,
    pub crop_settings: CropSettings,
    pub filter_settings: FilterSettings,
    pub export_settings: ExportSettings,
    pub current_task: Arc<Mutex<Option<TaskProgress>>>,
    pub status_message: String,

    // Player state
    pub player: Option<MediaPlayer>,
    pub current_time: f64,
    pub volume: f32,
    pub preview_texture: Option<egui::TextureHandle>,
    pub waveform: Option<WaveformData>,

    // Timeline state
    pub timeline_zoom: f32,
    pub timeline_scroll: f32,
    pub in_point: Option<f64>,
    pub out_point: Option<f64>,
}

impl FFmpegApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            project: Project::new(),
            ffmpeg: FFmpegWrapper::new(),
            runtime: Runtime::new().expect("Failed to create Tokio runtime"),
            active_tool: ActiveTool::Convert,
            selected_file_index: None,
            trim_settings: TrimSettings::default(),
            crop_settings: CropSettings::default(),
            filter_settings: FilterSettings::default(),
            export_settings: ExportSettings::default(),
            current_task: Arc::new(Mutex::new(None)),
            status_message: String::from("Ready"),

            // Player state
            player: None,
            current_time: 0.0,
            volume: 1.0,
            preview_texture: None,
            waveform: None,

            // Timeline state
            timeline_zoom: 1.0,
            timeline_scroll: 0.0,
            in_point: None,
            out_point: None,
        }
    }

    pub fn add_files(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            if let Some(media_file) = self.probe_file(&path) {
                self.project.files.push(media_file);
            }
        }
        if self.selected_file_index.is_none() && !self.project.files.is_empty() {
            self.selected_file_index = Some(0);
            self.load_player_for_selected_file();
        }
    }

    fn probe_file(&self, path: &PathBuf) -> Option<MediaFile> {
        match self.ffmpeg.probe(path) {
            Ok(info) => Some(MediaFile {
                path: path.clone(),
                info,
            }),
            Err(e) => {
                eprintln!("Failed to probe file {:?}: {}", path, e);
                None
            }
        }
    }

    pub fn selected_file(&self) -> Option<&MediaFile> {
        self.selected_file_index
            .and_then(|i| self.project.files.get(i))
    }

    pub fn select_file(&mut self, index: usize) {
        if index < self.project.files.len() {
            self.selected_file_index = Some(index);
            self.load_player_for_selected_file();
        }
    }

    pub fn remove_selected_file(&mut self) {
        if let Some(index) = self.selected_file_index {
            if index < self.project.files.len() {
                // Stop player before removing
                self.stop_player();
                self.player = None;

                self.project.files.remove(index);
                if self.project.files.is_empty() {
                    self.selected_file_index = None;
                } else if index >= self.project.files.len() {
                    self.selected_file_index = Some(self.project.files.len() - 1);
                }

                // Load new player for newly selected file
                if self.selected_file_index.is_some() {
                    self.load_player_for_selected_file();
                }
            }
        }
    }

    /// Load media player for the currently selected file
    pub fn load_player_for_selected_file(&mut self) {
        let file_info = self.selected_file().map(|f| (f.path.clone(), f.filename()));

        if let Some((path, filename)) = file_info {
            match MediaPlayer::new(&path) {
                Ok(player) => {
                    self.waveform = player.get_waveform();
                    self.player = Some(player);
                    self.current_time = 0.0;
                    self.in_point = None;
                    self.out_point = None;
                    self.status_message = format!("Loaded: {}", filename);
                }
                Err(e) => {
                    self.status_message = format!("Failed to load player: {}", e);
                    self.player = None;
                }
            }
        }
    }

    // Player controls
    pub fn play(&mut self) {
        if let Some(ref player) = self.player {
            player.play();
        }
    }

    pub fn pause(&mut self) {
        if let Some(ref player) = self.player {
            player.pause();
        }
    }

    pub fn toggle_play_pause(&mut self) {
        if let Some(ref player) = self.player {
            player.toggle_play_pause();
        }
    }

    pub fn stop_player(&mut self) {
        if let Some(ref player) = self.player {
            player.stop();
        }
        self.current_time = 0.0;
    }

    pub fn seek(&mut self, time: f64) {
        if let Some(ref player) = self.player {
            let duration = player.duration;
            let clamped_time = time.clamp(0.0, duration);
            player.seek(clamped_time);
            self.current_time = clamped_time;
        }
    }

    pub fn seek_relative(&mut self, delta: f64) {
        let new_time = self.current_time + delta;
        self.seek(new_time);
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol.clamp(0.0, 2.0);
        if let Some(ref player) = self.player {
            player.set_volume(self.volume);
        }
    }

    pub fn get_playback_state(&self) -> PlaybackState {
        self.player
            .as_ref()
            .map(|p| p.get_state())
            .unwrap_or(PlaybackState::Stopped)
    }

    pub fn get_duration(&self) -> f64 {
        self.player.as_ref().map(|p| p.duration).unwrap_or(0.0)
    }

    // In/Out points for trimming
    pub fn set_in_point(&mut self) {
        self.in_point = Some(self.current_time);
        self.trim_settings.start_time = self.current_time;
        self.trim_settings.start_time_str = crate::utils::format_time(self.current_time);
    }

    pub fn set_out_point(&mut self) {
        self.out_point = Some(self.current_time);
        self.trim_settings.end_time = self.current_time;
        self.trim_settings.end_time_str = crate::utils::format_time(self.current_time);
    }

    pub fn clear_in_out_points(&mut self) {
        self.in_point = None;
        self.out_point = None;
    }

    /// Update player state and get current frame
    pub fn update_player(&mut self, ctx: &egui::Context) {
        if let Some(ref player) = self.player {
            // Update current time
            self.current_time = player.get_current_time();

            // Get current frame and update texture
            if let Some(frame) = player.get_current_frame() {
                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [frame.width as usize, frame.height as usize],
                    &frame.data,
                );

                self.preview_texture = Some(ctx.load_texture(
                    "video_frame",
                    color_image,
                    egui::TextureOptions::LINEAR,
                ));
            }

            // Update waveform if available
            if self.waveform.is_none() {
                self.waveform = player.get_waveform();
            }

            // Request repaint if playing
            if player.get_state() == PlaybackState::Playing {
                ctx.request_repaint();
            }
        }
    }

    pub fn execute_current_tool(&mut self) {
        let Some(file) = self.selected_file() else {
            self.status_message = "No file selected".to_string();
            return;
        };

        let input_path = file.path.clone();
        let task_progress = self.current_task.clone();

        match self.active_tool {
            ActiveTool::Convert => {
                self.execute_convert(input_path, task_progress);
            }
            ActiveTool::Trim => {
                self.execute_trim(input_path, task_progress);
            }
            ActiveTool::Crop => {
                self.execute_crop(input_path, task_progress);
            }
            ActiveTool::Concat => {
                self.execute_concat(task_progress);
            }
            ActiveTool::Filters => {
                self.execute_filters(input_path, task_progress);
            }
        }
    }

    fn execute_convert(&mut self, input_path: PathBuf, task_progress: Arc<Mutex<Option<TaskProgress>>>) {
        let output_path = self.get_output_path(&input_path, &self.export_settings.format);
        let settings = self.export_settings.clone();
        let ffmpeg = self.ffmpeg.clone();

        self.status_message = format!("Converting to {}...", settings.format);

        self.runtime.spawn(async move {
            *task_progress.lock().unwrap() = Some(TaskProgress::new("Converting"));

            let result = ffmpeg.convert(&input_path, &output_path, &settings).await;

            let mut progress = task_progress.lock().unwrap();
            if let Some(ref mut p) = *progress {
                match result {
                    Ok(_) => p.complete("Conversion complete"),
                    Err(e) => p.fail(&format!("Conversion failed: {}", e)),
                }
            }
        });
    }

    fn execute_trim(&mut self, input_path: PathBuf, task_progress: Arc<Mutex<Option<TaskProgress>>>) {
        let output_path = self.get_output_path(&input_path, &self.get_extension(&input_path));
        let trim = self.trim_settings.clone();
        let ffmpeg = self.ffmpeg.clone();

        self.status_message = format!("Trimming from {} to {}...", trim.start_time, trim.end_time);

        self.runtime.spawn(async move {
            *task_progress.lock().unwrap() = Some(TaskProgress::new("Trimming"));

            let result = ffmpeg.trim(&input_path, &output_path, trim.start_time, trim.end_time, trim.copy_codec).await;

            let mut progress = task_progress.lock().unwrap();
            if let Some(ref mut p) = *progress {
                match result {
                    Ok(_) => p.complete("Trim complete"),
                    Err(e) => p.fail(&format!("Trim failed: {}", e)),
                }
            }
        });
    }

    fn execute_crop(&mut self, input_path: PathBuf, task_progress: Arc<Mutex<Option<TaskProgress>>>) {
        let output_path = self.get_output_path(&input_path, &self.get_extension(&input_path));
        let crop = self.crop_settings.clone();
        let ffmpeg = self.ffmpeg.clone();

        self.status_message = "Cropping video...".to_string();

        self.runtime.spawn(async move {
            *task_progress.lock().unwrap() = Some(TaskProgress::new("Cropping"));

            let result = ffmpeg.crop(&input_path, &output_path, crop.x, crop.y, crop.width, crop.height).await;

            let mut progress = task_progress.lock().unwrap();
            if let Some(ref mut p) = *progress {
                match result {
                    Ok(_) => p.complete("Crop complete"),
                    Err(e) => p.fail(&format!("Crop failed: {}", e)),
                }
            }
        });
    }

    fn execute_concat(&mut self, task_progress: Arc<Mutex<Option<TaskProgress>>>) {
        if self.project.files.len() < 2 {
            self.status_message = "Need at least 2 files to concatenate".to_string();
            return;
        }

        let files: Vec<PathBuf> = self.project.files.iter().map(|f| f.path.clone()).collect();
        let output_path = self.get_output_path(&files[0], &self.get_extension(&files[0]));
        let ffmpeg = self.ffmpeg.clone();

        self.status_message = "Concatenating files...".to_string();

        self.runtime.spawn(async move {
            *task_progress.lock().unwrap() = Some(TaskProgress::new("Concatenating"));

            let result = ffmpeg.concat(&files, &output_path).await;

            let mut progress = task_progress.lock().unwrap();
            if let Some(ref mut p) = *progress {
                match result {
                    Ok(_) => p.complete("Concatenation complete"),
                    Err(e) => p.fail(&format!("Concatenation failed: {}", e)),
                }
            }
        });
    }

    fn execute_filters(&mut self, input_path: PathBuf, task_progress: Arc<Mutex<Option<TaskProgress>>>) {
        let output_path = self.get_output_path(&input_path, &self.get_extension(&input_path));
        let filters = self.filter_settings.clone();
        let ffmpeg = self.ffmpeg.clone();

        self.status_message = "Applying filters...".to_string();

        self.runtime.spawn(async move {
            *task_progress.lock().unwrap() = Some(TaskProgress::new("Applying filters"));

            let result = ffmpeg.apply_filters(&input_path, &output_path, &filters).await;

            let mut progress = task_progress.lock().unwrap();
            if let Some(ref mut p) = *progress {
                match result {
                    Ok(_) => p.complete("Filters applied"),
                    Err(e) => p.fail(&format!("Filter application failed: {}", e)),
                }
            }
        });
    }

    fn get_output_path(&self, input: &PathBuf, extension: &str) -> PathBuf {
        let stem = input.file_stem().unwrap_or_default().to_string_lossy();
        let parent = input.parent().unwrap_or(std::path::Path::new("."));
        parent.join(format!("{}_output.{}", stem, extension))
    }

    fn get_extension(&self, path: &PathBuf) -> String {
        path.extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }

    /// Handle keyboard shortcuts
    pub fn handle_input(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            // Space - Play/Pause
            if i.key_pressed(egui::Key::Space) {
                self.toggle_play_pause();
            }

            // Arrow keys - Seek
            if i.key_pressed(egui::Key::ArrowLeft) {
                self.seek_relative(-5.0);
            }
            if i.key_pressed(egui::Key::ArrowRight) {
                self.seek_relative(5.0);
            }

            // J/K/L - Playback control
            if i.key_pressed(egui::Key::J) {
                self.seek_relative(-10.0);
            }
            if i.key_pressed(egui::Key::K) {
                self.pause();
            }
            if i.key_pressed(egui::Key::L) {
                self.seek_relative(10.0);
            }

            // Home/End - Go to start/end
            if i.key_pressed(egui::Key::Home) {
                self.seek(0.0);
            }
            if i.key_pressed(egui::Key::End) {
                self.seek(self.get_duration());
            }

            // I/O - Set In/Out points
            if i.key_pressed(egui::Key::I) {
                self.set_in_point();
            }
            if i.key_pressed(egui::Key::O) {
                self.set_out_point();
            }
        });
    }
}

impl eframe::App for FFmpegApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle keyboard input
        self.handle_input(ctx);

        // Update player
        self.update_player(ctx);

        // Render UI
        crate::ui::render_main_window(self, ctx);

        // Update status from task progress
        if let Ok(progress) = self.current_task.lock() {
            if let Some(ref p) = *progress {
                self.status_message = p.message.clone();
            }
        }

        // Request repaint for progress updates
        if self.current_task.lock().map(|p| p.is_some()).unwrap_or(false) {
            ctx.request_repaint();
        }
    }
}
