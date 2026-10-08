use std::path::{Path, PathBuf};
use std::process::Command;

pub struct VideoInfo {
    pub duration_seconds: f64,
}

pub fn probe_video_duration(input: &Path) -> Result<f64, String> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(input)
        .output()
        .map_err(|e| format!("Failed to run ffprobe: {}", e))?;

    if !output.status.success() {
        return Err("Could not detect video duration".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let duration: f64 = stdout
        .trim()
        .parse()
        .map_err(|_| "Failed to parse duration".to_string())?;

    Ok(duration)
}

pub fn compress_video_target_mb(
    input: &Path,
    output: &Path,
    target_mb: f64,
) -> Result<(), String> {
    let duration = probe_video_duration(input).unwrap_or(0.0);

    // If duration can be determined, calculate optimal bitrate:
    // target_bits = target_mb * 8 * 1024 * 1024 * 0.92 (safety margin)
    // total_bitrate = target_bits / duration
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-i"]).arg(input);

    if duration > 1.0 {
        let total_bitrate_kbps = ((target_mb * 8192.0 * 0.92) / duration).floor() as u64;
        let audio_bitrate_kbps = 96.min(total_bitrate_kbps.saturating_sub(64) / 4);
        let video_bitrate_kbps = total_bitrate_kbps.saturating_sub(audio_bitrate_kbps).max(100);

        cmd.args([
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-b:v",
            &format!("{}k", video_bitrate_kbps),
            "-maxrate",
            &format!("{}k", (video_bitrate_kbps as f64 * 1.3) as u64),
            "-bufsize",
            &format!("{}k", video_bitrate_kbps * 2),
            "-vf",
            "scale='min(1280,iw)':-2", // downscale to 720p if higher
            "-c:a",
            "aac",
            "-b:a",
            &format!("{}k", audio_bitrate_kbps),
            "-movflags",
            "+faststart",
        ]);
    } else {
        // Fallback with CRF
        cmd.args([
            "-c:v",
            "libx264",
            "-crf",
            "28",
            "-preset",
            "fast",
            "-vf",
            "scale='min(1280,iw)':-2",
            "-c:a",
            "aac",
            "-b:a",
            "96k",
            "-movflags",
            "+faststart",
        ]);
    }

    cmd.arg(output);

    let status = cmd
        .status()
        .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err("FFmpeg encoding failed".to_string())
    }
}

pub fn generate_output_path(input: &Path, suffix: &str, ext: &str) -> PathBuf {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("compressed");

    let filename = format!("{}_{}.{}", stem, suffix, ext);
    parent.join(filename)
}
