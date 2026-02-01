/// Silence detection, bitrate mapping, and intelligent cut-point computation.

use std::path::Path;
use std::process::{Command, Stdio};

/// A detected silence interval from FFmpeg's silencedetect filter.
#[derive(Debug, Clone)]
pub struct SilenceInterval {
    pub start: f64,
    pub end: f64,
}

impl SilenceInterval {
    pub fn midpoint(&self) -> f64 {
        (self.start + self.end) / 2.0
    }
}

/// Build FFmpeg arguments for silence detection.
///
/// Runs the silencedetect audio filter and discards all output (`-f null`),
/// so the only useful data comes from stderr log lines.
pub fn build_silence_detect_args(input: &str, noise_db: f64, min_duration: f64) -> Vec<String> {
    vec![
        "-i".to_string(),
        input.to_string(),
        "-vn".to_string(),              // skip video decoding (huge speedup)
        "-ac".to_string(),
        "1".to_string(),                // downmix to mono
        "-ar".to_string(),
        "8000".to_string(),             // 8 kHz sample rate (plenty for silence detection)
        "-af".to_string(),
        format!("silencedetect=noise={}dB:d={}", noise_db, min_duration),
        "-f".to_string(),
        "null".to_string(),
        "-".to_string(),
    ]
}

/// Parse FFmpeg stderr output to extract silence intervals.
///
/// FFmpeg silencedetect outputs lines like:
///   [silencedetect @ ...] silence_start: 12.345
///   [silencedetect @ ...] silence_end: 13.678 | silence_duration: 1.333
pub fn parse_silence_output(stderr_lines: &[String]) -> Vec<SilenceInterval> {
    let mut intervals = Vec::new();
    let mut pending_start: Option<f64> = None;

    for line in stderr_lines {
        if let Some(pos) = line.find("silence_start:") {
            let after = &line[pos + "silence_start:".len()..];
            let value_str = after.trim().split_whitespace().next().unwrap_or("");
            if let Ok(v) = value_str.parse::<f64>() {
                pending_start = Some(v);
            }
        }

        if let Some(pos) = line.find("silence_end:") {
            let after = &line[pos + "silence_end:".len()..];
            let value_str = after.trim().split('|').next().unwrap_or("").trim();
            if let Ok(end) = value_str.parse::<f64>() {
                if let Some(start) = pending_start.take() {
                    intervals.push(SilenceInterval { start, end });
                }
            }
        }
    }

    intervals
}

/// Compute cut points that respect a maximum byte size per segment,
/// preferring to cut at silence boundaries for natural transitions.
///
/// # Arguments
/// * `duration`        – total duration of the media in seconds
/// * `bitrate_bps`     – estimated total bitrate in bits per second
/// * `max_bytes`       – maximum size per segment in bytes
/// * `tolerance_secs`  – search window (±) around the ideal cut point
/// * `silences`        – detected silence intervals
///
/// # Returns
/// A list of `(start, end)` pairs covering the full duration.
pub fn compute_cut_points(
    duration: f64,
    bitrate_bps: f64,
    max_bytes: u64,
    tolerance_secs: f64,
    silences: &[SilenceInterval],
) -> Vec<(f64, f64)> {
    if duration <= 0.0 || bitrate_bps <= 0.0 || max_bytes == 0 {
        return vec![(0.0, duration.max(0.0))];
    }

    let bytes_per_sec = bitrate_bps / 8.0;
    // Apply 2% safety margin
    let effective_max_bytes = (max_bytes as f64 * 0.98) as u64;
    let max_duration = effective_max_bytes as f64 / bytes_per_sec;

    // If the whole file fits in one segment, return it directly
    if duration <= max_duration {
        return vec![(0.0, duration)];
    }

    let mut segments = Vec::new();
    let mut cursor = 0.0;

    while cursor < duration - 0.1 {
        let ideal_end = (cursor + max_duration).min(duration);

        // If this chunk reaches the end, just take it
        if ideal_end >= duration - 0.1 {
            segments.push((cursor, duration));
            break;
        }

        // Search for the best silence to cut at within ±tolerance of ideal_end
        let window_start = (ideal_end - tolerance_secs).max(cursor + 1.0);
        let window_end = (ideal_end + tolerance_secs).min(duration);

        let best_silence = silences
            .iter()
            .filter(|s| s.midpoint() >= window_start && s.midpoint() <= window_end)
            .min_by(|a, b| {
                let dist_a = (a.midpoint() - ideal_end).abs();
                let dist_b = (b.midpoint() - ideal_end).abs();
                dist_a.partial_cmp(&dist_b).unwrap_or(std::cmp::Ordering::Equal)
            });

        let cut_point = if let Some(silence) = best_silence {
            // Cut at midpoint of the silence interval, but don't exceed max_duration from cursor
            let mid = silence.midpoint();
            if mid - cursor <= max_duration {
                mid
            } else {
                ideal_end
            }
        } else {
            ideal_end
        };

        segments.push((cursor, cut_point));
        cursor = cut_point;
    }

    segments
}

/// Cumulative byte size at each second boundary.
/// `cumulative_bytes[i]` = total bytes from time 0 to second `i`.
#[derive(Debug, Clone)]
pub struct BitrateMap {
    /// cumulative_bytes[i] = bytes from start to second i
    pub cumulative_bytes: Vec<u64>,
    pub duration: f64,
}

impl BitrateMap {
    /// Get the estimated byte count between two timestamps.
    pub fn bytes_between(&self, start: f64, end: f64) -> u64 {
        let start_sec = (start.floor() as usize).min(self.cumulative_bytes.len().saturating_sub(1));
        let end_sec = (end.ceil() as usize).min(self.cumulative_bytes.len().saturating_sub(1));
        self.cumulative_bytes[end_sec].saturating_sub(self.cumulative_bytes[start_sec])
    }

    /// Find the time at which `target_bytes` bytes have been consumed since `start_time`.
    /// Returns the second boundary where the cumulative size first exceeds start + target_bytes.
    pub fn time_for_bytes(&self, start_time: f64, target_bytes: u64) -> f64 {
        let start_sec = (start_time.floor() as usize).min(self.cumulative_bytes.len().saturating_sub(1));
        let base_bytes = self.cumulative_bytes[start_sec];

        for i in (start_sec + 1)..self.cumulative_bytes.len() {
            if self.cumulative_bytes[i] - base_bytes >= target_bytes {
                return i as f64;
            }
        }

        self.duration
    }

    /// Check if empty / failed to extract.
    pub fn is_empty(&self) -> bool {
        self.cumulative_bytes.is_empty()
    }
}

/// Extract a bitrate map from a video using ffprobe packet sizes.
/// Groups packet sizes by second to build a cumulative byte curve.
/// This gives accurate size data even for variable bitrate content.
pub fn extract_bitrate_map(path: &Path, duration: f64) -> BitrateMap {
    let mut cmd = Command::new("ffprobe");
    cmd.args([
        "-v", "quiet",
        "-select_streams", "v:0",
        "-show_entries", "packet=pts_time,size",
        "-of", "csv=p=0",
    ])
    .arg(path)
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .stdin(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = match cmd.output() {
        Ok(o) => o,
        Err(_) => return BitrateMap { cumulative_bytes: Vec::new(), duration },
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let num_seconds = (duration.ceil() as usize) + 1;
    let mut bytes_per_second = vec![0u64; num_seconds];

    // Parse lines like "1.234,5678" (pts_time,size)
    for line in stdout.lines() {
        let mut parts = line.split(',');
        let time_str = match parts.next() {
            Some(s) => s.trim(),
            None => continue,
        };
        let size_str = match parts.next() {
            Some(s) => s.trim(),
            None => continue,
        };

        let time: f64 = match time_str.parse() {
            Ok(t) => t,
            Err(_) => continue,
        };
        let size: u64 = match size_str.parse() {
            Ok(s) => s,
            Err(_) => continue,
        };

        let sec = (time.floor() as usize).min(num_seconds.saturating_sub(1));
        bytes_per_second[sec] += size;
    }

    // Also account for audio stream sizes
    let mut cmd_audio = Command::new("ffprobe");
    cmd_audio.args([
        "-v", "quiet",
        "-select_streams", "a:0",
        "-show_entries", "packet=pts_time,size",
        "-of", "csv=p=0",
    ])
    .arg(path)
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .stdin(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd_audio.creation_flags(CREATE_NO_WINDOW);
    }

    if let Ok(audio_output) = cmd_audio.output() {
        let audio_stdout = String::from_utf8_lossy(&audio_output.stdout);
        for line in audio_stdout.lines() {
            let mut parts = line.split(',');
            let time_str = match parts.next() {
                Some(s) => s.trim(),
                None => continue,
            };
            let size_str = match parts.next() {
                Some(s) => s.trim(),
                None => continue,
            };
            let time: f64 = match time_str.parse() {
                Ok(t) => t,
                Err(_) => continue,
            };
            let size: u64 = match size_str.parse() {
                Ok(s) => s,
                Err(_) => continue,
            };
            let sec = (time.floor() as usize).min(num_seconds.saturating_sub(1));
            bytes_per_second[sec] += size;
        }
    }

    // Build cumulative array
    let mut cumulative = vec![0u64; num_seconds];
    for i in 1..num_seconds {
        cumulative[i] = cumulative[i - 1] + bytes_per_second[i - 1];
    }

    BitrateMap {
        cumulative_bytes: cumulative,
        duration,
    }
}

/// Compute cut points using actual per-second bitrate data (BitrateMap)
/// instead of an average bitrate estimate. Handles variable bitrate content.
pub fn compute_cut_points_accurate(
    duration: f64,
    max_bytes: u64,
    tolerance_secs: f64,
    silences: &[SilenceInterval],
    bitrate_map: &BitrateMap,
) -> Vec<(f64, f64)> {
    if duration <= 0.0 || max_bytes == 0 || bitrate_map.is_empty() {
        return vec![(0.0, duration.max(0.0))];
    }

    // Apply 2% safety margin
    let effective_max_bytes = (max_bytes as f64 * 0.98) as u64;

    // If the whole file fits in one segment
    let total_bytes = bitrate_map.bytes_between(0.0, duration);
    if total_bytes <= effective_max_bytes {
        return vec![(0.0, duration)];
    }

    let mut segments = Vec::new();
    let mut cursor = 0.0;

    while cursor < duration - 0.1 {
        // Find where the cumulative size from cursor reaches effective_max_bytes
        let ideal_end = bitrate_map.time_for_bytes(cursor, effective_max_bytes).min(duration);

        // If this chunk reaches the end, just take it
        if ideal_end >= duration - 0.1 {
            segments.push((cursor, duration));
            break;
        }

        // Search for the best silence to cut at within ±tolerance of ideal_end
        let window_start = (ideal_end - tolerance_secs).max(cursor + 1.0);
        let window_end = (ideal_end + tolerance_secs).min(duration);

        let best_silence = silences
            .iter()
            .filter(|s| s.midpoint() >= window_start && s.midpoint() <= window_end)
            .min_by(|a, b| {
                let dist_a = (a.midpoint() - ideal_end).abs();
                let dist_b = (b.midpoint() - ideal_end).abs();
                dist_a.partial_cmp(&dist_b).unwrap_or(std::cmp::Ordering::Equal)
            });

        let cut_point = if let Some(silence) = best_silence {
            let mid = silence.midpoint();
            // Verify that cutting here doesn't exceed max bytes
            let bytes = bitrate_map.bytes_between(cursor, mid);
            if bytes <= effective_max_bytes {
                mid
            } else {
                ideal_end
            }
        } else {
            ideal_end
        };

        // Safety: ensure we advance at least 1 second
        let cut_point = if cut_point <= cursor + 0.5 {
            (cursor + 1.0).min(duration)
        } else {
            cut_point
        };

        segments.push((cursor, cut_point));
        cursor = cut_point;
    }

    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_silence_output() {
        let lines = vec![
            "[silencedetect @ 0x1234] silence_start: 10.5".to_string(),
            "[silencedetect @ 0x1234] silence_end: 12.3 | silence_duration: 1.8".to_string(),
            "[silencedetect @ 0x1234] silence_start: 45.0".to_string(),
            "[silencedetect @ 0x1234] silence_end: 46.5 | silence_duration: 1.5".to_string(),
        ];

        let intervals = parse_silence_output(&lines);
        assert_eq!(intervals.len(), 2);
        assert!((intervals[0].start - 10.5).abs() < 0.001);
        assert!((intervals[0].end - 12.3).abs() < 0.001);
        assert!((intervals[1].start - 45.0).abs() < 0.001);
        assert!((intervals[1].end - 46.5).abs() < 0.001);
    }

    #[test]
    fn test_compute_cut_points_single_segment() {
        // 100 seconds at 1 Mbps = 12.5 MB, max = 100 MB => single segment
        let segments = compute_cut_points(100.0, 1_000_000.0, 100_000_000, 30.0, &[]);
        assert_eq!(segments.len(), 1);
        assert!((segments[0].0).abs() < 0.001);
        assert!((segments[0].1 - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_compute_cut_points_multiple_segments() {
        // 600 seconds at 8 Mbps = 600 MB, max = 200 MB (200_000_000 bytes)
        // 8 Mbps = 1 MB/s, so 200 MB = ~200s per segment => ~3 segments
        let silences = vec![
            SilenceInterval { start: 195.0, end: 197.0 },  // near first cut
            SilenceInterval { start: 390.0, end: 392.0 },  // near second cut
        ];

        let segments = compute_cut_points(600.0, 8_000_000.0, 200_000_000, 30.0, &silences);
        assert!(segments.len() >= 3);

        // Each segment should start where the previous ended
        for i in 1..segments.len() {
            assert!((segments[i].0 - segments[i - 1].1).abs() < 0.001);
        }
        // Last segment should end at duration
        assert!((segments.last().unwrap().1 - 600.0).abs() < 0.001);
    }

    #[test]
    fn test_compute_cut_points_no_silences() {
        // Falls back to uniform cuts
        let segments = compute_cut_points(600.0, 8_000_000.0, 200_000_000, 30.0, &[]);
        assert!(segments.len() >= 3);

        for i in 1..segments.len() {
            assert!((segments[i].0 - segments[i - 1].1).abs() < 0.001);
        }
    }
}
