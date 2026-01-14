use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MediaInfo {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub video_bitrate: Option<u64>,
    pub audio_bitrate: Option<u64>,
    pub framerate: Option<f64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub format_name: String,
    pub file_size: u64,
}

#[derive(Debug, Deserialize)]
struct FFProbeOutput {
    format: Option<FFProbeFormat>,
    streams: Option<Vec<FFProbeStream>>,
}

#[derive(Debug, Deserialize)]
struct FFProbeFormat {
    duration: Option<String>,
    format_name: Option<String>,
    size: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FFProbeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    bit_rate: Option<String>,
    r_frame_rate: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
}

pub fn probe_file(path: &Path) -> Result<MediaInfo> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("ffprobe failed: {}", stderr));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let probe_output: FFProbeOutput = serde_json::from_str(&json_str)
        .map_err(|e| anyhow!("Failed to parse ffprobe output: {}", e))?;

    let mut info = MediaInfo::default();

    // Parse format info
    if let Some(format) = probe_output.format {
        info.duration = format.duration
            .and_then(|d| d.parse::<f64>().ok())
            .unwrap_or(0.0);
        info.format_name = format.format_name.unwrap_or_default();
        info.file_size = format.size
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
    }

    // Parse stream info
    if let Some(streams) = probe_output.streams {
        for stream in streams {
            let codec_type = stream.codec_type.as_deref().unwrap_or("");

            match codec_type {
                "video" => {
                    info.video_codec = stream.codec_name;
                    info.width = stream.width.unwrap_or(0);
                    info.height = stream.height.unwrap_or(0);
                    info.video_bitrate = stream.bit_rate
                        .and_then(|b| b.parse::<u64>().ok());
                    info.framerate = stream.r_frame_rate
                        .and_then(|r| parse_framerate(&r));
                }
                "audio" => {
                    info.audio_codec = stream.codec_name;
                    info.audio_bitrate = stream.bit_rate
                        .and_then(|b| b.parse::<u64>().ok());
                    info.sample_rate = stream.sample_rate
                        .and_then(|s| s.parse::<u32>().ok());
                    info.channels = stream.channels;
                }
                _ => {}
            }
        }
    }

    Ok(info)
}

fn parse_framerate(fps_str: &str) -> Option<f64> {
    let parts: Vec<&str> = fps_str.split('/').collect();
    if parts.len() == 2 {
        let num: f64 = parts[0].parse().ok()?;
        let den: f64 = parts[1].parse().ok()?;
        if den > 0.0 {
            return Some(num / den);
        }
    }
    fps_str.parse().ok()
}

/// Extract a thumbnail frame from a video at a specific timestamp
pub fn extract_frame(video_path: &Path, output_path: &Path, timestamp: f64) -> Result<()> {
    let output = Command::new("ffmpeg")
        .args([
            "-y",
            "-ss", &timestamp.to_string(),
            "-i",
        ])
        .arg(video_path)
        .args([
            "-vframes", "1",
            "-q:v", "2",
        ])
        .arg(output_path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to extract frame: {}", stderr));
    }

    Ok(())
}
