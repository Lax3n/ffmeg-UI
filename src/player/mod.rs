mod video_decoder;
mod sync;
mod audio_player;
mod stream_decoder;

#[cfg(feature = "mpv")]
mod mpv_player;

pub use sync::*;
pub use audio_player::*;
pub use stream_decoder::*;

#[cfg(feature = "mpv")]
pub use mpv_player::MpvPlayer;

use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone)]
pub struct VideoFrame {
    pub data: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub pts: f64,
}

pub const SPEED_PRESETS: &[f64] = &[0.25, 0.5, 1.0, 1.5, 2.0, 4.0];

pub struct MediaPlayer {
    pub duration: f64,
    pub framerate: f64,
    state: Arc<Mutex<PlaybackState>>,
    clock: Arc<Mutex<PlaybackClock>>,
    audio_player: Option<AudioPlayer>,
    stream_decoder: Option<StreamDecoder>,
    speed: f64,
}

impl MediaPlayer {
    pub fn new(path: &PathBuf) -> Result<Self, String> {
        let info = crate::ffmpeg::probe_file(path)
            .map_err(|e| format!("Failed to probe file: {}", e))?;

        let state = Arc::new(Mutex::new(PlaybackState::Stopped));
        let clock = Arc::new(Mutex::new(PlaybackClock::new()));

        let audio_player = AudioPlayer::new(path, info.duration).ok();
        let fps = info.framerate.unwrap_or(30.0);
        let decoder = StreamDecoder::new(path, info.width, info.height, info.duration, fps).ok();

        if let Some(ref dec) = decoder {
            dec.seek(0.0);
        }

        Ok(Self {
            duration: info.duration,
            framerate: fps,
            state,
            clock,
            audio_player,
            stream_decoder: decoder,
            speed: 1.0,
        })
    }

    pub fn play(&self) {
        *self.state.lock() = PlaybackState::Playing;
        self.clock.lock().resume();
        if (self.speed - 1.0).abs() < 0.01 {
            if let Some(ref audio) = self.audio_player {
                // Re-sync audio to current decoder time before playing
                if let Some(ref dec) = self.stream_decoder {
                    let t = dec.get_decoder_time();
                    audio.seek(t);
                }
                audio.play();
            }
        }
        if let Some(ref decoder) = self.stream_decoder {
            decoder.play();
        }
    }

    pub fn pause(&self) {
        *self.state.lock() = PlaybackState::Paused;
        self.clock.lock().pause();
        if let Some(ref audio) = self.audio_player {
            audio.pause();
        }
        if let Some(ref decoder) = self.stream_decoder {
            decoder.pause();
        }
    }

    pub fn stop(&self) {
        *self.state.lock() = PlaybackState::Stopped;
        self.clock.lock().reset();
        if let Some(ref audio) = self.audio_player {
            audio.stop();
        }
        if let Some(ref decoder) = self.stream_decoder {
            decoder.seek(0.0);
        }
    }

    pub fn seek(&self, time: f64) {
        let clamped = time.clamp(0.0, self.duration);
        self.clock.lock().set_time(clamped);
        if let Some(ref decoder) = self.stream_decoder {
            decoder.seek(clamped);
        }
        if let Some(ref audio) = self.audio_player {
            audio.seek(clamped);
            if *self.state.lock() == PlaybackState::Playing && (self.speed - 1.0).abs() < 0.01 {
                audio.play();
            }
        }
    }

    pub fn set_volume(&self, vol: f32) {
        if let Some(ref audio) = self.audio_player {
            audio.set_volume(vol);
        }
    }

    pub fn set_speed(&mut self, speed: f64) {
        let speed = speed.clamp(0.25, 4.0);
        self.speed = speed;
        self.clock.lock().set_speed(speed);
        if let Some(ref decoder) = self.stream_decoder {
            decoder.set_speed(speed as f32);
        }
        if let Some(ref audio) = self.audio_player {
            if (speed - 1.0).abs() > 0.01 {
                audio.pause();
            } else if *self.state.lock() == PlaybackState::Playing {
                audio.play();
            }
        }
    }

    pub fn get_speed(&self) -> f64 {
        self.speed
    }

    pub fn get_state(&self) -> PlaybackState {
        *self.state.lock()
    }

    /// Get current time. During playback, uses the decoder's authoritative time
    /// for proper A/V sync. When paused, uses the clock's paused time.
    pub fn get_current_time(&self) -> f64 {
        if *self.state.lock() == PlaybackState::Playing {
            // Use decoder time as authoritative source during playback
            if let Some(ref dec) = self.stream_decoder {
                let t = dec.get_decoder_time().min(self.duration);
                self.clock.lock().set_time(t);
                return t;
            }
            self.clock.lock().get_time().min(self.duration)
        } else {
            self.clock.lock().get_time()
        }
    }

    pub fn get_current_frame(&self) -> Option<VideoFrame> {
        self.stream_decoder.as_ref().and_then(|d| d.get_frame())
    }

    pub fn toggle_play_pause(&self) {
        match self.get_state() {
            PlaybackState::Playing => self.pause(),
            PlaybackState::Paused | PlaybackState::Stopped => self.play(),
        }
    }

    pub fn frame_step_forward(&self) {
        let current = self.get_current_time();
        let step = 1.0 / self.framerate;
        let new_time = (current + step).min(self.duration);
        self.clock.lock().set_time(new_time);
        if let Some(ref decoder) = self.stream_decoder {
            decoder.seek(new_time);
        }
    }

    pub fn frame_step_backward(&self) {
        let current = self.get_current_time();
        let step = 1.0 / self.framerate;
        let new_time = (current - step).max(0.0);
        self.clock.lock().set_time(new_time);
        if let Some(ref decoder) = self.stream_decoder {
            decoder.seek(new_time);
        }
    }
}
