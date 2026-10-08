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

/// Strip all EXIF / GPS / camera metadata by decoding raw pixel buffer and writing clean file
pub fn strip_metadata(input: &Path, output: &Path) -> Result<(), String> {
    let reader = ImageReader::open(input)
        .map_err(|e| format!("Failed to read image: {}", e))?
        .with_guessed_format()
        .map_err(|e| format!("Failed to detect format: {}", e))?;

    let format = reader.format().unwrap_or(image::ImageFormat::Jpeg);
    let img = reader
        .decode()
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    let out_file = File::create(output).map_err(|e| format!("Failed to create output file: {}", e))?;
    let mut writer = BufWriter::new(out_file);

    img.write_to(&mut writer, format)
        .map_err(|e| format!("Failed to save clean image: {}", e))?;

    Ok(())
}

/// Scale image by percentage (e.g. 50%) or fit into square avatar box (e.g. 512x512)
pub fn resize_image(
    input: &Path,
    output: &Path,
    scale_factor: Option<f32>,
    exact_box: Option<(u32, u32)>,
) -> Result<(), String> {
    let reader = ImageReader::open(input)
        .map_err(|e| format!("Failed to read image: {}", e))?
        .with_guessed_format()
        .map_err(|e| format!("Failed to detect format: {}", e))?;

    let format = reader.format().unwrap_or(image::ImageFormat::Png);
    let img = reader
        .decode()
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    let processed = if let Some(factor) = scale_factor {
        let new_w = ((img.width() as f32 * factor).round() as u32).max(1);
        let new_h = ((img.height() as f32 * factor).round() as u32).max(1);
        img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3)
    } else if let Some((target_w, target_h)) = exact_box {
        img.resize_to_fill(target_w, target_h, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let out_file = File::create(output).map_err(|e| format!("Failed to create output file: {}", e))?;
    let mut writer = BufWriter::new(out_file);

    processed
        .write_to(&mut writer, format)
        .map_err(|e| format!("Failed to write resized image: {}", e))?;

    Ok(())
}

pub fn generate_image_output_path(
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
