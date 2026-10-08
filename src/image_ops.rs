use image::ImageReader;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Convert to WebP using FFmpeg libwebp encoder for true lossy / lossless control.
/// If lossless = false, uses -q:v 78 (typical 40-70% size reduction with visually indistinguishable quality).
pub fn convert_to_webp(input: &Path, output: &Path, quality_lossless: bool) -> Result<(), String> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-y", "-i"]).arg(input);

    if quality_lossless {
        cmd.args(["-c:v", "libwebp", "-lossless", "1", "-compression_level", "6"]);
    } else {
        // High efficiency lossy WebP at quality 78 with photo preset
        cmd.args([
            "-c:v",
            "libwebp",
            "-lossless",
            "0",
            "-q:v",
            "78",
            "-preset",
            "photo",
            "-compression_level",
            "4",
        ]);
    }

    cmd.arg(output);

    let status = cmd
        .status()
        .map_err(|e| format!("Failed to execute ffmpeg for WebP: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err("FFmpeg WebP encoding failed".to_string())
    }
}

pub fn optimize_jpeg(input: &Path, output: &Path, max_dimension: u32) -> Result<(), String> {
    let img = ImageReader::open(input)
        .map_err(|e| format!("Failed to read image: {}", e))?
        .with_guessed_format()
        .map_err(|e| format!("Failed to detect format: {}", e))?
        .decode()
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    let processed = if img.width() > max_dimension || img.height() > max_dimension {
        img.resize(
            max_dimension,
            max_dimension,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };

    let rgb = processed.to_rgb8();
    let out_file = File::create(output).map_err(|e| format!("Failed to create output file: {}", e))?;
    let mut writer = BufWriter::new(out_file);

    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 80);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| format!("Failed to encode JPEG: {}", e))?;

    Ok(())
}

pub fn generate_image_output_path(input: &Path, suffix: &str, ext: &str) -> PathBuf {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("compressed");

    let filename = format!("{}_{}.{}", stem, suffix, ext);
    parent.join(filename)
}
