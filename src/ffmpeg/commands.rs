use crate::project::ExportSettings;
use crate::ui::{FilterSettings, TrimMode};
use std::path::PathBuf;

/// Build FFmpeg arguments for conversion
pub fn build_convert_args(
    input: &PathBuf,
    output: &PathBuf,
    settings: &ExportSettings,
) -> Vec<String> {
    let mut args = vec![
        "-y".to_string(),
        "-i".to_string(),
        input.to_string_lossy().to_string(),
    ];

    // Video codec
    if let Some(ref vcodec) = settings.video_codec {
        args.push("-c:v".to_string());
        args.push(vcodec.clone());
    }

    // Audio codec
    if let Some(ref acodec) = settings.audio_codec {
        args.push("-c:a".to_string());
        args.push(acodec.clone());
    }

    // Video bitrate
    if let Some(vbitrate) = settings.video_bitrate {
        args.push("-b:v".to_string());
        args.push(format!("{}k", vbitrate));
    }

    // Audio bitrate
    if let Some(abitrate) = settings.audio_bitrate {
        args.push("-b:a".to_string());
        args.push(format!("{}k", abitrate));
    }

    // Resolution
    if let Some((width, height)) = settings.resolution {
        args.push("-vf".to_string());
        args.push(format!("scale={}:{}", width, height));
    }

    // CRF (quality)
    if let Some(crf) = settings.crf {
        args.push("-crf".to_string());
        args.push(crf.to_string());
    }

    args.push(output.to_string_lossy().to_string());
    args
}

/// Build FFmpeg arguments for trimming with different modes
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
            // -ss AVANT -i pour seeking rapide
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
            // Ré-encodage ultrafast pour coupe précise mais rapide
            // -ss APRÈS -i pour précision à la frame
            vec![
                "-y".to_string(),
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
                "-crf".to_string(),
                "18".to_string(),
                "-c:a".to_string(),
                "aac".to_string(),
                "-b:a".to_string(),
                "192k".to_string(),
                output.to_string_lossy().to_string(),
            ]
        }
        TrimMode::HighQuality => {
            // Ré-encodage complet haute qualité
            // -ss APRÈS -i pour précision maximale
            vec![
                "-y".to_string(),
                "-i".to_string(),
                input.to_string_lossy().to_string(),
                "-ss".to_string(),
                format!("{:.3}", start),
                "-t".to_string(),
                format!("{:.3}", duration),
                "-c:v".to_string(),
                "libx264".to_string(),
                "-preset".to_string(),
                "slow".to_string(),
                "-crf".to_string(),
                "18".to_string(),
                "-c:a".to_string(),
                "aac".to_string(),
                "-b:a".to_string(),
                "256k".to_string(),
                output.to_string_lossy().to_string(),
            ]
        }
    }
}

/// Build FFmpeg arguments for cropping
pub fn build_crop_args(
    input: &PathBuf,
    output: &PathBuf,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Vec<String> {
    vec![
        "-y".to_string(),
        "-i".to_string(),
        input.to_string_lossy().to_string(),
        "-vf".to_string(),
        format!("crop={}:{}:{}:{}", width, height, x, y),
        output.to_string_lossy().to_string(),
    ]
}

/// Build FFmpeg arguments for concatenation
pub fn build_concat_args(
    inputs: &[PathBuf],
    output: &PathBuf,
    list_file: &PathBuf,
) -> Vec<String> {
    // Create concat list content
    let _ = inputs; // Used to create list_file content externally

    vec![
        "-y".to_string(),
        "-f".to_string(),
        "concat".to_string(),
        "-safe".to_string(),
        "0".to_string(),
        "-i".to_string(),
        list_file.to_string_lossy().to_string(),
        "-c".to_string(),
        "copy".to_string(),
        output.to_string_lossy().to_string(),
    ]
}

/// Build FFmpeg arguments for applying filters
pub fn build_filter_args(
    input: &PathBuf,
    output: &PathBuf,
    filters: &FilterSettings,
) -> Vec<String> {
    let mut args = vec![
        "-y".to_string(),
        "-i".to_string(),
        input.to_string_lossy().to_string(),
    ];

    let mut video_filters = Vec::new();
    let mut audio_filters = Vec::new();

    // Resize
    if let Some((width, height)) = filters.resize {
        video_filters.push(format!("scale={}:{}", width, height));
    }

    // Rotation
    if let Some(rotation) = filters.rotation {
        let transpose = match rotation {
            90 => "transpose=1",
            180 => "transpose=1,transpose=1",
            270 => "transpose=2",
            _ => "",
        };
        if !transpose.is_empty() {
            video_filters.push(transpose.to_string());
        }
    }

    // Flip
    if filters.flip_horizontal {
        video_filters.push("hflip".to_string());
    }
    if filters.flip_vertical {
        video_filters.push("vflip".to_string());
    }

    // Volume adjustment
    if let Some(volume) = filters.volume {
        if (volume - 1.0).abs() > 0.01 {
            audio_filters.push(format!("volume={}", volume));
        }
    }

    // Audio normalization
    if filters.normalize_audio {
        audio_filters.push("loudnorm".to_string());
    }

    // Apply video filters
    if !video_filters.is_empty() {
        args.push("-vf".to_string());
        args.push(video_filters.join(","));
    }

    // Apply audio filters
    if !audio_filters.is_empty() {
        args.push("-af".to_string());
        args.push(audio_filters.join(","));
    }

    args.push(output.to_string_lossy().to_string());
    args
}

/// Get recommended codec for a format
pub fn get_default_codec_for_format(format: &str) -> (Option<String>, Option<String>) {
    match format.to_lowercase().as_str() {
        "mp4" => (Some("libx264".to_string()), Some("aac".to_string())),
        "mkv" => (Some("libx264".to_string()), Some("aac".to_string())),
        "webm" => (Some("libvpx-vp9".to_string()), Some("libopus".to_string())),
        "avi" => (Some("mpeg4".to_string()), Some("mp3".to_string())),
        "mov" => (Some("libx264".to_string()), Some("aac".to_string())),
        "mp3" => (None, Some("libmp3lame".to_string())),
        "aac" => (None, Some("aac".to_string())),
        "wav" => (None, Some("pcm_s16le".to_string())),
        "flac" => (None, Some("flac".to_string())),
        _ => (None, None),
    }
}
