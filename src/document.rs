use std::str::FromStr;

use crate::{Error, Image, Limits, Palette, PreviewOptions, Result, Sprite};

/// An animation: frames shown in order, each for a number of ticks.
///
/// A tick is one step of the game's fixed update; Diamon steps at 60 ticks a second.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Animation {
    pub(crate) name: String,
    pub(crate) frames: Vec<Frame>,
    pub(crate) looping: bool,
}

impl Animation {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }
    /// Whether the animation starts again after its last frame (`loop`), or stays on
    /// it (`once`).
    pub fn looping(&self) -> bool {
        self.looping
    }
    /// The ticks of all frames together.
    pub fn total_ticks(&self) -> u64 {
        self.frames.iter().map(|f| f.ticks as u64).sum()
    }
}

/// One frame of an [`Animation`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub(crate) sprite: String,
    pub(crate) ticks: u32,
}

impl Frame {
    /// The frame's sprite: a sprite's name, or a modifier chain such as
    /// `coin:flip_x`, which [`Document::sprite`] finds too.
    pub fn sprite(&self) -> &str {
        &self.sprite
    }
    /// How many ticks the frame shows, at least 1.
    pub fn ticks(&self) -> u32 {
        self.ticks
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
    pub(crate) frame_sprites: Vec<Sprite>,
    pub(crate) animations: Vec<Animation>,
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
    /// The named sprites, drawn or derived, in the order they are defined.
    pub fn sprites(&self) -> &[Sprite] {
        &self.sprites
    }
    /// A named sprite, or a sprite that an animation frame builds with a modifier
    /// chain, such as `coin:flip_x`.
    pub fn sprite(&self, name: &str) -> Option<&Sprite> {
        self.sprites
            .iter()
            .chain(&self.frame_sprites)
            .find(|s| s.name == name)
    }
    /// The sprites that animation frames build with modifier chains, in the order
    /// they are first used. Their names are the chains, written without spaces.
    pub fn frame_sprites(&self) -> &[Sprite] {
        &self.frame_sprites
    }
    /// The animations, in the order they are defined.
    pub fn animations(&self) -> &[Animation] {
        &self.animations
    }
    pub fn animation(&self, name: &str) -> Option<&Animation> {
        self.animations.iter().find(|a| a.name == name)
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
