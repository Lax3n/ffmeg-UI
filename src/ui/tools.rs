use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Mode de trim - détermine la vitesse vs précision du cut
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrimMode {
    /// Lossless: -c copy, coupe aux keyframes (~instantané)
    #[default]
    Lossless,
    /// Précis avec ré-encodage ultrafast (quelques secondes)
    Precise,
    /// Ré-encodage complet haute qualité (plus lent)
    HighQuality,
}

impl TrimMode {
    pub fn all() -> &'static [TrimMode] {
        &[TrimMode::Lossless, TrimMode::Precise, TrimMode::HighQuality]
    }

    pub fn name(&self) -> &'static str {
        match self {
            TrimMode::Lossless => "Lossless (instantané)",
            TrimMode::Precise => "Précis (rapide)",
            TrimMode::HighQuality => "Haute qualité (lent)",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            TrimMode::Lossless => "Coupe aux keyframes, pas de ré-encodage (~1 sec)",
            TrimMode::Precise => "Ré-encode en ultrafast, coupe précise (~10 sec)",
            TrimMode::HighQuality => "Ré-encode complet, qualité maximale (~1 min+)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrimSettings {
    pub start_time: f64,
    pub end_time: f64,
    pub mode: TrimMode,
    pub start_time_str: String,
    pub end_time_str: String,
}

impl Default for TrimSettings {
    fn default() -> Self {
        Self {
            start_time: 0.0,
            end_time: 10.0,
            mode: TrimMode::Lossless,
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

/// Un segment de découpe vidéo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitSegment {
    pub start_time: f64,
    pub end_time: f64,
    pub label: String,
    pub enabled: bool,
    pub estimated_size_bytes: u64,
}

impl SplitSegment {
    pub fn new(start_time: f64, end_time: f64, label: String) -> Self {
        Self {
            start_time,
            end_time,
            label,
            enabled: true,
            estimated_size_bytes: 0,
        }
    }

    pub fn duration(&self) -> f64 {
        (self.end_time - self.start_time).max(0.0)
    }
}

/// Paramètres globaux de découpe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitSettings {
    pub trim_mode: TrimMode,
    pub max_size_mb: f64,
    pub output_folder: Option<PathBuf>,
}

impl Default for SplitSettings {
    fn default() -> Self {
        Self {
            trim_mode: TrimMode::Lossless,
            max_size_mb: 1000.0, // défaut 1000 MB pour Auto-Cut
            output_folder: None,
        }
    }
}
