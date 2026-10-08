use std::path::{Path, PathBuf};
use std::process::Command;

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

/// Extract a single frame at seconds timestamp to a destination image file
pub fn extract_video_frame(input: &Path, seconds: f64, output: &Path) -> Result<(), String> {
    let output_cmd = Command::new("ffmpeg")
        .args([
            "-y",
            "-ss",
            &format!("{:.2}", seconds.max(0.0)),
            "-i",
        ])
        .arg(input)
        .args([
            "-vframes",
            "1",
            "-q:v",
            "2",
            "-update",
            "1",
        ])
        .arg(output)
        .output()
        .map_err(|e| format!("Failed to run ffmpeg frame extraction: {}", e))?;

    if !output_cmd.status.success() {
        return Err("Failed to extract preview frame from video".to_string());
    }

    Ok(())
}

pub fn compress_video_target_mb(
    input: &Path,
    output: &Path,
    target_mb: f64,
    start_sec: Option<f64>,
    end_sec: Option<f64>,
) -> Result<(), String> {
    let full_duration = probe_video_duration(input).unwrap_or(0.0);

    let start = start_sec.unwrap_or(0.0);
    let end = end_sec.unwrap_or(full_duration);
    let effective_duration = if end > start {
        end - start
    } else {
        full_duration
    };

    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-y");

    // Fast seek before input if trimming start
    if start > 0.0 {
        cmd.args(["-ss", &format!("{:.2}", start)]);
    }

    cmd.args(["-i"]).arg(input);

    // End time
    if end > start && end < full_duration {
        cmd.args(["-to", &format!("{:.2}", end)]);
    }

    if effective_duration > 1.0 {
        let total_bitrate_kbps = ((target_mb * 8192.0 * 0.92) / effective_duration).floor() as u64;
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

pub fn generate_output_path(
    input: &Path,
    custom_dir: Option<&Path>,
    suffix: &str,
    ext: &str,
) -> PathBuf {
    let parent = custom_dir.unwrap_or_else(|| input.parent().unwrap_or_else(|| Path::new(".")));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("compressed");

    let filename = format!("{}_{}.{}", stem, suffix, ext);
    parent.join(filename)
}
