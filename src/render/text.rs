//! Text rendering pipeline
//!
//! Converts terminal grid cells into GPU vertices for rendering.

use crate::term::{Grid, Cell, CellFlags, Color};
use crate::render::atlas::{Atlas, GlyphKey, GlyphInfo};
use crate::render::glyph::Font;
use crate::render::renderer::Vertex;

/// Text renderer that converts terminal state to vertices
pub struct TextRenderer {
    /// Glyph atlas for caching
    atlas: Atlas,
    /// Font for rasterization
    font: Option<Font>,
    /// Cell dimensions
    cell_width: f32,
    cell_height: f32,
    /// Viewport dimensions in pixels
    viewport_width: f32,
    viewport_height: f32,
}

impl TextRenderer {
    /// Create a new text renderer
    pub fn new(atlas_size: u32) -> Self {
        Self {
            atlas: Atlas::new(atlas_size, atlas_size),
            font: None,
            cell_width: 8.0,
            cell_height: 16.0,
            viewport_width: 800.0,
            viewport_height: 600.0,
        }
    }

    /// Set the font
    pub fn set_font(&mut self, font: Font) {
        self.cell_width = font.cell_width();
        self.cell_height = font.line_height();
        self.font = Some(font);
    }

    /// Set viewport size
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    /// Get cell dimensions
    pub fn cell_size(&self) -> (f32, f32) {
        (self.cell_width, self.cell_height)
    }

    /// Get atlas reference (for GPU upload)
    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    /// Get mutable atlas
    pub fn atlas_mut(&mut self) -> &mut Atlas {
        &mut self.atlas
    }

    /// Check if atlas needs GPU upload
    pub fn atlas_dirty(&self) -> bool {
        self.atlas.is_dirty()
    }

    /// Mark atlas as uploaded
    pub fn mark_atlas_clean(&mut self) {
        self.atlas.mark_clean();
    }

    /// Render a grid to vertices
    pub fn render_grid(&mut self, grid: &Grid, cursor_col: u16, cursor_line: u16, cursor_visible: bool) -> (Vec<Vertex>, Vec<u32>) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let cols = grid.cols();
        let rows = grid.lines();

        // Render each cell
        for row in 0..rows {
            for col in 0..cols {
                let cell = grid.cell(col, row);
                self.render_cell(
                    &mut vertices,
                    &mut indices,
                    cell,
                    col,
                    row,
                );
            }
        }

        // Render cursor
        if cursor_visible && cursor_col < cols && cursor_line < rows {
            self.render_cursor(&mut vertices, &mut indices, cursor_col, cursor_line);
        }

        (vertices, indices)
    }

    /// Render a single cell
    fn render_cell(
        &mut self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        cell: &Cell,
        col: u16,
        row: u16,
    ) {
        // Calculate screen position (normalized device coordinates)
        let x = self.col_to_ndc(col as f32);
        let y = self.row_to_ndc(row as f32);
        let w = self.cell_width / self.viewport_width * 2.0;
        let h = self.cell_height / self.viewport_height * 2.0;

        // Get colors (handle inverse)
        let (fg, bg) = if cell.flags.contains(CellFlags::INVERSE) {
            (cell.bg, cell.fg)
        } else {
            (cell.fg, cell.bg)
        };

        let fg_arr = color_to_array(&fg);
        let bg_arr = color_to_array(&bg);

        // Always render background quad
        let base_idx = vertices.len() as u32;
        
        // Background quad (full cell)
        vertices.push(Vertex {
            position: [x, y],
            tex_coords: [0.0, 0.0],
            color: fg_arr,
            bg_color: bg_arr,
        });
        vertices.push(Vertex {
            position: [x + w, y],
            tex_coords: [0.0, 0.0],
            color: fg_arr,
            bg_color: bg_arr,
        });
        vertices.push(Vertex {
            position: [x + w, y - h],
            tex_coords: [0.0, 0.0],
            color: fg_arr,
            bg_color: bg_arr,
        });
        vertices.push(Vertex {
            position: [x, y - h],
            tex_coords: [0.0, 0.0],
            color: fg_arr,
            bg_color: bg_arr,
        });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);

        // Skip glyph rendering for space or empty
        if cell.c == ' ' || cell.c == '\0' {
            return;
        }

        // Skip wide spacer cells
        if cell.flags.contains(CellFlags::WIDE_SPACER) {
            return;
        }

        // Get glyph info from atlas
        let flags = self.cell_flags_to_glyph_flags(&cell.flags);
        let key = GlyphKey::new(cell.c, flags);

        if let Some(font) = &self.font {
            if let Some(glyph_info) = self.atlas.cache(key, font) {
                if glyph_info.width > 0 && glyph_info.height > 0 {
                    self.render_glyph(vertices, indices, &glyph_info, x, y, w, h, fg_arr, bg_arr);
                }
            }
        }
    }

    /// Render a glyph quad
    fn render_glyph(
        &self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        glyph: &GlyphInfo,
        cell_x: f32,
        cell_y: f32,
        cell_w: f32,
        cell_h: f32,
        fg: [f32; 4],
        bg: [f32; 4],
    ) {
        let atlas_size = self.atlas.size().0 as f32;

        // Calculate glyph position within cell
        let glyph_x = cell_x + (glyph.metrics.xmin as f32 / self.viewport_width * 2.0);
        let glyph_y = cell_y - ((self.cell_height - glyph.metrics.ymin as f32 - glyph.height as f32) / self.viewport_height * 2.0);
        let glyph_w = glyph.width as f32 / self.viewport_width * 2.0;
        let glyph_h = glyph.height as f32 / self.viewport_height * 2.0;

        // Texture coordinates
        let tx0 = glyph.atlas_x as f32 / atlas_size;
        let ty0 = glyph.atlas_y as f32 / atlas_size;
        let tx1 = (glyph.atlas_x + glyph.width) as f32 / atlas_size;
        let ty1 = (glyph.atlas_y + glyph.height) as f32 / atlas_size;

        let base_idx = vertices.len() as u32;

        vertices.push(Vertex {
            position: [glyph_x, glyph_y],
            tex_coords: [tx0, ty0],
            color: fg,
            bg_color: bg,
        });
        vertices.push(Vertex {
            position: [glyph_x + glyph_w, glyph_y],
            tex_coords: [tx1, ty0],
            color: fg,
            bg_color: bg,
        });
        vertices.push(Vertex {
            position: [glyph_x + glyph_w, glyph_y - glyph_h],
            tex_coords: [tx1, ty1],
            color: fg,
            bg_color: bg,
        });
        vertices.push(Vertex {
            position: [glyph_x, glyph_y - glyph_h],
            tex_coords: [tx0, ty1],
            color: fg,
            bg_color: bg,
        });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);
    }

    /// Render cursor
    fn render_cursor(
        &self,
        vertices: &mut Vec<Vertex>,
        indices: &mut Vec<u32>,
        col: u16,
        row: u16,
    ) {
        let x = self.col_to_ndc(col as f32);
        let y = self.row_to_ndc(row as f32);
        let w = self.cell_width / self.viewport_width * 2.0;
        let h = self.cell_height / self.viewport_height * 2.0;

        // Cursor color (white, semi-transparent)
        let cursor_color = [1.0, 1.0, 1.0, 0.7];
        let bg = [0.0, 0.0, 0.0, 0.0];

        let base_idx = vertices.len() as u32;

        // Block cursor
        vertices.push(Vertex {
            position: [x, y],
            tex_coords: [0.0, 0.0],
            color: cursor_color,
            bg_color: bg,
        });
        vertices.push(Vertex {
            position: [x + w, y],
            tex_coords: [0.0, 0.0],
            color: cursor_color,
            bg_color: bg,
        });
        vertices.push(Vertex {
            position: [x + w, y - h],
            tex_coords: [0.0, 0.0],
            color: cursor_color,
            bg_color: bg,
        });
        vertices.push(Vertex {
            position: [x, y - h],
            tex_coords: [0.0, 0.0],
            color: cursor_color,
            bg_color: bg,
        });

        indices.extend_from_slice(&[
            base_idx, base_idx + 1, base_idx + 2,
            base_idx, base_idx + 2, base_idx + 3,
        ]);
    }

    /// Convert column to NDC x coordinate
    fn col_to_ndc(&self, col: f32) -> f32 {
        (col * self.cell_width / self.viewport_width) * 2.0 - 1.0
    }

    /// Convert row to NDC y coordinate (top = 1, bottom = -1)
    fn row_to_ndc(&self, row: f32) -> f32 {
        1.0 - (row * self.cell_height / self.viewport_height) * 2.0
    }

    /// Convert cell flags to glyph flags
    fn cell_flags_to_glyph_flags(&self, flags: &CellFlags) -> u8 {
        let mut glyph_flags = 0u8;
        if flags.contains(CellFlags::BOLD) {
            glyph_flags |= 1;
        }
        if flags.contains(CellFlags::ITALIC) {
            glyph_flags |= 2;
        }
        glyph_flags
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_renderer_new() {
        let renderer = TextRenderer::new(512);
        assert_eq!(renderer.atlas().size(), (512, 512));
    }

    #[test]
    fn text_renderer_cell_size() {
        let renderer = TextRenderer::new(512);
        let (w, h) = renderer.cell_size();
        assert!(w > 0.0);
        assert!(h > 0.0);
    }

    #[test]
    fn text_renderer_viewport() {
        let mut renderer = TextRenderer::new(512);
        renderer.set_viewport(1024.0, 768.0);
        assert_eq!(renderer.viewport_width, 1024.0);
        assert_eq!(renderer.viewport_height, 768.0);
    }

    #[test]
    fn text_renderer_col_to_ndc() {
        let renderer = TextRenderer::new(512);
        // At col 0, should be at left edge (-1)
        let x = renderer.col_to_ndc(0.0);
        assert!((x - (-1.0)).abs() < 0.01);
    }

    #[test]
    fn text_renderer_row_to_ndc() {
        let renderer = TextRenderer::new(512);
        // At row 0, should be at top edge (1)
        let y = renderer.row_to_ndc(0.0);
        assert!((y - 1.0).abs() < 0.01);
    }

    #[test]
    fn text_renderer_render_empty_grid() {
        let mut renderer = TextRenderer::new(512);
        let grid = Grid::new(10, 5, 100);

        let (vertices, indices) = renderer.render_grid(&grid, 0, 0, false);

        // Should have vertices for each cell (4 per cell, 10x5 = 50 cells)
        assert_eq!(vertices.len(), 50 * 4);
        // 6 indices per quad
        assert_eq!(indices.len(), 50 * 6);
    }

    #[test]
    fn text_renderer_render_with_cursor() {
        let mut renderer = TextRenderer::new(512);
        let grid = Grid::new(10, 5, 100);

        let (vertices, indices) = renderer.render_grid(&grid, 5, 2, true);

        // 50 cells + 1 cursor = 51 quads
        assert_eq!(vertices.len(), 51 * 4);
        assert_eq!(indices.len(), 51 * 6);
    }

    #[test]
    fn text_renderer_atlas_dirty() {
        let renderer = TextRenderer::new(512);
        assert!(!renderer.atlas_dirty());
    }

    #[test]
    fn cell_flags_to_glyph_flags() {
        let renderer = TextRenderer::new(512);

        let empty = CellFlags::empty();
        assert_eq!(renderer.cell_flags_to_glyph_flags(&empty), 0);

        let bold = CellFlags::BOLD;
        assert_eq!(renderer.cell_flags_to_glyph_flags(&bold), 1);

        let italic = CellFlags::ITALIC;
        assert_eq!(renderer.cell_flags_to_glyph_flags(&italic), 2);

        let both = CellFlags::BOLD | CellFlags::ITALIC;
        assert_eq!(renderer.cell_flags_to_glyph_flags(&both), 3);
    }
}
