use super::commands::*;
use super::probe::{probe_file, MediaInfo};
use crate::project::ExportSettings;
use crate::ui::FilterSettings;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Clone)]
pub struct FFmpegWrapper {
    ffmpeg_path: String,
    ffprobe_path: String,
}

impl FFmpegWrapper {
    pub fn new() -> Self {
        Self {
            ffmpeg_path: "ffmpeg".to_string(),
            ffprobe_path: "ffprobe".to_string(),
        }
    }

    pub fn with_paths(ffmpeg_path: String, ffprobe_path: String) -> Self {
        Self {
            ffmpeg_path,
            ffprobe_path,
        }
    }

    /// Check if FFmpeg is available
    pub fn is_available(&self) -> bool {
        std::process::Command::new(&self.ffmpeg_path)
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Probe a media file for information
    pub fn probe(&self, path: &Path) -> Result<MediaInfo> {
        probe_file(path)
    }

    /// Convert a media file to a different format
    pub async fn convert(
        &self,
        input: &PathBuf,
        output: &PathBuf,
        settings: &ExportSettings,
    ) -> Result<()> {
        let args = build_convert_args(input, output, settings);
        self.execute_ffmpeg(&args).await
    }

    /// Trim a video between start and end times
    pub async fn trim(
        &self,
        input: &PathBuf,
        output: &PathBuf,
        start: f64,
        end: f64,
        copy_codec: bool,
    ) -> Result<()> {
        let args = build_trim_args(input, output, start, end, copy_codec);
        self.execute_ffmpeg(&args).await
    }

    /// Crop a video to specified dimensions
    pub async fn crop(
        &self,
        input: &PathBuf,
        output: &PathBuf,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let args = build_crop_args(input, output, x, y, width, height);
        self.execute_ffmpeg(&args).await
    }

    /// Concatenate multiple files
    pub async fn concat(&self, inputs: &[PathBuf], output: &PathBuf) -> Result<()> {
        // Create temporary list file
        let list_file = std::env::temp_dir().join("ffmpeg_concat_list.txt");
        let list_content: String = inputs
            .iter()
            .map(|p| format!("file '{}'", p.to_string_lossy().replace('\'', "'\\''")))
            .collect::<Vec<_>>()
            .join("\n");

        std::fs::write(&list_file, list_content)?;

        let args = build_concat_args(inputs, output, &list_file);
        let result = self.execute_ffmpeg(&args).await;

        // Clean up temp file
        let _ = std::fs::remove_file(&list_file);

        result
    }

    /// Apply filters to a video
    pub async fn apply_filters(
        &self,
        input: &PathBuf,
        output: &PathBuf,
        filters: &FilterSettings,
    ) -> Result<()> {
        let args = build_filter_args(input, output, filters);
        self.execute_ffmpeg(&args).await
    }

    /// Execute an FFmpeg command with the given arguments
    async fn execute_ffmpeg(&self, args: &[String]) -> Result<()> {
        let mut child = Command::new(&self.ffmpeg_path)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stderr = child.stderr.take().ok_or_else(|| anyhow!("Failed to capture stderr"))?;
        let mut reader = BufReader::new(stderr).lines();

        // Read progress output
        while let Some(line) = reader.next_line().await? {
            // Progress parsing could be done here
            // For now, we just log errors
            if line.contains("Error") || line.contains("error") {
                eprintln!("FFmpeg: {}", line);
            }
        }

        let status = child.wait().await?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow!("FFmpeg exited with status: {}", status))
        }
    }

    /// Extract a single frame as thumbnail
    pub async fn extract_thumbnail(
        &self,
        input: &PathBuf,
        output: &PathBuf,
        timestamp: f64,
    ) -> Result<()> {
        let args = vec![
            "-y".to_string(),
            "-ss".to_string(),
            timestamp.to_string(),
            "-i".to_string(),
            input.to_string_lossy().to_string(),
            "-vframes".to_string(),
            "1".to_string(),
            "-q:v".to_string(),
            "2".to_string(),
            output.to_string_lossy().to_string(),
        ];

        self.execute_ffmpeg(&args).await
    }
}

impl Default for FFmpegWrapper {
    fn default() -> Self {
        Self::new()
    }
}
