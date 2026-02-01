use crate::ui::TrimMode;
use std::path::{Path, PathBuf};

/// Build FFmpeg arguments for trimming with different modes
/// Maximise l'utilisation CPU avec -threads 0 et x264 threads=auto
pub fn build_trim_args(
    input: &PathBuf,
    output: &PathBuf,
    start: f64,
    end: f64,
    mode: TrimMode,
) -> Vec<String> {
    let duration = end - start;

    match mode {
        TrimMode::Lossless => {
            // -c copy: pas de ré-encodage, coupe aux keyframes (~instantané)
            // Pas besoin de threads ici, c'est juste du copy
            vec![
                "-y".to_string(),
                "-ss".to_string(),
                format!("{:.3}", start),
                "-i".to_string(),
                input.to_string_lossy().to_string(),
                "-t".to_string(),
                format!("{:.3}", duration),
                "-c".to_string(),
                "copy".to_string(),
                "-avoid_negative_ts".to_string(),
                "make_zero".to_string(),
                output.to_string_lossy().to_string(),
            ]
        }
        TrimMode::Precise => {
            // Ré-encodage ultrafast, tous les coeurs CPU
            vec![
                "-y".to_string(),
                "-threads".to_string(),
                "0".to_string(),
                "-i".to_string(),
                input.to_string_lossy().to_string(),
                "-ss".to_string(),
                format!("{:.3}", start),
                "-t".to_string(),
                format!("{:.3}", duration),
                "-c:v".to_string(),
                "libx264".to_string(),
                "-preset".to_string(),
                "ultrafast".to_string(),
                "-tune".to_string(),
                "fastdecode".to_string(),
                "-crf".to_string(),
                "18".to_string(),
                "-threads".to_string(),
                "0".to_string(),
                "-x264-params".to_string(),
                "threads=auto:lookahead_threads=auto".to_string(),
                "-c:a".to_string(),
                "aac".to_string(),
                "-b:a".to_string(),
                "192k".to_string(),
                output.to_string_lossy().to_string(),
            ]
        }
        TrimMode::HighQuality => {
            // Ré-encodage haute qualité, tous les coeurs CPU
            // medium au lieu de slow : meilleur ratio qualité/vitesse en multi-thread
            vec![
                "-y".to_string(),
                "-threads".to_string(),
                "0".to_string(),
                "-i".to_string(),
                input.to_string_lossy().to_string(),
                "-ss".to_string(),
                format!("{:.3}", start),
                "-t".to_string(),
                format!("{:.3}", duration),
                "-c:v".to_string(),
                "libx264".to_string(),
                "-preset".to_string(),
                "medium".to_string(),
                "-crf".to_string(),
                "18".to_string(),
                "-threads".to_string(),
                "0".to_string(),
                "-x264-params".to_string(),
                "threads=auto:lookahead_threads=auto:sliced-threads=1".to_string(),
                "-c:a".to_string(),
                "aac".to_string(),
                "-b:a".to_string(),
                "256k".to_string(),
                output.to_string_lossy().to_string(),
            ]
        }
    }
}

/// Build FFmpeg arguments for lossless concatenation.
/// Uses the concat demuxer with `-c copy` — no re-encoding, quality preserved 1:1.
/// Requires all inputs to share the same codec, resolution, and framerate.
pub fn build_concat_args(concat_list_path: &Path, output: &Path) -> Vec<String> {
    vec![
        "-y".to_string(),
        "-f".to_string(),
        "concat".to_string(),
        "-safe".to_string(),
        "0".to_string(),
        "-i".to_string(),
        concat_list_path.to_string_lossy().to_string(),
        "-c".to_string(),
        "copy".to_string(),
        output.to_string_lossy().to_string(),
    ]
}
