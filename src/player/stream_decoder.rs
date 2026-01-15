//! Persistent FFmpeg decoder that streams frames continuously
//! Much faster than spawning a new process for each frame

use std::io::{BufRead, BufReader, Read, Write};
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

        // Preview at 480p max for performance
        let preview_width = width.min(854);
        let preview_height = height.min(480);

        // Start decoder thread
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
        // Try to get latest frame from channel
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

/// The decoder thread that manages the FFmpeg process
fn decoder_thread(
    path: PathBuf,
    width: u32,
    height: u32,
    _duration: f64,
    command_rx: Receiver<DecoderCommand>,
    frame_tx: Sender<VideoFrame>,
    current_frame: Arc<Mutex<Option<VideoFrame>>>,
    is_running: Arc<Mutex<bool>>,
) {
    let frame_size = (width * height * 4) as usize;
    let mut current_time = 0.0;
    let mut is_playing = false;

    loop {
        // Check for commands (non-blocking)
        match command_rx.try_recv() {
            Ok(DecoderCommand::Seek(time)) => {
                current_time = time;
                // Extract frame at seek position
                if let Ok(frame) = extract_single_frame(&path, time, width, height, frame_size) {
                    *current_frame.lock().unwrap() = Some(frame.clone());
                    let _ = frame_tx.send(frame);
                }
            }
            Ok(DecoderCommand::Play) => {
                is_playing = true;
            }
            Ok(DecoderCommand::Pause) => {
                is_playing = false;
            }
            Ok(DecoderCommand::Stop) => {
                *is_running.lock().unwrap() = false;
                break;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                break;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }

        // If playing, extract next frame
        if is_playing {
            if let Ok(frame) = extract_single_frame(&path, current_time, width, height, frame_size) {
                *current_frame.lock().unwrap() = Some(frame.clone());
                let _ = frame_tx.send(frame);
                current_time += 0.1; // Advance time
            }
            thread::sleep(std::time::Duration::from_millis(80)); // ~12 fps
        } else {
            thread::sleep(std::time::Duration::from_millis(16)); // Check commands at 60fps
        }
    }
}

/// Extract a single frame using FFmpeg with rawvideo pipe
fn extract_single_frame(
    path: &PathBuf,
    time: f64,
    width: u32,
    height: u32,
    expected_size: usize,
) -> Result<VideoFrame, String> {
    let output = Command::new("ffmpeg")
        .args([
            "-ss", &format!("{:.3}", time),
            "-i",
        ])
        .arg(path)
        .args([
            "-vframes", "1",
            "-vf", &format!("scale={}:{}", width, height),
            "-f", "rawvideo",
            "-pix_fmt", "rgba",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;

    if output.stdout.len() != expected_size {
        return Err("Invalid frame size".to_string());
    }

    Ok(VideoFrame {
        data: output.stdout,
        width,
        height,
        pts: time,
    })
}
