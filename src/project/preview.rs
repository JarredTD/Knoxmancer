//! PNG preview generation and full image decoding.

use std::io::Cursor;

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, GenericImageView, ImageEncoder, ImageFormat, ImageReader, Limits};

use crate::error::{Error, Result};

/// Maximum memory made available to the image decoder.
const DECODED_DATA_MAX_BYTES: u64 = 2_000_000;

/// Generates an opaque RGBA PNG filled with Knoxmancer's scaffold color.
pub(crate) fn generate(width: u32, height: u32) -> Result<Vec<u8>> {
    let Some(pixel_count) = u64::from(width).checked_mul(u64::from(height)) else {
        return Err(Error::project(
            "preview dimensions exceed addressable memory",
        ));
    };
    let Ok(pixel_count) = usize::try_from(pixel_count) else {
        return Err(Error::project(
            "preview dimensions exceed addressable memory",
        ));
    };
    let Some(byte_count) = pixel_count.checked_mul(4) else {
        return Err(Error::project(
            "preview pixel data exceeds addressable memory",
        ));
    };
    let mut pixels = Vec::new();
    if let Err(error) = pixels.try_reserve_exact(byte_count) {
        return Err(Error::project(format!(
            "could not allocate preview pixels: {error}"
        )));
    }
    for _ in 0..pixel_count {
        pixels.extend_from_slice(&[44, 48, 46, 255]);
    }
    let mut png = Vec::new();
    if let Err(error) =
        PngEncoder::new(&mut png).write_image(&pixels, width, height, ExtendedColorType::Rgba8)
    {
        return Err(Error::project(format!(
            "could not encode preview PNG: {error}"
        )));
    }
    Ok(png)
}

/// Fully decodes a bounded PNG and returns its dimensions.
pub(crate) fn inspect(data: &[u8]) -> std::result::Result<(u32, u32), String> {
    let dimensions = ImageReader::with_format(Cursor::new(data), ImageFormat::Png)
        .into_dimensions()
        .map_err(|error| format!("invalid PNG header: {error}"))?;
    let decoded_bytes = u64::from(dimensions.0)
        .checked_mul(u64::from(dimensions.1))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "PNG dimensions exceed the decoding limit".to_owned())?;
    if decoded_bytes > DECODED_DATA_MAX_BYTES {
        return Err("PNG image data exceeds the decoding limit".to_owned());
    }

    let mut reader = ImageReader::with_format(Cursor::new(data), ImageFormat::Png);
    let mut limits = Limits::default();
    limits.max_image_width = Some(dimensions.0);
    limits.max_image_height = Some(dimensions.1);
    limits.max_alloc = Some(DECODED_DATA_MAX_BYTES);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|error| format!("invalid PNG image data: {error}"))?;
    debug_assert_eq!(decoded.dimensions(), dimensions);
    Ok(dimensions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_and_fully_decodes_pngs() {
        let image = generate(256, 256).unwrap();
        assert_eq!(inspect(&image).unwrap(), (256, 256));

        let mut corrupt = image.clone();
        let midpoint = corrupt.len() / 2;
        corrupt[midpoint] ^= 1;
        assert!(inspect(&corrupt).is_err());
        assert!(inspect(&image[..image.len() - 8]).is_err());
        assert!(inspect(b"not png").is_err());
    }

    #[test]
    fn bounds_decoded_png_data() {
        assert!(inspect(&generate(1024, 1024).unwrap()).is_err());
    }
}
