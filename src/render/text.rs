//! Text rendering - connects terminal grid to GPU renderer

use std::time::{Duration, Instant};

use crate::config::{ColorScheme, FontConfig};
use crate::term::cell::{Cell, CellFlags, Color};
use crate::term::cursor::{Cursor, CursorShape};
use crate::term::grid::Grid;
use crate::term::selection::SelectionManager;
use crate::Result;

use super::atlas::{Atlas, GlyphKey, ColorAtlas};
use super::glyph::{Font, load_system_font, load_font_file};
use super::renderer::Vertex;

/// Check if a character is likely an emoji or color glyph
fn is_emoji(c: char) -> bool {
    let cp = c as u32;
    // Common emoji ranges (simplified to avoid overlapping patterns)
    matches!(cp,
        0x1F600..=0x1F64F |  // Emoticons
        0x1F300..=0x1F5FF |  // Misc Symbols and Pictographs
        0x1F680..=0x1F6FF |  // Transport and Map
        0x1F1E0..=0x1F1FF |  // Regional Indicators
        0x2600..=0x26FF   |  // Misc symbols (includes many emoji)
        0x2700..=0x27BF   |  // Dingbats
        0xFE00..=0xFE0F   |  // Variation Selectors
        0x1F900..=0x1F9FF |  // Supplemental Symbols and Pictographs
        0x1FA00..=0x1FA6F |  // Chess Symbols
        0x1FA70..=0x1FAFF |  // Symbols and Pictographs Extended-A
        0x231A..=0x231B   |  // Watch, Hourglass
        0x23E9..=0x23FA   |  // Various media symbols
        0x25AA..=0x25FE   |  // Geometric shapes (squares, etc.)
        0x2934..=0x2935   |  // Arrows
        0x2B05..=0x2B07   |  // Arrows
        0x2B1B..=0x2B1C   |  // Large squares
        0x2B50            |  // Star
        0x2B55            |  // Circle
        0x3030            |  // Wavy dash
        0x303D            |  // Part alternation mark
        0x3297            |  // Circled ideograph congratulation
        0x3299               // Circled ideograph secret
    )
}

#[allow(dead_code)]

/// Text renderer - generates GPU vertices from terminal state
pub struct TextRenderer {
    font: Font,
    bold_font: Option<Font>,
    italic_font: Option<Font>,
    /// Bold-italic font (for when both flags are set)
    bold_italic_font: Option<Font>,
    atlas: Atlas,
    /// Separate RGBA atlas for color emoji
    color_atlas: ColorAtlas,
    cell_width: f32,
    cell_height: f32,
    ascent: f32,
    screen_width: f32,
    screen_height: f32,
    /// Window padding in pixels
    padding: f32,
    /// Hyperlink color
    hyperlink_color: Color,
    /// Cell blink state (for SGR 5 blinking text)
    cell_blink_visible: bool,
    /// Last cell blink toggle time
    cell_blink_last_toggle: Instant,
    /// Cell blink interval
    cell_blink_interval: Duration,
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
        let bold_italic_font = config.bold_italic_font.as_ref()
            .and_then(|path| load_font_file(path))
            .and_then(|data| Font::from_bytes(&data, config.size));

        let cell_width = font.cell_width();
        let cell_height = font.line_height();
        let ascent = font.ascent();

        let atlas = Atlas::new(2048, 2048);
        let color_atlas = ColorAtlas::new(2048, 2048);

        Ok(Self {
            font,
            bold_font,
            italic_font,
            bold_italic_font,
            atlas,
            color_atlas,
            cell_width,
            cell_height,
            ascent,
            screen_width: 800.0,
            screen_height: 600.0,
            padding: 0.0,
            hyperlink_color: Color::rgb(100, 149, 237), // Cornflower blue
            cell_blink_visible: true,
            cell_blink_last_toggle: Instant::now(),
            cell_blink_interval: Duration::from_millis(530),
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

    /// Set window padding
    pub fn set_padding(&mut self, padding: f32) {
        self.padding = padding;
    }

    /// Get current padding
    pub fn padding(&self) -> f32 {
        self.padding
    }

    /// Update cell blink state - call each frame
    /// Returns true if visibility changed
    pub fn update_cell_blink(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.cell_blink_last_toggle) >= self.cell_blink_interval {
            self.cell_blink_visible = !self.cell_blink_visible;
            self.cell_blink_last_toggle = now;
            true
        } else {
            false
        }
    }

    /// Check if blinking cells should be visible
    pub fn cell_blink_visible(&self) -> bool {
        self.cell_blink_visible
    }

    /// Check if atlas needs upload
    pub fn atlas_dirty(&self) -> bool {
        self.atlas.is_dirty()
    }

    /// Mark atlas as uploaded
    pub fn mark_atlas_clean(&mut self) {
        self.atlas.mark_clean();
    }

    /// Get atlas data for GPU upload (grayscale)
    pub fn atlas_data(&self) -> (&[u8], u32, u32) {
        let (w, h) = self.atlas.size();
        (self.atlas.data(), w, h)
    }

    /// Check if color atlas needs upload
    pub fn color_atlas_dirty(&self) -> bool {
        self.color_atlas.is_dirty()
    }

    /// Mark color atlas as uploaded
    pub fn mark_color_atlas_clean(&mut self) {
        self.color_atlas.mark_clean();
    }

    /// Get color atlas data for GPU upload (RGBA)
    pub fn color_atlas_data(&self) -> (&[u8], u32, u32) {
        let (w, h) = self.color_atlas.size();
        (self.color_atlas.data(), w, h)
    }

    /// Get font for given flags
    fn get_font(&self, flags: CellFlags) -> &Font {
        let is_bold = flags.contains(CellFlags::BOLD);
        let is_italic = flags.contains(CellFlags::ITALIC);
        
        if is_bold && is_italic {
            // Try bold-italic first, fall back to bold, then italic, then regular
            self.bold_italic_font.as_ref()
                .or(self.bold_font.as_ref())
                .or(self.italic_font.as_ref())
                .unwrap_or(&self.font)
        } else if is_bold {
            self.bold_font.as_ref().unwrap_or(&self.font)
        } else if is_italic {
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
    #[allow(dead_code)] // Kept for reference; render_cell_with_search is used instead
    fn render_cell(
        &mut self,
        col: u16,
        row: u16,
        cell: &Cell,
        cursor_here: bool,
        cursor_shape: CursorShape,
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

        // Calculate cell position (with padding offset)
        let x = self.padding + col as f32 * self.cell_width;
        let y = self.padding + row as f32 * self.cell_height;

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

        // Handle cursor - only invert for block cursor
        if cursor_here && cursor_shape == CursorShape::Block {
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

        // Handle hyperlink (use hyperlink color)
        let has_hyperlink = cell.hyperlink_id != 0;
        if has_hyperlink && !selected && !cursor_here {
            fg = self.hyperlink_color;
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
        if cell.c != ' ' && cell.c != '\0' {
            self.render_glyph(col, row, cell, fg_arr, bg_arr, vertices, indices);
        }

        // Render underline (also for hyperlinks)
        if cell.flags.contains(CellFlags::UNDERLINE) || has_hyperlink {
            self.render_underline(x, y, cell_w, fg_arr, bg_arr, vertices, indices);
        }

        // Render strikethrough
        if cell.flags.contains(CellFlags::STRIKETHROUGH) {
            self.render_strikethrough(x, y, cell_w, fg_arr, bg_arr, vertices, indices);
        }

        // Render cursor (for non-block shapes)
        if cursor_here && cursor_shape != CursorShape::Block {
            let cursor_arr = Self::color_to_array(cursor_color);
            self.render_cursor_shape(x, y, cell_w, cursor_shape, cursor_arr, bg_arr, vertices, indices);
        }
    }

    /// Render cursor for underline and beam shapes
    fn render_cursor_shape(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        shape: CursorShape,
        fg: [f32; 4],
        bg: [f32; 4],
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
    ) {
        match shape {
            CursorShape::Block => {
                // Block cursor is handled by background color
            }
            CursorShape::Underline => {
                // Underline cursor at bottom of cell
                let line_y = y + self.cell_height - 2.0;
                let line_height = 2.0;

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
            CursorShape::Beam => {
                // Vertical bar cursor at left of cell
                let bar_width = 2.0;

                let x0 = self.to_clip_x(x);
                let y0 = self.to_clip_y(y);
                let x1 = self.to_clip_x(x + bar_width);
                let y1 = self.to_clip_y(y + self.cell_height);

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
        // Check if this is an emoji character
        // TODO: Full color emoji support requires:
        // 1. A second RGBA texture in renderer.rs
        // 2. A second bind group for the color atlas
        // 3. Modified shader to sample from color texture for emoji
        // For now, emoji are rendered as grayscale glyphs
        let _is_color_glyph = is_emoji(cell.c);

        let flags = Self::glyph_flags(cell.flags);
        let key = GlyphKey::new(cell.c, flags);

        // Select font based on style flags
        let font = self.get_font(cell.flags);

        // Cache glyph if needed
        let glyph_info = if let Some(info) = self.atlas.get(&key) {
            *info
        } else {
            // Rasterize using the selected font
            let rasterized = font.rasterize(cell.c);

            // Now cache it
            match self.atlas.cache_rasterized(key, rasterized) {
                Some(info) => info,
                None => return, // Atlas full
            }
        };

        if glyph_info.width == 0 || glyph_info.height == 0 {
            return; // Empty glyph (space)
        }

        // Calculate glyph position (with padding offset)
        let cell_x = self.padding + col as f32 * self.cell_width;
        let cell_y = self.padding + row as f32 * self.cell_height;

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
    #[allow(dead_code)] // Called from render_cell which is kept for reference
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
        self.render_grid_with_search(grid, cursor, selection, colors, &[], None)
    }

    /// Render grid with search highlighting
    pub fn render_grid_with_search(
        &mut self,
        grid: &Grid,
        cursor: &Cursor,
        selection: &SelectionManager,
        colors: &ColorScheme,
        search_matches: &[(u16, u16, u16)], // (row, start_col, end_col)
        current_match: Option<usize>,
    ) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::with_capacity(grid.cols() as usize * grid.lines() as usize * 8);
        let mut indices = Vec::with_capacity(grid.cols() as usize * grid.lines() as usize * 12);

        let default_fg = Self::parse_hex_color(&colors.foreground);
        let default_bg = Self::parse_hex_color(&colors.background);
        let cursor_color = Self::parse_hex_color(&colors.cursor);

        for row in 0..grid.lines() {
            for col in 0..grid.cols() {
                let cell = grid.cell(col, row);
                let cursor_here = cursor.should_draw() && cursor.col == col && cursor.line == row;
                let selected = selection.is_selected(col, row);

                // Check if this cell is in a search match
                let (is_search_match, is_current_match) = search_matches
                    .iter()
                    .enumerate()
                    .find(|(_, (r, start, end))| *r == row && col >= *start && col < *end)
                    .map(|(idx, _)| (true, current_match == Some(idx)))
                    .unwrap_or((false, false));

                self.render_cell_with_search(
                    col,
                    row,
                    cell,
                    cursor_here,
                    cursor.shape,
                    selected,
                    is_search_match,
                    is_current_match,
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

    /// Render a cell with search highlight support
    fn render_cell_with_search(
        &mut self,
        col: u16,
        row: u16,
        cell: &Cell,
        cursor_here: bool,
        cursor_shape: CursorShape,
        selected: bool,
        search_match: bool,
        is_current_match: bool,
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

        // Calculate cell position (with padding offset)
        let x = self.padding + col as f32 * self.cell_width;
        let y = self.padding + row as f32 * self.cell_height;

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

        // Apply search highlighting
        if is_current_match {
            bg = Color::rgb(255, 153, 0); // Orange for current match
            fg = Color::rgb(0, 0, 0);
        } else if search_match {
            bg = Color::rgb(255, 255, 77); // Yellow highlight
            fg = Color::rgb(0, 0, 0);
        }

        // Handle inverse
        if cell.flags.contains(CellFlags::INVERSE) && !search_match {
            std::mem::swap(&mut fg, &mut bg);
        }

        // Handle selection
        if selected && !search_match {
            std::mem::swap(&mut fg, &mut bg);
        }

        // Handle cursor - only invert for block cursor
        if cursor_here && cursor_shape == CursorShape::Block {
            bg = *cursor_color;
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

        // Handle blink - hide text when blink is in "off" phase
        if cell.flags.contains(CellFlags::BLINK) && !self.cell_blink_visible {
            fg = bg;
        }

        let fg_arr = Self::color_to_array(&fg);
        let bg_arr = Self::color_to_array(&bg);

        // Render background
        let x0 = self.to_clip_x(x);
        let y0 = self.to_clip_y(y);
        let x1 = self.to_clip_x(x + cell_w);
        let y1 = self.to_clip_y(y + self.cell_height);

        let base_idx = vertices.len() as u32;
        vertices.push(Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color: fg_arr, bg_color: bg_arr });
        vertices.push(Vertex { position: [x1, y0], tex_coords: [0.0, 0.0], color: fg_arr, bg_color: bg_arr });
        vertices.push(Vertex { position: [x1, y1], tex_coords: [0.0, 0.0], color: fg_arr, bg_color: bg_arr });
        vertices.push(Vertex { position: [x0, y1], tex_coords: [0.0, 0.0], color: fg_arr, bg_color: bg_arr });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);

        // Render cursor for non-block shapes
        if cursor_here && cursor_shape != CursorShape::Block {
            let cursor_fg = Self::color_to_array(cursor_color);
            self.render_cursor_shape(x, y, cell_w, cursor_shape, cursor_fg, bg_arr, vertices, indices);
        }

        // Render glyph
        if !cell.is_empty() {
            self.render_glyph(col, row, cell, fg_arr, bg_arr, vertices, indices);
        }

        // Render underline
        if cell.flags.contains(CellFlags::UNDERLINE) {
            self.render_underline(x, y + self.cell_height - 2.0, cell_w, fg_arr, bg_arr, vertices, indices);
        }

        // Render strikethrough
        if cell.flags.contains(CellFlags::STRIKETHROUGH) {
            let strike_y = y + self.cell_height / 2.0;
            self.render_underline(x, strike_y, cell_w, fg_arr, bg_arr, vertices, indices);
        }
    }

    /// Render a pane border separator
    pub fn render_pane_border(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        horizontal: bool,
        border_color: &Color,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
    ) {
        let color_arr = Self::color_to_array(border_color);
        let border_thickness = 1.0;

        let (x0, y0, x1, y1) = if horizontal {
            // Horizontal border
            (
                self.to_clip_x(x),
                self.to_clip_y(y),
                self.to_clip_x(x + width),
                self.to_clip_y(y + border_thickness),
            )
        } else {
            // Vertical border
            (
                self.to_clip_x(x),
                self.to_clip_y(y),
                self.to_clip_x(x + border_thickness),
                self.to_clip_y(y + height),
            )
        };

        let base_idx = vertices.len() as u32;

        vertices.push(Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color: color_arr, bg_color: color_arr });
        vertices.push(Vertex { position: [x1, y0], tex_coords: [0.0, 0.0], color: color_arr, bg_color: color_arr });
        vertices.push(Vertex { position: [x1, y1], tex_coords: [0.0, 0.0], color: color_arr, bg_color: color_arr });
        vertices.push(Vertex { position: [x0, y1], tex_coords: [0.0, 0.0], color: color_arr, bg_color: color_arr });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);
    }

    /// Render visual bell overlay
    pub fn render_visual_bell(
        &mut self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
    ) {
        // Semi-transparent white flash
        let color = [1.0, 1.0, 1.0, 0.3];

        let base_idx = vertices.len() as u32;

        vertices.push(Vertex { position: [-1.0, 1.0], tex_coords: [0.0, 0.0], color, bg_color: color });
        vertices.push(Vertex { position: [1.0, 1.0], tex_coords: [0.0, 0.0], color, bg_color: color });
        vertices.push(Vertex { position: [1.0, -1.0], tex_coords: [0.0, 0.0], color, bg_color: color });
        vertices.push(Vertex { position: [-1.0, -1.0], tex_coords: [0.0, 0.0], color, bg_color: color });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);
    }

    /// Render status bar at bottom of screen showing window list
    pub fn render_status_bar(
        &mut self,
        windows: &[(String, bool)],  // (name, is_active)
        search_query: Option<&str>,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
    ) {
        let bar_height = self.cell_height;
        let bar_y = self.screen_height - bar_height;

        // Background color for status bar
        let bg_color = [0.1, 0.1, 0.1, 1.0];
        let fg_color = [0.8, 0.8, 0.8, 1.0];
        let active_bg = [0.3, 0.5, 0.7, 1.0];
        let search_bg = [0.5, 0.3, 0.1, 1.0];

        // Render status bar background
        let y0 = self.to_clip_y(bar_y);
        let y1 = self.to_clip_y(bar_y + bar_height);

        let base_idx = vertices.len() as u32;
        vertices.push(Vertex { position: [-1.0, y0], tex_coords: [0.0, 0.0], color: fg_color, bg_color });
        vertices.push(Vertex { position: [1.0, y0], tex_coords: [0.0, 0.0], color: fg_color, bg_color });
        vertices.push(Vertex { position: [1.0, y1], tex_coords: [0.0, 0.0], color: fg_color, bg_color });
        vertices.push(Vertex { position: [-1.0, y1], tex_coords: [0.0, 0.0], color: fg_color, bg_color });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);

        // Render window names
        let mut x = 4.0; // Left padding
        for (idx, (name, is_active)) in windows.iter().enumerate() {
            let item_bg = if *is_active { active_bg } else { bg_color };

            // Format: [idx:name]
            let label = format!("[{}:{}]", idx, if name.len() > 10 { &name[..10] } else { name });
            let label_width = label.len() as f32 * self.cell_width;

            // Render background for this item
            let x0 = self.to_clip_x(x);
            let x1 = self.to_clip_x(x + label_width);

            let base_idx = vertices.len() as u32;
            vertices.push(Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color: fg_color, bg_color: item_bg });
            vertices.push(Vertex { position: [x1, y0], tex_coords: [0.0, 0.0], color: fg_color, bg_color: item_bg });
            vertices.push(Vertex { position: [x1, y1], tex_coords: [0.0, 0.0], color: fg_color, bg_color: item_bg });
            vertices.push(Vertex { position: [x0, y1], tex_coords: [0.0, 0.0], color: fg_color, bg_color: item_bg });

            indices.extend_from_slice(&[
                base_idx, base_idx + 1, base_idx + 2,
                base_idx, base_idx + 2, base_idx + 3,
            ]);

            // Render each character
            for c in label.chars() {
                self.render_status_char(c, x, bar_y, fg_color, item_bg, vertices, indices);
                x += self.cell_width;
            }

            x += self.cell_width; // Space between items
        }

        // Render search query on the right side if active
        if let Some(query) = search_query {
            let search_label = format!("Search: {}_", query);
            let search_width = search_label.len() as f32 * self.cell_width;
            let search_x = self.screen_width - search_width - 4.0;

            // Background for search
            let x0 = self.to_clip_x(search_x);
            let x1 = self.to_clip_x(search_x + search_width);

            let base_idx = vertices.len() as u32;
            vertices.push(Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color: fg_color, bg_color: search_bg });
            vertices.push(Vertex { position: [x1, y0], tex_coords: [0.0, 0.0], color: fg_color, bg_color: search_bg });
            vertices.push(Vertex { position: [x1, y1], tex_coords: [0.0, 0.0], color: fg_color, bg_color: search_bg });
            vertices.push(Vertex { position: [x0, y1], tex_coords: [0.0, 0.0], color: fg_color, bg_color: search_bg });

            indices.extend_from_slice(&[
                base_idx, base_idx + 1, base_idx + 2,
                base_idx, base_idx + 2, base_idx + 3,
            ]);

            // Render search text
            let mut sx = search_x;
            for c in search_label.chars() {
                self.render_status_char(c, sx, bar_y, fg_color, search_bg, vertices, indices);
                sx += self.cell_width;
            }
        }
    }

    /// Render a single character in the status bar
    fn render_status_char(
        &mut self,
        c: char,
        x: f32,
        y: f32,
        fg: [f32; 4],
        bg: [f32; 4],
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
    ) {
        let key = GlyphKey::regular(c);

        let glyph_info = if let Some(info) = self.atlas.get(&key) {
            *info
        } else {
            let rasterized = self.font.rasterize(c);
            match self.atlas.cache_rasterized(key, rasterized) {
                Some(info) => info,
                None => return,
            }
        };

        if glyph_info.width == 0 || glyph_info.height == 0 {
            return;
        }

        let glyph_x = x + glyph_info.metrics.xmin as f32;
        let glyph_y = y + self.ascent - glyph_info.metrics.ymin as f32 - glyph_info.height as f32;

        let x0 = self.to_clip_x(glyph_x);
        let y0 = self.to_clip_y(glyph_y);
        let x1 = self.to_clip_x(glyph_x + glyph_info.width as f32);
        let y1 = self.to_clip_y(glyph_y + glyph_info.height as f32);

        let (atlas_w, atlas_h) = self.atlas.size();
        let u0 = glyph_info.atlas_x as f32 / atlas_w as f32;
        let v0 = glyph_info.atlas_y as f32 / atlas_h as f32;
        let u1 = (glyph_info.atlas_x + glyph_info.width) as f32 / atlas_w as f32;
        let v1 = (glyph_info.atlas_y + glyph_info.height) as f32 / atlas_h as f32;

        let base_idx = vertices.len() as u32;

        vertices.push(Vertex { position: [x0, y0], tex_coords: [u0, v0], color: fg, bg_color: bg });
        vertices.push(Vertex { position: [x1, y0], tex_coords: [u1, v0], color: fg, bg_color: bg });
        vertices.push(Vertex { position: [x1, y1], tex_coords: [u1, v1], color: fg, bg_color: bg });
        vertices.push(Vertex { position: [x0, y1], tex_coords: [u0, v1], color: fg, bg_color: bg });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);
    }

    /// Render copy mode cursor distinctly
    pub fn render_copy_mode_cursor(
        &mut self,
        col: u16,
        row: u16,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
    ) {
        let x = self.padding + col as f32 * self.cell_width;
        let y = self.padding + row as f32 * self.cell_height;

        // Use a distinctive color for copy mode cursor (yellow outline)
        let cursor_color = [1.0, 1.0, 0.0, 0.8];
        let thickness = 2.0;

        // Top edge
        let base_idx = vertices.len() as u32;
        let x0 = self.to_clip_x(x);
        let y0 = self.to_clip_y(y);
        let x1 = self.to_clip_x(x + self.cell_width);
        let y1 = self.to_clip_y(y + thickness);

        vertices.push(Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });
        vertices.push(Vertex { position: [x1, y0], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });
        vertices.push(Vertex { position: [x1, y1], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });
        vertices.push(Vertex { position: [x0, y1], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);

        // Bottom edge
        let base_idx = vertices.len() as u32;
        let y0 = self.to_clip_y(y + self.cell_height - thickness);
        let y1 = self.to_clip_y(y + self.cell_height);

        vertices.push(Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });
        vertices.push(Vertex { position: [x1, y0], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });
        vertices.push(Vertex { position: [x1, y1], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });
        vertices.push(Vertex { position: [x0, y1], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);

        // Left edge
        let base_idx = vertices.len() as u32;
        let y0 = self.to_clip_y(y);
        let y1 = self.to_clip_y(y + self.cell_height);
        let x1 = self.to_clip_x(x + thickness);

        vertices.push(Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });
        vertices.push(Vertex { position: [x1, y0], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });
        vertices.push(Vertex { position: [x1, y1], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });
        vertices.push(Vertex { position: [x0, y1], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);

        // Right edge
        let base_idx = vertices.len() as u32;
        let x0 = self.to_clip_x(x + self.cell_width - thickness);
        let x1 = self.to_clip_x(x + self.cell_width);

        vertices.push(Vertex { position: [x0, y0], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });
        vertices.push(Vertex { position: [x1, y0], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });
        vertices.push(Vertex { position: [x1, y1], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });
        vertices.push(Vertex { position: [x0, y1], tex_coords: [0.0, 0.0], color: cursor_color, bg_color: cursor_color });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);
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
