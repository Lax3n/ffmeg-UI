use super::commands::*;
use super::probe::{probe_file, MediaInfo};
use super::silence::{build_silence_detect_args, parse_silence_output, SilenceInterval};
use crate::ui::TrimMode;
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

    /// Trim a video between start and end times
    pub async fn trim(
        &self,
        input: &PathBuf,
        output: &PathBuf,
        start: f64,
        end: f64,
        mode: TrimMode,
    ) -> Result<()> {
        let args = build_trim_args(input, output, start, end, mode);
        self.execute_ffmpeg(&args).await
    }

    /// Execute an FFmpeg command with the given arguments
    async fn execute_ffmpeg(&self, args: &[String]) -> Result<()> {
        let mut cmd = Command::new(&self.ffmpeg_path);
        cmd.args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // On Windows GUI apps, prevent FFmpeg from creating a console window
        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn()
            .map_err(|e| anyhow!("Failed to start FFmpeg: {}. Is FFmpeg installed and in PATH?", e))?;

        let stderr = child.stderr.take().ok_or_else(|| anyhow!("Failed to capture stderr"))?;
        let mut reader = BufReader::new(stderr).lines();

        // Collect stderr output for error reporting
        let mut error_lines = Vec::new();
        while let Some(line) = reader.next_line().await? {
            if line.contains("Error") || line.contains("error") || line.contains("Invalid") {
                error_lines.push(line);
            }
        }

        let status = child.wait().await?;

        if status.success() {
            Ok(())
        } else {
            let error_detail = if error_lines.is_empty() {
                format!("FFmpeg exited with status: {}", status)
            } else {
                format!("FFmpeg error: {}", error_lines.join("; "))
            };
            Err(anyhow!(error_detail))
        }
    }

    /// Detect silence intervals in a media file using FFmpeg's silencedetect filter.
    pub async fn detect_silence(
        &self,
        input: &PathBuf,
        noise_db: f64,
        min_duration: f64,
    ) -> Result<Vec<SilenceInterval>> {
        let input_str = input.to_string_lossy().to_string();
        let args = build_silence_detect_args(&input_str, noise_db, min_duration);

        let mut cmd = Command::new(&self.ffmpeg_path);
        cmd.args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        {
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn().map_err(|e| {
            anyhow!(
                "Failed to start FFmpeg for silence detection: {}. Is FFmpeg installed and in PATH?",
                e
            )
        })?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("Failed to capture stderr for silence detection"))?;
        let mut reader = BufReader::new(stderr).lines();

        let mut all_lines = Vec::new();
        while let Some(line) = reader.next_line().await? {
            all_lines.push(line);
        }

        let _ = child.wait().await?;

        Ok(parse_silence_output(&all_lines))
    }

    /// Concatenate multiple video files into one using the concat demuxer.
    /// Creates a temp file list, runs FFmpeg, then cleans up.
    pub async fn concat(
        &self,
        inputs: &[PathBuf],
        output: &PathBuf,
    ) -> Result<()> {
        if inputs.is_empty() {
            return Err(anyhow!("No input files for concatenation"));
        }

        // Create concat list file next to output
        let list_path = output.with_file_name("_concat_list.txt");
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&list_path)
                .map_err(|e| anyhow!("Failed to create concat list: {}", e))?;
            for input in inputs {
                // Use forward slashes and escape single quotes for FFmpeg
                let path_str = input.to_string_lossy().replace('\\', "/");
                writeln!(f, "file '{}'", path_str.replace('\'', "'\\''"))
                    .map_err(|e| anyhow!("Failed to write concat list: {}", e))?;
            }
        }

        let args = super::commands::build_concat_args(&list_path, output);
        let result = self.execute_ffmpeg(&args).await;

        // Clean up temp file
        let _ = std::fs::remove_file(&list_path);

        result
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
