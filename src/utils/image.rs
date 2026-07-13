//! Image format detection from magic bytes.
//!
//! Callers must never trust a client-supplied `Content-Type` header: it decides
//! what R2 later serves the object as, so a forged one turns an upload endpoint
//! into an arbitrary-content host. Detect the format here and store *that*.

use crate::result::AppError;

/// Detects the image format from `data`'s magic bytes and checks it against
/// `allowed`. Returns the detected MIME type — pass that to R2, not the header.
pub fn sniff_image_format(data: &[u8], allowed: &[&str]) -> Result<&'static str, AppError> {
    if data.len() < 4 {
        return Err(AppError::BadRequest("File too small".to_string()));
    }

    let actual_format = if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        "image/png"
    } else if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        "image/webp"
    } else if data.starts_with(b"<svg") || data.starts_with(b"<?xml") {
        if let Ok(content) = std::str::from_utf8(data) {
            if content.contains("<svg") {
                "image/svg+xml"
            } else {
                return Err(AppError::BadRequest("Invalid SVG format".to_string()));
            }
        } else {
            return Err(AppError::BadRequest("Invalid SVG encoding".to_string()));
        }
    } else {
        return Err(AppError::BadRequest("Invalid image format".to_string()));
    };

    if !allowed.contains(&actual_format) {
        return Err(AppError::BadRequest(format!(
            "Unsupported image type: {}",
            actual_format
        )));
    }

    Ok(actual_format)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [&str; 4] = ["image/jpeg", "image/png", "image/webp", "image/svg+xml"];

    fn png() -> Vec<u8> {
        let mut v = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        v.extend_from_slice(b"rest of the file");
        v
    }

    #[test]
    fn detects_each_format() {
        assert_eq!(sniff_image_format(&png(), &ALL).unwrap(), "image/png");
        assert_eq!(
            sniff_image_format(&[0xFF, 0xD8, 0xFF, 0xE0], &ALL).unwrap(),
            "image/jpeg"
        );
        let webp = [b"RIFF".as_slice(), &[0; 4], b"WEBP".as_slice()].concat();
        assert_eq!(sniff_image_format(&webp, &ALL).unwrap(), "image/webp");
        assert_eq!(
            sniff_image_format(b"<svg xmlns='http://www.w3.org/2000/svg'/>", &ALL).unwrap(),
            "image/svg+xml"
        );
    }

    /// The whole point: a forged header cannot make non-image bytes pass, and it
    /// cannot relabel real bytes as something else.
    #[test]
    fn rejects_non_image_bytes_whatever_the_client_claims() {
        let html = b"<html><script>alert(1)</script></html>";
        assert!(matches!(
            sniff_image_format(html, &ALL),
            Err(AppError::BadRequest(_))
        ));
        assert!(matches!(
            sniff_image_format(b"PK\x03\x04zipfile", &ALL),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn rejects_format_outside_the_allowlist() {
        let no_svg = ["image/jpeg", "image/png", "image/webp"];
        let err = sniff_image_format(b"<svg xmlns='x'></svg>", &no_svg).unwrap_err();
        assert!(matches!(err, AppError::BadRequest(m) if m.contains("image/svg+xml")));
    }

    #[test]
    fn rejects_too_small() {
        assert!(matches!(
            sniff_image_format(b"ab", &ALL),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn xml_prologue_without_svg_root_is_rejected() {
        assert!(matches!(
            sniff_image_format(b"<?xml version='1.0'?><rss></rss>", &ALL),
            Err(AppError::BadRequest(_))
        ));
    }
}
