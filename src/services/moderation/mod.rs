//! Image content moderation via AWS Rekognition.
//!
//! Stateless (no postgres/redis/r2) — AWS config is loaded from env on every
//! call, same as `R2Client`/`MetadataService` do for their own AWS clients.
//! Callers decide the *policy* for a positive result. `/metadata/image` caches
//! it as an `is_nsfw` flag and still uploads (`services::metadata`). This module
//! only answers the judgment question.

use std::env;
use std::io::Cursor;
use std::time::Instant;

use aws_config::{BehaviorVersion, Region};
use aws_sdk_rekognition::Client;
use aws_sdk_rekognition::primitives::Blob;
use aws_sdk_rekognition::types::{Image, ModerationLabel};
use bytes::Bytes;
use image::{GenericImageView, ImageFormat, imageops::FilterType};
use resvg::usvg;
use tiny_skia::Pixmap;
use tracing::info;

use crate::result::AppError;

/// Convert SVG to PNG (Rekognition doesn't accept SVG directly).
fn convert_svg_to_png(
    svg_data: &[u8],
    max_width: u32,
    max_height: u32,
) -> Result<Vec<u8>, AppError> {
    info!("🎨 Starting SVG to PNG conversion");
    let start_time = Instant::now();

    let opts = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg_data, &opts)
        .map_err(|e| AppError::BadRequest(format!("Failed to parse SVG: {}", e)))?;

    let svg_size = tree.size();
    info!("📐 SVG size: {}x{}", svg_size.width(), svg_size.height());

    let scale = (max_width as f32 / svg_size.width())
        .min(max_height as f32 / svg_size.height())
        .min(1.0);
    let target_width = (svg_size.width() * scale) as u32;
    let target_height = (svg_size.height() * scale) as u32;

    info!("🔄 Rendering SVG to {}x{}", target_width, target_height);

    let mut pixmap = Pixmap::new(target_width, target_height)
        .ok_or_else(|| AppError::InternalError("Failed to create pixmap".to_string()))?;

    let tree_size = tree.size().to_int_size();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(
            target_width as f32 / tree_size.width() as f32,
            target_height as f32 / tree_size.height() as f32,
        ),
        &mut pixmap.as_mut(),
    );

    let png_data = pixmap
        .encode_png()
        .map_err(|e| AppError::InternalError(format!("Failed to encode PNG: {}", e)))?;

    let conversion_time = start_time.elapsed();
    info!(
        "✅ SVG to PNG conversion completed - Time: {:?}, Output size: {} bytes",
        conversion_time,
        png_data.len()
    );

    Ok(png_data)
}

/// Convert any image format to PNG for Rekognition, with optional resize.
fn convert_to_png(image_data: &[u8], max_width: u32, max_height: u32) -> Result<Vec<u8>, AppError> {
    info!("🚀 Starting image conversion process");
    let start_time = Instant::now();

    let img = image::load_from_memory(image_data)
        .map_err(|e| AppError::BadRequest(format!("Failed to decode image: {}", e)))?;

    let (width, height) = img.dimensions();
    info!("📐 Original image size: {}x{}", width, height);

    let processed_img = if width > max_width || height > max_height {
        info!("🔄 Resizing image to fit {}x{}", max_width, max_height);
        img.resize(max_width, max_height, FilterType::Lanczos3)
    } else {
        img
    };

    let mut png_data = Vec::new();
    let mut cursor = Cursor::new(&mut png_data);

    processed_img
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| AppError::InternalError(format!("Failed to convert to PNG: {}", e)))?;

    let conversion_time = start_time.elapsed();
    info!(
        "✅ PNG conversion completed - Time: {:?}, Output size: {} bytes",
        conversion_time,
        png_data.len()
    );

    Ok(png_data)
}

/// Check if an image is NSFW using AWS Rekognition `detect_moderation_labels`.
///
/// `format` is the sniffed MIME type (see `utils::image::sniff_image_format`),
/// used only to pick the PNG conversion path (SVG needs rendering; everything
/// else goes through the generic decoder). Rekognition requires a raster
/// format under 5MB, so the image is always converted to PNG first.
pub async fn check_nsfw(image_data: &[u8], format: &str) -> Result<bool, AppError> {
    info!(
        "🔍 Starting NSFW check - Image size: {} bytes, format: {}",
        image_data.len(),
        format
    );

    let start_conversion = Instant::now();
    let image_data_owned = image_data.to_vec();
    let format_owned = format.to_string();

    let png_data = tokio::task::spawn_blocking(move || {
        if format_owned == "image/svg+xml" {
            convert_svg_to_png(&image_data_owned, 512, 512)
        } else {
            convert_to_png(&image_data_owned, 512, 512)
        }
    })
    .await
    .map_err(|e| AppError::InternalError(format!("Task join error: {}", e)))??;

    info!(
        "⏱️  Image conversion took: {:?}, PNG size: {} bytes",
        start_conversion.elapsed(),
        png_data.len()
    );

    // AWS Rekognition PNG limit is 5MB (5,242,880 bytes)
    const MAX_PNG_SIZE: usize = 5_242_880;
    if png_data.len() > MAX_PNG_SIZE {
        return Err(AppError::BadRequest(format!(
            "Converted PNG size ({} bytes) exceeds AWS Rekognition limit ({} bytes). Please use a smaller image.",
            png_data.len(),
            MAX_PNG_SIZE
        )));
    }

    info!("☁️  Loading AWS configuration");
    let aws_region = env::var("AWS_REGION").expect("AWS_REGION must be set");
    let region = Region::new(aws_region);
    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(region)
        .load()
        .await;

    info!("📡 Creating Rekognition client");
    let client = Client::new(&config);

    let bytes = Bytes::from(png_data);
    let blob = Blob::new(bytes);
    let image = Image::builder().bytes(blob).build();

    info!("🌐 Calling AWS Rekognition API");
    let api_start = Instant::now();
    let resp = client
        .detect_moderation_labels()
        .image(image)
        .min_confidence(10.0)
        .send()
        .await
        .map_err(|e| AppError::InternalError(format!("AWS Rekognition error: {}", e)))?;

    info!("⏱️  Rekognition API took: {:?}", api_start.elapsed());

    let labels = resp.moderation_labels();
    let is_nsfw = is_adult_content(labels);

    info!("✅ NSFW check completed - Result: {}", is_nsfw);

    Ok(is_nsfw)
}

/// 성인물 여부를 판단하는 함수
fn is_adult_content(labels: &[ModerationLabel]) -> bool {
    let adult_categories = [
        ("Explicit", 50.0),
        ("Explicit Nudity", 50.0),
        ("Explicit Sexual Activity", 60.0),
        ("Exposed Buttocks or Anus", 70.0),
        ("Exposed Male Genitalia", 60.0),
        ("Exposed Female Genitalia", 60.0),
        ("Exposed Female Nipple", 80.0),
        ("Non-Explicit Nudity", 95.0),
    ];

    for label in labels {
        let name = label.name().unwrap_or_default();
        let confidence = label.confidence().unwrap_or_default();

        for (category, threshold) in &adult_categories {
            if name.to_lowercase() == category.to_lowercase() && confidence >= *threshold {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(name: &str, confidence: f32) -> ModerationLabel {
        ModerationLabel::builder()
            .name(name)
            .confidence(confidence)
            .build()
    }

    #[test]
    fn empty_labels_is_not_adult_content() {
        assert!(!is_adult_content(&[]));
    }

    #[test]
    fn non_adult_label_does_not_flag_regardless_of_confidence() {
        assert!(!is_adult_content(&[label("Violence", 99.0)]));
    }

    #[test]
    fn each_adult_category_flags_at_and_above_its_threshold() {
        let categories = [
            ("Explicit", 50.0),
            ("Explicit Nudity", 50.0),
            ("Explicit Sexual Activity", 60.0),
            ("Exposed Buttocks or Anus", 70.0),
            ("Exposed Male Genitalia", 60.0),
            ("Exposed Female Genitalia", 60.0),
            ("Exposed Female Nipple", 80.0),
            ("Non-Explicit Nudity", 95.0),
        ];

        for (name, threshold) in categories {
            assert!(
                is_adult_content(&[label(name, threshold)]),
                "{name} should flag at its threshold ({threshold})"
            );
            assert!(
                !is_adult_content(&[label(name, threshold - 0.1)]),
                "{name} should not flag just below its threshold ({threshold})"
            );
        }
    }

    #[test]
    fn label_name_match_is_case_insensitive() {
        assert!(is_adult_content(&[label("EXPLICIT NUDITY", 50.0)]));
        assert!(is_adult_content(&[label("explicit nudity", 50.0)]));
    }

    #[test]
    fn any_matching_label_among_several_flags_the_whole_set() {
        assert!(is_adult_content(&[
            label("Violence", 99.0),
            label("Rude Gestures", 40.0),
            label("Exposed Female Nipple", 85.0),
        ]));
    }

    #[test]
    fn missing_confidence_defaults_to_zero_and_does_not_flag() {
        let unset_confidence = ModerationLabel::builder().name("Explicit Nudity").build();
        assert!(!is_adult_content(&[unset_confidence]));
    }
}
