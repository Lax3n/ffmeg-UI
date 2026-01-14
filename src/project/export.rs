use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportSettings {
    pub format: String,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub video_bitrate: Option<u32>,
    pub audio_bitrate: Option<u32>,
    pub resolution: Option<(u32, u32)>,
    pub crf: Option<u32>,
    pub preset: ExportPreset,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            format: "mp4".to_string(),
            video_codec: Some("libx264".to_string()),
            audio_codec: Some("aac".to_string()),
            video_bitrate: None,
            audio_bitrate: Some(192),
            resolution: None,
            crf: Some(23),
            preset: ExportPreset::Medium,
        }
    }
}

impl ExportSettings {
    pub fn apply_preset(&mut self, preset: ExportPreset) {
        self.preset = preset;
        match preset {
            ExportPreset::High => {
                self.crf = Some(18);
                self.video_bitrate = None;
                self.audio_bitrate = Some(320);
            }
            ExportPreset::Medium => {
                self.crf = Some(23);
                self.video_bitrate = None;
                self.audio_bitrate = Some(192);
            }
            ExportPreset::Low => {
                self.crf = Some(28);
                self.video_bitrate = None;
                self.audio_bitrate = Some(128);
            }
            ExportPreset::Custom => {
                // Keep current settings
            }
        }
    }

    pub fn set_format(&mut self, format: &str) {
        self.format = format.to_string();
        let (vcodec, acodec) = crate::ffmpeg::get_default_codec_for_format(format);
        self.video_codec = vcodec;
        self.audio_codec = acodec;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportPreset {
    High,
    Medium,
    Low,
    Custom,
}

impl ExportPreset {
    pub fn all() -> &'static [ExportPreset] {
        &[
            ExportPreset::High,
            ExportPreset::Medium,
            ExportPreset::Low,
            ExportPreset::Custom,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            ExportPreset::High => "High Quality",
            ExportPreset::Medium => "Medium Quality",
            ExportPreset::Low => "Low Quality / Fast",
            ExportPreset::Custom => "Custom",
        }
    }
}

pub const SUPPORTED_VIDEO_FORMATS: &[&str] = &["mp4", "mkv", "webm", "avi", "mov"];
pub const SUPPORTED_AUDIO_FORMATS: &[&str] = &["mp3", "aac", "wav", "flac", "ogg"];

pub const VIDEO_CODECS: &[(&str, &str)] = &[
    ("libx264", "H.264 (x264)"),
    ("libx265", "H.265 (x265)"),
    ("libvpx-vp9", "VP9"),
    ("mpeg4", "MPEG-4"),
    ("copy", "Copy (no re-encode)"),
];

pub const AUDIO_CODECS: &[(&str, &str)] = &[
    ("aac", "AAC"),
    ("libmp3lame", "MP3"),
    ("libopus", "Opus"),
    ("flac", "FLAC"),
    ("pcm_s16le", "PCM (WAV)"),
    ("copy", "Copy (no re-encode)"),
];

pub const RESOLUTION_PRESETS: &[(&str, (u32, u32))] = &[
    ("4K (3840x2160)", (3840, 2160)),
    ("1080p (1920x1080)", (1920, 1080)),
    ("720p (1280x720)", (1280, 720)),
    ("480p (854x480)", (854, 480)),
    ("360p (640x360)", (640, 360)),
];
