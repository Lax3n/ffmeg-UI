//! Persistent FFmpeg decoder with two modes:
//! - **Scrub mode**: Fast single-frame extraction for seeking (debounced)
//! - **Play mode**: Continuous frame output with epoch-based pacing
//!
//! Key optimizations:
//! - `-an -sn` skips audio/subtitle decoding (huge speedup)
//! - `-frames:v 1` for scrub mode (instant frame grab)
//! - Seek debouncing: coalesces rapid seeks during scrubbing
//! - Shared decoder time for A/V sync

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::VideoFrame;

#[derive(Debug)]
pub enum DecoderCommand {
    Seek(f64),
    Play,
    Pause,
    Stop,
}

pub struct StreamDecoder {
    command_tx: Sender<DecoderCommand>,
    current_frame: Arc<Mutex<Option<VideoFrame>>>,
    speed: Arc<Mutex<f32>>,
    /// Shared decoder time — the authoritative video position for A/V sync
    pub decoder_time: Arc<Mutex<f64>>,
}

fn compute_preview_size(src_w: u32, src_h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if src_w <= max_w && src_h <= max_h {
        return (src_w & !1, src_h & !1);
    }
    let scale = f64::min(max_w as f64 / src_w as f64, max_h as f64 / src_h as f64);
    let w = ((src_w as f64 * scale) as u32).max(2) & !1;
    let h = ((src_h as f64 * scale) as u32).max(2) & !1;
    (w, h)
}

impl StreamDecoder {
    pub fn new(path: &PathBuf, width: u32, height: u32, duration: f64, src_fps: f64) -> Result<Self, String> {
        let (command_tx, command_rx) = mpsc::channel();

        let current_frame = Arc::new(Mutex::new(None));
        let speed = Arc::new(Mutex::new(1.0f32));
        let decoder_time = Arc::new(Mutex::new(0.0f64));

        let (preview_width, preview_height) = compute_preview_size(width, height, 640, 360);
        let decode_fps = (src_fps.round() as u32).clamp(24, 30);

        let path_clone = path.clone();
        let current_frame_clone = current_frame.clone();
        let speed_clone = speed.clone();
        let decoder_time_clone = decoder_time.clone();

        thread::spawn(move || {
            decoder_thread(
                path_clone,
                preview_width,
                preview_height,
                duration,
                decode_fps,
                command_rx,
                current_frame_clone,
                speed_clone,
                decoder_time_clone,
            );
        });

        Ok(Self {
            command_tx,
            current_frame,
            speed,
            decoder_time,
        })
    }

    pub fn seek(&self, time: f64) {
        let _ = self.command_tx.send(DecoderCommand::Seek(time));
    }

    pub fn play(&self) {
        let _ = self.command_tx.send(DecoderCommand::Play);
    }

    pub fn pause(&self) {
        let _ = self.command_tx.send(DecoderCommand::Pause);
    }

    pub fn set_speed(&self, speed: f32) {
        *self.speed.lock().unwrap() = speed.clamp(0.25, 4.0);
    }

    pub fn get_frame(&self) -> Option<VideoFrame> {
        self.current_frame.lock().unwrap().clone()
    }

    pub fn get_decoder_time(&self) -> f64 {
        *self.decoder_time.lock().unwrap()
    }
}

impl Drop for StreamDecoder {
    fn drop(&mut self) {
        let _ = self.command_tx.send(DecoderCommand::Stop);
    }
}

// ---- FFmpeg process helpers ----

/// Spawn FFmpeg for continuous playback (no frame limit)
fn spawn_ffmpeg_play(path: &PathBuf, start_time: f64, width: u32, height: u32, fps: u32) -> Option<Child> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-ss", &format!("{:.3}", start_time), "-i"])
        .arg(path)
        .args([
            "-an", "-sn",       // skip audio + subtitles = much faster
            "-vf", &format!("scale={}:{}:flags=fast_bilinear,fps={}", width, height, fps),
            "-f", "rawvideo",
            "-pix_fmt", "rgba",
            "-vsync", "cfr",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    cmd.spawn().ok()
}

/// Spawn FFmpeg for a single frame grab (scrubbing) — ultra fast
fn spawn_ffmpeg_scrub(path: &PathBuf, time: f64, width: u32, height: u32) -> Option<Child> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-ss", &format!("{:.3}", time), "-i"])
        .arg(path)
        .args([
            "-an", "-sn",
            "-frames:v", "1",   // decode only ONE frame
            "-vf", &format!("scale={}:{}:flags=fast_bilinear", width, height),
            "-f", "rawvideo",
            "-pix_fmt", "rgba",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }

    cmd.spawn().ok()
}

fn kill_process(proc: &mut Option<Child>) {
    if let Some(ref mut child) = proc {
        let _ = child.kill();
        let _ = child.wait();
    }
    *proc = None;
}

fn read_one_frame(proc: &mut Child, frame_size: usize, width: u32, height: u32, pts: f64) -> Option<VideoFrame> {
    let stdout = proc.stdout.as_mut()?;
    let mut buf = vec![0u8; frame_size];
    let mut offset = 0;

    while offset < frame_size {
        match stdout.read(&mut buf[offset..]) {
            Ok(0) => return None,
            Ok(n) => offset += n,
            Err(_) => return None,
        }
    }

    Some(VideoFrame {
        data: Arc::new(buf),
        width,
        height,
        pts,
    })
}

/// Debounce seeks: wait up to `delay` for more Seek commands, return the latest.
/// Also handles Play/Pause/Stop that arrive during the wait.
fn debounce_seek(
    command_rx: &Receiver<DecoderCommand>,
    initial_time: f64,
    delay: Duration,
) -> (f64, Option<bool>, bool) {
    let mut final_time = initial_time;
    let mut play_state: Option<bool> = None;
    let deadline = Instant::now() + delay;

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match command_rx.recv_timeout(remaining) {
            Ok(DecoderCommand::Seek(t)) => final_time = t,
            Ok(DecoderCommand::Play) => play_state = Some(true),
            Ok(DecoderCommand::Pause) => play_state = Some(false),
            Ok(DecoderCommand::Stop) => return (final_time, None, true),
            Err(_) => break, // timeout = no more seeks
        }
    }

    (final_time, play_state, false)
}

/// The decoder thread.
fn decoder_thread(
    path: PathBuf,
    width: u32,
    height: u32,
    duration: f64,
    fps: u32,
    command_rx: Receiver<DecoderCommand>,
    current_frame: Arc<Mutex<Option<VideoFrame>>>,
    speed: Arc<Mutex<f32>>,
    decoder_time: Arc<Mutex<f64>>,
) {
    let frame_size = (width * height * 4) as usize;
    let mut current_time: f64 = 0.0;
    let mut is_playing = false;
    let mut play_process: Option<Child> = None;
    let mut playback_epoch: Option<(Instant, f64)> = None;

    // Reusable buffer for frame reads to avoid allocations during playback
    let mut frame_buf = vec![0u8; frame_size];

    loop {
        if is_playing {
            // ---- PLAYBACK MODE ----
            // Check for commands without blocking
            loop {
                match command_rx.try_recv() {
                    Ok(DecoderCommand::Seek(t)) => {
                        let t = t.clamp(0.0, duration);
                        current_time = t;
                        *decoder_time.lock().unwrap() = t;
                        kill_process(&mut play_process);
                        playback_epoch = Some((Instant::now(), t));
                        // Spawn new process at seek position
                        play_process = spawn_ffmpeg_play(&path, t, width, height, fps);
                    }
                    Ok(DecoderCommand::Play) => {} // already playing
                    Ok(DecoderCommand::Pause) => {
                        is_playing = false;
                        playback_epoch = None;
                        kill_process(&mut play_process);
                        break;
                    }
                    Ok(DecoderCommand::Stop) => {
                        kill_process(&mut play_process);
                        return;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        kill_process(&mut play_process);
                        return;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                }
            }

            if !is_playing {
                continue; // was paused in the command loop
            }

            // Ensure we have a process
            if play_process.is_none() {
                playback_epoch = Some((Instant::now(), current_time));
                play_process = spawn_ffmpeg_play(&path, current_time, width, height, fps);
            }

            if let Some(ref mut child) = play_process {
                // Read frame directly into reusable buffer
                let frame = read_frame_into_buf(child, &mut frame_buf, frame_size, width, height, current_time);
                match frame {
                    Some(f) => {
                        *current_frame.lock().unwrap() = Some(f);
                        current_time += 1.0 / fps as f64;
                        *decoder_time.lock().unwrap() = current_time;

                        // Frame pacing
                        let spd = *speed.lock().unwrap();
                        if let Some((epoch_wall, epoch_time)) = playback_epoch {
                            let wall_target = epoch_wall
                                + Duration::from_secs_f64((current_time - epoch_time) / spd as f64);
                            let now = Instant::now();
                            if now < wall_target {
                                // Sleep but check for commands every 5ms
                                let mut remaining = wall_target - now;
                                while remaining > Duration::ZERO {
                                    let chunk = remaining.min(Duration::from_millis(5));
                                    thread::sleep(chunk);
                                    // Quick check for stop/seek/pause
                                    match command_rx.try_recv() {
                                        Ok(DecoderCommand::Stop) => {
                                            kill_process(&mut play_process);
                                            return;
                                        }
                                        Ok(DecoderCommand::Pause) => {
                                            is_playing = false;
                                            playback_epoch = None;
                                            kill_process(&mut play_process);
                                            break;
                                        }
                                        Ok(DecoderCommand::Seek(t)) => {
                                            let t = t.clamp(0.0, duration);
                                            current_time = t;
                                            *decoder_time.lock().unwrap() = t;
                                            kill_process(&mut play_process);
                                            playback_epoch = Some((Instant::now(), t));
                                            play_process = spawn_ffmpeg_play(&path, t, width, height, fps);
                                            break;
                                        }
                                        _ => {}
                                    }
                                    remaining = wall_target.saturating_duration_since(Instant::now());
                                }
                            }
                        }

                        // End of video
                        if current_time >= duration {
                            is_playing = false;
                            playback_epoch = None;
                            kill_process(&mut play_process);
                        }
                    }
                    None => {
                        is_playing = false;
                        playback_epoch = None;
                        kill_process(&mut play_process);
                    }
                }
            } else {
                is_playing = false;
                playback_epoch = None;
            }
        } else {
            // ---- IDLE / SCRUB MODE ----
            // Block on next command (no CPU burn)
            match command_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(DecoderCommand::Seek(t)) => {
                    // Debounce: wait 15ms for more seeks (scrubbing)
                    let (final_t, play_cmd, stop) =
                        debounce_seek(&command_rx, t, Duration::from_millis(15));

                    if stop { return; }

                    let t = final_t.clamp(0.0, duration);
                    current_time = t;
                    *decoder_time.lock().unwrap() = t;

                    // Fast single-frame grab
                    if let Some(mut child) = spawn_ffmpeg_scrub(&path, t, width, height) {
                        if let Some(frame) = read_one_frame(&mut child, frame_size, width, height, t) {
                            *current_frame.lock().unwrap() = Some(frame);
                        }
                        kill_process(&mut Some(child));
                    }

                    // If Play was received during debounce, start playing
                    if play_cmd == Some(true) {
                        is_playing = true;
                        playback_epoch = Some((Instant::now(), current_time));
                    }
                }
                Ok(DecoderCommand::Play) => {
                    is_playing = true;
                    playback_epoch = Some((Instant::now(), current_time));
                }
                Ok(DecoderCommand::Pause) => {} // already paused
                Ok(DecoderCommand::Stop) => { return; }
                Err(mpsc::RecvTimeoutError::Disconnected) => { return; }
                Err(mpsc::RecvTimeoutError::Timeout) => {} // idle
            }
        }
    }
}

/// Read a frame directly into a pre-allocated buffer (avoids allocation per frame during playback)
fn read_frame_into_buf(proc: &mut Child, buf: &mut [u8], frame_size: usize, width: u32, height: u32, pts: f64) -> Option<VideoFrame> {
    let stdout = proc.stdout.as_mut()?;
    let mut offset = 0;

    while offset < frame_size {
        match stdout.read(&mut buf[offset..frame_size]) {
            Ok(0) => return None,
            Ok(n) => offset += n,
            Err(_) => return None,
        }
    }

    Some(VideoFrame {
        data: Arc::new(buf[..frame_size].to_vec()),
        width,
        height,
        pts,
    })
}
