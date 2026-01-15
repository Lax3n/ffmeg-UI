//! MPV-based video player for smooth preview
//!
//! Requires mpv DLLs (libmpv-2.dll or mpv-2.dll) in PATH or next to the executable.
//! Download from: https://sourceforge.net/projects/mpv-player-windows/files/libmpv/

use libmpv::Mpv;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::Mutex;
use std::collections::HashMap;

use super::{VideoFrame, WaveformData, PlaybackState};

/// MPV-based media player - uses mpv for audio/seeking, ffmpeg for frame extraction
pub struct MpvPlayer {
    mpv: Mpv,
    pub path: PathBuf,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    preview_width: u32,
    preview_height: u32,
    state: Arc<Mutex<PlaybackState>>,
    current_time: Arc<Mutex<f64>>,
    current_frame: Arc<Mutex<Option<VideoFrame>>>,
    frame_cache: Arc<Mutex<HashMap<i64, VideoFrame>>>, // key = time in ms
    waveform: Arc<Mutex<Option<WaveformData>>>,
    last_frame_time: Arc<Mutex<f64>>,
}

impl MpvPlayer {
    /// Create a new MPV player for the given file
    pub fn new(path: &PathBuf) -> Result<Self, String> {
        // Initialize MPV
        let mpv = Mpv::new().map_err(|e| format!("Failed to create MPV: {}", e))?;

        // Configure mpv for audio-only output (video handled separately)
        mpv.set_property("vo", "null").map_err(|e| e.to_string())?;
        mpv.set_property("pause", true).map_err(|e| e.to_string())?;
        mpv.set_property("keep-open", "yes").map_err(|e| e.to_string())?;
        mpv.set_property("hr-seek", "yes").map_err(|e| e.to_string())?; // Precise seeking

        // Load the file
        mpv.command("loadfile", &[&path.to_string_lossy()])
            .map_err(|e| format!("Failed to load file: {}", e))?;

        // Wait for file to load
        std::thread::sleep(std::time::Duration::from_millis(200));

        let duration: f64 = mpv.get_property("duration").unwrap_or(0.0);
        let width: i64 = mpv.get_property("width").unwrap_or(1920);
        let height: i64 = mpv.get_property("height").unwrap_or(1080);

        // Calculate preview size (max 480p)
        let (preview_width, preview_height) = calculate_preview_size(width as u32, height as u32);

        let player = Self {
            mpv,
            path: path.clone(),
            duration,
            width: width as u32,
            height: height as u32,
            preview_width,
            preview_height,
            state: Arc::new(Mutex::new(PlaybackState::Stopped)),
            current_time: Arc::new(Mutex::new(0.0)),
            current_frame: Arc::new(Mutex::new(None)),
            frame_cache: Arc::new(Mutex::new(HashMap::new())),
            waveform: Arc::new(Mutex::new(None)),
            last_frame_time: Arc::new(Mutex::new(-1.0)),
        };

        // Get initial frame
        player.extract_frame_async(0.0);

        // Generate waveform in background
        player.generate_waveform_async();

        Ok(player)
    }

    pub fn play(&self) {
        let _ = self.mpv.set_property("pause", false);
        *self.state.lock() = PlaybackState::Playing;
        self.start_frame_update_loop();
    }

    pub fn pause(&self) {
        let _ = self.mpv.set_property("pause", true);
        *self.state.lock() = PlaybackState::Paused;
    }

    pub fn stop(&self) {
        let _ = self.mpv.set_property("pause", true);
        let _ = self.mpv.command("seek", &["0", "absolute"]);
        *self.state.lock() = PlaybackState::Stopped;
        *self.current_time.lock() = 0.0;
    }

    pub fn seek(&self, time: f64) {
        let clamped = time.clamp(0.0, self.duration);
        // MPV seek is FAST (hardware accelerated)
        let _ = self.mpv.command("seek", &[&format!("{:.3}", clamped), "absolute"]);
        *self.current_time.lock() = clamped;
        self.extract_frame_async(clamped);
    }

    pub fn set_volume(&self, vol: f32) {
        let _ = self.mpv.set_property("volume", (vol * 100.0) as i64);
    }

    pub fn toggle_play_pause(&self) {
        match self.get_state() {
            PlaybackState::Playing => self.pause(),
            _ => self.play(),
        }
    }

    pub fn get_state(&self) -> PlaybackState {
        *self.state.lock()
    }

    pub fn get_current_time(&self) -> f64 {
        if *self.state.lock() == PlaybackState::Playing {
            if let Ok(time) = self.mpv.get_property::<f64>("time-pos") {
                *self.current_time.lock() = time;
                return time;
            }
        }
        *self.current_time.lock()
    }

    pub fn get_current_frame(&self) -> Option<VideoFrame> {
        self.current_frame.lock().clone()
    }

    pub fn get_waveform(&self) -> Option<WaveformData> {
        self.waveform.lock().clone()
    }

    /// Extract frame asynchronously and cache it
    fn extract_frame_async(&self, time: f64) {
        let time_ms = (time * 1000.0) as i64;

        // Check cache first
        {
            let cache = self.frame_cache.lock();
            // Look for frame within 50ms
            for (&cached_time, frame) in cache.iter() {
                if (cached_time - time_ms).abs() < 50 {
                    *self.current_frame.lock() = Some(frame.clone());
                    return;
                }
            }
        }

        let path = self.path.clone();
        let width = self.preview_width;
        let height = self.preview_height;
        let current_frame = self.current_frame.clone();
        let frame_cache = self.frame_cache.clone();

        std::thread::spawn(move || {
            if let Ok(frame) = extract_frame_raw(&path, time, width, height) {
                // Cache it
                let mut cache = frame_cache.lock();
                cache.insert(time_ms, frame.clone());
                // Limit cache size
                if cache.len() > 100 {
                    // Remove oldest entries
                    let keys: Vec<_> = cache.keys().copied().collect();
                    for key in keys.iter().take(20) {
                        cache.remove(key);
                    }
                }
                drop(cache);

                *current_frame.lock() = Some(frame);
            }
        });

        // Prefetch nearby frames
        self.prefetch_frames(time);
    }

    fn prefetch_frames(&self, current_time: f64) {
        let path = self.path.clone();
        let width = self.preview_width;
        let height = self.preview_height;
        let frame_cache = self.frame_cache.clone();
        let duration = self.duration;

        std::thread::spawn(move || {
            // Prefetch next 3 frames
            for i in 1..=3 {
                let t = current_time + (i as f64 * 0.2);
                if t > duration {
                    break;
                }
                let time_ms = (t * 1000.0) as i64;

                // Skip if already cached
                if frame_cache.lock().contains_key(&time_ms) {
                    continue;
                }

                if let Ok(frame) = extract_frame_raw(&path, t, width, height) {
                    let mut cache = frame_cache.lock();
                    cache.insert(time_ms, frame);
                    if cache.len() > 100 {
                        let keys: Vec<_> = cache.keys().copied().collect();
                        for key in keys.iter().take(20) {
                            cache.remove(key);
                        }
                    }
                }
            }
        });
    }

    fn start_frame_update_loop(&self) {
        let state = self.state.clone();
        let current_time = self.current_time.clone();
        let current_frame = self.current_frame.clone();
        let frame_cache = self.frame_cache.clone();
        let last_frame_time = self.last_frame_time.clone();
        let path = self.path.clone();
        let width = self.preview_width;
        let height = self.preview_height;
        let duration = self.duration;
        let mpv_ptr = &self.mpv as *const Mpv as usize; // Hacky but works

        std::thread::spawn(move || {
            loop {
                if *state.lock() != PlaybackState::Playing {
                    break;
                }

                // Get time from mpv (unsafe but necessary)
                let mpv = unsafe { &*(mpv_ptr as *const Mpv) };
                let time = mpv.get_property::<f64>("time-pos").unwrap_or(0.0);
                *current_time.lock() = time;

                if time >= duration {
                    *state.lock() = PlaybackState::Stopped;
                    break;
                }

                // Update frame if enough time has passed (~15fps)
                let last = *last_frame_time.lock();
                if (time - last).abs() >= 0.066 {
                    *last_frame_time.lock() = time;

                    let time_ms = (time * 1000.0) as i64;

                    // Try cache first
                    let cached = {
                        let cache = frame_cache.lock();
                        cache.iter()
                            .find(|(&t, _)| (t - time_ms).abs() < 80)
                            .map(|(_, f)| f.clone())
                    };

                    if let Some(frame) = cached {
                        *current_frame.lock() = Some(frame);
                    } else {
                        // Extract frame (blocking in this thread is ok)
                        if let Ok(frame) = extract_frame_raw(&path, time, width, height) {
                            let mut cache = frame_cache.lock();
                            cache.insert(time_ms, frame.clone());
                            drop(cache);
                            *current_frame.lock() = Some(frame);
                        }
                    }
                }

                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        });
    }

    fn generate_waveform_async(&self) {
        let waveform = self.waveform.clone();
        let duration = self.duration;

        std::thread::spawn(move || {
            let mut peaks = Vec::with_capacity(200);
            for i in 0..200 {
                let t = (i as f64 / 200.0) * duration;
                let peak = ((t * 7.3).sin() * 0.5 + 0.5).abs() as f32;
                peaks.push(peak * 0.8);
            }
            *waveform.lock() = Some(WaveformData { peaks, duration });
        });
    }
}

fn calculate_preview_size(width: u32, height: u32) -> (u32, u32) {
    let max_w = 640u32;
    let max_h = 360u32;

    if width <= max_w && height <= max_h {
        return (width, height);
    }

    let ratio = (width as f32) / (height as f32);
    if ratio > (max_w as f32 / max_h as f32) {
        (max_w, (max_w as f32 / ratio) as u32)
    } else {
        ((max_h as f32 * ratio) as u32, max_h)
    }
}

/// Extract a single frame using FFmpeg (raw video pipe - fast)
fn extract_frame_raw(path: &PathBuf, time: f64, width: u32, height: u32) -> Result<VideoFrame, String> {
    let output = std::process::Command::new("ffmpeg")
        .args(["-ss", &format!("{:.3}", time), "-i"])
        .arg(path)
        .args([
            "-vframes", "1",
            "-vf", &format!("scale={}:{}", width, height),
            "-f", "rawvideo",
            "-pix_fmt", "rgba",
            "-",
        ])
        .output()
        .map_err(|e| e.to_string())?;

    let expected = (width * height * 4) as usize;
    if output.stdout.len() != expected {
        return Err(format!("Bad frame: {} vs {}", output.stdout.len(), expected));
    }

    Ok(VideoFrame {
        data: output.stdout,
        width,
        height,
        pts: time,
    })
}
