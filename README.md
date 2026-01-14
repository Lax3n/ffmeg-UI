# FFmpeg UI

A lightweight video editing UI for FFmpeg built with Rust and egui.

![Rust](https://img.shields.io/badge/Rust-1.70+-orange.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)

## Features

- **Format Conversion**: Convert videos between formats (MP4, MKV, AVI, MOV, WebM, etc.)
- **Trim/Cut**: Extract segments from videos with precise in/out points
- **Crop**: Crop video dimensions with visual preview
- **Concatenate**: Merge multiple video files into one
- **Filters**: Apply video filters (brightness, contrast, saturation, rotation, speed)
- **Video Preview**: Real-time video playback with audio
- **Timeline**: Professional editing timeline with waveform visualization
- **Keyboard Shortcuts**: Efficient editing with standard keyboard controls

## Requirements

- **Rust** 1.70 or higher
- **FFmpeg** installed and available in PATH
  - Windows: Download from [gyan.dev](https://www.gyan.dev/ffmpeg/builds/) or install via `winget install FFmpeg`
  - macOS: `brew install ffmpeg`
  - Linux: `sudo apt install ffmpeg` or equivalent

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/yourusername/ffmpeg_ui.git
cd ffmpeg_ui

# Build in release mode
cargo build --release

# Run the application
cargo run --release
```

The compiled binary will be in `target/release/ffmpeg_ui.exe` (Windows) or `target/release/ffmpeg_ui` (Linux/macOS).

## Usage

### Basic Workflow

1. **Add Files**: Click "Add Files" or drag and drop media files
2. **Select Tool**: Choose from Convert, Trim, Crop, Concat, or Filters
3. **Configure Settings**: Adjust parameters for the selected tool
4. **Preview**: Use the video player to preview your changes
5. **Export**: Click the action button to process the video

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Space` | Play/Pause |
| `←` / `→` | Seek -5s / +5s |
| `J` / `L` | Seek -10s / +10s |
| `K` | Pause |
| `Home` | Go to start |
| `End` | Go to end |
| `I` | Set In point |
| `O` | Set Out point |

### Tools

#### Convert
Convert videos to different formats with customizable settings:
- Output format (MP4, MKV, AVI, MOV, WebM)
- Video codec (H.264, H.265, VP9, copy)
- Audio codec (AAC, MP3, Opus, copy)
- Quality/bitrate settings

#### Trim
Extract a segment from a video:
- Set start and end times manually or using I/O points
- Option to copy codec (faster) or re-encode

#### Crop
Crop video dimensions:
- Set X, Y offset and width, height
- Visual preview of crop area

#### Concat
Merge multiple files:
- Add files in desired order
- All files should have compatible codecs

#### Filters
Apply video filters:
- Brightness adjustment
- Contrast adjustment
- Saturation adjustment
- Rotation (90°, 180°, 270°)
- Playback speed (0.5x - 2x)

## Project Structure

```
ffmpeg_ui/
├── Cargo.toml          # Dependencies and project config
├── src/
│   ├── main.rs         # Application entry point
│   ├── app.rs          # Main application state and logic
│   ├── ffmpeg/         # FFmpeg wrapper module
│   │   ├── mod.rs
│   │   ├── wrapper.rs  # FFmpeg command execution
│   │   ├── probe.rs    # Media file probing
│   │   ├── commands.rs # FFmpeg command building
│   │   └── progress.rs # Progress parsing
│   ├── player/         # Media player module
│   │   ├── mod.rs      # Video playback
│   │   ├── audio_player.rs  # Audio playback with rodio
│   │   ├── sync.rs     # A/V synchronization
│   │   └── video_decoder.rs
│   ├── project/        # Project management
│   │   ├── mod.rs
│   │   ├── media.rs    # Media file handling
│   │   ├── timeline.rs # Timeline data structures
│   │   └── export.rs   # Export settings
│   ├── ui/             # User interface
│   │   ├── mod.rs
│   │   ├── main_window.rs   # Main UI layout
│   │   ├── timeline_widget.rs  # Timeline with waveform
│   │   └── tools.rs    # Tool settings UI
│   └── utils/          # Utilities
│       ├── mod.rs
│       └── time.rs     # Time formatting
```

## Dependencies

- [eframe](https://github.com/emilk/egui) - Immediate mode GUI framework
- [egui](https://github.com/emilk/egui) - GUI library
- [rodio](https://github.com/RustAudio/rodio) - Audio playback
- [tokio](https://tokio.rs/) - Async runtime
- [image](https://github.com/image-rs/image) - Image processing
- [rfd](https://github.com/PolyMeilex/rfd) - Native file dialogs
- [serde](https://serde.rs/) - Serialization

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## Acknowledgments

- [FFmpeg](https://ffmpeg.org/) - The powerful multimedia framework
- [egui](https://github.com/emilk/egui) - The amazing immediate mode GUI library
