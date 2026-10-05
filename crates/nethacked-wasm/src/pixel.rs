//! Pixel renderer for the clean terminal page, built on pixel-ssh (the engine
//! behind yemelianov.dev): an 8-bit indexed framebuffer with CP437 and
//! Cyrillic bitmap fonts, retro palettes and WebGL2 CRT effects.
//!
//! The page keeps its game logic in JavaScript and hands this module 24 lines
//! of 80 characters plus one color role per cell.

use pixel_ssh_framebuffer::Framebuffer;
use pixel_ssh_view::{ColorPalette, PaletteMode, ResolutionMode, VisualEffects};

/// Grid the game draws: NetHack's 80x24 (message line, 21 map rows, 2 status rows).
pub const GRID_COLS: u16 = 80;
pub const GRID_ROWS: u16 = 24;
/// Glyph cell width in pixels.
const CELL_W: u16 = 8;

/// Display systems with at least 80 text columns, so the 80x24 game fits.
/// CGA, C64, Atari and ZX Spectrum have 32-40 columns and are left out.
pub const SYSTEMS: [ResolutionMode; 4] = [
    ResolutionMode::Vga,
    ResolutionMode::Ega,
    ResolutionMode::Sga,
    ResolutionMode::Svga,
];

/// Glyph font family for each system (pixel-ssh's `PaletteMode`).
fn font_mode(system: ResolutionMode) -> PaletteMode {
    match system {
        ResolutionMode::Ega => PaletteMode::Ega,
        ResolutionMode::Sga => PaletteMode::Sga,
        ResolutionMode::Svga => PaletteMode::Svga,
        _ => PaletteMode::Vga,
    }
}

/// Default palette for each system.
fn default_palette(system: ResolutionMode) -> ColorPalette {
    match system {
        ResolutionMode::Ega => ColorPalette::Ega,
        _ => ColorPalette::VgaModern,
    }
}

/// Visual effect presets, cycled with F4.
pub fn effect_presets() -> [(&'static str, VisualEffects); 6] {
    [
        ("Default", VisualEffects::default()),
        ("Clean", VisualEffects::clean()),
        ("CRT", VisualEffects::crt_trinitron()),
        ("Arcade", VisualEffects::crt_arcade()),
        ("Bloom", VisualEffects::phosphor_bloom()),
        ("Glitch", VisualEffects::retro_glitch()),
    ]
}

/// Color roles the page assigns to each cell. pixel-ssh palettes use the same
/// UI-role layout for their first 16 entries in every theme, so these map to
/// consistent colors whichever palette is active.
pub mod role {
    pub const TEXT: u8 = 0;
    pub const HERO: u8 = 1;
    pub const MONSTER: u8 = 2;
    pub const GOLD: u8 = 3;
    pub const ITEM: u8 = 4;
    pub const STAIRS: u8 = 5;
    pub const DOOR: u8 = 6;
    pub const WALL: u8 = 7;
    pub const WATER: u8 = 8;
    pub const MESSAGE: u8 = 9;
    pub const ACCENT: u8 = 10;
}

/// Palette index for a color role (pixel-ssh UI roles: 0 background,
/// 4 muted, 5 body text, 6 bright text, 7 accent blue, 8 green, 9 yellow,
/// 10 red, 12 cyan, 14 white).
pub fn palette_index(role: u8) -> u8 {
    match role {
        role::HERO => 14,
        role::MONSTER => 10,
        role::GOLD | role::DOOR => 9,
        role::ITEM => 12,
        role::STAIRS | role::MESSAGE => 6,
        role::WALL => 4,
        role::WATER => 7,
        role::ACCENT => 8,
        _ => 5,
    }
}

/// Framebuffer plus the active system and palette; no browser APIs, so it is
/// testable natively.
pub struct PixelGrid {
    pub fb: Framebuffer,
    pub system: usize,
    pub palette: ColorPalette,
}

impl Default for PixelGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl PixelGrid {
    pub fn new() -> Self {
        let mut grid = Self {
            fb: Framebuffer::new(640, 400),
            system: 0,
            palette: default_palette(SYSTEMS[0]),
        };
        grid.apply_system();
        grid
    }

    pub fn mode(&self) -> ResolutionMode {
        SYSTEMS[self.system]
    }

    fn apply_system(&mut self) {
        let mode = self.mode();
        let (w, h) = mode.resolution();
        self.fb.resize(w, h);
        self.fb.font_mode = font_mode(mode);
        self.fb.font_height = mode.line_height();
        self.fb.set_color_palette(self.palette);
    }

    /// Next display system; resets to that system's default palette.
    pub fn cycle_system(&mut self) {
        self.system = (self.system + 1) % SYSTEMS.len();
        self.palette = default_palette(self.mode());
        self.apply_system();
    }

    pub fn cycle_palette(&mut self) {
        let all = ColorPalette::ALL;
        let i = all.iter().position(|p| *p == self.palette).unwrap_or(0);
        self.palette = all[(i + 1) % all.len()];
        self.fb.set_color_palette(self.palette);
    }

    /// Top-left pixel of the 80x24 game grid, centered in the system's text grid.
    pub fn origin(&self) -> (u16, u16) {
        let (cols, rows) = self.mode().char_grid();
        let x = cols.saturating_sub(GRID_COLS) / 2 * CELL_W;
        let y = rows.saturating_sub(GRID_ROWS) / 2 * self.mode().line_height();
        (x, y)
    }

    /// Draw `text` (up to 24 lines of up to 80 chars) with one color role per
    /// cell (`roles[row * 80 + col]`; missing roles default to plain text).
    pub fn draw(&mut self, text: &str, roles: &[u8]) {
        self.fb.clear(0);
        let (ox, oy) = self.origin();
        let lh = self.mode().line_height();
        for (row, line) in text.split('\n').take(GRID_ROWS as usize).enumerate() {
            for (col, ch) in line.chars().take(GRID_COLS as usize).enumerate() {
                if ch == ' ' {
                    continue;
                }
                let r = roles
                    .get(row * GRID_COLS as usize + col)
                    .copied()
                    .unwrap_or(role::TEXT);
                self.fb.draw_char(
                    ox + col as u16 * CELL_W,
                    oy + row as u16 * lh,
                    ch,
                    palette_index(r),
                    None,
                );
            }
        }
    }

    pub fn label(&self) -> String {
        format!("{} · {}", self.mode().name(), self.palette.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell_has_ink(g: &PixelGrid, col: u16, row: u16) -> Option<u8> {
        let (ox, oy) = g.origin();
        let lh = g.mode().line_height();
        for y in oy + row * lh..oy + (row + 1) * lh {
            for x in ox + col * CELL_W..ox + (col + 1) * CELL_W {
                let p = g.fb.pixels[y as usize * g.fb.width as usize + x as usize];
                if p != 0 {
                    return Some(p);
                }
            }
        }
        None
    }

    #[test]
    fn grid_fits_every_system() {
        let mut g = PixelGrid::new();
        for _ in 0..SYSTEMS.len() {
            let (cols, rows) = g.mode().char_grid();
            assert!(cols >= GRID_COLS && rows >= GRID_ROWS, "{:?}", g.mode());
            let (ox, oy) = g.origin();
            assert!(ox + GRID_COLS * CELL_W <= g.fb.width);
            assert!(oy + GRID_ROWS * g.mode().line_height() <= g.fb.height);
            g.cycle_system();
        }
        assert_eq!(g.mode(), ResolutionMode::Vga, "cycle wraps");
    }

    #[test]
    fn draws_glyph_in_its_cell_with_role_color() {
        let mut g = PixelGrid::new();
        let mut text = String::new();
        for row in 0..GRID_ROWS {
            if row == 5 {
                text.push_str(&format!("{}@", " ".repeat(10)));
            }
            text.push('\n');
        }
        let mut roles = vec![role::TEXT; (GRID_COLS * GRID_ROWS) as usize];
        roles[5 * 80 + 10] = role::HERO;
        g.draw(&text, &roles);
        assert_eq!(cell_has_ink(&g, 10, 5), Some(palette_index(role::HERO)));
        assert_eq!(cell_has_ink(&g, 11, 5), None);
        assert_eq!(cell_has_ink(&g, 10, 4), None);
    }

    #[test]
    fn ukrainian_text_draws_real_glyphs() {
        let mut g = PixelGrid::new();
        g.draw("Ї?", &[]);
        let glyph = |col: u16| {
            let (ox, oy) = g.origin();
            (0..16u16)
                .map(|y| {
                    (0..8u16).fold(0u8, |a, x| {
                        let p = g.fb.pixels
                            [(oy + y) as usize * g.fb.width as usize + (ox + col * 8 + x) as usize];
                        a | ((p != 0) as u8) << (7 - x)
                    })
                })
                .collect::<Vec<u8>>()
        };
        assert!(glyph(0).iter().any(|r| *r != 0));
        assert_ne!(glyph(0), glyph(1), "Ї must not render as '?'");
    }

    #[test]
    fn palette_cycles_through_all_and_back() {
        let mut g = PixelGrid::new();
        let first = g.palette;
        for _ in 0..ColorPalette::ALL.len() {
            g.cycle_palette();
        }
        assert_eq!(g.palette, first);
    }
}

/// Browser wrapper: a [`PixelGrid`] shown on a `<canvas>` through pixel-ssh's
/// WebGL2 renderer with CRT effects. Construction fails without WebGL2; the
/// page then falls back to its text renderer.
#[cfg(target_arch = "wasm32")]
mod screen {
    use super::*;
    use pixel_ssh_render_web::WebGlRenderer;
    use wasm_bindgen::prelude::*;
    use web_sys::HtmlCanvasElement;

    #[wasm_bindgen]
    pub struct PixelScreen {
        grid: PixelGrid,
        renderer: WebGlRenderer,
        canvas: HtmlCanvasElement,
        effects: usize,
        dirty: bool,
    }

    #[wasm_bindgen]
    impl PixelScreen {
        #[wasm_bindgen(constructor)]
        pub fn new(canvas: HtmlCanvasElement) -> Result<PixelScreen, JsError> {
            let grid = PixelGrid::new();
            let renderer = WebGlRenderer::new(canvas.clone(), grid.fb.width, grid.fb.height)
                .map_err(|e| JsError::new(&e))?;
            let mut s = Self {
                grid,
                renderer,
                canvas,
                effects: 0,
                dirty: true,
            };
            s.size_canvas();
            Ok(s)
        }

        /// Canvas backing store at 2x the system resolution, like yemelianov.dev.
        fn size_canvas(&mut self) {
            let (w, h) = (
                self.grid.fb.width as u32 * 2,
                self.grid.fb.height as u32 * 2,
            );
            if self.canvas.width() != w || self.canvas.height() != h {
                self.canvas.set_width(w);
                self.canvas.set_height(h);
            }
        }

        /// Draw 24 lines of text with one color role per cell (see `pixel::role`).
        pub fn draw(&mut self, text: &str, roles: &[u8]) {
            self.grid.draw(text, roles);
            self.dirty = true;
        }

        /// Render one frame; call from `requestAnimationFrame` with seconds since start.
        pub fn frame(&mut self, time_s: f32) {
            let fx = effect_presets()[self.effects].1;
            let animated = fx.jitter > 0.001 || fx.noise > 0.001 || fx.antenna_hum > 0.001;
            if self.dirty || animated {
                self.renderer.render_frame_with_effects(
                    &self.grid.fb,
                    &fx,
                    time_s,
                    (0.5, 0.5),
                    false,
                    self.dirty,
                );
                self.dirty = false;
            }
        }

        /// F2: next display system. Returns its label.
        #[wasm_bindgen(js_name = cycleSystem)]
        pub fn cycle_system(&mut self) -> String {
            self.grid.cycle_system();
            self.size_canvas();
            self.dirty = true;
            self.grid.label()
        }

        /// F3: next palette. Returns its label.
        #[wasm_bindgen(js_name = cyclePalette)]
        pub fn cycle_palette(&mut self) -> String {
            self.grid.cycle_palette();
            self.dirty = true;
            self.grid.label()
        }

        /// F4: next visual effects preset. Returns its name.
        #[wasm_bindgen(js_name = cycleEffects)]
        pub fn cycle_effects(&mut self) -> String {
            self.effects = (self.effects + 1) % effect_presets().len();
            self.dirty = true;
            format!("Effects: {}", effect_presets()[self.effects].0)
        }

        /// Restore a saved state (system, palette and effects indices).
        pub fn restore(&mut self, system: usize, palette: usize, effects: usize) {
            while self.grid.system != system % SYSTEMS.len() {
                self.grid.cycle_system();
            }
            if let Some(p) = ColorPalette::ALL.get(palette) {
                self.grid.palette = *p;
                self.grid.fb.set_color_palette(*p);
            }
            self.effects = effects % effect_presets().len();
            self.size_canvas();
            self.dirty = true;
        }

        /// Current `[system, palette, effects]` indices, for saving.
        pub fn state(&self) -> Vec<u32> {
            let p = ColorPalette::ALL
                .iter()
                .position(|x| *x == self.grid.palette)
                .unwrap_or(0);
            vec![self.grid.system as u32, p as u32, self.effects as u32]
        }
    }
}
#[cfg(target_arch = "wasm32")]
pub use screen::PixelScreen;
