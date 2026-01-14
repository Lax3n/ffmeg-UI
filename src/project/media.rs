use crate::ffmpeg::MediaInfo;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFile {
    pub path: PathBuf,
    pub info: MediaInfo,
}

impl MediaFile {
    pub fn filename(&self) -> String {
        self.path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string())
    }

    pub fn is_video(&self) -> bool {
        self.info.video_codec.is_some()
    }

    pub fn is_audio_only(&self) -> bool {
        self.info.audio_codec.is_some() && self.info.video_codec.is_none()
    }

    pub fn resolution_string(&self) -> String {
        if self.info.width > 0 && self.info.height > 0 {
            format!("{}x{}", self.info.width, self.info.height)
        } else {
            "N/A".to_string()
        }
    }

    pub fn duration_string(&self) -> String {
        crate::utils::format_time(self.info.duration)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    pub files: Vec<MediaFile>,
}

impl Project {
    pub fn new() -> Self {
        Self {
            name: "Untitled Project".to_string(),
            files: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.files.clear();
    }

    pub fn total_duration(&self) -> f64 {
        self.files.iter().map(|f| f.info.duration).sum()
    }
}
