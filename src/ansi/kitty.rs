//! Kitty graphics protocol decoder
//!
//! Decodes APC graphics sequences: ESC _ G <payload> ESC \
//!
//! The Kitty graphics protocol supports:
//! - Direct PNG/RGB/RGBA data transmission
//! - File-based image loading
//! - Image placement and scaling
//! - Animation frames
//!
//! Control data format: key=value pairs separated by commas
//! Required keys for transmission: a (action), f (format)

use std::collections::HashMap;

/// Maximum image dimensions to prevent memory exhaustion
const MAX_IMAGE_WIDTH: u32 = 4096;
const MAX_IMAGE_HEIGHT: u32 = 4096;
#[allow(dead_code)] // Reserved for payload validation
const MAX_PAYLOAD_SIZE: usize = 4 * 1024 * 1024; // 4MB

/// Image format in Kitty protocol
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KittyFormat {
    /// RGB (24-bit, 3 bytes per pixel)
    Rgb = 24,
    /// RGBA (32-bit, 4 bytes per pixel)
    Rgba = 32,
    /// PNG compressed data
    Png = 100,
}

impl KittyFormat {
    fn from_value(v: u32) -> Option<Self> {
        match v {
            24 => Some(KittyFormat::Rgb),
            32 => Some(KittyFormat::Rgba),
            100 => Some(KittyFormat::Png),
            _ => None,
        }
    }

    fn bytes_per_pixel(&self) -> usize {
        match self {
            KittyFormat::Rgb => 3,
            KittyFormat::Rgba => 4,
            KittyFormat::Png => 0, // Variable
        }
    }
}

/// Transmission type
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KittyTransmission {
    /// Direct data (base64 encoded)
    Direct,
    /// File path
    File,
    /// Temporary file
    TempFile,
    /// Shared memory
    SharedMemory,
}

impl KittyTransmission {
    fn from_char(c: char) -> Option<Self> {
        match c {
            'd' => Some(KittyTransmission::Direct),
            'f' => Some(KittyTransmission::File),
            't' => Some(KittyTransmission::TempFile),
            's' => Some(KittyTransmission::SharedMemory),
            _ => None,
        }
    }
}

/// Action type
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KittyAction {
    /// Transmit image data (default)
    Transmit,
    /// Transmit and display
    TransmitDisplay,
    /// Query terminal support
    Query,
    /// Place a previously transmitted image
    Place,
    /// Delete images
    Delete,
    /// Animation frame
    Frame,
    /// Animation control
    Animation,
}

impl KittyAction {
    fn from_char(c: char) -> Option<Self> {
        match c {
            't' => Some(KittyAction::Transmit),
            'T' => Some(KittyAction::TransmitDisplay),
            'q' => Some(KittyAction::Query),
            'p' => Some(KittyAction::Place),
            'd' => Some(KittyAction::Delete),
            'f' => Some(KittyAction::Frame),
            'a' => Some(KittyAction::Animation),
            _ => None,
        }
    }
}

/// Kitty graphics command parsed from payload
#[derive(Clone, Debug)]
pub struct KittyCommand {
    /// Action (a=)
    pub action: KittyAction,
    /// Image format (f=)
    pub format: Option<KittyFormat>,
    /// Transmission type (t=)
    pub transmission: KittyTransmission,
    /// Image ID for later reference (i=)
    pub image_id: Option<u32>,
    /// Image number (I=) - alternative to image_id
    pub image_number: Option<u32>,
    /// Placement ID (p=)
    pub placement_id: Option<u32>,
    /// Width in pixels (s=)
    pub width: Option<u32>,
    /// Height in pixels (v=)
    pub height: Option<u32>,
    /// X offset in pixels (x=)
    pub x_offset: Option<u32>,
    /// Y offset in pixels (y=)
    pub y_offset: Option<u32>,
    /// Number of columns to occupy (c=)
    pub columns: Option<u32>,
    /// Number of rows to occupy (r=)
    pub rows: Option<u32>,
    /// More data coming (m=1)
    pub more_data: bool,
    /// Compression (o=z for zlib)
    pub compression: Option<char>,
    /// Quiet mode - don't report errors (q=)
    pub quiet: u8,
    /// Delete target (d=)
    pub delete_target: Option<char>,
    /// Raw data payload (base64 decoded)
    pub data: Vec<u8>,
}

impl Default for KittyCommand {
    fn default() -> Self {
        Self {
            action: KittyAction::Transmit,
            format: None,
            transmission: KittyTransmission::Direct,
            image_id: None,
            image_number: None,
            placement_id: None,
            width: None,
            height: None,
            x_offset: None,
            y_offset: None,
            columns: None,
            rows: None,
            more_data: false,
            compression: None,
            quiet: 0,
            delete_target: None,
            data: Vec::new(),
        }
    }
}

/// A decoded Kitty image ready for rendering
#[derive(Clone, Debug, PartialEq)]
pub struct KittyImage {
    /// RGBA pixel data (4 bytes per pixel)
    pub data: Vec<u8>,
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
    /// Image ID for reference
    pub id: Option<u32>,
}

impl KittyImage {
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0 || self.data.is_empty()
    }
}

/// Kitty graphics decoder
pub struct KittyDecoder {
    /// Partial data buffer for multi-chunk transmissions
    partial_data: HashMap<u32, Vec<u8>>,
    /// Partial command info for multi-chunk
    partial_cmd: HashMap<u32, KittyCommand>,
}

impl KittyDecoder {
    pub fn new() -> Self {
        Self {
            partial_data: HashMap::new(),
            partial_cmd: HashMap::new(),
        }
    }

    /// Parse a Kitty graphics payload
    /// Format: control_data;base64_data (semicolon separates control from data)
    pub fn parse(&mut self, payload: &[u8]) -> Result<Option<KittyImage>, KittyError> {
        // Find the semicolon separator
        let (control_part, data_part) = match payload.iter().position(|&b| b == b';') {
            Some(pos) => (&payload[..pos], &payload[pos + 1..]),
            None => (payload, &[][..]),
        };

        // Parse control data
        let control_str = std::str::from_utf8(control_part)
            .map_err(|_| KittyError::InvalidControlData)?;
        
        let mut cmd = self.parse_control(control_str)?;

        // Decode base64 data if present
        if !data_part.is_empty() {
            let data_str = std::str::from_utf8(data_part)
                .map_err(|_| KittyError::InvalidPayload)?;
            
            // Strip whitespace and decode base64
            let clean_data: String = data_str.chars().filter(|c| !c.is_whitespace()).collect();
            cmd.data = base64_decode(&clean_data)?;
        }

        // Handle multi-chunk transmission
        if cmd.more_data {
            let id = cmd.image_id.unwrap_or(0);
            self.partial_data.entry(id).or_default().extend(&cmd.data);
            self.partial_cmd.insert(id, cmd);
            return Ok(None);
        }

        // Check for partial data to combine
        let id = cmd.image_id.unwrap_or(0);
        if let Some(mut partial) = self.partial_data.remove(&id) {
            partial.extend(&cmd.data);
            cmd.data = partial;
            if let Some(partial_cmd) = self.partial_cmd.remove(&id) {
                // Use settings from first chunk
                cmd.format = cmd.format.or(partial_cmd.format);
                cmd.width = cmd.width.or(partial_cmd.width);
                cmd.height = cmd.height.or(partial_cmd.height);
            }
        }

        // Process based on action
        match cmd.action {
            KittyAction::Transmit | KittyAction::TransmitDisplay => {
                self.decode_image(cmd)
            }
            KittyAction::Query => {
                // Query response would be handled by terminal
                Ok(None)
            }
            KittyAction::Delete => {
                // Delete handling would be done by image manager
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Parse control data key=value pairs
    fn parse_control(&self, control: &str) -> Result<KittyCommand, KittyError> {
        let mut cmd = KittyCommand::default();

        for part in control.split(',') {
            if part.is_empty() {
                continue;
            }

            let mut iter = part.splitn(2, '=');
            let key = iter.next().unwrap_or("");
            let value = iter.next().unwrap_or("");

            match key {
                "a" => {
                    if let Some(c) = value.chars().next() {
                        cmd.action = KittyAction::from_char(c)
                            .ok_or(KittyError::InvalidAction)?;
                    }
                }
                "f" => {
                    let v: u32 = value.parse().map_err(|_| KittyError::InvalidFormat)?;
                    cmd.format = KittyFormat::from_value(v);
                }
                "t" => {
                    if let Some(c) = value.chars().next() {
                        cmd.transmission = KittyTransmission::from_char(c)
                            .ok_or(KittyError::InvalidTransmission)?;
                    }
                }
                "i" => {
                    cmd.image_id = value.parse().ok();
                }
                "I" => {
                    cmd.image_number = value.parse().ok();
                }
                "p" => {
                    cmd.placement_id = value.parse().ok();
                }
                "s" => {
                    cmd.width = value.parse().ok();
                }
                "v" => {
                    cmd.height = value.parse().ok();
                }
                "x" => {
                    cmd.x_offset = value.parse().ok();
                }
                "y" => {
                    cmd.y_offset = value.parse().ok();
                }
                "c" => {
                    cmd.columns = value.parse().ok();
                }
                "r" => {
                    cmd.rows = value.parse().ok();
                }
                "m" => {
                    cmd.more_data = value == "1";
                }
                "o" => {
                    cmd.compression = value.chars().next();
                }
                "q" => {
                    cmd.quiet = value.parse().unwrap_or(0);
                }
                "d" => {
                    cmd.delete_target = value.chars().next();
                }
                _ => {
                    // Ignore unknown keys
                }
            }
        }

        Ok(cmd)
    }

    /// Decode image data to RGBA
    fn decode_image(&self, cmd: KittyCommand) -> Result<Option<KittyImage>, KittyError> {
        let format = cmd.format.ok_or(KittyError::MissingFormat)?;

        // Decompress if needed
        let data = if cmd.compression == Some('z') {
            decompress_zlib(&cmd.data)?
        } else {
            cmd.data
        };

        match format {
            KittyFormat::Png => {
                self.decode_png(&data, cmd.image_id)
            }
            KittyFormat::Rgb | KittyFormat::Rgba => {
                let width = cmd.width.ok_or(KittyError::MissingDimensions)?;
                let height = cmd.height.ok_or(KittyError::MissingDimensions)?;
                
                if width > MAX_IMAGE_WIDTH || height > MAX_IMAGE_HEIGHT {
                    return Err(KittyError::ImageTooLarge);
                }

                self.decode_raw(&data, width, height, format, cmd.image_id)
            }
        }
    }

    /// Decode PNG data
    fn decode_png(&self, data: &[u8], id: Option<u32>) -> Result<Option<KittyImage>, KittyError> {
        use image::ImageReader;
        use std::io::Cursor;
        
        if data.len() < 8 {
            return Err(KittyError::InvalidPngData);
        }
        
        // Check PNG signature
        let png_sig = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        if &data[..8] != &png_sig {
            return Err(KittyError::InvalidPngData);
        }

        // Decode PNG using image crate
        let cursor = Cursor::new(data);
        let reader = ImageReader::with_format(cursor, image::ImageFormat::Png);
        let img = reader.decode()
            .map_err(|_| KittyError::InvalidPngData)?;
        
        let rgba = img.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        
        // Check dimensions
        if width > MAX_IMAGE_WIDTH || height > MAX_IMAGE_HEIGHT {
            return Err(KittyError::ImageTooLarge);
        }

        Ok(Some(KittyImage {
            data: rgba.into_raw(),
            width,
            height,
            id,
        }))
    }

    /// Decode raw RGB/RGBA data
    fn decode_raw(
        &self,
        data: &[u8],
        width: u32,
        height: u32,
        format: KittyFormat,
        id: Option<u32>,
    ) -> Result<Option<KittyImage>, KittyError> {
        // Check dimensions first (before calculating sizes that could overflow)
        if width > MAX_IMAGE_WIDTH || height > MAX_IMAGE_HEIGHT {
            return Err(KittyError::ImageTooLarge);
        }

        let bpp = format.bytes_per_pixel();
        let expected_size = (width * height) as usize * bpp;
        
        if data.len() < expected_size {
            return Err(KittyError::InsufficientData);
        }

        // Convert to RGBA
        let rgba_data = match format {
            KittyFormat::Rgba => data[..expected_size].to_vec(),
            KittyFormat::Rgb => {
                let mut rgba = Vec::with_capacity((width * height * 4) as usize);
                for chunk in data[..expected_size].chunks(3) {
                    rgba.push(chunk[0]); // R
                    rgba.push(chunk[1]); // G
                    rgba.push(chunk[2]); // B
                    rgba.push(255);      // A
                }
                rgba
            }
            KittyFormat::Png => unreachable!(),
        };

        Ok(Some(KittyImage {
            data: rgba_data,
            width,
            height,
            id,
        }))
    }

    /// Clear partial data for an image ID
    pub fn clear_partial(&mut self, id: u32) {
        self.partial_data.remove(&id);
        self.partial_cmd.remove(&id);
    }
}

impl Default for KittyDecoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Kitty graphics protocol errors
#[derive(Clone, Debug, PartialEq)]
pub enum KittyError {
    InvalidControlData,
    InvalidPayload,
    InvalidAction,
    InvalidFormat,
    InvalidTransmission,
    MissingFormat,
    MissingDimensions,
    ImageTooLarge,
    InvalidPngData,
    InsufficientData,
    Base64Error,
    DecompressionError,
}

impl std::fmt::Display for KittyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KittyError::InvalidControlData => write!(f, "Invalid control data"),
            KittyError::InvalidPayload => write!(f, "Invalid payload"),
            KittyError::InvalidAction => write!(f, "Invalid action"),
            KittyError::InvalidFormat => write!(f, "Invalid format"),
            KittyError::InvalidTransmission => write!(f, "Invalid transmission type"),
            KittyError::MissingFormat => write!(f, "Missing format"),
            KittyError::MissingDimensions => write!(f, "Missing dimensions"),
            KittyError::ImageTooLarge => write!(f, "Image too large"),
            KittyError::InvalidPngData => write!(f, "Invalid PNG data"),
            KittyError::InsufficientData => write!(f, "Insufficient data"),
            KittyError::Base64Error => write!(f, "Base64 decode error"),
            KittyError::DecompressionError => write!(f, "Decompression error"),
        }
    }
}

impl std::error::Error for KittyError {}

/// Simple base64 decoder
fn base64_decode(input: &str) -> Result<Vec<u8>, KittyError> {
    const DECODE_TABLE: [i8; 256] = {
        let mut table = [-1i8; 256];
        let mut i = 0u8;
        while i < 26 {
            table[(b'A' + i) as usize] = i as i8;
            table[(b'a' + i) as usize] = (i + 26) as i8;
            i += 1;
        }
        let mut i = 0u8;
        while i < 10 {
            table[(b'0' + i) as usize] = (i + 52) as i8;
            i += 1;
        }
        table[b'+' as usize] = 62;
        table[b'/' as usize] = 63;
        table
    };

    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;

    for &b in bytes {
        if b == b'=' {
            break;
        }
        let val = DECODE_TABLE[b as usize];
        if val < 0 {
            continue; // Skip invalid chars
        }
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(output)
}

/// Decompress zlib data
fn decompress_zlib(data: &[u8]) -> Result<Vec<u8>, KittyError> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)
        .map_err(|_| KittyError::DecompressionError)?;
    Ok(decompressed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_control_basic() {
        let decoder = KittyDecoder::new();
        let cmd = decoder.parse_control("a=T,f=32,s=10,v=10").unwrap();
        
        assert_eq!(cmd.action, KittyAction::TransmitDisplay);
        assert_eq!(cmd.format, Some(KittyFormat::Rgba));
        assert_eq!(cmd.width, Some(10));
        assert_eq!(cmd.height, Some(10));
    }

    #[test]
    fn test_parse_control_with_id() {
        let decoder = KittyDecoder::new();
        let cmd = decoder.parse_control("a=t,f=24,i=42,s=100,v=50").unwrap();
        
        assert_eq!(cmd.action, KittyAction::Transmit);
        assert_eq!(cmd.format, Some(KittyFormat::Rgb));
        assert_eq!(cmd.image_id, Some(42));
        assert_eq!(cmd.width, Some(100));
        assert_eq!(cmd.height, Some(50));
    }

    #[test]
    fn test_parse_control_png() {
        let decoder = KittyDecoder::new();
        let cmd = decoder.parse_control("a=T,f=100").unwrap();
        
        assert_eq!(cmd.format, Some(KittyFormat::Png));
    }

    #[test]
    fn test_parse_control_more_data() {
        let decoder = KittyDecoder::new();
        let cmd = decoder.parse_control("a=t,f=32,m=1,i=1").unwrap();
        
        assert!(cmd.more_data);
    }

    #[test]
    fn test_base64_decode() {
        let decoded = base64_decode("SGVsbG8=").unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn test_base64_decode_no_padding() {
        let decoded = base64_decode("SGVsbG8").unwrap();
        assert_eq!(decoded, b"Hello");
    }

    #[test]
    fn test_decode_raw_rgba() {
        let decoder = KittyDecoder::new();
        // 2x2 RGBA image (16 bytes)
        let data = vec![
            255, 0, 0, 255,   // Red
            0, 255, 0, 255,   // Green
            0, 0, 255, 255,   // Blue
            255, 255, 0, 255, // Yellow
        ];
        
        let result = decoder.decode_raw(&data, 2, 2, KittyFormat::Rgba, Some(1)).unwrap();
        assert!(result.is_some());
        let img = result.unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
        assert_eq!(img.data.len(), 16);
    }

    #[test]
    fn test_decode_raw_rgb() {
        let decoder = KittyDecoder::new();
        // 2x2 RGB image (12 bytes)
        let data = vec![
            255, 0, 0,   // Red
            0, 255, 0,   // Green
            0, 0, 255,   // Blue
            255, 255, 0, // Yellow
        ];
        
        let result = decoder.decode_raw(&data, 2, 2, KittyFormat::Rgb, None).unwrap();
        assert!(result.is_some());
        let img = result.unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
        assert_eq!(img.data.len(), 16); // Converted to RGBA
    }

    #[test]
    fn test_image_dimensions_checked_first() {
        let decoder = KittyDecoder::new();
        // Small data buffer - dimensions should be checked before data size
        // This proves we validate dimensions before attempting allocation
        let data = vec![0u8; 100];
        
        // Width exceeds MAX_IMAGE_WIDTH (4096)
        let result = decoder.decode_raw(&data, 5000, 100, KittyFormat::Rgba, None);
        assert_eq!(result, Err(KittyError::ImageTooLarge));
        
        // Height exceeds MAX_IMAGE_HEIGHT (4096)
        let result = decoder.decode_raw(&data, 100, 5000, KittyFormat::Rgba, None);
        assert_eq!(result, Err(KittyError::ImageTooLarge));
    }

    #[test]
    fn test_insufficient_data() {
        let decoder = KittyDecoder::new();
        let data = vec![0u8; 10]; // Too small for 10x10 RGBA
        
        let result = decoder.decode_raw(&data, 10, 10, KittyFormat::Rgba, None);
        assert_eq!(result, Err(KittyError::InsufficientData));
    }
}
