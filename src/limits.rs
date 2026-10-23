use crate::{Error, Result};

/// Bounds on input and output, checked while text is parsed, before images are
/// allocated and while PNG files are decoded.
///
/// The defaults suit hand-written files and untrusted input. Raise them for trusted
/// input that needs more:
///
/// ```
/// use spritebox::{Document, Limits};
///
/// let limits = Limits { max_sprite_side: 4096, ..Limits::default() };
/// let document = Document::parse_with("palette p\n  x #ff0000\n", &limits)?;
/// # Ok::<(), spritebox::Error>(())
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Length of the text in bytes. Default 4 MiB.
    pub max_text_bytes: usize,
    /// Number of top-level definitions: palettes, sprites and so on. Default 4096.
    pub max_definitions: usize,
    /// Keys in one palette, inherited keys included. Default 256.
    pub max_palette_colors: usize,
    /// Width or height of one sprite in pixels. Default 1024.
    pub max_sprite_side: u32,
    /// Layers in one sprite, layers of sprites used as layers included. Default 16.
    pub max_layers: usize,
    /// Frames in one animation. Default 1024.
    pub max_frames: usize,
    /// Tiles in one tileset, terrain variants included. Default 4096.
    pub max_tiles: usize,
    /// Width or height of one tilemap in tiles. Default 1024.
    pub max_map_side: u32,
    /// Glyphs in one font. Default 4096.
    pub max_glyphs: usize,
    /// Kerning pairs in one font. Default 4096.
    pub max_kerning_pairs: usize,
    /// Files in an export, including theme variants. Default 8192.
    pub max_export_files: usize,
    /// Placements in one screen. Default 1024.
    pub max_screen_items: usize,
    /// Authored scene operations and list records. Default 4096.
    pub max_scene_commands: usize,
    /// Retained scene cells, profile entries, placements and operation images. Default 16 Mi.
    pub max_scene_pixels: u64,
    /// Raster samples, edge tests, noise octaves and scatter work per operation. Default 64 Mi.
    pub max_scene_work: u64,
    /// Candidate attempts across all scatter groups in a document. Default 1,000,000.
    pub max_scatter_attempts: u64,
    /// Depth of definitions built on other definitions: palettes copied `from=`
    /// another, and sprites derived from sprites. Default 32.
    pub max_nesting: usize,
    /// Width or height of any image the crate creates or decodes, previews and
    /// atlases included. Default 8192.
    pub max_image_side: u32,
    /// Pixels of any image the crate creates or decodes, and of all the sprites of a
    /// document together. Default 16 Mi (64 MiB of RGBA8).
    pub max_image_pixels: u64,
    /// Warnings that [`Document::lint`](crate::Document::lint) keeps; its counts
    /// include those left out. Default 1000.
    pub max_warnings: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_text_bytes: 4 * 1024 * 1024,
            max_definitions: 4096,
            max_palette_colors: 256,
            max_sprite_side: 1024,
            max_layers: 16,
            max_frames: 1024,
            max_tiles: 4096,
            max_map_side: 1024,
            max_glyphs: 4096,
            max_kerning_pairs: 4096,
            max_export_files: 8192,
            max_screen_items: 1024,
            max_scene_commands: 4096,
            max_scene_pixels: 16 * 1024 * 1024,
            max_scene_work: 64 * 1024 * 1024,
            max_scatter_attempts: 1_000_000,
            max_nesting: 32,
            max_image_side: 8192,
            max_image_pixels: 16 * 1024 * 1024,
            max_warnings: 1000,
        }
    }
}

impl Limits {
    /// Checks that an image of `width` × `height` pixels is within the image limits.
    pub fn check_image(&self, width: u32, height: u32) -> Result<()> {
        if width as u64 * height as u64 > isize::MAX as u64 / 4 {
            return Err(Error::limit(
                "Image bytes exceed the platform allocation limit",
            ));
        }
        if width > self.max_image_side || height > self.max_image_side {
            return Err(Error::limit(format!(
                "Image of {width}x{height} pixels is larger than the limit of {} pixels a side",
                self.max_image_side
            )));
        }
        if width as u64 * height as u64 > self.max_image_pixels {
            return Err(Error::limit(format!(
                "Image of {width}x{height} pixels has more than the limit of {} pixels",
                self.max_image_pixels
            )));
        }
        Ok(())
    }
}
