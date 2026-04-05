//! Sixel graphics decoder
//!
//! Decodes DCS sixel sequences into bitmap images for terminal display.

use std::collections::HashMap;

/// Color in RGB format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SixelColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl SixelColor {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Convert from HLS to RGB
    fn from_hls(h: u16, l: u16, s: u16) -> Self {
        // H: 0-360, L: 0-100, S: 0-100
        if s == 0 {
            let v = (l as f32 * 2.55) as u8;
            return Self::rgb(v, v, v);
        }

        let h = h as f32;
        let l = l as f32 / 100.0;
        let s = s as f32 / 100.0;

        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;

        let (r, g, b) = if h < 60.0 {
            (c, x, 0.0)
        } else if h < 120.0 {
            (x, c, 0.0)
        } else if h < 180.0 {
            (0.0, c, x)
        } else if h < 240.0 {
            (0.0, x, c)
        } else if h < 300.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };

        Self::rgb(
            ((r + m) * 255.0) as u8,
            ((g + m) * 255.0) as u8,
            ((b + m) * 255.0) as u8,
        )
    }
}

/// Decoded sixel image
#[derive(Debug)]
pub struct SixelImage {
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
    /// RGBA pixel data (width * height * 4 bytes)
    pub data: Vec<u8>,
    /// Background is transparent
    pub transparent: bool,
}

impl SixelImage {
    fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            data: Vec::new(),
            transparent: false,
        }
    }

    /// Set pixel at (x, y) to color
    fn set_pixel(&mut self, x: u32, y: u32, color: &SixelColor) {
        if x >= self.width || y >= self.height {
            return;
        }

        let idx = ((y * self.width + x) * 4) as usize;
        if idx + 3 < self.data.len() {
            self.data[idx] = color.r;
            self.data[idx + 1] = color.g;
            self.data[idx + 2] = color.b;
            self.data[idx + 3] = 255; // Alpha
        }
    }

    /// Resize image, preserving existing content
    fn resize(&mut self, new_width: u32, new_height: u32) {
        if new_width <= self.width && new_height <= self.height {
            return;
        }

        let new_w = new_width.max(self.width);
        let new_h = new_height.max(self.height);

        let mut new_data = vec![0u8; (new_w * new_h * 4) as usize];

        // Copy existing data
        for y in 0..self.height {
            for x in 0..self.width {
                let old_idx = ((y * self.width + x) * 4) as usize;
                let new_idx = ((y * new_w + x) * 4) as usize;
                if old_idx + 3 < self.data.len() && new_idx + 3 < new_data.len() {
                    new_data[new_idx..new_idx + 4].copy_from_slice(&self.data[old_idx..old_idx + 4]);
                }
            }
        }

        self.width = new_w;
        self.height = new_h;
        self.data = new_data;
    }
}

/// Sixel decoder state
pub struct SixelDecoder {
    /// Color palette (up to 256 colors)
    palette: HashMap<u16, SixelColor>,
    /// Current color index
    current_color: u16,
    /// Current X position
    x: u32,
    /// Current Y position (in sixels, multiply by 6 for pixels)
    y: u32,
    /// Aspect ratio numerator
    aspect_num: u16,
    /// Aspect ratio denominator
    aspect_den: u16,
    /// Horizontal grid size
    grid_width: u16,
    /// Vertical grid size
    grid_height: u16,
    /// Background mode (0 = device default, 1 = no change, 2 = transparent)
    bg_mode: u8,
    /// The decoded image
    image: SixelImage,
    /// Parser state
    state: DecodeState,
    /// Parameter buffer for parsing
    params: Vec<u16>,
    /// Current parameter being parsed
    current_param: u16,
    /// Repeat count for next sixel
    repeat_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodeState {
    Normal,
    Repeat,        // Parsing ! repeat count
    ColorDef,      // Parsing # color definition
    Raster,        // Parsing " raster attributes
}

impl Default for SixelDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl SixelDecoder {
    pub fn new() -> Self {
        let mut decoder = Self {
            palette: HashMap::new(),
            current_color: 0,
            x: 0,
            y: 0,
            aspect_num: 1,
            aspect_den: 1,
            grid_width: 0,
            grid_height: 0,
            bg_mode: 0,
            image: SixelImage::new(),
            state: DecodeState::Normal,
            params: Vec::new(),
            current_param: 0,
            repeat_count: 1,
        };

        // Initialize default palette (VT340 16-color palette)
        decoder.init_default_palette();

        decoder
    }

    fn init_default_palette(&mut self) {
        let colors = [
            (0, 0, 0),       // 0: Black
            (20, 20, 80),    // 1: Blue
            (80, 13, 13),    // 2: Red
            (20, 80, 20),    // 3: Green
            (80, 20, 80),    // 4: Magenta
            (20, 80, 80),    // 5: Cyan
            (80, 80, 20),    // 6: Yellow
            (53, 53, 53),    // 7: Gray 50%
            (26, 26, 26),    // 8: Gray 25%
            (33, 33, 100),   // 9: Blue
            (100, 26, 26),   // 10: Red
            (33, 100, 33),   // 11: Green
            (100, 33, 100),  // 12: Magenta
            (33, 100, 100),  // 13: Cyan
            (100, 100, 33),  // 14: Yellow
            (80, 80, 80),    // 15: Gray 75%
        ];

        for (i, (r, g, b)) in colors.iter().enumerate() {
            // Convert from percentage (0-100) to 0-255
            self.palette.insert(i as u16, SixelColor::rgb(
                (*r as u32 * 255 / 100) as u8,
                (*g as u32 * 255 / 100) as u8,
                (*b as u32 * 255 / 100) as u8,
            ));
        }
    }

    /// Decode a sixel data stream
    pub fn decode(&mut self, data: &[u8]) -> &SixelImage {
        for &byte in data {
            self.process_byte(byte);
        }
        &self.image
    }

    /// Process a single byte
    fn process_byte(&mut self, byte: u8) {
        match self.state {
            DecodeState::Normal => self.process_normal(byte),
            DecodeState::Repeat => self.process_repeat(byte),
            DecodeState::ColorDef => self.process_color_def(byte),
            DecodeState::Raster => self.process_raster(byte),
        }
    }

    fn process_normal(&mut self, byte: u8) {
        match byte {
            // Sixel data characters (63-126 map to 0-63)
            0x3f..=0x7e => {
                let sixel = byte - 0x3f;
                self.draw_sixel(sixel, self.repeat_count);
                self.repeat_count = 1;
            }

            // Graphics New Line ($)
            0x24 => {
                self.x = 0;
            }

            // Graphics Carriage Return (-)
            0x2d => {
                self.x = 0;
                self.y += 1;
            }

            // Repeat introducer (!)
            0x21 => {
                self.state = DecodeState::Repeat;
                self.current_param = 0;
            }

            // Color introducer (#)
            0x23 => {
                self.state = DecodeState::ColorDef;
                self.params.clear();
                self.current_param = 0;
            }

            // Raster attributes (")
            0x22 => {
                self.state = DecodeState::Raster;
                self.params.clear();
                self.current_param = 0;
            }

            _ => {}
        }
    }

    fn process_repeat(&mut self, byte: u8) {
        match byte {
            b'0'..=b'9' => {
                self.current_param = self.current_param
                    .saturating_mul(10)
                    .saturating_add((byte - b'0') as u16);
            }
            _ => {
                self.repeat_count = self.current_param as u32;
                if self.repeat_count == 0 {
                    self.repeat_count = 1;
                }
                self.state = DecodeState::Normal;
                // Re-process this byte in normal mode
                self.process_normal(byte);
            }
        }
    }

    fn process_color_def(&mut self, byte: u8) {
        match byte {
            b'0'..=b'9' => {
                self.current_param = self.current_param
                    .saturating_mul(10)
                    .saturating_add((byte - b'0') as u16);
            }
            b';' => {
                self.params.push(self.current_param);
                self.current_param = 0;
            }
            _ => {
                self.params.push(self.current_param);
                self.finish_color_def();
                self.state = DecodeState::Normal;
                // Re-process this byte in normal mode
                if byte >= 0x3f && byte <= 0x7e {
                    self.process_normal(byte);
                }
            }
        }
    }

    fn process_raster(&mut self, byte: u8) {
        match byte {
            b'0'..=b'9' => {
                self.current_param = self.current_param
                    .saturating_mul(10)
                    .saturating_add((byte - b'0') as u16);
            }
            b';' => {
                self.params.push(self.current_param);
                self.current_param = 0;
            }
            _ => {
                self.params.push(self.current_param);
                self.finish_raster();
                self.state = DecodeState::Normal;
            }
        }
    }

    fn finish_color_def(&mut self) {
        if self.params.is_empty() {
            return;
        }

        let color_idx = self.params[0];

        if self.params.len() == 1 {
            // Just select color
            self.current_color = color_idx;
        } else if self.params.len() >= 5 {
            // Define color
            let color_type = self.params[1];
            let p1 = self.params[2];
            let p2 = self.params[3];
            let p3 = self.params[4];

            let color = match color_type {
                1 => {
                    // HLS color space
                    SixelColor::from_hls(p1, p2, p3)
                }
                2 | _ => {
                    // RGB color space (percentages 0-100)
                    SixelColor::rgb(
                        (p1 as u32 * 255 / 100) as u8,
                        (p2 as u32 * 255 / 100) as u8,
                        (p3 as u32 * 255 / 100) as u8,
                    )
                }
            };

            self.palette.insert(color_idx, color);
            self.current_color = color_idx;
        }
    }

    fn finish_raster(&mut self) {
        // Raster attributes: Pan; Pad; Ph; Pv
        // Pan/Pad = aspect ratio, Ph/Pv = image dimensions
        if self.params.len() >= 4 {
            self.aspect_num = self.params[0].max(1);
            self.aspect_den = self.params[1].max(1);
            let width = self.params[2] as u32;
            let height = self.params[3] as u32;

            // Pre-allocate image if dimensions given
            if width > 0 && height > 0 {
                self.image.resize(width, height);
            }
        }
    }

    /// Draw a sixel value (6 vertical pixels)
    fn draw_sixel(&mut self, sixel: u8, repeat: u32) {
        let color = self.palette.get(&self.current_color)
            .copied()
            .unwrap_or(SixelColor::rgb(255, 255, 255));

        // Calculate pixel Y position (sixels are 6 pixels tall)
        let base_y = self.y * 6;

        // Ensure image is large enough
        let needed_width = self.x + repeat;
        let needed_height = base_y + 6;
        self.image.resize(needed_width, needed_height);

        // Draw pixels for each repeat
        for dx in 0..repeat {
            let px = self.x + dx;

            // Each bit in sixel represents one vertical pixel
            for bit in 0..6 {
                if (sixel & (1 << bit)) != 0 {
                    let py = base_y + bit as u32;
                    self.image.set_pixel(px, py, &color);
                }
            }
        }

        self.x += repeat;
    }

    /// Reset decoder state for a new image
    pub fn reset(&mut self) {
        self.current_color = 0;
        self.x = 0;
        self.y = 0;
        self.repeat_count = 1;
        self.state = DecodeState::Normal;
        self.params.clear();
        self.current_param = 0;
        self.image = SixelImage::new();
    }

    /// Get the decoded image
    pub fn image(&self) -> &SixelImage {
        &self.image
    }

    /// Take ownership of the decoded image
    pub fn take_image(&mut self) -> SixelImage {
        std::mem::replace(&mut self.image, SixelImage::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixel_color_rgb() {
        let color = SixelColor::rgb(255, 128, 0);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 128);
        assert_eq!(color.b, 0);
    }

    #[test]
    fn sixel_color_from_hls_gray() {
        let gray = SixelColor::from_hls(0, 50, 0);
        assert_eq!(gray.r, gray.g);
        assert_eq!(gray.g, gray.b);
    }

    #[test]
    fn sixel_image_new() {
        let image = SixelImage::new();
        assert_eq!(image.width, 0);
        assert_eq!(image.height, 0);
    }

    #[test]
    fn sixel_image_resize() {
        let mut image = SixelImage::new();
        image.resize(10, 10);
        assert_eq!(image.width, 10);
        assert_eq!(image.height, 10);
        assert_eq!(image.data.len(), 10 * 10 * 4);
    }

    #[test]
    fn sixel_image_set_pixel() {
        let mut image = SixelImage::new();
        image.resize(5, 5);
        let red = SixelColor::rgb(255, 0, 0);
        image.set_pixel(2, 2, &red);

        let idx = (2 * 5 + 2) * 4;
        assert_eq!(image.data[idx as usize], 255);
        assert_eq!(image.data[idx as usize + 1], 0);
        assert_eq!(image.data[idx as usize + 2], 0);
    }

    #[test]
    fn decoder_new() {
        let decoder = SixelDecoder::new();
        assert_eq!(decoder.current_color, 0);
        assert_eq!(decoder.x, 0);
        assert_eq!(decoder.y, 0);
        // Check default palette has 16 colors
        assert!(decoder.palette.len() >= 16);
    }

    #[test]
    fn decoder_simple_sixel() {
        let mut decoder = SixelDecoder::new();
        // 0x3f = ?, which is sixel value 0 (no pixels)
        // 0x40 = @, which is sixel value 1 (bottom pixel only)
        // 0x7e = ~, which is sixel value 63 (all 6 pixels)
        decoder.decode(b"~");

        assert!(decoder.image.width > 0);
        assert!(decoder.image.height >= 6);
    }

    #[test]
    fn decoder_color_selection() {
        let mut decoder = SixelDecoder::new();
        // #1 selects color 1
        decoder.decode(b"#1~");
        assert_eq!(decoder.current_color, 1);
    }

    #[test]
    fn decoder_color_definition() {
        let mut decoder = SixelDecoder::new();
        // #100;2;100;0;0 defines color 100 as RGB red
        decoder.decode(b"#100;2;100;0;0");
        let color = decoder.palette.get(&100);
        assert!(color.is_some());
        let color = color.unwrap();
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
    }

    #[test]
    fn decoder_repeat() {
        let mut decoder = SixelDecoder::new();
        // !10~ means repeat ~ 10 times
        decoder.decode(b"!10~");
        assert!(decoder.image.width >= 10);
    }

    #[test]
    fn decoder_newline() {
        let mut decoder = SixelDecoder::new();
        decoder.decode(b"~-~");
        // Should have drawn on two sixel rows
        assert!(decoder.image.height >= 12);
    }

    #[test]
    fn decoder_carriage_return() {
        let mut decoder = SixelDecoder::new();
        decoder.decode(b"~$~");
        // $ should reset X but not advance Y
        assert_eq!(decoder.y, 0);
    }

    #[test]
    fn decoder_reset() {
        let mut decoder = SixelDecoder::new();
        decoder.decode(b"~~~");
        decoder.reset();
        assert_eq!(decoder.x, 0);
        assert_eq!(decoder.y, 0);
        assert_eq!(decoder.image.width, 0);
    }

    #[test]
    fn decoder_raster_attributes() {
        let mut decoder = SixelDecoder::new();
        // "1;1;100;50 sets aspect 1:1, size 100x50
        decoder.decode(b"\"1;1;100;50");
        assert_eq!(decoder.image.width, 100);
        assert_eq!(decoder.image.height, 50);
    }
}
