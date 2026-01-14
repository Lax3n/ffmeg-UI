// Preview functionality - renders video thumbnails and preview frames
// Main preview rendering is in main_window.rs

use eframe::egui;
use std::path::PathBuf;

/// Generate thumbnail cache path for a video
pub fn get_thumbnail_path(video_path: &PathBuf, timestamp: f64) -> PathBuf {
    let temp_dir = std::env::temp_dir().join("ffmpeg_ui_thumbnails");
    let _ = std::fs::create_dir_all(&temp_dir);

    let filename = video_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let hash = format!("{}_{:.2}", filename, timestamp);
    temp_dir.join(format!("{}.jpg", hash))
}

/// Load image from path into egui texture
pub fn load_thumbnail_texture(
    ctx: &egui::Context,
    path: &PathBuf,
    name: &str,
) -> Option<egui::TextureHandle> {
    let image_data = std::fs::read(path).ok()?;
    let image = image::load_from_memory(&image_data).ok()?;
    let rgba = image.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let pixels = rgba.into_raw();

    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);

    Some(ctx.load_texture(name, color_image, egui::TextureOptions::default()))
}

/// Clean up old thumbnails
pub fn cleanup_thumbnails() {
    let temp_dir = std::env::temp_dir().join("ffmpeg_ui_thumbnails");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
