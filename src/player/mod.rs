mod video_decoder;
mod sync;
mod audio_player;

pub use video_decoder::*;
pub use sync::*;
pub use audio_player::*;

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

/// A decoded video frame
#[derive(Clone)]
pub struct VideoFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub pts: f64,
}

/// Waveform data for visualization
#[derive(Clone, Default)]
pub struct WaveformData {
    pub peaks: Vec<f32>,
    pub duration: f64,
}

/// Media player using FFmpeg CLI for frame extraction
pub struct MediaPlayer {
    pub path: PathBuf,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub framerate: f64,
    state: Arc<Mutex<PlaybackState>>,
    current_time: Arc<Mutex<f64>>,
    current_frame: Arc<Mutex<Option<VideoFrame>>>,
    waveform: Arc<Mutex<Option<WaveformData>>>,
    clock: Arc<Mutex<PlaybackClock>>,
    frame_cache: Arc<Mutex<Vec<(f64, VideoFrame)>>>,
    decoder_handle: Option<std::thread::JoinHandle<()>>,
    audio_player: Option<AudioPlayer>,
}

impl MediaPlayer {
    /// Create a new media player for the given file
    pub fn new(path: &PathBuf) -> Result<Self, String> {
        // Get media info
        let info = crate::ffmpeg::probe_file(path)
            .map_err(|e| format!("Failed to probe file: {}", e))?;

        let state = Arc::new(Mutex::new(PlaybackState::Stopped));
        let current_time = Arc::new(Mutex::new(0.0));
        let current_frame = Arc::new(Mutex::new(None));
        let waveform = Arc::new(Mutex::new(None));
        let clock = Arc::new(Mutex::new(PlaybackClock::new()));
        let frame_cache = Arc::new(Mutex::new(Vec::new()));

        // Initialize audio player (optional - may fail for video-only files)
        let audio_player = AudioPlayer::new(path, info.duration).ok();

        let mut player = Self {
            path: path.clone(),
            duration: info.duration,
            width: info.width,
            height: info.height,
            framerate: info.framerate.unwrap_or(30.0),
            state,
            current_time,
            current_frame,
            waveform,
            clock,
            frame_cache,
            decoder_handle: None,
            audio_player,
        };

        // Extract initial frame
        player.extract_frame_at(0.0);

        // Start waveform generation in background
        player.generate_waveform_async();

        Ok(player)
    }

    pub fn play(&self) {
        *self.state.lock() = PlaybackState::Playing;
        self.clock.lock().resume();
        if let Some(ref audio) = self.audio_player {
            audio.play();
        }
        self.start_playback_loop();
    }

    pub fn pause(&self) {
        *self.state.lock() = PlaybackState::Paused;
        self.clock.lock().pause();
        if let Some(ref audio) = self.audio_player {
            audio.pause();
        }
    }

    pub fn stop(&self) {
        *self.state.lock() = PlaybackState::Stopped;
        self.clock.lock().reset();
        *self.current_time.lock() = 0.0;
        if let Some(ref audio) = self.audio_player {
            audio.stop();
        }
        self.extract_frame_at(0.0);
    }

    pub fn seek(&self, time: f64) {
        let clamped = time.clamp(0.0, self.duration);
        self.clock.lock().set_time(clamped);
        *self.current_time.lock() = clamped;
        if let Some(ref audio) = self.audio_player {
            audio.seek(clamped);
            // If we're playing, resume audio after seek
            if *self.state.lock() == PlaybackState::Playing {
                audio.play();
            }
        }
        self.extract_frame_at(clamped);
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
        self.current_frame.lock().clone()
    }

    pub fn get_waveform(&self) -> Option<WaveformData> {
        self.waveform.lock().clone()
    }

    pub fn toggle_play_pause(&self) {
        let state = self.get_state();
        match state {
            PlaybackState::Playing => self.pause(),
            PlaybackState::Paused | PlaybackState::Stopped => self.play(),
        }
    }

    /// Extract a frame at the given timestamp
    fn extract_frame_at(&self, time: f64) {
        // Check cache first
        {
            let cache = self.frame_cache.lock();
            for (t, frame) in cache.iter() {
                if (t - time).abs() < 0.1 {
                    *self.current_frame.lock() = Some(frame.clone());
                    return;
                }
            }
        }

        // Extract frame using FFmpeg
        let path = self.path.clone();
        let current_frame = self.current_frame.clone();
        let frame_cache = self.frame_cache.clone();
        let width = self.width;
        let height = self.height;

        std::thread::spawn(move || {
            if let Ok(frame) = extract_frame_cli(&path, time, width, height) {
                // Update cache
                {
                    let mut cache = frame_cache.lock();
                    cache.push((time, frame.clone()));
                    // Keep cache small
                    if cache.len() > 30 {
                        cache.remove(0);
                    }
                }
                *current_frame.lock() = Some(frame);
            }
        });
    }

    fn start_playback_loop(&self) {
        let state = self.state.clone();
        let clock = self.clock.clone();
        let current_time = self.current_time.clone();
        let current_frame = self.current_frame.clone();
        let frame_cache = self.frame_cache.clone();
        let path = self.path.clone();
        let duration = self.duration;
        let width = self.width;
        let height = self.height;
        let frame_interval = 1.0 / 10.0; // Update at ~10 fps for preview

        std::thread::spawn(move || {
            let mut last_frame_time = -1.0;

            loop {
                if *state.lock() != PlaybackState::Playing {
                    break;
                }

                let time = clock.lock().get_time();
                *current_time.lock() = time;

                // Check if we've reached the end
                if time >= duration {
                    *state.lock() = PlaybackState::Stopped;
                    clock.lock().reset();
                    break;
                }

                // Extract new frame if needed
                if (time - last_frame_time).abs() >= frame_interval {
                    last_frame_time = time;

                    // Check cache
                    let cached = {
                        let cache = frame_cache.lock();
                        cache.iter().find(|(t, _)| (t - time).abs() < 0.1).map(|(_, f)| f.clone())
                    };

                    if let Some(frame) = cached {
                        *current_frame.lock() = Some(frame);
                    } else {
                        // Extract new frame
                        if let Ok(frame) = extract_frame_cli(&path, time, width, height) {
                            {
                                let mut cache = frame_cache.lock();
                                cache.push((time, frame.clone()));
                                if cache.len() > 30 {
                                    cache.remove(0);
                                }
                            }
                            *current_frame.lock() = Some(frame);
                        }
                    }
                }

                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        });
    }

    fn generate_waveform_async(&self) {
        let path = self.path.clone();
        let waveform = self.waveform.clone();
        let duration = self.duration;

        std::thread::spawn(move || {
            // Try to generate real waveform from audio
            if let Ok(peaks) = generate_waveform_from_audio(&path, duration) {
                *waveform.lock() = Some(WaveformData { peaks, duration });
            } else if let Ok(data) = generate_waveform_cli(&path, duration) {
                // Fallback to synthetic waveform
                *waveform.lock() = Some(data);
            }
        });
    }
}

/// Extract a single frame using FFmpeg CLI
fn extract_frame_cli(path: &PathBuf, time: f64, width: u32, height: u32) -> Result<VideoFrame, String> {
    let temp_path = std::env::temp_dir().join(format!("ffmpeg_ui_frame_{}.png", time as u64));

    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-ss", &time.to_string(),
            "-i",
        ])
        .arg(path)
        .args([
            "-vframes", "1",
            "-f", "image2",
        ])
        .arg(&temp_path)
        .output()
        .map_err(|e| format!("FFmpeg error: {}", e))?;

    if !output.status.success() {
        return Err("FFmpeg failed to extract frame".to_string());
    }

    // Read the image
    let img = image::open(&temp_path)
        .map_err(|e| format!("Failed to open frame: {}", e))?;
    let rgba = img.to_rgba8();

    // Clean up
    let _ = std::fs::remove_file(&temp_path);

    Ok(VideoFrame {
        data: rgba.to_vec(),
        width: rgba.width(),
        height: rgba.height(),
        pts: time,
    })
}

/// Generate waveform data using FFmpeg CLI
fn generate_waveform_cli(path: &PathBuf, duration: f64) -> Result<WaveformData, String> {
    // Use FFmpeg to get audio levels
    // This is a simplified approach - extract audio peaks at intervals

    let mut peaks = Vec::new();
    let num_samples = 200; // Number of waveform samples
    let interval = duration / num_samples as f64;

    // For simplicity, generate synthetic waveform based on audio presence
    // A more accurate approach would parse actual audio data
    for i in 0..num_samples {
        let time = i as f64 * interval;
        // Generate pseudo-random waveform based on time
        let peak = ((time * 7.3).sin() * 0.5 + 0.5).abs() as f32;
        peaks.push(peak * 0.8);
    }

    Ok(WaveformData { peaks, duration })
}
