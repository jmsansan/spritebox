//! 2D pixel art as plain text, in safe Rust with zero dependencies.
//!
//! A spritebox file defines palettes, sprites and animations as text that a person or an AI
//! assistant can write, read and diff. The crate validates it with exact
//! diagnostics, renders it to RGBA8 images and PNG files, and draws preview sheets to
//! review a whole file at a glance. The same text gives the same pixels on every
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
//! steps at 60 a second). `loop`, the default, or `once`.
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
//! structs and as JSON. [`png`] encodes and decodes PNG files, and
//! [`Document::preview`] draws a preview sheet.
#![forbid(unsafe_code)]

mod atlas;
mod document;
mod error;
mod image;
mod json;
mod label;
mod limits;
pub mod png;
mod preview;
mod sprite;
mod text;

pub use atlas::{Atlas, AtlasAnimation, AtlasFrame, AtlasOptions, Rect, Region};
pub use document::{Animation, Document, Frame, check};
pub use error::{Error, ErrorCode, Position, Result};
pub use image::{Color, Image};
pub use limits::Limits;
pub use preview::PreviewOptions;
pub use sprite::{Palette, Sprite};
