mod video_decoder;
mod sync;
mod audio_player;
mod stream_decoder;

#[cfg(feature = "mpv")]
mod mpv_player;

pub use video_decoder::*;
pub use sync::*;
pub use audio_player::*;
pub use stream_decoder::*;

#[cfg(feature = "mpv")]
pub use mpv_player::MpvPlayer;

use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::Mutex;

/// Playback state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

/// A decoded video frame (Arc data for cheap cloning)
#[derive(Clone)]
pub struct VideoFrame {
    pub data: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    pub pts: f64,
}

/// Media player using a persistent FFmpeg pipe for frame decoding
pub struct MediaPlayer {
    pub path: PathBuf,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub framerate: f64,
    state: Arc<Mutex<PlaybackState>>,
    current_time: Arc<Mutex<f64>>,
    clock: Arc<Mutex<PlaybackClock>>,
    audio_player: Option<AudioPlayer>,
    stream_decoder: Option<StreamDecoder>,
}

impl MediaPlayer {
    /// Create a new media player for the given file.
    /// Probes metadata and seeks to the first frame via StreamDecoder.
    pub fn new(path: &PathBuf) -> Result<Self, String> {
        let info = crate::ffmpeg::probe_file(path)
            .map_err(|e| format!("Failed to probe file: {}", e))?;

        let state = Arc::new(Mutex::new(PlaybackState::Stopped));
        let current_time = Arc::new(Mutex::new(0.0));
        let clock = Arc::new(Mutex::new(PlaybackClock::new()));

        let audio_player = AudioPlayer::new(path, info.duration).ok();

        // Create the persistent stream decoder
        let decoder = StreamDecoder::new(path, info.width, info.height, info.duration).ok();

        // Seek to 0 to get the first frame for preview
        if let Some(ref dec) = decoder {
            dec.seek(0.0);
        }

        Ok(Self {
            path: path.clone(),
            duration: info.duration,
            width: info.width,
            height: info.height,
            framerate: info.framerate.unwrap_or(30.0),
            state,
            current_time,
            clock,
            audio_player,
            stream_decoder: decoder,
        })
    }

    pub fn play(&self) {
        *self.state.lock() = PlaybackState::Playing;
        self.clock.lock().resume();
        if let Some(ref audio) = self.audio_player {
            audio.play();
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
        *self.current_time.lock() = 0.0;
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
        *self.current_time.lock() = clamped;
        if let Some(ref audio) = self.audio_player {
            audio.seek(clamped);
            if *self.state.lock() == PlaybackState::Playing {
                audio.play();
            }
        }
        if let Some(ref decoder) = self.stream_decoder {
            decoder.seek(clamped);
        }
    }

    pub fn set_volume(&self, vol: f32) {
        if let Some(ref audio) = self.audio_player {
            audio.set_volume(vol);
        }
    }

    pub fn get_state(&self) -> PlaybackState {
        *self.state.lock()
    }

    pub fn get_current_time(&self) -> f64 {
        if *self.state.lock() == PlaybackState::Playing {
            let time = self.clock.lock().get_time();
            *self.current_time.lock() = time;
            time
        } else {
            *self.current_time.lock()
        }
    }

    pub fn get_current_frame(&self) -> Option<VideoFrame> {
        self.stream_decoder.as_ref().and_then(|d| d.get_frame())
    }

    pub fn toggle_play_pause(&self) {
        let state = self.get_state();
        match state {
            PlaybackState::Playing => self.pause(),
            PlaybackState::Paused | PlaybackState::Stopped => self.play(),
        }
    }
}
