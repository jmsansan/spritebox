use std::str::FromStr;

use crate::{
    Atlas, AtlasOptions, Error, Export, ExportOptions, Font, Image, Limits, Lint, LintOptions,
    Palette, PreviewOptions, Result, Sprite, Tilemap, Tileset,
};

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
    pub(crate) flip_x: bool,
    pub(crate) flip_y: bool,
}

impl Frame {
    /// The frame's sprite before its flips: a sprite's name, or a modifier chain such
    /// as `coin:swap(dusk)`, which [`Document::sprite`] finds too.
    pub fn sprite(&self) -> &str {
        &self.sprite
    }
    /// Whether the frame draws its sprite mirrored left to right. A chain's trailing
    /// `flip_x` (or `rot180`) sets it, rather than a flipped copy of the pixels.
    pub fn flip_x(&self) -> bool {
        self.flip_x
    }
    /// Whether the frame draws its sprite mirrored top to bottom: a trailing `flip_y`
    /// or `rot180`.
    pub fn flip_y(&self) -> bool {
        self.flip_y
    }
    /// How many ticks the frame shows, at least 1.
    pub fn ticks(&self) -> u32 {
        self.ticks
    }
}

/// A group: sprites that belong together, such as the icons of an inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Group {
    pub(crate) name: String,
    pub(crate) members: Vec<String>,
}

impl Group {
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The sprites' names, in the order written.
    pub fn members(&self) -> &[String] {
        &self.members
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
    pub(crate) screens: Vec<crate::Screen>,
    pub(crate) themes: Vec<crate::Theme>,
    pub(crate) theme_bases: Vec<Palette>,
    pub(crate) states: Vec<crate::StateSet>,
    pub(crate) icon_sets: Vec<crate::IconSet>,
    pub(crate) palettes: Vec<Palette>,
    pub(crate) sprites: Vec<Sprite>,
    pub(crate) frame_sprites: Vec<Sprite>,
    pub(crate) animations: Vec<Animation>,
    pub(crate) tilesets: Vec<Tileset>,
    pub(crate) tilemaps: Vec<Tilemap>,
    pub(crate) fonts: Vec<Font>,
    pub(crate) groups: Vec<Group>,
    /// Where the definitions are in the text, for [`Document::lint`].
    pub(crate) sources: crate::lint::Sources,
}

impl Document {
    /// Parses and validates `text` within the default [`Limits`]. The error is the
    /// first one in the text; [`check`](crate::check) returns all of them.
    pub fn parse(text: &str) -> Result<Self> {
        Self::parse_with(text, &Limits::default())
    }

    /// Parses and validates `text` within `limits`.
    pub fn parse_with(text: &str, limits: &Limits) -> Result<Self> {
        Self::finish(crate::text::parse(text, limits, None))
    }

    /// Parses and validates `text` within `limits`, loading the files it imports with
    /// `loader`, which takes a file's name and returns its text. Names resolve from
    /// the importing file's directory, with `/` between parts, so the loader decides
    /// what they mean: files on disk, assets embedded in a game, or anything else.
    ///
    /// ```
    /// use spritebox::{Document, Error, ErrorCode, Limits};
    ///
    /// let mut loader = |name: &str| match name {
    ///     "shared/palettes.spritebox" => Ok("palette dawn\n  k #1a1c2c\n".to_owned()),
    ///     _ => Err(Error::new(ErrorCode::Io, format!("no file {name}"))),
    /// };
    /// let text = "import shared/palettes.spritebox\nsprite dot 1x1 palette=dawn\n  k\n";
    /// let document = Document::parse_with_loader(text, &Limits::default(), &mut loader)?;
    /// assert!(document.palette("dawn").is_some());
    /// # Ok::<(), Error>(())
    /// ```
    pub fn parse_with_loader(
        text: &str,
        limits: &Limits,
        loader: &mut dyn FnMut(&str) -> Result<String>,
    ) -> Result<Self> {
        Self::finish(crate::text::parse(text, limits, Some(loader)))
    }

    fn finish((document, mut errors): (Self, Vec<Error>)) -> Result<Self> {
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
    /// they are first used. Their names are the chains, written without spaces and
    /// without the trailing flips that frames state instead.
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
    /// The tilesets, in the order they are defined.
    pub fn tilesets(&self) -> &[Tileset] {
        &self.tilesets
    }
    pub fn tileset(&self, name: &str) -> Option<&Tileset> {
        self.tilesets.iter().find(|t| t.name == name)
    }
    /// The tilemaps, in the order they are defined.
    pub fn tilemaps(&self) -> &[Tilemap] {
        &self.tilemaps
    }
    pub fn tilemap(&self, name: &str) -> Option<&Tilemap> {
        self.tilemaps.iter().find(|t| t.name == name)
    }
    /// The fonts, in the order they are defined.
    pub fn fonts(&self) -> &[Font] {
        &self.fonts
    }
    pub fn font(&self, name: &str) -> Option<&Font> {
        self.fonts.iter().find(|f| f.name == name)
    }
    /// The groups, in the order they are defined.
    pub fn groups(&self) -> &[Group] {
        &self.groups
    }
    pub fn group(&self, name: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.name == name)
    }

    /// Packs every sprite, and every sprite that animation frames build, into one
    /// texture, within the default [`Limits`]. The [`Atlas`] has the rectangle of each
    /// and the animations' frames, and writes them as JSON.
    pub fn atlas(&self, options: &AtlasOptions) -> Result<Atlas> {
        self.atlas_with(options, &Limits::default())
    }

    /// [`Document::atlas`] within `limits`, which bound the texture's size.
    pub fn atlas_with(&self, options: &AtlasOptions, limits: &Limits) -> Result<Atlas> {
        crate::atlas::build(self, options, limits)
    }

    /// Everything a game needs: the atlas, each tileset's texture, the tilemaps, and
    /// their metadata, within the default [`Limits`].
    pub fn export(&self, options: &ExportOptions) -> Result<Export> {
        self.export_with(options, &Limits::default())
    }

    /// [`Document::export`] within `limits`.
    pub fn export_with(&self, options: &ExportOptions, limits: &Limits) -> Result<Export> {
        crate::export::build(self, options, limits)
    }

    /// Checks the art against pixel-art practice, within the default [`Limits`]: the
    /// warnings, ordered by file, then position, then code, and how many there are of
    /// each code. The text silences a code for a definition and what is built from it
    /// with `lint=allow(CODE,...)`, and for a whole file with `lint allow CODE...`.
    ///
    /// ```
    /// use spritebox::{Document, LintCode, LintOptions};
    ///
    /// let document: Document = "
    /// palette p
    ///   g  #3a7d44
    ///   h  #468a50
    ///   k  #000000
    ///
    /// sprite leaf 3x3 palette=p
    ///   ggg
    ///   ghg
    ///   ggg
    /// ".parse()?;
    /// let lint = document.lint(&LintOptions::default());
    /// assert_eq!(lint.count(LintCode::OrphanPixel), 1);
    /// let warnings = &lint.warnings;
    /// let codes: Vec<LintCode> = warnings.iter().map(|w| w.code).collect();
    /// assert_eq!(codes, [LintCode::UnusedKey, LintCode::OrphanPixel]);
    /// assert_eq!(warnings[1].position.unwrap().line, 9);
    /// assert_eq!(warnings[1].pixels, [(1, 1)]);
    /// # Ok::<(), spritebox::Error>(())
    /// ```
    pub fn lint(&self, options: &LintOptions) -> Lint {
        self.lint_with(options, &Limits::default())
    }

    /// [`Document::lint`] within `limits`, which bound the number of warnings kept;
    /// the counts include those left out.
    pub fn lint_with(&self, options: &LintOptions, limits: &Limits) -> Lint {
        crate::lint::run(self, options, limits)
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
    crate::text::parse(text, limits, None).1
}

/// [`check`], loading imported files with `loader` as
/// [`Document::parse_with_loader`] does. Errors in an imported file name it in
/// [`Error::file`].
pub fn check_with_loader(
    text: &str,
    limits: &Limits,
    loader: &mut dyn FnMut(&str) -> Result<String>,
) -> Vec<Error> {
    crate::text::parse(text, limits, Some(loader)).1
}
