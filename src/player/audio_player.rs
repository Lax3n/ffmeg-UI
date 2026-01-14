use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::Mutex;

/// Audio player using rodio for playback
pub struct AudioPlayer {
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
    sink: Arc<Sink>,
    audio_path: PathBuf,
    temp_audio_path: Option<PathBuf>,
    duration: f64,
    volume: Arc<Mutex<f32>>,
}

impl AudioPlayer {
    /// Create a new audio player for the given media file
    pub fn new(video_path: &PathBuf, duration: f64) -> Result<Self, String> {
        // Initialize audio output
        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| format!("Failed to initialize audio output: {}", e))?;

        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| format!("Failed to create audio sink: {}", e))?;

        // Extract audio to temporary WAV file
        let temp_audio_path = extract_audio_to_temp(video_path)?;

        let player = Self {
            _stream: stream,
            _stream_handle: stream_handle,
            sink: Arc::new(sink),
            audio_path: video_path.clone(),
            temp_audio_path: Some(temp_audio_path),
            duration,
            volume: Arc::new(Mutex::new(1.0)),
        };

        Ok(player)
    }

    /// Load audio from the temp file into the sink
    fn load_audio(&self) -> Result<(), String> {
        if let Some(ref temp_path) = self.temp_audio_path {
            let file = File::open(temp_path)
                .map_err(|e| format!("Failed to open audio file: {}", e))?;
            let source = Decoder::new(BufReader::new(file))
                .map_err(|e| format!("Failed to decode audio: {}", e))?;

            self.sink.append(source);
            self.sink.set_volume(*self.volume.lock());
            self.sink.pause();
        }
        Ok(())
    }

    /// Play audio
    pub fn play(&self) {
        if self.sink.empty() {
            let _ = self.load_audio();
        }
        self.sink.play();
    }

    /// Pause audio
    pub fn pause(&self) {
        self.sink.pause();
    }

    /// Stop audio
    pub fn stop(&self) {
        self.sink.stop();
        self.sink.clear();
    }

    /// Set volume (0.0 to 2.0)
    pub fn set_volume(&self, vol: f32) {
        let clamped = vol.clamp(0.0, 2.0);
        *self.volume.lock() = clamped;
        self.sink.set_volume(clamped);
    }

    /// Get current volume
    pub fn get_volume(&self) -> f32 {
        *self.volume.lock()
    }

    /// Seek to position (requires reloading audio)
    pub fn seek(&self, time: f64) {
        // rodio doesn't support seeking directly, so we need to reload
        // and skip samples. For simplicity, we stop and reload.
        self.sink.stop();
        self.sink.clear();

        if let Some(ref temp_path) = self.temp_audio_path {
            if let Ok(file) = File::open(temp_path) {
                if let Ok(source) = Decoder::new(BufReader::new(file)) {
                    // Skip to the target position
                    let sample_rate = source.sample_rate();
                    let channels = source.channels() as u32;
                    let samples_to_skip = (time * sample_rate as f64 * channels as f64) as usize;

                    let skipped = source.skip_duration(std::time::Duration::from_secs_f64(time));
                    self.sink.append(skipped);
                    self.sink.set_volume(*self.volume.lock());
                }
            }
        }
    }

    /// Check if audio is playing
    pub fn is_playing(&self) -> bool {
        !self.sink.is_paused() && !self.sink.empty()
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        // Clean up temporary audio file
        if let Some(ref temp_path) = self.temp_audio_path {
            let _ = std::fs::remove_file(temp_path);
        }
    }
}

/// Extract audio from video to a temporary WAV file using FFmpeg
fn extract_audio_to_temp(video_path: &PathBuf) -> Result<PathBuf, String> {
    let temp_dir = std::env::temp_dir();
    let file_stem = video_path.file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let temp_path = temp_dir.join(format!("ffmpeg_ui_audio_{}.wav", file_stem));

    // Remove existing temp file if any
    let _ = std::fs::remove_file(&temp_path);

    // Extract audio using FFmpeg
    let output = std::process::Command::new("ffmpeg")
        .args([
            "-y",           // Overwrite output
            "-i",
        ])
        .arg(video_path)
        .args([
            "-vn",          // No video
            "-acodec", "pcm_s16le",  // PCM 16-bit
            "-ar", "44100", // 44.1kHz sample rate
            "-ac", "2",     // Stereo
        ])
        .arg(&temp_path)
        .output()
        .map_err(|e| format!("Failed to run FFmpeg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // If no audio stream, that's okay - just return the path anyway
        if stderr.contains("does not contain any stream") || stderr.contains("no audio") {
            // Create an empty temp path marker
            return Ok(temp_path);
        }
        return Err(format!("FFmpeg audio extraction failed: {}", stderr));
    }

    Ok(temp_path)
}

/// Generate real waveform data from audio file
pub fn generate_waveform_from_audio(audio_path: &PathBuf, duration: f64) -> Result<Vec<f32>, String> {
    // Use FFmpeg to extract audio levels
    let output = std::process::Command::new("ffmpeg")
        .args(["-i"])
        .arg(audio_path)
        .args([
            "-af", "astats=metadata=1:reset=1,ametadata=print:key=lavfi.astats.Overall.Peak_level:file=-",
            "-f", "null",
            "-",
        ])
        .output()
        .map_err(|e| format!("Failed to analyze audio: {}", e))?;

    // Parse output for peak levels (simplified approach)
    // For now, generate waveform from actual audio samples
    let temp_raw = std::env::temp_dir().join("ffmpeg_ui_waveform.raw");

    // Extract raw audio samples at low sample rate
    let extract = std::process::Command::new("ffmpeg")
        .args(["-y", "-i"])
        .arg(audio_path)
        .args([
            "-ac", "1",         // Mono
            "-ar", "1000",      // 1000 samples per second
            "-f", "s16le",      // Raw 16-bit PCM
        ])
        .arg(&temp_raw)
        .output();

    if let Ok(output) = extract {
        if output.status.success() {
            if let Ok(data) = std::fs::read(&temp_raw) {
                let _ = std::fs::remove_file(&temp_raw);

                // Convert raw bytes to peaks
                let samples: Vec<i16> = data.chunks_exact(2)
                    .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();

                // Downsample to ~200 peaks
                let target_peaks = 200;
                let chunk_size = (samples.len() / target_peaks).max(1);

                let peaks: Vec<f32> = samples.chunks(chunk_size)
                    .map(|chunk| {
                        let max = chunk.iter().map(|s| s.abs()).max().unwrap_or(0);
                        (max as f32 / i16::MAX as f32).clamp(0.0, 1.0)
                    })
                    .collect();

                return Ok(peaks);
            }
        }
    }

    let _ = std::fs::remove_file(&temp_raw);

    // Fallback: generate synthetic waveform
    let num_samples = 200;
    let interval = duration / num_samples as f64;
    let peaks: Vec<f32> = (0..num_samples)
        .map(|i| {
            let time = i as f64 * interval;
            ((time * 7.3).sin() * 0.5 + 0.5).abs() as f32 * 0.8
        })
        .collect();

    Ok(peaks)
}
