use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineClip {
    pub file_index: usize,
    pub start_time: f64,
    pub end_time: f64,
    pub position: f64,
}

impl TimelineClip {
    pub fn new(file_index: usize, duration: f64, position: f64) -> Self {
        Self {
            file_index,
            start_time: 0.0,
            end_time: duration,
            position,
        }
    }

    pub fn duration(&self) -> f64 {
        self.end_time - self.start_time
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Timeline {
    pub clips: Vec<TimelineClip>,
    pub zoom: f32,
    pub scroll_offset: f32,
}

impl Timeline {
    pub fn new() -> Self {
        Self {
            clips: Vec::new(),
            zoom: 1.0,
            scroll_offset: 0.0,
        }
    }

    pub fn total_duration(&self) -> f64 {
        self.clips
            .iter()
            .map(|c| c.position + c.duration())
            .fold(0.0, f64::max)
    }

    pub fn add_clip(&mut self, file_index: usize, duration: f64) {
        let position = self.total_duration();
        self.clips.push(TimelineClip::new(file_index, duration, position));
    }

    pub fn remove_clip(&mut self, index: usize) {
        if index < self.clips.len() {
            self.clips.remove(index);
        }
    }

    pub fn clear(&mut self) {
        self.clips.clear();
    }
}
