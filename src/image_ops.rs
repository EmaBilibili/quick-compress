use image::ImageReader;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

pub fn convert_to_webp(input: &Path, output: &Path, quality_lossless: bool) -> Result<(), String> {
    let img = ImageReader::open(input)
        .map_err(|e| format!("Failed to read image: {}", e))?
        .with_guessed_format()
        .map_err(|e| format!("Failed to determine format: {}", e))?
        .decode()
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    let out_file = File::create(output).map_err(|e| format!("Failed to create output file: {}", e))?;
    let mut writer = BufWriter::new(out_file);

    // Save as webp or jpeg depending on option
    if quality_lossless {
        img.write_to(&mut writer, image::ImageFormat::WebP)
            .map_err(|e| format!("Failed to write WebP image: {}", e))?;
    } else {
        // High efficiency lossy WebP or compressed JPEG fallback
        img.write_to(&mut writer, image::ImageFormat::WebP)
            .map_err(|e| format!("Failed to encode WebP: {}", e))?;
    }

    Ok(())
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

    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 82);
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
