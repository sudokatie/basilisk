//! Glyph rasterization using fontdue

use fontdue::{Font as FontdueFont, FontSettings};

/// A loaded font for text rendering
pub struct Font {
    inner: FontdueFont,
    size: f32,
}

/// Metrics for a rasterized glyph
#[derive(Debug, Clone, Copy)]
pub struct GlyphMetrics {
    pub advance_width: f32,
    pub width: usize,
    pub height: usize,
    pub xmin: i32,
    pub ymin: i32,
}

/// A rasterized glyph with bitmap data
pub struct RasterizedGlyph {
    pub metrics: GlyphMetrics,
    pub bitmap: Vec<u8>,
}

impl Font {
    /// Load a font from bytes
    pub fn from_bytes(data: &[u8], size: f32) -> Option<Self> {
        let settings = FontSettings::default();
        let inner = FontdueFont::from_bytes(data, settings).ok()?;
        Some(Self { inner, size })
    }

    /// Get the font size
    pub fn size(&self) -> f32 {
        self.size
    }

    /// Get line height (ascent - descent + line gap)
    pub fn line_height(&self) -> f32 {
        let metrics = self.inner.horizontal_line_metrics(self.size);
        match metrics {
            Some(m) => m.new_line_size,
            None => self.size * 1.2, // Fallback
        }
    }

    /// Get the cell width for monospace rendering
    pub fn cell_width(&self) -> f32 {
        // Use 'M' as reference for cell width
        let (metrics, _) = self.inner.rasterize('M', self.size);
        metrics.advance_width
    }

    /// Get ascent (baseline to top)
    pub fn ascent(&self) -> f32 {
        self.inner.horizontal_line_metrics(self.size)
            .map(|m| m.ascent)
            .unwrap_or(self.size * 0.8)
    }

    /// Get descent (baseline to bottom, typically negative)
    pub fn descent(&self) -> f32 {
        self.inner.horizontal_line_metrics(self.size)
            .map(|m| m.descent)
            .unwrap_or(-self.size * 0.2)
    }

    /// Rasterize a character
    pub fn rasterize(&self, c: char) -> RasterizedGlyph {
        let (metrics, bitmap) = self.inner.rasterize(c, self.size);

        RasterizedGlyph {
            metrics: GlyphMetrics {
                advance_width: metrics.advance_width,
                width: metrics.width,
                height: metrics.height,
                xmin: metrics.xmin,
                ymin: metrics.ymin,
            },
            bitmap,
        }
    }

    /// Rasterize with indexed variant for emoji/ligature support
    pub fn rasterize_indexed(&self, index: u16) -> RasterizedGlyph {
        let (metrics, bitmap) = self.inner.rasterize_indexed(index, self.size);

        RasterizedGlyph {
            metrics: GlyphMetrics {
                advance_width: metrics.advance_width,
                width: metrics.width,
                height: metrics.height,
                xmin: metrics.xmin,
                ymin: metrics.ymin,
            },
            bitmap,
        }
    }

    /// Check if the font contains a glyph for the character
    pub fn has_glyph(&self, c: char) -> bool {
        self.inner.lookup_glyph_index(c) != 0
    }

    /// Get glyph metrics without rasterizing
    pub fn metrics(&self, c: char) -> GlyphMetrics {
        let metrics = self.inner.metrics(c, self.size);
        GlyphMetrics {
            advance_width: metrics.advance_width,
            width: metrics.width,
            height: metrics.height,
            xmin: metrics.xmin,
            ymin: metrics.ymin,
        }
    }
}

/// Try to load a system monospace font
pub fn load_system_font() -> Option<Vec<u8>> {
    // Try common monospace font paths
    let paths = [
        // macOS
        "/System/Library/Fonts/Menlo.ttc",
        "/System/Library/Fonts/Monaco.ttf",
        "/Library/Fonts/SF-Mono-Regular.otf",
        // Linux
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    ];

    for path in paths {
        if let Ok(data) = std::fs::read(path) {
            return Some(data);
        }
    }
    None
}

/// Load a font from a file path
/// Supports direct paths and font family names (searches common directories)
pub fn load_font_file(path: &str) -> Option<Vec<u8>> {
    // Try direct path first
    if let Ok(data) = std::fs::read(path) {
        return Some(data);
    }

    // Try common font directories with the filename
    let font_dirs = [
        // macOS
        "/System/Library/Fonts",
        "/Library/Fonts",
        "~/Library/Fonts",
        // Linux
        "/usr/share/fonts/truetype",
        "/usr/share/fonts/TTF",
        "/usr/local/share/fonts",
        "~/.local/share/fonts",
        // Windows
        "C:/Windows/Fonts",
    ];

    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path);

    for dir in font_dirs {
        let expanded = if dir.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                home.join(&dir[2..])
            } else {
                continue;
            }
        } else {
            std::path::PathBuf::from(dir)
        };

        // Try exact filename
        let full_path = expanded.join(filename);
        if let Ok(data) = std::fs::read(&full_path) {
            return Some(data);
        }

        // Try with common extensions
        for ext in &["ttf", "otf", "ttc"] {
            let with_ext = expanded.join(format!("{}.{}", filename, ext));
            if let Ok(data) = std::fs::read(&with_ext) {
                return Some(data);
            }
        }
    }

    None
}

/// A chain of fonts for fallback support
/// Tries each font in order until one has the glyph
pub struct FontChain {
    fonts: Vec<Font>,
}

impl FontChain {
    /// Create a new font chain with an initial font
    pub fn new(primary: Font) -> Self {
        Self {
            fonts: vec![primary],
        }
    }

    /// Add a fallback font to the chain
    pub fn add_fallback(&mut self, font: Font) {
        self.fonts.push(font);
    }

    /// Get the primary font
    pub fn primary(&self) -> &Font {
        &self.fonts[0]
    }

    /// Rasterize a character, trying fonts in order
    pub fn rasterize(&self, c: char) -> RasterizedGlyph {
        // Try each font until one has the glyph
        for font in &self.fonts {
            if font.has_glyph(c) {
                return font.rasterize(c);
            }
        }
        
        // Fall back to primary font (will render placeholder/tofu)
        self.fonts[0].rasterize(c)
    }

    /// Check if any font in the chain has the glyph
    pub fn has_glyph(&self, c: char) -> bool {
        self.fonts.iter().any(|f| f.has_glyph(c))
    }

    /// Get the font that has the glyph (or primary as fallback)
    pub fn font_for(&self, c: char) -> &Font {
        for font in &self.fonts {
            if font.has_glyph(c) {
                return font;
            }
        }
        &self.fonts[0]
    }
}

/// Load system fallback fonts for emoji and symbols
pub fn load_fallback_fonts(size: f32) -> Vec<Font> {
    let mut fonts = Vec::new();
    
    // Emoji fonts
    let emoji_paths = [
        // macOS
        "/System/Library/Fonts/Apple Color Emoji.ttc",
        // Linux
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/google-noto-emoji/NotoColorEmoji.ttf",
    ];

    for path in emoji_paths {
        if let Ok(data) = std::fs::read(path) {
            if let Some(font) = Font::from_bytes(&data, size) {
                fonts.push(font);
                break; // Only need one emoji font
            }
        }
    }

    // Symbol fallback fonts
    let symbol_paths = [
        // macOS
        "/System/Library/Fonts/Supplemental/Symbol.ttf",
        // Linux - DejaVu has good symbol coverage
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ];

    for path in symbol_paths {
        if let Ok(data) = std::fs::read(path) {
            if let Some(font) = Font::from_bytes(&data, size) {
                fonts.push(font);
                break;
            }
        }
    }

    fonts
}

#[cfg(test)]
mod tests {
    use super::*;

    // Use a simple test that doesn't require embedded font
    #[test]
    fn glyph_metrics_fields() {
        let metrics = GlyphMetrics {
            advance_width: 10.0,
            width: 8,
            height: 12,
            xmin: 1,
            ymin: -2,
        };
        assert_eq!(metrics.advance_width, 10.0);
        assert_eq!(metrics.width, 8);
        assert_eq!(metrics.height, 12);
    }

    #[test]
    fn rasterized_glyph_fields() {
        let glyph = RasterizedGlyph {
            metrics: GlyphMetrics {
                advance_width: 10.0,
                width: 2,
                height: 2,
                xmin: 0,
                ymin: 0,
            },
            bitmap: vec![255, 128, 64, 32],
        };
        assert_eq!(glyph.bitmap.len(), 4);
    }
}
