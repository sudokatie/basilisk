//! Sixel graphics parsing for ANSI sequences
//!
//! Re-exports sixel decoding from the render module and provides
//! convenience functions for parsing sixel data from DCS sequences.

// Re-export core types from render module
pub use crate::render::sixel::{SixelColor, SixelImage, SixelDecoder};

/// Decode sixel data from a byte slice.
///
/// This is a convenience function that creates a decoder, feeds it
/// the data, and returns the resulting image.
///
/// # Arguments
/// * `data` - Raw sixel data (without DCS introducer or ST terminator)
///
/// # Returns
/// The decoded image, or None if the image is empty
pub fn decode_sixel(data: &[u8]) -> Option<SixelImage> {
    let mut decoder = SixelDecoder::new();
    decoder.decode(data);
    
    let image = decoder.take_image();
    if image.width == 0 || image.height == 0 {
        None
    } else {
        Some(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_empty() {
        let result = decode_sixel(b"");
        // Empty input should produce None
        assert!(result.is_none());
    }

    #[test]
    fn decode_simple_sixel() {
        // Simple sixel: define color and draw
        let data = b"#0;2;0;0;0~";
        let result = decode_sixel(data);
        // Should decode without panic
        let _ = result;
    }
}
