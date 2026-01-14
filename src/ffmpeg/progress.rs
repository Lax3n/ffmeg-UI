use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProgress {
    pub task_name: String,
    pub message: String,
    pub progress: f32,
    pub is_complete: bool,
    pub is_error: bool,
}

impl TaskProgress {
    pub fn new(task_name: &str) -> Self {
        Self {
            task_name: task_name.to_string(),
            message: format!("{}...", task_name),
            progress: 0.0,
            is_complete: false,
            is_error: false,
        }
    }

    pub fn update(&mut self, progress: f32, message: &str) {
        self.progress = progress.clamp(0.0, 1.0);
        self.message = message.to_string();
    }

    pub fn complete(&mut self, message: &str) {
        self.progress = 1.0;
        self.message = message.to_string();
        self.is_complete = true;
    }

    pub fn fail(&mut self, message: &str) {
        self.message = message.to_string();
        self.is_complete = true;
        self.is_error = true;
    }
}

/// Parse FFmpeg progress output line
/// FFmpeg outputs progress in format: frame=  123 fps= 30 q=28.0 size=    1234kB time=00:00:05.00 bitrate= 2000.0kbits/s
pub fn parse_progress_line(line: &str, total_duration: f64) -> Option<f32> {
    // Look for time= pattern
    if let Some(time_pos) = line.find("time=") {
        let time_str = &line[time_pos + 5..];
        if let Some(end_pos) = time_str.find(' ') {
            let time_value = &time_str[..end_pos];
            if let Some(current_time) = parse_time_string(time_value) {
                if total_duration > 0.0 {
                    return Some((current_time / total_duration) as f32);
                }
            }
        }
    }
    None
}

/// Parse FFmpeg time format (HH:MM:SS.ms)
fn parse_time_string(time_str: &str) -> Option<f64> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() == 3 {
        let hours: f64 = parts[0].parse().ok()?;
        let minutes: f64 = parts[1].parse().ok()?;
        let seconds: f64 = parts[2].parse().ok()?;
        return Some(hours * 3600.0 + minutes * 60.0 + seconds);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_progress_line() {
        let line = "frame=  100 fps= 30 q=28.0 size=    1024kB time=00:00:10.00 bitrate= 838.9kbits/s";
        let progress = parse_progress_line(line, 100.0);
        assert!(progress.is_some());
        assert!((progress.unwrap() - 0.1).abs() < 0.01);
    }

    #[test]
    fn test_parse_time_string() {
        assert_eq!(parse_time_string("00:01:30.50"), Some(90.5));
        assert_eq!(parse_time_string("01:00:00.00"), Some(3600.0));
    }
}
