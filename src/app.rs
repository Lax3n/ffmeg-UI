use crate::export_queue::{JobStatus, SharedQueue, create_shared_queue};
use crate::ffmpeg::{FFmpegWrapper, SilenceInterval, TaskProgress, compute_cut_points};
use crate::player::{MediaPlayer, PlaybackState};
use crate::project::{MediaFile, Project};
use crate::ui::{SplitSegment, SplitSettings, TrimMode};
use eframe::egui;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

pub struct FFmpegApp {
    pub project: Project,
    pub ffmpeg: FFmpegWrapper,
    pub runtime: Runtime,
    pub selected_file_index: Option<usize>,
    pub trim_settings: crate::ui::TrimSettings,
    pub current_task: Arc<Mutex<Option<TaskProgress>>>,
    pub status_message: String,

    // Player state
    pub player: Option<MediaPlayer>,
    pub current_time: f64,
    pub volume: f32,
    pub preview_texture: Option<egui::TextureHandle>,
    last_frame_pts: f64,

    // Timeline state
    pub timeline_zoom: f32,
    pub timeline_scroll: f32,
    pub in_point: Option<f64>,
    pub out_point: Option<f64>,

    // Segments
    pub segments: Vec<SplitSegment>,
    pub split_settings: SplitSettings,
    pub selected_segment: Option<usize>,
    pub show_export_progress: bool,

    // Export queue
    pub export_queue: SharedQueue,

    // Auto-cut state
    pub auto_cut_running: bool,
    pub auto_cut_status: String,
    auto_cut_silences: Arc<Mutex<Option<Vec<SilenceInterval>>>>,

    // Per-file segments (persisted when switching files)
    pub file_segments: HashMap<PathBuf, Vec<SplitSegment>>,

    // Batch processing state
    pub batch_running: bool,
    pub batch_status: String,
    batch_total: usize,
    batch_results: Arc<Mutex<Vec<(usize, Vec<SilenceInterval>)>>>,
    /// When true, automatically export all files once batch detection finishes
    pub batch_auto_export: bool,

    // Merge state
    pub merge_file_order: Vec<usize>,
}

impl FFmpegApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            project: Project::new(),
            ffmpeg: FFmpegWrapper::new(),
            runtime: Runtime::new().expect("Failed to create Tokio runtime"),
            selected_file_index: None,
            trim_settings: crate::ui::TrimSettings::default(),
            current_task: Arc::new(Mutex::new(None)),
            status_message: String::from("Ready"),

            // Player state
            player: None,
            current_time: 0.0,
            volume: 1.0,
            preview_texture: None,
            last_frame_pts: -1.0,

            // Timeline state
            timeline_zoom: 1.0,
            timeline_scroll: 0.0,
            in_point: None,
            out_point: None,

            // Segments
            segments: Vec::new(),
            split_settings: SplitSettings::default(),
            selected_segment: None,
            show_export_progress: false,

            // Export queue
            export_queue: create_shared_queue(),

            // Auto-cut state
            auto_cut_running: false,
            auto_cut_status: String::new(),
            auto_cut_silences: Arc::new(Mutex::new(None)),

            // Per-file segments
            file_segments: HashMap::new(),

            // Batch processing
            batch_running: false,
            batch_status: String::new(),
            batch_total: 0,
            batch_results: Arc::new(Mutex::new(Vec::new())),
            batch_auto_export: false,

            // Merge
            merge_file_order: Vec::new(),
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
        self.sync_merge_order();
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
            self.save_current_segments();
            self.selected_file_index = Some(index);
            self.load_player_for_selected_file();
        }
    }

    pub fn remove_selected_file(&mut self) {
        if let Some(index) = self.selected_file_index {
            if index < self.project.files.len() {
                // Remove from per-file map
                let path = self.project.files[index].path.clone();
                self.file_segments.remove(&path);

                self.stop_player();
                self.player = None;

                self.project.files.remove(index);
                self.segments.clear();
                self.selected_segment = None;

                if self.project.files.is_empty() {
                    self.selected_file_index = None;
                } else if index >= self.project.files.len() {
                    self.selected_file_index = Some(self.project.files.len() - 1);
                }

                if self.selected_file_index.is_some() {
                    self.load_player_for_selected_file();
                }

                self.sync_merge_order();
            }
        }
    }

    /// Load media player for the currently selected file
    pub fn load_player_for_selected_file(&mut self) {
        let file_info = self.selected_file().map(|f| (f.path.clone(), f.filename()));

        if let Some((path, filename)) = file_info {
            match MediaPlayer::new(&path) {
                Ok(player) => {
                    self.player = Some(player);
                    self.current_time = 0.0;
                    self.last_frame_pts = -1.0;
                    self.in_point = None;
                    self.out_point = None;
                    // Restore segments from per-file map (or empty)
                    self.segments = self.file_segments.get(&path).cloned().unwrap_or_default();
                    self.selected_segment = if self.segments.is_empty() { None } else { Some(0) };
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

    // ---- Segment management ----

    /// Add a segment from current in/out points
    pub fn add_segment(&mut self) {
        let (in_pt, out_pt) = match (self.in_point, self.out_point) {
            (Some(i), Some(o)) if o > i => (i, o),
            _ => {
                self.status_message = "Set valid IN and OUT points first".to_string();
                return;
            }
        };

        let index = self.segments.len() + 1;
        let mut segment = SplitSegment::new(
            in_pt,
            out_pt,
            format!("Segment {}", index),
        );

        // Estimate size
        if let Some(file) = self.selected_file() {
            segment.estimated_size_bytes =
                crate::utils::estimate_segment_size(&file.info, in_pt, out_pt);
        }

        self.segments.push(segment);
        self.selected_segment = Some(self.segments.len() - 1);

        // Reset in/out points for next segment
        self.in_point = None;
        self.out_point = None;

        self.status_message = format!("{} segment(s) defined", self.segments.len());
    }

    /// Remove a segment by index
    pub fn remove_segment(&mut self, index: usize) {
        if index < self.segments.len() {
            self.segments.remove(index);
            // Re-label segments
            for (i, seg) in self.segments.iter_mut().enumerate() {
                seg.label = format!("Segment {}", i + 1);
            }
            // Adjust selection
            if self.segments.is_empty() {
                self.selected_segment = None;
            } else if let Some(sel) = self.selected_segment {
                if sel >= self.segments.len() {
                    self.selected_segment = Some(self.segments.len() - 1);
                }
            }
        }
    }

    /// Recalculate estimated sizes for all segments
    pub fn recalculate_sizes(&mut self) {
        if let Some(file) = self.selected_file() {
            let info = file.info.clone();
            for seg in &mut self.segments {
                seg.estimated_size_bytes =
                    crate::utils::estimate_segment_size(&info, seg.start_time, seg.end_time);
            }
        }
    }

    /// Auto-split a segment that exceeds max_bytes into smaller sub-segments
    fn auto_split_segment(segment: &SplitSegment, max_bytes: u64, bitrate_bps: f64) -> Vec<SplitSegment> {
        if max_bytes == 0 || segment.estimated_size_bytes <= max_bytes {
            return vec![segment.clone()];
        }

        // Calculate max duration per sub-segment
        let bytes_per_second = bitrate_bps / 8.0;
        if bytes_per_second <= 0.0 {
            return vec![segment.clone()];
        }
        let max_duration = max_bytes as f64 / bytes_per_second;
        let total_duration = segment.duration();
        let num_parts = (total_duration / max_duration).ceil() as usize;

        let mut result = Vec::new();
        for i in 0..num_parts {
            let start = segment.start_time + i as f64 * max_duration;
            let end = (segment.start_time + (i + 1) as f64 * max_duration).min(segment.end_time);
            let mut sub = SplitSegment::new(
                start,
                end,
                format!("{} ({}/{})", segment.label, i + 1, num_parts),
            );
            sub.enabled = segment.enabled;
            sub.estimated_size_bytes = (bytes_per_second * (end - start)) as u64;
            result.push(sub);
        }
        result
    }

    /// Start automatic silence-aware cutting.
    /// Spawns an async task to detect silence, then `poll_auto_cut` picks up the result.
    pub fn start_auto_cut(&mut self) {
        let file = match self.selected_file() {
            Some(f) => f,
            None => {
                self.status_message = "No file selected".to_string();
                return;
            }
        };

        if self.split_settings.max_size_mb <= 0.0 {
            self.status_message = "Set max size > 0 for Auto-Cut".to_string();
            return;
        }

        let input_path = file.path.clone();
        let ffmpeg = self.ffmpeg.clone();
        let slot = self.auto_cut_silences.clone();

        // Clear previous result
        *slot.lock().unwrap() = None;
        self.auto_cut_running = true;
        self.auto_cut_status = "Detecting silence...".to_string();
        self.status_message = "Auto-Cut: detecting silence...".to_string();

        self.runtime.spawn(async move {
            let result = ffmpeg.detect_silence(&input_path, -30.0, 0.3).await;
            let silences = result.unwrap_or_default();
            *slot.lock().unwrap() = Some(silences);
        });
    }

    /// Called every frame to check if silence detection finished and compute segments.
    pub fn poll_auto_cut(&mut self) {
        if !self.auto_cut_running {
            return;
        }

        // Check if the async task wrote its result
        let silences = {
            let mut slot = self.auto_cut_silences.lock().unwrap();
            slot.take()
        };

        let silences = match silences {
            Some(s) => s,
            None => return, // still running
        };

        // Detection is done
        self.auto_cut_running = false;

        // Clone what we need from the selected file before mutating self
        let (file_path, info) = match self.selected_file() {
            Some(f) => (f.path.clone(), f.info.clone()),
            None => {
                self.auto_cut_status = "File removed during detection".to_string();
                self.status_message = self.auto_cut_status.clone();
                return;
            }
        };

        let total_bitrate_bps = Self::compute_bitrate(&info);
        let max_bytes = (self.split_settings.max_size_mb * 1024.0 * 1024.0) as u64;

        let cut_points = compute_cut_points(
            info.duration,
            total_bitrate_bps,
            max_bytes,
            30.0,
            &silences,
        );

        // Replace segments
        self.segments.clear();
        for (i, (start, end)) in cut_points.iter().enumerate() {
            let mut seg = SplitSegment::new(
                *start,
                *end,
                format!("Segment {}", i + 1),
            );
            seg.estimated_size_bytes =
                crate::utils::estimate_segment_size(&info, *start, *end);
            self.segments.push(seg);
        }

        self.selected_segment = if self.segments.is_empty() {
            None
        } else {
            Some(0)
        };

        let silence_info = if silences.is_empty() {
            "no silence detected, uniform split"
        } else {
            "silence-aware split"
        };

        // Save to per-file map
        self.file_segments.insert(file_path, self.segments.clone());

        self.auto_cut_status = format!(
            "Auto-Cut: {} segment(s) ({})",
            self.segments.len(),
            silence_info
        );
        self.status_message = self.auto_cut_status.clone();
    }

    // ---- Per-file segment persistence ----

    /// Save current segments to the per-file map
    pub fn save_current_segments(&mut self) {
        if let Some(file) = self.selected_file() {
            let path = file.path.clone();
            self.file_segments.insert(path, self.segments.clone());
        }
    }

    /// Restore segments for the currently selected file from the map
    fn restore_segments_for_current_file(&mut self) {
        if let Some(file) = self.selected_file() {
            if let Some(segs) = self.file_segments.get(&file.path) {
                self.segments = segs.clone();
                self.selected_segment = if self.segments.is_empty() { None } else { Some(0) };
            }
        }
    }

    /// Total segment count across all files
    pub fn total_segments_all_files(&self) -> usize {
        self.file_segments.values().map(|s| s.len()).sum()
    }

    /// Count files that have segments
    pub fn files_with_segments_count(&self) -> usize {
        self.file_segments.values().filter(|s| !s.is_empty()).count()
    }

    // ---- Batch processing ----

    /// Launch silence detection on ALL loaded files in parallel
    pub fn start_batch_auto_cut(&mut self) {
        if self.project.files.is_empty() {
            self.status_message = "No files loaded".to_string();
            return;
        }
        if self.split_settings.max_size_mb <= 0.0 {
            self.status_message = "Set max size > 0 for batch Auto-Cut".to_string();
            return;
        }

        // Save current file's segments first
        self.save_current_segments();

        let files: Vec<(usize, PathBuf)> = self.project.files.iter().enumerate()
            .map(|(i, f)| (i, f.path.clone()))
            .collect();

        let results: Arc<Mutex<Vec<(usize, Vec<SilenceInterval>)>>> =
            Arc::new(Mutex::new(Vec::new()));

        self.batch_total = files.len();
        self.batch_running = true;
        self.batch_results = results.clone();
        self.batch_status = format!("Analyzing 0/{}...", files.len());
        self.status_message = self.batch_status.clone();

        let ffmpeg = self.ffmpeg.clone();

        // Spawn one async task per file — they run in parallel on the tokio runtime
        for (idx, path) in files {
            let ffmpeg = ffmpeg.clone();
            let results = results.clone();

            self.runtime.spawn(async move {
                let silences = ffmpeg.detect_silence(&path, -30.0, 0.3).await.unwrap_or_default();
                results.lock().unwrap().push((idx, silences));
            });
        }
    }

    /// Poll batch processing progress. Called each frame.
    pub fn poll_batch(&mut self) {
        if !self.batch_running {
            return;
        }

        let completed = self.batch_results.lock().unwrap().len();
        self.batch_status = format!("Analyzing {}/{}...", completed, self.batch_total);

        if completed < self.batch_total {
            return; // still running
        }

        // All done — compute segments for each file
        self.batch_running = false;

        let results: Vec<(usize, Vec<SilenceInterval>)> = {
            let mut guard = self.batch_results.lock().unwrap();
            std::mem::take(&mut *guard)
        };

        let max_bytes = (self.split_settings.max_size_mb * 1024.0 * 1024.0) as u64;
        let mut total_segments = 0usize;

        for (file_idx, silences) in results {
            let Some(file) = self.project.files.get(file_idx) else { continue };
            let info = &file.info;

            let bitrate_bps = Self::compute_bitrate(info);
            let cut_points = compute_cut_points(
                info.duration, bitrate_bps, max_bytes, 30.0, &silences,
            );

            let segments: Vec<SplitSegment> = cut_points.iter().enumerate()
                .map(|(i, (start, end))| {
                    let mut seg = SplitSegment::new(*start, *end, format!("Segment {}", i + 1));
                    seg.estimated_size_bytes = crate::utils::estimate_segment_size(info, *start, *end);
                    seg
                })
                .collect();

            total_segments += segments.len();
            self.file_segments.insert(file.path.clone(), segments);
        }

        // Restore current file's segments from the map
        self.restore_segments_for_current_file();

        self.batch_status = format!(
            "Batch done: {} files, {} total segments",
            self.batch_total, total_segments
        );
        self.status_message = self.batch_status.clone();

        // Auto-chain: if batch_auto_export is set, start export immediately
        if self.batch_auto_export {
            self.batch_auto_export = false;
            self.export_all_files();
        }
    }

    /// Combined: detect silences on all files, then auto-export when done
    pub fn batch_process_and_export(&mut self) {
        self.batch_auto_export = true;
        self.start_batch_auto_cut();
    }

    /// Export ALL files' segments into per-file subfolders
    pub fn export_all_files(&mut self) {
        // Save current file's segments first
        self.save_current_segments();

        let output_base = self.split_settings.output_folder.clone()
            .unwrap_or_else(|| {
                // Default: parent of first file, or current dir
                self.project.files.first()
                    .and_then(|f| f.path.parent())
                    .unwrap_or(std::path::Path::new("."))
                    .to_path_buf()
            });

        let max_size_bytes = if self.split_settings.max_size_mb > 0.0 {
            (self.split_settings.max_size_mb * 1024.0 * 1024.0) as u64
        } else {
            0
        };
        let mode = self.split_settings.trim_mode;
        let mut total_queued = 0usize;

        for file in &self.project.files {
            let Some(segments) = self.file_segments.get(&file.path) else { continue };
            let enabled: Vec<_> = segments.iter().filter(|s| s.enabled).cloned().collect();
            if enabled.is_empty() { continue; }

            let stem = file.path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let ext = file.path.extension().unwrap_or_default().to_string_lossy().to_string();
            let info = &file.info;
            let bitrate_bps = Self::compute_bitrate(info);

            // Per-file subfolder
            let subfolder = output_base.join(&stem);
            if let Err(e) = std::fs::create_dir_all(&subfolder) {
                self.status_message = format!("Cannot create folder {}: {}", subfolder.display(), e);
                return;
            }

            // Build final segments (with auto-split safety net)
            let mut final_segments = Vec::new();
            for seg in &enabled {
                if max_size_bytes > 0 {
                    final_segments.extend(Self::auto_split_segment(seg, max_size_bytes, bitrate_bps));
                } else {
                    final_segments.push(seg.clone());
                }
            }

            // Queue exports
            {
                let mut queue = self.export_queue.lock().unwrap();
                for (i, seg) in final_segments.iter().enumerate() {
                    let output_path = subfolder.join(format!("{}_{:03}.{}", stem, i + 1, ext));
                    queue.add_trim_with_label(
                        file.path.clone(),
                        output_path,
                        seg.start_time,
                        seg.end_time,
                        mode,
                        format!("{} - {}", stem, seg.label),
                    );
                }
                total_queued += final_segments.len();
            }
        }

        if total_queued == 0 {
            self.status_message = "No segments to export. Run Batch Auto-Cut first.".to_string();
            return;
        }

        self.show_export_progress = true;
        self.status_message = format!("Exporting {} segment(s) from {} file(s)...",
            total_queued, self.files_with_segments_count());
    }

    // ---- Merge / Concat ----

    /// Sync merge_file_order with the current project files.
    /// Keeps existing order for files still present, appends new ones.
    pub fn sync_merge_order(&mut self) {
        let file_count = self.project.files.len();
        // Remove indices that are out of bounds
        self.merge_file_order.retain(|&i| i < file_count);
        // Add any new files not yet in the order
        for i in 0..file_count {
            if !self.merge_file_order.contains(&i) {
                self.merge_file_order.push(i);
            }
        }
    }

    /// Move a file up in the merge order
    pub fn merge_move_up(&mut self, pos: usize) {
        if pos > 0 && pos < self.merge_file_order.len() {
            self.merge_file_order.swap(pos, pos - 1);
        }
    }

    /// Move a file down in the merge order
    pub fn merge_move_down(&mut self, pos: usize) {
        if pos + 1 < self.merge_file_order.len() {
            self.merge_file_order.swap(pos, pos + 1);
        }
    }

    /// Start merging all files in merge_file_order into one output file
    pub fn start_merge(&mut self) {
        self.sync_merge_order();

        if self.merge_file_order.len() < 2 {
            self.status_message = "Need at least 2 files to merge".to_string();
            return;
        }

        // Collect ordered input paths
        let inputs: Vec<PathBuf> = self.merge_file_order.iter()
            .filter_map(|&i| self.project.files.get(i).map(|f| f.path.clone()))
            .collect();

        if inputs.len() < 2 {
            self.status_message = "Need at least 2 valid files to merge".to_string();
            return;
        }

        // Determine output path
        let output_folder = self.split_settings.output_folder.clone()
            .unwrap_or_else(|| {
                inputs[0].parent()
                    .unwrap_or(std::path::Path::new("."))
                    .to_path_buf()
            });

        let ext = inputs[0].extension().unwrap_or_default().to_string_lossy().to_string();
        let output_path = output_folder.join(format!("merged_output.{}", ext));

        if let Err(e) = std::fs::create_dir_all(&output_folder) {
            self.status_message = format!("Cannot create output folder: {}", e);
            return;
        }

        // Add concat job to queue
        {
            let mut queue = self.export_queue.lock().unwrap();
            queue.add_concat(
                inputs,
                output_path,
                format!("Merge {} files", self.merge_file_order.len()),
            );
        }

        self.show_export_progress = true;
        self.status_message = format!("Merging {} files...", self.merge_file_order.len());
    }

    /// Compute total bitrate from MediaInfo
    fn compute_bitrate(info: &crate::ffmpeg::MediaInfo) -> f64 {
        match (info.video_bitrate, info.audio_bitrate) {
            (Some(vbr), Some(abr)) => (vbr + abr) as f64,
            (Some(vbr), None) => vbr as f64,
            (None, Some(abr)) => abr as f64,
            (None, None) => {
                if info.duration > 0.0 {
                    info.file_size as f64 / info.duration * 8.0
                } else {
                    0.0
                }
            }
        }
    }

    /// Export all enabled segments
    pub fn export_all(&mut self) {
        let Some(file) = self.selected_file() else {
            self.status_message = "No file selected".to_string();
            return;
        };

        let enabled_segments: Vec<_> = self.segments.iter().filter(|s| s.enabled).cloned().collect();
        if enabled_segments.is_empty() {
            self.status_message = "No segments to export".to_string();
            return;
        }

        let input_path = file.path.clone();
        let info = file.info.clone();

        // Determine output folder
        let output_folder = self.split_settings.output_folder.clone()
            .unwrap_or_else(|| input_path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf());

        let max_size_bytes = if self.split_settings.max_size_mb > 0.0 {
            (self.split_settings.max_size_mb * 1024.0 * 1024.0) as u64
        } else {
            0
        };

        // Calculate bitrate for auto-split
        let total_bitrate_bps = match (info.video_bitrate, info.audio_bitrate) {
            (Some(vbr), Some(abr)) => (vbr + abr) as f64,
            (Some(vbr), None) => vbr as f64,
            (None, Some(abr)) => abr as f64,
            (None, None) => {
                if info.duration > 0.0 {
                    info.file_size as f64 / info.duration * 8.0
                } else {
                    0.0
                }
            }
        };

        // Build final segment list (with auto-split if needed)
        let mut final_segments = Vec::new();
        for seg in &enabled_segments {
            if max_size_bytes > 0 {
                final_segments.extend(Self::auto_split_segment(seg, max_size_bytes, total_bitrate_bps));
            } else {
                final_segments.push(seg.clone());
            }
        }

        let stem = input_path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let ext = input_path.extension().unwrap_or_default().to_string_lossy().to_string();
        let mode = self.split_settings.trim_mode;

        // Ensure output folder exists
        if let Err(e) = std::fs::create_dir_all(&output_folder) {
            self.status_message = format!("Cannot create output folder: {}", e);
            return;
        }

        // Add all segments to queue
        {
            let mut queue = self.export_queue.lock().unwrap();
            for (i, seg) in final_segments.iter().enumerate() {
                let output_path = output_folder.join(format!("{}_{:03}.{}", stem, i + 1, ext));
                queue.add_trim_with_label(
                    input_path.clone(),
                    output_path,
                    seg.start_time,
                    seg.end_time,
                    mode,
                    seg.label.clone(),
                );
            }
        }

        self.show_export_progress = true;
        self.status_message = format!(
            "Exporting {} segment(s)...",
            final_segments.len()
        );
    }

    /// Process the next job in the queue
    pub fn process_queue(&mut self) {
        let queue = self.export_queue.clone();
        let ffmpeg = self.ffmpeg.clone();

        // Check if already processing
        {
            let q = queue.lock().unwrap();
            if q.is_processing || !q.has_pending() {
                return;
            }
        }

        // Get next job
        let job_info = {
            let mut q = queue.lock().unwrap();
            q.is_processing = true;
            if let Some(job) = q.next_pending() {
                job.status = JobStatus::Running;
                Some((job.id, job.input.clone(), job.output.clone(), job.operation.clone()))
            } else {
                q.is_processing = false;
                None
            }
        };

        if let Some((job_id, input, output, operation)) = job_info {
            self.status_message = "Processing queue...".to_string();

            self.runtime.spawn(async move {
                let result = match operation {
                    crate::export_queue::ExportOperation::Trim { start, end, mode } => {
                        ffmpeg.trim(&input, &output, start, end, mode).await
                    }
                    crate::export_queue::ExportOperation::Concat { inputs } => {
                        ffmpeg.concat(&inputs, &output).await
                    }
                };

                let mut q = queue.lock().unwrap();
                if let Some(job) = q.get_job_mut(job_id) {
                    match result {
                        Ok(_) => {
                            job.status = JobStatus::Completed;
                            job.progress = 1.0;
                        }
                        Err(e) => {
                            job.status = JobStatus::Failed(e.to_string());
                        }
                    }
                }
                q.is_processing = false;
            });
        }
    }

    /// Cancel all pending exports and stop processing
    pub fn cancel_exports(&mut self) {
        let mut queue = self.export_queue.lock().unwrap();
        queue.cancel_all();
        drop(queue);
        self.show_export_progress = false;
        self.status_message = "Exports cancelled".to_string();
    }

    /// Clear finished jobs from queue
    pub fn clear_finished_jobs(&mut self) {
        let mut queue = self.export_queue.lock().unwrap();
        queue.clear_finished();
    }

    /// Update player state and get current frame.
    /// Only recreates the GPU texture when the frame actually changed (PTS check).
    pub fn update_player(&mut self, ctx: &egui::Context) {
        if let Some(ref player) = self.player {
            self.current_time = player.get_current_time();

            // Detect end of video
            if player.get_state() == PlaybackState::Playing && self.current_time >= player.duration - 0.1 {
                player.stop();
            }

            if let Some(frame) = player.get_current_frame() {
                // Only upload to GPU if the frame is new
                if (frame.pts - self.last_frame_pts).abs() > 0.001 {
                    self.last_frame_pts = frame.pts;
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
            }

            if player.get_state() == PlaybackState::Playing {
                ctx.request_repaint_after(std::time::Duration::from_millis(30));
            }
        }
    }

    fn get_extension(&self, path: &PathBuf) -> String {
        path.extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }

    /// Open current file in LosslessCut (if installed)
    pub fn open_in_losslesscut(&self) {
        if let Some(file) = self.selected_file() {
            let path = file.path.clone();

            #[cfg(windows)]
            {
                let possible_paths = [
                    "LosslessCut.exe",
                    r"C:\Program Files\LosslessCut\LosslessCut.exe",
                    r"C:\Program Files (x86)\LosslessCut\LosslessCut.exe",
                ];

                for exe_path in possible_paths {
                    if std::process::Command::new(exe_path)
                        .arg(&path)
                        .spawn()
                        .is_ok()
                    {
                        return;
                    }
                }

                let _ = std::process::Command::new("cmd")
                    .args(["/C", "start", "", "LosslessCut"])
                    .arg(&path)
                    .spawn();
            }

            #[cfg(unix)]
            {
                let _ = std::process::Command::new("losslesscut")
                    .arg(&path)
                    .spawn();
            }
        }
    }

    /// Open current file in mpv (if installed)
    pub fn open_in_mpv(&self) {
        if let Some(file) = self.selected_file() {
            let path = file.path.clone();
            let start_time = self.current_time;

            let _ = std::process::Command::new("mpv")
                .arg(format!("--start={}", start_time))
                .arg(&path)
                .spawn();
        }
    }

    /// Open current file in system default player
    pub fn open_in_default_player(&self) {
        if let Some(file) = self.selected_file() {
            let _ = open::that(&file.path);
        }
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

            // S or Enter - Add segment
            if i.key_pressed(egui::Key::S) || i.key_pressed(egui::Key::Enter) {
                self.add_segment();
            }

            // Delete - Remove selected segment
            if i.key_pressed(egui::Key::Delete) {
                if let Some(idx) = self.selected_segment {
                    self.remove_segment(idx);
                }
            }

            // Ctrl+E - Export all
            if i.modifiers.ctrl && i.key_pressed(egui::Key::E) {
                self.export_all();
            }

            // Ctrl+O - Open file
            if i.modifiers.ctrl && i.key_pressed(egui::Key::O) {
                // Handled in UI (file dialog needs to be on main thread)
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

        // Process export queue
        self.process_queue();

        // Poll auto-cut silence detection
        self.poll_auto_cut();

        // Poll batch processing
        self.poll_batch();

        // Render UI
        crate::ui::render_main_window(self, ctx);

        // Update status from task progress (only if not showing export status)
        if !self.show_export_progress {
            if let Ok(progress) = self.current_task.lock() {
                if let Some(ref p) = *progress {
                    self.status_message = p.message.clone();
                }
            }
        }

        // Update export progress status
        {
            let queue = self.export_queue.lock().unwrap();
            let (completed, total) = queue.total_progress();
            if total > 0 && self.show_export_progress {
                let failed_count = queue.jobs.iter()
                    .filter(|j| matches!(j.status, JobStatus::Failed(_)))
                    .count();
                let success_count = completed - failed_count;

                if completed == total {
                    if failed_count > 0 {
                        // Show first error message
                        let first_error = queue.jobs.iter()
                            .find_map(|j| {
                                if let JobStatus::Failed(ref e) = j.status {
                                    Some(e.clone())
                                } else {
                                    None
                                }
                            })
                            .unwrap_or_default();
                        self.status_message = format!(
                            "Export: {} OK, {} failed - {}",
                            success_count, failed_count, first_error
                        );
                    } else {
                        self.status_message = format!("Export complete! ({}/{})", success_count, total);
                    }
                    self.show_export_progress = false;
                } else if self.show_export_progress {
                    self.status_message = format!("Exporting... {}/{}", completed, total);
                }
            }
        }

        // Request repaint for progress updates
        let needs_repaint = self.current_task.lock().map(|p| p.is_some()).unwrap_or(false)
            || self.export_queue.lock().map(|q| q.is_processing || q.has_pending()).unwrap_or(false)
            || self.auto_cut_running
            || self.batch_running;

        if needs_repaint {
            ctx.request_repaint();
        }
    }
}
