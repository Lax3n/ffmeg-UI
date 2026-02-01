//! Persistent FFmpeg decoder that streams frames continuously via stdout pipe.
//! One FFmpeg process runs at a time; on seek we kill+respawn at the new position.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use super::VideoFrame;

/// Commands sent to the decoder thread
#[derive(Debug)]
pub enum DecoderCommand {
    Seek(f64),
    Play,
    Pause,
    Stop,
}

/// A persistent FFmpeg decoder that keeps a process running
pub struct StreamDecoder {
    command_tx: Sender<DecoderCommand>,
    frame_rx: Arc<Mutex<Receiver<VideoFrame>>>,
    current_frame: Arc<Mutex<Option<VideoFrame>>>,
    is_running: Arc<Mutex<bool>>,
    width: u32,
    height: u32,
}

impl StreamDecoder {
    /// Create a new stream decoder for the given video file
    pub fn new(path: &PathBuf, width: u32, height: u32, duration: f64) -> Result<Self, String> {
        let (command_tx, command_rx) = mpsc::channel();
        let (frame_tx, frame_rx) = mpsc::channel();

        let current_frame = Arc::new(Mutex::new(None));
        let is_running = Arc::new(Mutex::new(true));

        // Preview at 640x360 max for performance
        let preview_width = width.min(640);
        let preview_height = height.min(360);

        let path_clone = path.clone();
        let current_frame_clone = current_frame.clone();
        let is_running_clone = is_running.clone();

        thread::spawn(move || {
            decoder_thread(
                path_clone,
                preview_width,
                preview_height,
                duration,
                command_rx,
                frame_tx,
                current_frame_clone,
                is_running_clone,
            );
        });

        Ok(Self {
            command_tx,
            frame_rx: Arc::new(Mutex::new(frame_rx)),
            current_frame,
            is_running,
            width: preview_width,
            height: preview_height,
        })
    }

    /// Seek to a specific time
    pub fn seek(&self, time: f64) {
        let _ = self.command_tx.send(DecoderCommand::Seek(time));
    }

    /// Start playback
    pub fn play(&self) {
        let _ = self.command_tx.send(DecoderCommand::Play);
    }

    /// Pause playback
    pub fn pause(&self) {
        let _ = self.command_tx.send(DecoderCommand::Pause);
    }

    /// Get the current frame
    pub fn get_frame(&self) -> Option<VideoFrame> {
        // Drain all available frames, keep the latest
        if let Ok(rx) = self.frame_rx.lock() {
            while let Ok(frame) = rx.try_recv() {
                *self.current_frame.lock().unwrap() = Some(frame);
            }
        }
        self.current_frame.lock().unwrap().clone()
    }

    /// Check if decoder is still running
    pub fn is_running(&self) -> bool {
        *self.is_running.lock().unwrap()
    }
}

impl Drop for StreamDecoder {
    fn drop(&mut self) {
        let _ = self.command_tx.send(DecoderCommand::Stop);
        *self.is_running.lock().unwrap() = false;
    }
}

// ---- Internal helpers ----

/// Spawn a persistent FFmpeg process that outputs raw RGBA frames to stdout
fn spawn_ffmpeg(path: &PathBuf, start_time: f64, width: u32, height: u32, fps: u32) -> Option<Child> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-ss", &format!("{:.3}", start_time),
        "-i",
    ])
    .arg(path)
    .args([
        "-vf", &format!("scale={}:{},fps={}", width, height, fps),
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
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd.spawn().ok()
}

/// Kill a process cleanly (kill + wait to avoid zombies)
fn kill_process(proc: &mut Option<Child>) {
    if let Some(ref mut child) = proc {
        let _ = child.kill();
        let _ = child.wait();
    }
    *proc = None;
}

/// Read exactly one frame (width*height*4 bytes) from the process stdout.
/// Handles partial reads by looping until the buffer is full.
fn read_one_frame(
    proc: &mut Child,
    width: u32,
    height: u32,
    frame_size: usize,
    pts: f64,
) -> Option<VideoFrame> {
    let stdout = proc.stdout.as_mut()?;
    let mut buf = vec![0u8; frame_size];
    let mut offset = 0;

    while offset < frame_size {
        match stdout.read(&mut buf[offset..]) {
            Ok(0) => return None, // EOF — process ended
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

/// The decoder thread — runs a persistent FFmpeg process
fn decoder_thread(
    path: PathBuf,
    width: u32,
    height: u32,
    duration: f64,
    command_rx: Receiver<DecoderCommand>,
    frame_tx: Sender<VideoFrame>,
    current_frame: Arc<Mutex<Option<VideoFrame>>>,
    is_running: Arc<Mutex<bool>>,
) {
    let frame_size = (width * height * 4) as usize;
    let fps: u32 = 15;
    let mut current_time: f64 = 0.0;
    let mut is_playing = false;
    let mut process: Option<Child> = None;

    loop {
        // Drain all pending commands (non-blocking), coalescing multiple seeks
        let mut last_seek: Option<f64> = None;
        loop {
            match command_rx.try_recv() {
                Ok(DecoderCommand::Seek(time)) => {
                    // Only keep the last seek — avoids spawning FFmpeg for every intermediate position
                    last_seek = Some(time);
                }
                Ok(DecoderCommand::Play) => {
                    is_playing = true;
                }
                Ok(DecoderCommand::Pause) => {
                    is_playing = false;
                    kill_process(&mut process);
                }
                Ok(DecoderCommand::Stop) => {
                    kill_process(&mut process);
                    *is_running.lock().unwrap() = false;
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    kill_process(&mut process);
                    *is_running.lock().unwrap() = false;
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }

        // Process only the last seek (all intermediate ones are skipped)
        if let Some(time) = last_seek {
            let t = time.clamp(0.0, duration);
            current_time = t;

            kill_process(&mut process);
            if let Some(mut child) = spawn_ffmpeg(&path, t, width, height, fps) {
                if let Some(frame) = read_one_frame(&mut child, width, height, frame_size, t) {
                    *current_frame.lock().unwrap() = Some(frame.clone());
                    let _ = frame_tx.send(frame);
                }
                if !is_playing {
                    kill_process(&mut Some(child));
                } else {
                    process = Some(child);
                }
            }
        }

        if is_playing {
            // Spawn process if needed
            if process.is_none() {
                process = spawn_ffmpeg(&path, current_time, width, height, fps);
            }

            if let Some(ref mut child) = process {
                // Read the next frame — FFmpeg's fps filter does rate limiting
                match read_one_frame(child, width, height, frame_size, current_time) {
                    Some(frame) => {
                        *current_frame.lock().unwrap() = Some(frame.clone());
                        let _ = frame_tx.send(frame);
                        current_time += 1.0 / fps as f64;

                        // End of video
                        if current_time >= duration {
                            is_playing = false;
                            kill_process(&mut process);
                        }
                    }
                    None => {
                        // EOF or error — stop playback
                        is_playing = false;
                        kill_process(&mut process);
                    }
                }
            } else {
                // Failed to spawn — stop
                is_playing = false;
            }
        } else {
            // Idle — sleep briefly and check commands
            thread::sleep(std::time::Duration::from_millis(16));
        }
    }
}
