use std::str::FromStr;

use crate::{Color, Error, Image, Limits, PreviewOptions, Result};

/// A named set of colors, each under a one-character key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Palette {
    pub(crate) name: String,
    pub(crate) colors: Vec<(char, Color)>,
}

impl Palette {
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The keys and their colors, inherited keys first, in the order written.
    pub fn colors(&self) -> &[(char, Color)] {
        &self.colors
    }
    /// The color of `key`, if the palette has it.
    pub fn color(&self, key: char) -> Option<Color> {
        self.colors.iter().find(|(k, _)| *k == key).map(|&(_, c)| c)
    }
}

/// Marks a pixel that no key covers in a layer.
pub(crate) const EMPTY: char = '\0';

/// One layer of a sprite: a key for each pixel, row by row, or [`EMPTY`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Layer {
    pub name: Option<String>,
    pub keys: Vec<char>,
}

/// A sprite: a grid of palette keys in one or more layers, and the palette that
/// colors them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sprite {
    pub(crate) name: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) palette: Palette,
    pub(crate) layers: Vec<Layer>,
}

impl Sprite {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    /// The palette that colors the sprite's keys.
    pub fn palette(&self) -> &Palette {
        &self.palette
    }
    /// The key of the topmost layer that covers `(x, y)`, or `None` where no layer
    /// does or outside the sprite.
    pub fn key(&self, x: u32, y: u32) -> Option<char> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = y as usize * self.width as usize + x as usize;
        self.layers
            .iter()
            .rev()
            .map(|layer| layer.keys[i])
            .find(|&k| k != EMPTY)
    }

    /// Renders the sprite to RGBA8: each layer's keys are colored by the palette and
    /// the layers are drawn over each other in order with [`Color::over`].
    pub fn render(&self) -> Image {
        let mut lookup = ColorLookup::new(&self.palette);
        let mut image = Image::blank(self.width, self.height);
        for layer in &self.layers {
            for (i, &key) in layer.keys.iter().enumerate() {
                if key == EMPTY {
                    continue;
                }
                let color = lookup.get(key);
                let w = self.width as usize;
                let (x, y) = ((i % w) as u32, (i / w) as u32);
                if let Some(below) = image.pixel(x, y) {
                    image.set_pixel(x, y, color.over(below));
                }
            }
        }
        image
    }
}

/// Colors of palette keys, with a table for ASCII keys.
struct ColorLookup<'a> {
    palette: &'a Palette,
    ascii: [Option<Color>; 128],
}

impl<'a> ColorLookup<'a> {
    fn new(palette: &'a Palette) -> Self {
        let mut ascii = [None; 128];
        for &(key, color) in palette.colors.iter().rev() {
            if key.is_ascii() {
                ascii[key as usize] = Some(color);
            }
        }
        Self { palette, ascii }
    }
    fn get(&mut self, key: char) -> Color {
        let found = if key.is_ascii() {
            self.ascii[key as usize]
        } else {
            self.palette.color(key)
        };
        found.unwrap_or(Color::TRANSPARENT)
    }
}

/// A parsed and validated spritebox file.
///
/// ```
/// use spritebox::{Color, Document};
///
/// let document: Document = "
/// palette dawn
///   .  transparent
///   k  #1a1c2c
///   o  #f4b41b
///
/// sprite dot 3x3 palette=dawn
///   .k.
///   kok
///   .k.
/// ".parse()?;
/// let image = document.sprite("dot").unwrap().render();
/// assert_eq!(image.pixel(1, 1), Some(Color::rgb(0xf4, 0xb4, 0x1b)));
/// assert_eq!(image.pixel(0, 0), Some(Color::TRANSPARENT));
/// # Ok::<(), spritebox::Error>(())
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Document {
    pub(crate) palettes: Vec<Palette>,
    pub(crate) sprites: Vec<Sprite>,
}

impl Document {
    /// Parses and validates `text` within the default [`Limits`]. The error is the
    /// first one in the text; [`check`](crate::check) returns all of them.
    pub fn parse(text: &str) -> Result<Self> {
        Self::parse_with(text, &Limits::default())
    }

    /// Parses and validates `text` within `limits`.
    pub fn parse_with(text: &str, limits: &Limits) -> Result<Self> {
        let (document, mut errors) = crate::text::parse(text, limits);
        if errors.is_empty() {
            return Ok(document);
        }
        Err(errors.swap_remove(0))
    }

    /// The palettes, in the order they are defined.
    pub fn palettes(&self) -> &[Palette] {
        &self.palettes
    }
    pub fn palette(&self, name: &str) -> Option<&Palette> {
        self.palettes.iter().find(|p| p.name == name)
    }
    /// The sprites, in the order they are defined.
    pub fn sprites(&self) -> &[Sprite] {
        &self.sprites
    }
    pub fn sprite(&self, name: &str) -> Option<&Sprite> {
        self.sprites.iter().find(|s| s.name == name)
    }

    /// Draws a preview sheet: every definition, scaled up, on a grid and labelled with
    /// its name, within the default [`Limits`].
    pub fn preview(&self, options: &PreviewOptions) -> Result<Image> {
        self.preview_with(options, &Limits::default())
    }

    /// [`Document::preview`] within `limits`, which bound the sheet's size.
    pub fn preview_with(&self, options: &PreviewOptions, limits: &Limits) -> Result<Image> {
        crate::preview::sheet(self, options, limits)
    }
}

impl FromStr for Document {
    type Err = Error;
    fn from_str(text: &str) -> Result<Self> {
        Self::parse(text)
    }
}

/// Parses and validates `text`, and returns every error found, ordered by position.
/// An empty list means that [`Document::parse_with`] succeeds.
///
/// ```
/// let errors = spritebox::check("palette p\n  x #12345\n  y #zzzzzz\n", &Default::default());
/// assert_eq!(errors.len(), 2);
/// assert_eq!(errors[0].position.unwrap().line, 2);
/// ```
pub fn check(text: &str, limits: &Limits) -> Vec<Error> {
    crate::text::parse(text, limits).1
}
