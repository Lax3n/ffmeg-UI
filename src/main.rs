#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod export_queue;
mod ffmpeg;
mod player;
mod project;
mod ui;
mod utils;

use app::FFmpegApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    // Charge un éventuel fichier .env (ex: FFMPEG_BIN / FFPROBE_BIN) avant que
    // la résolution des binaires ffmpeg ne lise l'environnement. Absent = ignoré.
    let _ = dotenvy::dotenv();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("FFmpeg Studio"),
        ..Default::default()
    };

    eframe::run_native(
        "FFmpeg Studio",
        options,
        Box::new(|cc| Ok(Box::new(FFmpegApp::new(cc)))),
    )
}
