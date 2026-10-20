//! 2D pixel art as plain text, in safe Rust with zero dependencies.
//!
//! A spritebox file defines palettes, sprites, procedural textures, animations,
//! tilesets, tilemaps and pixel fonts as text that a person or an AI assistant can write, read and diff. The crate
//! validates it with exact diagnostics, renders it to RGBA8 images and PNG files, and
//! draws preview sheets to review a whole file at a glance. The same text gives the same pixels on every
//! platform.
//!
//! ```
//! use spritebox::{Color, Document, PreviewOptions};
//!
//! let document: Document = "
//! palette dawn
//!   .  transparent
//!   k  #1a1c2c
//!   o  #f4b41b
//!   y  #ffe478
//!   w  #ffffff
//!
//! sprite coin 8x8 palette=dawn
//!   ..kkkk..
//!   .kooyyk.
//!   kooyywok
//!   kooyyook
//!   kooooook
//!   koooooyk
//!   .koooyk.
//!   ..kkkk..
//! ".parse()?;
//! let coin = document.sprite("coin").unwrap().render();
//! assert_eq!(coin.pixel(5, 2), Some(Color::WHITE));
//! let png = coin.to_png();
//! let sheet = document.preview(&PreviewOptions::default())?;
//! # Ok::<(), spritebox::Error>(())
//! ```
//!
//! # The text format
//!
//! A definition starts at the first column with a directive and a name; the lines
//! under it are indented with spaces (any number, the same for every line of a
//! block). `;` starts a comment that runs to the end of the line. Blank lines are
//! ignored. Names use ASCII letters, digits, `_` and `-`.
//!
//! `import NAME` brings in the definitions of another file, loaded by a function the
//! caller gives [`Document::parse_with_loader`]; the crate itself reads no files.
//!
//! ## Palettes
//!
//! ```text
//! palette dawn
//!   .  transparent
//!   k  #1a1c2c
//!   o  #f4b41b80
//!
//! palette dusk from=dawn    ; dawn's keys, then these replace or add
//!   o  #b86f50
//! ```
//!
//! Each line is a key, one character, and a color: `#rrggbb`, `#rrggbbaa` or
//! `transparent`, in sRGB with alpha that is not premultiplied. Keys may be any
//! character but spaces, control characters and `; ( ) , =`. A palette may hold at
//! most [`Limits::max_palette_colors`] keys.
//!
//! ## Sprites
//!
//! ```text
//! sprite coin 8x8 palette=dawn
//!   ..kkkk..
//!   ...
//! ```
//!
//! `WxH` is the size in pixels, and each of the `H` rows is one word of `W` keys of
//! the palette. `palette=` may be left out when the file defines one palette.
//! `slice=N` or `slice=LEFT,TOP,RIGHT,BOTTOM` marks a nine-slice panel, whose
//! [`Insets`] the metadata states.
//!
//! Instead of rows, a sprite may have layers, drawn bottom first, each with rows or
//! made from another sprite of the same size and palette:
//!
//! ```text
//! sprite hero 16x16 palette=dawn
//!   layer body
//!     ...
//!   layer shadow = blob:offset(1,1)
//! ```
//!
//! ## Procedural textures
//!
//! ```text
//! texture grass 16x16 palette=dawn seed=7
//!   noise cell=8 octaves=3 roughness=60
//!   ramp h g g G
//!   dither bayer4
//! ```
//!
//! Seeded value [`Noise`] that wraps, mapped onto a ramp of keys with [`Dither`]; a
//! texture is a sprite, usable as a layer, a tile or anything else a sprite is.
//!
//! ## Derived sprites
//!
//! `sprite NAME = CHAIN` builds a sprite from another with a modifier chain: a name
//! and `:modifier`s applied left to right, without spaces, such as
//! `coin:flip_x:swap(dusk):outline(k)`. The modifiers are `flip_x`, `flip_y`, `rot90`,
//! `rot180`, `rot270` (clockwise), `offset(dx,dy)`, `pad(n)`, `swap(palette)` and
//! `outline(key)`; [`Sprite`] has a method for each.
//!
//! ## Animations
//!
//! ```text
//! animation coin_spin loop
//!   coin              6
//!   coin_edge         6
//!   coin_edge:flip_x  6
//! ```
//!
//! One frame per line: a sprite or a modifier chain, and the ticks it shows (Diamon
//! steps at 60 a second). `loop`, the default, or `once`. A chain's trailing flips are
//! flags of the [`Frame`], since engines flip sprites when they draw them.
//!
//! ## Tilesets and tilemaps
//!
//! ```text
//! tileset ground 8x8 palette=dawn
//!   tile brick
//!     ...rows...
//!   terrain dirt 47
//!     island
//!       ...rows...
//!     h_strip
//!     v_strip
//!     inner
//!     fill
//!
//! tilemap level tileset=ground
//!   legend
//!     .  empty
//!     #  dirt   solid
//!     b  brick  solid
//!   map
//!     ..bb..
//!     ######
//! ```
//!
//! A terrain's pieces are whole tiles; spritebox assembles its 16 or 47 variants
//! from their quarters (see [`Piece::for_quarter`] and [`Scheme::variant`]). A map
//! cell of a terrain takes the variant for its neighbours. [`Tileset`] and
//! [`Tilemap`] hold the result: tile indices from 1, 0 for an empty cell, and the
//! solid indices. The README documents the rules in full.
//!
//! ## Pixel fonts
//!
//! ```text
//! font hud height=7 baseline=7 line_height=9
//!   glyph A
//!     .###.
//!     #...#
//!     ...
//! ```
//!
//! Rows of `#` and `.`; characters as themselves or `U+003B`. [`Font::render`] draws
//! text, and [`Font::to_ttf`] writes TrueType whose outlines are the pixels as
//! squares, with the em at the glyph height, so it is crisp at integer multiples of
//! its pixel size.
//!
//! # Errors
//!
//! Errors are [`Error`] values with a stable [`ErrorCode`] and, for text, the
//! [`Position`] of the error: its line and its column in characters.
//! [`Document::parse`] returns the first; [`check`] returns all of them. Parsing is
//! bounded by [`Limits`], and malformed input gives an error, never a panic.
//!
//! # Output
//!
//! [`Sprite::render`] gives an [`Image`]: RGBA8 pixels, sRGB-encoded and not
//! premultiplied, tightly packed, top row first. [`Document::atlas`] packs every
//! sprite into one texture, with each one's [`Rect`] and the animations' frames, as
//! structs and as JSON, and [`Document::export`] adds a texture for each tileset, the
//! tilemaps and a TrueType file for each font. [`png`] encodes and decodes PNG files, and
//! [`Document::preview`] draws a preview sheet.
#![forbid(unsafe_code)]

mod atlas;
mod document;
mod error;
mod export;
mod font;
mod image;
mod json;
mod label;
mod limits;
pub mod png;
mod preview;
mod sprite;
mod text;
mod texture;
mod tiles;
mod ttf;

pub use atlas::{Atlas, AtlasAnimation, AtlasFrame, AtlasOptions, Rect, Region};
pub use document::{Animation, Document, Frame, check, check_with_loader};
pub use error::{Error, ErrorCode, Position, Result};
pub use export::{ATLAS_FILE, Export, ExportOptions, FontExport, METADATA_FILE, TilesetExport};
pub use font::{Font, Glyph, TtfOptions};
pub use image::{Color, Image};
pub use limits::Limits;
pub use preview::PreviewOptions;
pub use sprite::{Insets, Palette, Sprite};
pub use texture::{Dither, Noise, bayer};
pub use tiles::{BLOB_MASKS, Piece, Scheme, Terrain, Tilemap, Tileset, neighbours, reduce};
