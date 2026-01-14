#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod ffmpeg;
mod player;
mod project;
mod ui;
mod utils;

use app::FFmpegApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("FFmpeg UI"),
        ..Default::default()
    };

    eframe::run_native(
        "FFmpeg UI",
        options,
        Box::new(|cc| Ok(Box::new(FFmpegApp::new(cc)))),
    )
}
