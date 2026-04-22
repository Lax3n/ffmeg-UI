use std::time::Instant;

/// Playback clock for A/V synchronization with variable speed support
pub struct PlaybackClock {
    start_time: Option<Instant>,
    paused_time: f64,
    offset: f64,
    is_paused: bool,
    speed: f64,
}

impl PlaybackClock {
    pub fn new() -> Self {
        Self {
            start_time: None,
            paused_time: 0.0,
            offset: 0.0,
            is_paused: true,
            speed: 1.0,
        }
    }

    /// Start or resume playback
    pub fn resume(&mut self) {
        if self.is_paused {
            self.start_time = Some(Instant::now());
            self.offset = self.paused_time;
            self.is_paused = false;
        }
    }

    /// Pause playback
    pub fn pause(&mut self) {
        if !self.is_paused {
            self.paused_time = self.get_time();
            self.is_paused = true;
        }
    }

    /// Reset clock to zero
    pub fn reset(&mut self) {
        self.start_time = None;
        self.paused_time = 0.0;
        self.offset = 0.0;
        self.is_paused = true;
    }

    /// Set current playback time (for seeking)
    pub fn set_time(&mut self, time: f64) {
        if self.is_paused {
            self.paused_time = time;
        } else {
            self.start_time = Some(Instant::now());
            self.offset = time;
        }
    }

    /// Get current playback time in seconds (accounts for speed)
    pub fn get_time(&self) -> f64 {
        if self.is_paused {
            self.paused_time
        } else if let Some(start) = self.start_time {
            start.elapsed().as_secs_f64() * self.speed + self.offset
        } else {
            0.0
        }
    }

    /// Set playback speed (0.25 to 4.0)
    pub fn set_speed(&mut self, speed: f64) {
        let new_speed = speed.clamp(0.25, 4.0);
        if !self.is_paused {
            // Save current position before changing speed
            self.paused_time = self.get_time();
            self.start_time = Some(Instant::now());
            self.offset = self.paused_time;
        }
        self.speed = new_speed;
    }

    /// Get current speed
    pub fn get_speed(&self) -> f64 {
        self.speed
    }
}

impl Default for PlaybackClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_clock_basic() {
        let mut clock = PlaybackClock::new();
        assert!((clock.get_time() - 0.0).abs() < 0.001);

        clock.resume();
        sleep(Duration::from_millis(100));
        let t = clock.get_time();
        assert!(t >= 0.09 && t <= 0.15);

        clock.pause();
        let t1 = clock.get_time();
        sleep(Duration::from_millis(50));
        let t2 = clock.get_time();
        assert!((t1 - t2).abs() < 0.001);
    }

    #[test]
    fn test_clock_seek() {
        let mut clock = PlaybackClock::new();
        clock.set_time(10.0);
        assert!((clock.get_time() - 10.0).abs() < 0.001);

        clock.resume();
        sleep(Duration::from_millis(100));
        let t = clock.get_time();
        assert!(t >= 10.09 && t <= 10.15);
    }

    #[test]
    fn test_clock_speed() {
        let mut clock = PlaybackClock::new();
        clock.set_speed(2.0);
        clock.resume();
        sleep(Duration::from_millis(100));
        let t = clock.get_time();
        // At 2x speed, 100ms real time = ~200ms video time
        assert!(t >= 0.18 && t <= 0.25);
    }
}
