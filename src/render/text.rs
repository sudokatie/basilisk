//! Text rendering - connects terminal grid to GPU renderer

use crate::config::{ColorScheme, FontConfig};
use crate::term::cell::{Cell, CellFlags, Color};
use crate::term::cursor::Cursor;
use crate::term::grid::Grid;
use crate::term::selection::SelectionManager;
use crate::Result;

use super::atlas::{Atlas, GlyphKey, GlyphInfo};
use super::glyph::{Font, load_system_font, load_font_file};
use super::renderer::Vertex;

/// Text renderer - generates GPU vertices from terminal state
pub struct TextRenderer {
    font: Font,
    bold_font: Option<Font>,
    italic_font: Option<Font>,
    atlas: Atlas,
    cell_width: f32,
    cell_height: f32,
    ascent: f32,
    screen_width: f32,
    screen_height: f32,
}

impl TextRenderer {
    /// Create a new text renderer
    pub fn new(config: &FontConfig) -> Result<Self> {
        // Load fonts
        let font_data = load_system_font()
            .ok_or_else(|| crate::Error::Font("No system font found".into()))?;

        let font = Font::from_bytes(&font_data, config.size)
            .ok_or_else(|| crate::Error::Font("Failed to parse font".into()))?;

        // Try to load bold/italic variants (optional)
        let bold_font = config.bold_font.as_ref()
            .and_then(|path| load_font_file(path))
            .and_then(|data| Font::from_bytes(&data, config.size));
        let italic_font = config.italic_font.as_ref()
            .and_then(|path| load_font_file(path))
            .and_then(|data| Font::from_bytes(&data, config.size));

        let cell_width = font.cell_width();
        let cell_height = font.line_height();
        let ascent = font.ascent();

        let atlas = Atlas::new(2048, 2048);

        Ok(Self {
            font,
            bold_font,
            italic_font,
            atlas,
            cell_width,
            cell_height,
            ascent,
            screen_width: 800.0,
            screen_height: 600.0,
        })
    }

    /// Get cell dimensions
    pub fn cell_width(&self) -> f32 {
        self.cell_width
    }

    pub fn cell_height(&self) -> f32 {
        self.cell_height
    }

    /// Set screen dimensions for coordinate conversion
    pub fn set_screen_size(&mut self, width: f32, height: f32) {
        self.screen_width = width;
        self.screen_height = height;
    }

    /// Check if atlas needs upload
    pub fn atlas_dirty(&self) -> bool {
        self.atlas.is_dirty()
    }

    /// Mark atlas as uploaded
    pub fn mark_atlas_clean(&mut self) {
        self.atlas.mark_clean();
    }

    /// Get atlas data for GPU upload
    pub fn atlas_data(&self) -> (&[u8], u32, u32) {
        let (w, h) = self.atlas.size();
        (self.atlas.data(), w, h)
    }

    /// Get font for given flags
    fn get_font(&self, flags: CellFlags) -> &Font {
        if flags.contains(CellFlags::BOLD) {
            self.bold_font.as_ref().unwrap_or(&self.font)
        } else if flags.contains(CellFlags::ITALIC) {
            self.italic_font.as_ref().unwrap_or(&self.font)
        } else {
            &self.font
        }
    }

    /// Get glyph flags for atlas key
    fn glyph_flags(cell_flags: CellFlags) -> u8 {
        let mut flags = 0u8;
        if cell_flags.contains(CellFlags::BOLD) { flags |= 1; }
        if cell_flags.contains(CellFlags::ITALIC) { flags |= 2; }
        flags
    }

    /// Convert color to float array
    fn color_to_array(color: &Color) -> [f32; 4] {
        [
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            1.0,
        ]
    }

    /// Parse hex color string
    fn parse_hex_color(hex: &str) -> Color {
        let hex = hex.trim_start_matches('#');
        if hex.len() >= 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
            Color::rgb(r, g, b)
        } else {
            Color::rgb(255, 255, 255)
        }
    }

    /// Convert pixel position to clip space (-1 to 1)
    fn to_clip_x(&self, x: f32) -> f32 {
        (x / self.screen_width) * 2.0 - 1.0
    }

    fn to_clip_y(&self, y: f32) -> f32 {
        1.0 - (y / self.screen_height) * 2.0
    }

    /// Render a grid cell to vertices
    fn render_cell(
        &mut self,
        col: u16,
        row: u16,
        cell: &Cell,
        cursor_here: bool,
        selected: bool,
        default_fg: &Color,
        default_bg: &Color,
        cursor_color: &Color,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
    ) {
        // Skip spacer cells (second half of wide chars)
        if cell.flags.contains(CellFlags::WIDE_SPACER) {
            return;
        }

        // Calculate cell position
        let x = col as f32 * self.cell_width;
        let y = row as f32 * self.cell_height;

        let cell_w = if cell.flags.contains(CellFlags::WIDE) {
            self.cell_width * 2.0
        } else {
            self.cell_width
        };

        // Determine colors
        let mut fg = if cell.fg.r == 255 && cell.fg.g == 255 && cell.fg.b == 255 &&
                        cell.flags.is_empty() {
            *default_fg
        } else {
            cell.fg
        };

        let mut bg = if cell.bg.r == 0 && cell.bg.g == 0 && cell.bg.b == 0 &&
                        cell.flags.is_empty() {
            *default_bg
        } else {
            cell.bg
        };

        // Handle inverse
        if cell.flags.contains(CellFlags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }

        // Handle selection
        if selected {
            std::mem::swap(&mut fg, &mut bg);
        }

        // Handle cursor
        if cursor_here {
            bg = *cursor_color;
            // Make fg visible against cursor
            fg = *default_bg;
        }

        // Handle dim
        if cell.flags.contains(CellFlags::DIM) {
            fg = Color::rgb(fg.r / 2, fg.g / 2, fg.b / 2);
        }

        // Handle hidden
        if cell.flags.contains(CellFlags::HIDDEN) {
            fg = bg;
        }

        let fg_arr = Self::color_to_array(&fg);
        let bg_arr = Self::color_to_array(&bg);

        // Render background quad
        let base_idx = vertices.len() as u32;

        let x0 = self.to_clip_x(x);
        let y0 = self.to_clip_y(y);
        let x1 = self.to_clip_x(x + cell_w);
        let y1 = self.to_clip_y(y + self.cell_height);

        // Background vertices (using UV 0,0 for solid color)
        vertices.push(Vertex {
            position: [x0, y0],
            tex_coords: [0.0, 0.0],
            color: fg_arr,
            bg_color: bg_arr,
        });
        vertices.push(Vertex {
            position: [x1, y0],
            tex_coords: [0.0, 0.0],
            color: fg_arr,
            bg_color: bg_arr,
        });
        vertices.push(Vertex {
            position: [x1, y1],
            tex_coords: [0.0, 0.0],
            color: fg_arr,
            bg_color: bg_arr,
        });
        vertices.push(Vertex {
            position: [x0, y1],
            tex_coords: [0.0, 0.0],
            color: fg_arr,
            bg_color: bg_arr,
        });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);

        // Render glyph if not a space
        let c = cell.c();
        if c != ' ' && c != '\0' {
            self.render_glyph(col, row, cell, fg_arr, bg_arr, vertices, indices);
        }

        // Render underline
        if cell.flags.contains(CellFlags::UNDERLINE) {
            self.render_underline(x, y, cell_w, fg_arr, bg_arr, vertices, indices);
        }

        // Render strikethrough
        if cell.flags.contains(CellFlags::STRIKETHROUGH) {
            self.render_strikethrough(x, y, cell_w, fg_arr, bg_arr, vertices, indices);
        }
    }

    /// Render a glyph
    fn render_glyph(
        &mut self,
        col: u16,
        row: u16,
        cell: &Cell,
        fg: [f32; 4],
        bg: [f32; 4],
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
    ) {
        let c = cell.c();
        let flags = Self::glyph_flags(cell.flags);
        let key = GlyphKey::new(c, flags);

        // Cache glyph if needed - we need to handle the borrow carefully
        // First check if cached, then rasterize if needed
        let glyph_info = if let Some(info) = self.atlas.get(&key) {
            *info
        } else {
            // Rasterize using the appropriate font
            let rasterized = if cell.flags.contains(CellFlags::BOLD) {
                self.bold_font.as_ref().unwrap_or(&self.font).rasterize(c)
            } else if cell.flags.contains(CellFlags::ITALIC) {
                self.italic_font.as_ref().unwrap_or(&self.font).rasterize(c)
            } else {
                self.font.rasterize(c)
            };

            // Now cache it
            match self.atlas.cache_rasterized(key, rasterized) {
                Some(info) => info,
                None => return, // Atlas full
            }
        };

        if glyph_info.width == 0 || glyph_info.height == 0 {
            return; // Empty glyph (space)
        }

        // Calculate glyph position
        let cell_x = col as f32 * self.cell_width;
        let cell_y = row as f32 * self.cell_height;

        let glyph_x = cell_x + glyph_info.metrics.xmin as f32;
        let glyph_y = cell_y + self.ascent - glyph_info.metrics.ymin as f32 - glyph_info.height as f32;

        let x0 = self.to_clip_x(glyph_x);
        let y0 = self.to_clip_y(glyph_y);
        let x1 = self.to_clip_x(glyph_x + glyph_info.width as f32);
        let y1 = self.to_clip_y(glyph_y + glyph_info.height as f32);

        // Calculate UV coordinates
        let (atlas_w, atlas_h) = self.atlas.size();
        let u0 = glyph_info.atlas_x as f32 / atlas_w as f32;
        let v0 = glyph_info.atlas_y as f32 / atlas_h as f32;
        let u1 = (glyph_info.atlas_x + glyph_info.width) as f32 / atlas_w as f32;
        let v1 = (glyph_info.atlas_y + glyph_info.height) as f32 / atlas_h as f32;

        let base_idx = vertices.len() as u32;

        vertices.push(Vertex {
            position: [x0, y0],
            tex_coords: [u0, v0],
            color: fg,
            bg_color: bg,
        });
        vertices.push(Vertex {
            position: [x1, y0],
            tex_coords: [u1, v0],
            color: fg,
            bg_color: bg,
        });
        vertices.push(Vertex {
            position: [x1, y1],
            tex_coords: [u1, v1],
            color: fg,
            bg_color: bg,
        });
        vertices.push(Vertex {
            position: [x0, y1],
            tex_coords: [u0, v1],
            color: fg,
            bg_color: bg,
        });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);
    }

    /// Render underline decoration
    fn render_underline(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        fg: [f32; 4],
        bg: [f32; 4],
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
    ) {
        let line_y = y + self.cell_height - 2.0;
        let line_height = 1.0;

        let x0 = self.to_clip_x(x);
        let y0 = self.to_clip_y(line_y);
        let x1 = self.to_clip_x(x + width);
        let y1 = self.to_clip_y(line_y + line_height);

        let base_idx = vertices.len() as u32;

        vertices.push(Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color: fg, bg_color: bg });
        vertices.push(Vertex { position: [x1, y0], tex_coords: [0.0, 0.0], color: fg, bg_color: bg });
        vertices.push(Vertex { position: [x1, y1], tex_coords: [0.0, 0.0], color: fg, bg_color: bg });
        vertices.push(Vertex { position: [x0, y1], tex_coords: [0.0, 0.0], color: fg, bg_color: bg });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);
    }

    /// Render strikethrough decoration
    fn render_strikethrough(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        fg: [f32; 4],
        bg: [f32; 4],
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
    ) {
        let line_y = y + self.cell_height / 2.0;
        let line_height = 1.0;

        let x0 = self.to_clip_x(x);
        let y0 = self.to_clip_y(line_y);
        let x1 = self.to_clip_x(x + width);
        let y1 = self.to_clip_y(line_y + line_height);

        let base_idx = vertices.len() as u32;

        vertices.push(Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color: fg, bg_color: bg });
        vertices.push(Vertex { position: [x1, y0], tex_coords: [0.0, 0.0], color: fg, bg_color: bg });
        vertices.push(Vertex { position: [x1, y1], tex_coords: [0.0, 0.0], color: fg, bg_color: bg });
        vertices.push(Vertex { position: [x0, y1], tex_coords: [0.0, 0.0], color: fg, bg_color: bg });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);
    }

    /// Render entire grid
    pub fn render_grid(
        &mut self,
        grid: &Grid,
        cursor: &Cursor,
        selection: &SelectionManager,
        colors: &ColorScheme,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::with_capacity(grid.cols() as usize * grid.lines() as usize * 8);
        let mut indices = Vec::with_capacity(grid.cols() as usize * grid.lines() as usize * 12);

        let default_fg = Self::parse_hex_color(&colors.foreground);
        let default_bg = Self::parse_hex_color(&colors.background);
        let cursor_color = Self::parse_hex_color(&colors.cursor);

        for row in 0..grid.lines() {
            for col in 0..grid.cols() {
                let cell = grid.cell(col, row);
                let cursor_here = cursor.visible && cursor.col == col && cursor.line == row;
                let selected = selection.is_selected(col, row);

                self.render_cell(
                    col,
                    row,
                    cell,
                    cursor_here,
                    selected,
                    &default_fg,
                    &default_bg,
                    &cursor_color,
                    &mut vertices,
                    &mut indices,
                );
            }
        }

        (vertices, indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_color_valid() {
        let color = TextRenderer::parse_hex_color("#ff0000");
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
    }

    #[test]
    fn parse_hex_color_no_hash() {
        let color = TextRenderer::parse_hex_color("00ff00");
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 255);
        assert_eq!(color.b, 0);
    }

    #[test]
    fn glyph_flags_none() {
        let flags = TextRenderer::glyph_flags(CellFlags::empty());
        assert_eq!(flags, 0);
    }

    #[test]
    fn glyph_flags_bold() {
        let flags = TextRenderer::glyph_flags(CellFlags::BOLD);
        assert_eq!(flags, 1);
    }

    #[test]
    fn glyph_flags_italic() {
        let flags = TextRenderer::glyph_flags(CellFlags::ITALIC);
        assert_eq!(flags, 2);
    }
}
