// File browser functionality is integrated into main_window.rs
// This module is reserved for future extended file browser features

use std::path::PathBuf;

/// Supported video file extensions
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm", "wmv", "flv", "m4v"];

/// Supported audio file extensions
pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "wav", "aac", "flac", "ogg", "m4a", "wma"];

/// Check if a path is a supported media file
pub fn is_supported_media(path: &PathBuf) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        VIDEO_EXTENSIONS.contains(&ext.as_str()) || AUDIO_EXTENSIONS.contains(&ext.as_str())
    } else {
        false
    }
}

/// Check if a path is a video file
pub fn is_video_file(path: &PathBuf) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        VIDEO_EXTENSIONS.contains(&ext.as_str())
    } else {
        false
    }
}

/// Check if a path is an audio file
pub fn is_audio_file(path: &PathBuf) -> bool {
    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy().to_lowercase();
        AUDIO_EXTENSIONS.contains(&ext.as_str())
    } else {
        false
    }
}
