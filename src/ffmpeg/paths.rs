//! Resolve ffmpeg / ffprobe binaries across platforms.
//!
//! GUI apps launched from Finder on macOS do not inherit the user's shell PATH,
//! so `Command::new("ffmpeg")` fails with ENOENT even when Homebrew has it
//! installed. We search the common install prefixes ourselves, giving
//! Apple Silicon Homebrew priority.

use std::path::PathBuf;
use std::sync::OnceLock;

static FFMPEG_PATH: OnceLock<String> = OnceLock::new();
static FFPROBE_PATH: OnceLock<String> = OnceLock::new();

/// Candidate directories searched on macOS, in order of preference.
/// Apple Silicon Homebrew first, then Intel / MacPorts.
#[cfg(target_os = "macos")]
const MACOS_CANDIDATE_DIRS: &[&str] = &[
    "/opt/homebrew/bin",     // Homebrew on Apple Silicon (M1/M2/M3/M4/M5)
    "/usr/local/bin",         // Homebrew on Intel Macs
    "/opt/local/bin",         // MacPorts
    "/opt/homebrew/opt/ffmpeg/bin",
    "/Applications/ffmpeg",
];

#[cfg(target_os = "linux")]
const UNIX_CANDIDATE_DIRS: &[&str] = &[
    "/usr/local/bin",
    "/usr/bin",
    "/opt/ffmpeg/bin",
    "/snap/bin",
];

fn resolve(binary: &str) -> String {
    // 1. Explicit override via environment variable
    let env_key = if binary == "ffmpeg" { "FFMPEG_BIN" } else { "FFPROBE_BIN" };
    if let Ok(val) = std::env::var(env_key) {
        if !val.is_empty() && PathBuf::from(&val).exists() {
            return val;
        }
    }

    // 2. Platform-specific well-known locations
    #[cfg(target_os = "macos")]
    {
        for dir in MACOS_CANDIDATE_DIRS {
            let candidate = PathBuf::from(dir).join(binary);
            if candidate.exists() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        for dir in UNIX_CANDIDATE_DIRS {
            let candidate = PathBuf::from(dir).join(binary);
            if candidate.exists() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }

    // 3. Fall back to bare name (relies on PATH)
    binary.to_string()
}

/// Absolute path to the ffmpeg binary (or bare "ffmpeg" if nothing found).
pub fn ffmpeg_path() -> &'static str {
    FFMPEG_PATH.get_or_init(|| resolve("ffmpeg"))
}

/// Absolute path to the ffprobe binary (or bare "ffprobe" if nothing found).
pub fn ffprobe_path() -> &'static str {
    FFPROBE_PATH.get_or_init(|| resolve("ffprobe"))
}

/// Returns true if ffmpeg is installed and responds to `-version`.
pub fn is_ffmpeg_available() -> bool {
    let mut cmd = std::process::Command::new(ffmpeg_path());
    cmd.arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    apply_platform_flags(&mut cmd);
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

/// Human-readable hint shown when ffmpeg is not found.
pub fn install_hint() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "FFmpeg introuvable. Installe-le avec Homebrew: `brew install ffmpeg` \
         (ou définis FFMPEG_BIN / FFPROBE_BIN vers les binaires)."
    }
    #[cfg(target_os = "linux")]
    {
        "FFmpeg introuvable. Installe-le via ton gestionnaire de paquets \
         (apt install ffmpeg, dnf install ffmpeg, ...)."
    }
    #[cfg(target_os = "windows")]
    {
        "FFmpeg introuvable. Télécharge-le depuis gyan.dev ou `winget install FFmpeg`."
    }
}

/// Apply platform-specific tweaks to a sync `std::process::Command`.
/// On Windows GUI builds, this hides the console window that would otherwise flash.
pub fn apply_platform_flags(cmd: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

/// Apply platform-specific tweaks to a `tokio::process::Command`.
pub fn apply_platform_flags_tokio(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

/// Build a sync `std::process::Command` for ffmpeg with platform flags applied.
pub fn ffmpeg_command() -> std::process::Command {
    let mut cmd = std::process::Command::new(ffmpeg_path());
    apply_platform_flags(&mut cmd);
    cmd
}

/// Build a sync `std::process::Command` for ffprobe with platform flags applied.
pub fn ffprobe_command() -> std::process::Command {
    let mut cmd = std::process::Command::new(ffprobe_path());
    apply_platform_flags(&mut cmd);
    cmd
}

/// Build a tokio async `Command` for ffmpeg with platform flags applied.
pub fn ffmpeg_command_async() -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(ffmpeg_path());
    apply_platform_flags_tokio(&mut cmd);
    cmd
}
