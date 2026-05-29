//! Extraction des positions de keyframes (I-frames) via ffprobe.
//!
//! Le smart-cut a besoin de connaître précisément où se trouvent les keyframes
//! pour décider quelles portions peuvent être copiées en lossless et lesquelles
//! doivent être ré-encodées.

use super::paths::ffprobe_command;
use std::path::Path;
use std::process::Stdio;

/// Liste triée (croissant) des PTS (en secondes) des keyframes du flux vidéo.
pub fn extract_keyframes(path: &Path) -> Vec<f64> {
    let mut cmd = ffprobe_command();
    cmd.args([
        "-v", "quiet",
        "-select_streams", "v:0",
        "-show_entries", "packet=pts_time,flags",
        "-of", "csv=p=0",
    ])
    .arg(path)
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .stdin(Stdio::null());

    let output = match cmd.output() {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut keyframes = Vec::new();

    // Format CSV : "pts_time,flags" — flags contient "K" pour les keyframes.
    // Exemples :
    //   0.000000,K__
    //   0.040000,___
    //   2.000000,K__
    for line in stdout.lines() {
        let mut parts = line.split(',');
        let time_str = match parts.next() {
            Some(s) => s.trim(),
            None => continue,
        };
        let flags = match parts.next() {
            Some(s) => s.trim(),
            None => continue,
        };

        if !flags.contains('K') {
            continue;
        }

        if let Ok(t) = time_str.parse::<f64>() {
            keyframes.push(t);
        }
    }

    keyframes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    keyframes
}

/// Trouve la première keyframe `>= t` (incluse), avec une tolérance ε pour
/// considérer comme "déjà alignée" une coupe à moins de `tolerance` d'une keyframe.
///
/// Retourne `None` si toutes les keyframes sont avant `t - tolerance`.
pub fn first_keyframe_at_or_after(keyframes: &[f64], t: f64, tolerance: f64) -> Option<f64> {
    keyframes
        .iter()
        .copied()
        .find(|&k| k >= t - tolerance)
}

/// Trouve la dernière keyframe `<= t` (incluse), avec une tolérance ε.
///
/// Retourne `None` si toutes les keyframes sont après `t + tolerance`.
pub fn last_keyframe_at_or_before(keyframes: &[f64], t: f64, tolerance: f64) -> Option<f64> {
    keyframes
        .iter()
        .copied()
        .rev()
        .find(|&k| k <= t + tolerance)
}

/// `true` si `t` tombe à moins de `tolerance` d'une keyframe.
pub fn is_keyframe_aligned(keyframes: &[f64], t: f64, tolerance: f64) -> bool {
    keyframes.iter().any(|&k| (k - t).abs() <= tolerance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_after_finds_next_keyframe() {
        let kf = vec![0.0, 2.0, 4.0, 6.0];
        assert_eq!(first_keyframe_at_or_after(&kf, 1.0, 0.01), Some(2.0));
        assert_eq!(first_keyframe_at_or_after(&kf, 2.0, 0.01), Some(2.0));
        assert_eq!(first_keyframe_at_or_after(&kf, 5.0, 0.01), Some(6.0));
        assert_eq!(first_keyframe_at_or_after(&kf, 7.0, 0.01), None);
    }

    #[test]
    fn first_after_respects_tolerance() {
        let kf = vec![0.0, 2.0, 4.0];
        // 2.005 est à 0.005 de 2.0 → considéré aligné, retour 2.0
        assert_eq!(first_keyframe_at_or_after(&kf, 2.005, 0.04), Some(2.0));
    }

    #[test]
    fn last_before_finds_previous_keyframe() {
        let kf = vec![0.0, 2.0, 4.0, 6.0];
        assert_eq!(last_keyframe_at_or_before(&kf, 5.0, 0.01), Some(4.0));
        assert_eq!(last_keyframe_at_or_before(&kf, 4.0, 0.01), Some(4.0));
        assert_eq!(last_keyframe_at_or_before(&kf, 0.5, 0.01), Some(0.0));
        assert_eq!(last_keyframe_at_or_before(&kf, -1.0, 0.01), None);
    }

    #[test]
    fn is_aligned_detects_close_keyframes() {
        let kf = vec![0.0, 2.0, 4.0];
        assert!(is_keyframe_aligned(&kf, 2.0, 0.04));
        assert!(is_keyframe_aligned(&kf, 2.03, 0.04));
        assert!(!is_keyframe_aligned(&kf, 2.5, 0.04));
    }
}
