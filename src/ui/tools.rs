use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTool {
    Convert,
    Trim,
    Crop,
    Concat,
    Filters,
}

impl ActiveTool {
    pub fn all() -> &'static [ActiveTool] {
        &[
            ActiveTool::Convert,
            ActiveTool::Trim,
            ActiveTool::Crop,
            ActiveTool::Concat,
            ActiveTool::Filters,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            ActiveTool::Convert => "Convert",
            ActiveTool::Trim => "Trim",
            ActiveTool::Crop => "Crop",
            ActiveTool::Concat => "Concat",
            ActiveTool::Filters => "Filters",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ActiveTool::Convert => "Convert video/audio to different formats",
            ActiveTool::Trim => "Cut a segment from video",
            ActiveTool::Crop => "Crop video to a region",
            ActiveTool::Concat => "Join multiple files together",
            ActiveTool::Filters => "Apply video/audio filters",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrimSettings {
    pub start_time: f64,
    pub end_time: f64,
    pub copy_codec: bool,
    pub start_time_str: String,
    pub end_time_str: String,
}

impl Default for TrimSettings {
    fn default() -> Self {
        Self {
            start_time: 0.0,
            end_time: 10.0,
            copy_codec: true,
            start_time_str: "00:00.000".to_string(),
            end_time_str: "00:10.000".to_string(),
        }
    }
}

impl TrimSettings {
    pub fn update_from_file_duration(&mut self, duration: f64) {
        self.end_time = duration;
        self.end_time_str = crate::utils::format_time(duration);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CropSettings {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub preset: CropPreset,
}

impl Default for CropSettings {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            preset: CropPreset::Free,
        }
    }
}

impl CropSettings {
    pub fn apply_preset(&mut self, preset: CropPreset, source_width: u32, source_height: u32) {
        self.preset = preset;
        let (target_w, target_h) = match preset {
            CropPreset::Free => return,
            CropPreset::Ratio16x9 => {
                let h = source_height;
                let w = (h as f32 * 16.0 / 9.0) as u32;
                if w <= source_width {
                    (w, h)
                } else {
                    let w = source_width;
                    let h = (w as f32 * 9.0 / 16.0) as u32;
                    (w, h)
                }
            }
            CropPreset::Ratio4x3 => {
                let h = source_height;
                let w = (h as f32 * 4.0 / 3.0) as u32;
                if w <= source_width {
                    (w, h)
                } else {
                    let w = source_width;
                    let h = (w as f32 * 3.0 / 4.0) as u32;
                    (w, h)
                }
            }
            CropPreset::Ratio1x1 => {
                let size = source_width.min(source_height);
                (size, size)
            }
            CropPreset::Ratio9x16 => {
                let w = source_width;
                let h = (w as f32 * 16.0 / 9.0) as u32;
                if h <= source_height {
                    (w, h)
                } else {
                    let h = source_height;
                    let w = (h as f32 * 9.0 / 16.0) as u32;
                    (w, h)
                }
            }
        };

        self.width = target_w;
        self.height = target_h;
        self.x = (source_width - target_w) / 2;
        self.y = (source_height - target_h) / 2;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CropPreset {
    Free,
    Ratio16x9,
    Ratio4x3,
    Ratio1x1,
    Ratio9x16,
}

impl CropPreset {
    pub fn all() -> &'static [CropPreset] {
        &[
            CropPreset::Free,
            CropPreset::Ratio16x9,
            CropPreset::Ratio4x3,
            CropPreset::Ratio1x1,
            CropPreset::Ratio9x16,
        ]
    }

    pub fn name(&self) -> &'static str {
        match self {
            CropPreset::Free => "Free",
            CropPreset::Ratio16x9 => "16:9",
            CropPreset::Ratio4x3 => "4:3",
            CropPreset::Ratio1x1 => "1:1",
            CropPreset::Ratio9x16 => "9:16",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterSettings {
    pub resize: Option<(u32, u32)>,
    pub rotation: Option<u32>,
    pub flip_horizontal: bool,
    pub flip_vertical: bool,
    pub volume: Option<f32>,
    pub normalize_audio: bool,
}

impl Default for FilterSettings {
    fn default() -> Self {
        Self {
            resize: None,
            rotation: None,
            flip_horizontal: false,
            flip_vertical: false,
            volume: Some(1.0),
            normalize_audio: false,
        }
    }
}
