use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;
use parking_lot::Mutex;

/// Audio player using rodio for playback.
/// Audio extraction happens in the background — playback starts once ready.
pub struct AudioPlayer {
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
    sink: Arc<Sink>,
    temp_audio_path: Arc<Mutex<Option<PathBuf>>>,
    volume: Arc<Mutex<f32>>,
}

impl AudioPlayer {
    /// Create a new audio player. Audio extraction runs in a background thread
    /// so the caller is NOT blocked.
    pub fn new(video_path: &PathBuf, _duration: f64) -> Result<Self, String> {
        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| format!("Failed to initialize audio output: {}", e))?;

        let sink = Sink::try_new(&stream_handle)
            .map_err(|e| format!("Failed to create audio sink: {}", e))?;

        let temp_audio_path: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));

        // Extract audio in background thread — non-blocking
        let path = video_path.clone();
        let slot = temp_audio_path.clone();
        std::thread::spawn(move || {
            if let Ok(temp_path) = extract_audio_to_temp(&path) {
                // Only store if the file actually exists (extraction succeeded)
                if temp_path.exists() && std::fs::metadata(&temp_path).map(|m| m.len() > 0).unwrap_or(false) {
                    *slot.lock() = Some(temp_path);
                }
            }
        });

        Ok(Self {
            _stream: stream,
            _stream_handle: stream_handle,
            sink: Arc::new(sink),
            temp_audio_path,
            volume: Arc::new(Mutex::new(1.0)),
        })
    }

    /// Load audio from the temp file into the sink (no-op if not extracted yet)
    fn load_audio(&self) -> Result<(), String> {
        let guard = self.temp_audio_path.lock();
        if let Some(ref temp_path) = *guard {
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

    /// Play audio (no-op if extraction not yet complete)
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

    /// Seek to position (requires reloading audio)
    pub fn seek(&self, time: f64) {
        self.sink.stop();
        self.sink.clear();

        let guard = self.temp_audio_path.lock();
        if let Some(ref temp_path) = *guard {
            if let Ok(file) = File::open(temp_path) {
                if let Ok(source) = Decoder::new(BufReader::new(file)) {
                    let skipped = source.skip_duration(std::time::Duration::from_secs_f64(time));
                    self.sink.append(skipped);
                    self.sink.set_volume(*self.volume.lock());
                }
            }
        }
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        if let Some(ref temp_path) = *self.temp_audio_path.lock() {
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

    let _ = std::fs::remove_file(&temp_path);

    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.args(["-y", "-i"])
        .arg(video_path)
        .args([
            "-vn",
            "-acodec", "pcm_s16le",
            "-ar", "44100",
            "-ac", "2",
        ])
        .arg(&temp_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let output = cmd.output()
        .map_err(|e| format!("Failed to run FFmpeg: {}", e))?;

    if !output.status.success() {
        return Err("FFmpeg audio extraction failed".to_string());
    }

    Ok(temp_path)
}
