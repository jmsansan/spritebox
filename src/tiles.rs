//! Tilesets, autotiling and tilemaps.

use crate::sprite::{EMPTY, Layer};
use crate::{Color, Error, Image, Limits, Palette, Result, Sprite};

/// Neighbour bits of an autotile mask, clockwise from north.
pub mod neighbours {
    pub const N: u8 = 1;
    pub const NE: u8 = 2;
    pub const E: u8 = 4;
    pub const SE: u8 = 8;
    pub const S: u8 = 16;
    pub const SW: u8 = 32;
    pub const W: u8 = 64;
    pub const NW: u8 = 128;
}
use neighbours::*;

/// An autotiling scheme: which neighbours choose a terrain's variant.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Scheme {
    /// The four edge neighbours: 16 variants, from 4 pieces. Written `16`.
    Edges,
    /// All eight neighbours, the "blob" scheme: 47 variants, from 5 pieces. Written
    /// `47`.
    Blob,
}

/// The 47 masks of the blob scheme in variant order: every 8-bit neighbour mask
/// whose corner bits are set only where both neighbouring edge bits are, ascending.
pub const BLOB_MASKS: [u8; 47] = blob_masks();

const fn blob_masks() -> [u8; 47] {
    let mut out = [0u8; 47];
    let mut n = 0;
    let mut m = 0u32;
    while m < 256 {
        if reduce(m as u8) == m as u8 {
            out[n] = m as u8;
            n += 1;
        }
        m += 1;
    }
    out
}

/// Clears each corner bit whose two neighbouring edge bits are not both set: a
/// corner only matters when both edges beside it connect.
pub const fn reduce(mask: u8) -> u8 {
    let mut m = mask & (N | E | S | W);
    if mask & NE != 0 && mask & N != 0 && mask & E != 0 {
        m |= NE;
    }
    if mask & SE != 0 && mask & S != 0 && mask & E != 0 {
        m |= SE;
    }
    if mask & SW != 0 && mask & S != 0 && mask & W != 0 {
        m |= SW;
    }
    if mask & NW != 0 && mask & N != 0 && mask & W != 0 {
        m |= NW;
    }
    m
}

impl Scheme {
    /// How many variants a terrain of this scheme has: 16 or 47.
    pub fn variants(self) -> u32 {
        match self {
            Scheme::Edges => 16,
            Scheme::Blob => 47,
        }
    }

    /// The variant for an 8-bit neighbour mask (see [`neighbours`]). [`Scheme::Edges`]
    /// ignores the corner bits: its variant is `N + 2E + 4S + 8W`, each 1 when that
    /// neighbour is set. [`Scheme::Blob`] reduces the mask with [`reduce`] and
    /// returns its position in [`BLOB_MASKS`].
    pub fn variant(self, mask: u8) -> u32 {
        match self {
            Scheme::Edges => {
                (mask & N != 0) as u32
                    | ((mask & E != 0) as u32) << 1
                    | ((mask & S != 0) as u32) << 2
                    | ((mask & W != 0) as u32) << 3
            }
            Scheme::Blob => BLOB_MASKS.binary_search(&reduce(mask)).unwrap_or(0) as u32,
        }
    }

    /// The neighbour mask that variant `variant` stands for.
    pub fn mask(self, variant: u32) -> u8 {
        match self {
            Scheme::Edges => {
                let v = variant as u8;
                (if v & 1 != 0 { N } else { 0 })
                    | (if v & 2 != 0 { E } else { 0 })
                    | (if v & 4 != 0 { S } else { 0 })
                    | (if v & 8 != 0 { W } else { 0 })
            }
            Scheme::Blob => BLOB_MASKS.get(variant as usize).copied().unwrap_or(0),
        }
    }
}

/// The hand-drawn pieces of a terrain, each a whole tile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Piece {
    /// A tile with no neighbours: edges on all four sides, outer corners.
    Island,
    /// A tile in a horizontal strip, with neighbours east and west: edges top and
    /// bottom.
    HStrip,
    /// A tile in a vertical strip, with neighbours north and south: edges left and
    /// right.
    VStrip,
    /// A tile with all four edge neighbours but no corner ones: an inner corner in
    /// each quarter. Only [`Scheme::Blob`] uses it.
    Inner,
    /// A tile surrounded on all sides: no edges.
    Fill,
}

impl Piece {
    pub const ALL: [Piece; 5] = [
        Piece::Island,
        Piece::HStrip,
        Piece::VStrip,
        Piece::Inner,
        Piece::Fill,
    ];

    /// The name written in the text format.
    pub fn name(self) -> &'static str {
        match self {
            Piece::Island => "island",
            Piece::HStrip => "h_strip",
            Piece::VStrip => "v_strip",
            Piece::Inner => "inner",
            Piece::Fill => "fill",
        }
    }

    /// The piece that a quarter of a tile comes from, given whether the tile's
    /// neighbours beside that quarter connect: `vertical` (north for the top quarters,
    /// south for the bottom ones), `horizontal` (west for the left quarters, east for
    /// the right ones) and the `corner` between them.
    pub fn for_quarter(vertical: bool, horizontal: bool, corner: bool) -> Piece {
        match (vertical, horizontal, corner) {
            (false, false, _) => Piece::Island,
            (false, true, _) => Piece::HStrip,
            (true, false, _) => Piece::VStrip,
            (true, true, false) => Piece::Inner,
            (true, true, true) => Piece::Fill,
        }
    }
}

/// Builds the tile for neighbour `mask` from `pieces`, indexed like [`Piece::ALL`]:
/// each quarter of the tile is copied from the same quarter of the piece that
/// [`Piece::for_quarter`] picks. With [`Scheme::Edges`], corners always connect.
pub(crate) fn assemble(pieces: &[Sprite; 5], scheme: Scheme, mask: u8, name: String) -> Sprite {
    let first = &pieces[0];
    let (w, h) = (first.width, first.height);
    let (hw, hh) = (w / 2, h / 2);
    let layers = pieces.iter().map(|p| p.layers.len()).max().unwrap_or(1);
    let mut out: Vec<Vec<char>> = vec![Vec::with_capacity((w * h) as usize); layers];
    let corner = |bit: u8| scheme == Scheme::Edges || mask & bit != 0;
    for y in 0..h {
        for x in 0..w {
            let (top, left) = (y < hh, x < hw);
            let vertical = mask & if top { N } else { S } != 0;
            let horizontal = mask & if left { W } else { E } != 0;
            let diagonal = corner(match (top, left) {
                (true, true) => NW,
                (true, false) => NE,
                (false, true) => SW,
                (false, false) => SE,
            });
            let piece = Piece::for_quarter(vertical, horizontal, diagonal);
            let source = &pieces[Piece::ALL.iter().position(|&p| p == piece).unwrap_or(0)];
            // Pieces with fewer layers are aligned to the top.
            let missing = layers - source.layers.len();
            let i = (y * w + x) as usize;
            for (l, layer) in out.iter_mut().enumerate() {
                layer.push(if l < missing {
                    EMPTY
                } else {
                    source.layers[l - missing].keys[i]
                });
            }
        }
    }
    Sprite {
        name,
        width: w,
        height: h,
        palette: first.palette.clone(),
        layers: out
            .into_iter()
            .map(|keys| Layer { name: None, keys })
            .collect(),
        slice: None,
    }
}

/// A terrain of a tileset: the variants that autotiling chooses from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Terrain {
    pub(crate) name: String,
    pub(crate) scheme: Scheme,
    pub(crate) first: u32,
}

impl Terrain {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn scheme(&self) -> Scheme {
        self.scheme
    }
    /// The tile index of variant 0; the variants follow it in order.
    pub fn first_index(&self) -> u32 {
        self.first
    }
    /// The tile index for a neighbour mask (see [`neighbours`]).
    pub fn index(&self, mask: u8) -> u32 {
        self.first + self.scheme.variant(mask)
    }
}

/// Tiles of one size and palette, in one texture.
///
/// Tile indices start at 1, so that 0 means an empty cell in a tilemap. Tiles are
/// numbered in the order written; a terrain's variants take consecutive indices.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tileset {
    pub(crate) name: String,
    pub(crate) tile_width: u32,
    pub(crate) tile_height: u32,
    pub(crate) columns: u32,
    pub(crate) palette: Palette,
    pub(crate) tiles: Vec<Sprite>,
    pub(crate) terrains: Vec<Terrain>,
}

impl Tileset {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn tile_width(&self) -> u32 {
        self.tile_width
    }
    pub fn tile_height(&self) -> u32 {
        self.tile_height
    }
    /// Tiles in a row of the texture.
    pub fn columns(&self) -> u32 {
        self.columns
    }
    pub fn palette(&self) -> &Palette {
        &self.palette
    }
    /// How many tiles there are; indices run from 1 to this.
    pub fn tile_count(&self) -> u32 {
        self.tiles.len() as u32
    }
    /// The tile at `index`, from 1. A terrain's variants are named `TERRAIN/VARIANT`.
    pub fn tile(&self, index: u32) -> Option<&Sprite> {
        self.tiles.get((index as usize).checked_sub(1)?)
    }
    /// The index of the tile named `name`, such as `brick` or `dirt/5`.
    pub fn index(&self, name: &str) -> Option<u32> {
        self.tiles
            .iter()
            .position(|t| t.name == name)
            .map(|i| i as u32 + 1)
    }
    /// The tiles, in index order.
    pub fn tiles(&self) -> &[Sprite] {
        &self.tiles
    }
    pub fn terrains(&self) -> &[Terrain] {
        &self.terrains
    }
    pub fn terrain(&self, name: &str) -> Option<&Terrain> {
        self.terrains.iter().find(|t| t.name == name)
    }
    /// Where tile `index` is in the texture, in texels.
    pub fn tile_rect(&self, index: u32) -> Option<crate::Rect> {
        let i = index.checked_sub(1).filter(|&i| i < self.tile_count())?;
        Some(crate::Rect {
            x: i % self.columns * self.tile_width,
            y: i / self.columns * self.tile_height,
            width: self.tile_width,
            height: self.tile_height,
        })
    }
    /// The texture's size in texels: `columns` tiles wide, as many rows as needed.
    pub fn image_size(&self) -> (u32, u32) {
        let rows = self.tile_count().div_ceil(self.columns).max(1);
        (self.columns * self.tile_width, rows * self.tile_height)
    }
    /// The texture's size with `padding` pixels around each tile.
    pub fn padded_size(&self, padding: u32) -> (u64, u64) {
        let rows = self.tile_count().div_ceil(self.columns).max(1) as u64;
        let cell = |side: u32| side as u64 + 2 * padding as u64;
        (
            self.columns as u64 * cell(self.tile_width),
            rows * cell(self.tile_height),
        )
    }

    /// The texture with `padding` pixels around each tile, extruded from its edges
    /// when `extrude` is set. The caller checks [`Tileset::padded_size`].
    pub(crate) fn render_padded(&self, padding: u32, extrude: bool) -> Image {
        let (w, h) = self.padded_size(padding);
        let mut image = Image::blank(w as u32, h as u32);
        for (i, tile) in self.tiles.iter().enumerate() {
            let (col, row) = (i as u32 % self.columns, i as u32 / self.columns);
            let x = padding + col * (self.tile_width + 2 * padding);
            let y = padding + row * (self.tile_height + 2 * padding);
            crate::atlas::place(&mut image, &tile.render(), x, y, padding, extrude);
        }
        image
    }

    /// The texture: tile 1 at the top left, then row by row, with no spacing.
    pub fn render(&self) -> Image {
        let (w, h) = self.image_size();
        let mut image = Image::blank(w, h);
        for (i, tile) in self.tiles.iter().enumerate() {
            if let Some(r) = self.tile_rect(i as u32 + 1) {
                image.copy_from(&tile.render(), r.x as i64, r.y as i64);
            }
        }
        image
    }
}

/// A grid of tile indices of one tileset, with the indices that are solid.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tilemap {
    pub(crate) name: String,
    pub(crate) tileset: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) tiles: Vec<u32>,
    pub(crate) solid: Vec<u32>,
}

impl Tilemap {
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The name of the tileset whose indices the map holds.
    pub fn tileset(&self) -> &str {
        &self.tileset
    }
    /// Width in tiles.
    pub fn width(&self) -> u32 {
        self.width
    }
    /// Height in tiles.
    pub fn height(&self) -> u32 {
        self.height
    }
    /// The tile indices, row by row from the top; 0 is an empty cell.
    pub fn tiles(&self) -> &[u32] {
        &self.tiles
    }
    /// The tile index at `(x, y)`, or `None` outside the map.
    pub fn tile(&self, x: u32, y: u32) -> Option<u32> {
        (x < self.width && y < self.height).then(|| self.tiles[(y * self.width + x) as usize])
    }
    /// The indices of solid tiles, ascending.
    pub fn solid(&self) -> &[u32] {
        &self.solid
    }
    pub fn is_solid(&self, index: u32) -> bool {
        self.solid.binary_search(&index).is_ok()
    }
    /// Draws the map with `tileset`'s tiles, within the default [`Limits`].
    pub fn render(&self, tileset: &Tileset) -> Result<Image> {
        self.render_with(tileset, &Limits::default())
    }
    /// [`Tilemap::render`] within `limits`.
    pub fn render_with(&self, tileset: &Tileset, limits: &Limits) -> Result<Image> {
        self.render_part(tileset, self.width, self.height, limits)
    }

    /// Draws the top-left `columns` × `rows` tiles of the map.
    pub(crate) fn render_part(
        &self,
        tileset: &Tileset,
        columns: u32,
        rows: u32,
        limits: &Limits,
    ) -> Result<Image> {
        let (columns, rows) = (columns.min(self.width), rows.min(self.height));
        let w = columns as u64 * tileset.tile_width as u64;
        let h = rows as u64 * tileset.tile_height as u64;
        if w > u32::MAX as u64 || h > u32::MAX as u64 {
            return Err(Error::limit("The tilemap is too large to draw"));
        }
        Image::check_size(w as u32, h as u32, limits)?;
        let images: Vec<Image> = tileset.tiles.iter().map(|t| t.render()).collect();
        let mut image = Image::blank(w as u32, h as u32);
        for y in 0..rows {
            for x in 0..columns {
                let index = self.tiles[(y * self.width + x) as usize];
                if let Some(tile) = index.checked_sub(1).and_then(|i| images.get(i as usize)) {
                    image.copy_from(
                        tile,
                        (x * tileset.tile_width) as i64,
                        (y * tileset.tile_height) as i64,
                    );
                }
            }
        }
        Ok(image)
    }

    /// Like [`Tilemap::render_part`], with solid tiles tinted.
    pub(crate) fn render_solid(
        &self,
        tileset: &Tileset,
        columns: u32,
        rows: u32,
        limits: &Limits,
    ) -> Result<Image> {
        let mut image = self.render_part(tileset, columns, rows, limits)?;
        let tint = Color::rgba(255, 40, 40, 120);
        for y in 0..rows.min(self.height) {
            for x in 0..columns.min(self.width) {
                if self.is_solid(self.tiles[(y * self.width + x) as usize]) {
                    image.fill_rect(
                        (x * tileset.tile_width) as i64,
                        (y * tileset.tile_height) as i64,
                        tileset.tile_width,
                        tileset.tile_height,
                        tint,
                    );
                }
            }
        }
        Ok(image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_masks() {
        assert_eq!(BLOB_MASKS[0], 0);
        assert_eq!(BLOB_MASKS[46], 255);
        assert!(BLOB_MASKS.windows(2).all(|w| w[0] < w[1]));
        // Every mask maps to a variant whose mask is its reduction.
        for m in 0..=255u8 {
            let v = Scheme::Blob.variant(m);
            assert!(v < 47);
            assert_eq!(Scheme::Blob.mask(v), reduce(m));
            let e = Scheme::Edges.variant(m);
            assert!(e < 16);
            assert_eq!(Scheme::Edges.mask(e), m & (N | E | S | W));
        }
    }

    #[test]
    fn quarters() {
        use Piece::*;
        assert_eq!(Piece::for_quarter(false, false, true), Island);
        assert_eq!(Piece::for_quarter(false, true, true), HStrip);
        assert_eq!(Piece::for_quarter(true, false, false), VStrip);
        assert_eq!(Piece::for_quarter(true, true, false), Inner);
        assert_eq!(Piece::for_quarter(true, true, true), Fill);
    }
}
