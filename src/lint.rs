//! Lint: warnings for art that is valid but does not follow pixel-art practice.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::str::FromStr;

use crate::sprite::EMPTY;
use crate::{Color, Document, Error, ErrorCode, Limits, Position, Sprite};

/// Stable lint categories. Match on the code rather than the message.
///
/// [`LintCode::as_str`] gives the name used in `lint=allow(...)`, `lint allow` and the
/// command-line tool's diagnostics, which is stable too.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum LintCode {
    /// A lone pixel on a flat area, close in lightness to the color around it, which
    /// reads as noise; or an opaque pixel that touches nothing at all.
    OrphanPixel,
    /// A palette key that nothing uses.
    UnusedKey,
    /// More warnings than [`Limits::max_warnings`]; the rest were left out.
    Truncated,
}

impl LintCode {
    /// Every code, in order.
    pub const ALL: [LintCode; 3] = [
        LintCode::OrphanPixel,
        LintCode::UnusedKey,
        LintCode::Truncated,
    ];

    /// The code's stable name in `snake_case`, such as `"orphan_pixel"`.
    pub fn as_str(self) -> &'static str {
        match self {
            LintCode::OrphanPixel => "orphan_pixel",
            LintCode::UnusedKey => "unused_key",
            LintCode::Truncated => "truncated",
        }
    }
}

impl fmt::Display for LintCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LintCode {
    type Err = Error;
    fn from_str(name: &str) -> Result<Self, Error> {
        LintCode::ALL
            .into_iter()
            .find(|c| c.as_str() == name)
            .ok_or_else(|| {
                let names: Vec<&str> = LintCode::ALL.iter().map(|c| c.as_str()).collect();
                Error::new(
                    ErrorCode::Syntax,
                    format!(
                        "Unknown lint `{name}`; expected one of {}",
                        names.join(", ")
                    ),
                )
            })
    }
}

/// A lint warning: a stable code, a message and, when it is about a place in the
/// text, its position.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Warning {
    pub code: LintCode,
    pub message: String,
    /// Where in the text the warning is: the pixel's character in its row, or else
    /// the definition's name.
    pub position: Option<Position>,
    /// The imported file that `position` is in, as in [`Error::file`].
    pub file: Option<String>,
    /// The sprite the warning is about, if any.
    pub sprite: Option<String>,
    /// The pixels of that sprite the warning is about, if any.
    pub pixels: Vec<(u32, u32)>,
}

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(file) = &self.file {
            write!(f, "{file}, ")?;
        }
        match self.position {
            Some(p) => write!(f, "line {}, column {}: {}", p.line, p.column, self.message),
            None => f.write_str(&self.message),
        }
    }
}

/// What [`Document::lint`] checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LintOptions {
    /// Codes that are not reported, besides those the text allows.
    pub allow: Vec<LintCode>,
    /// The largest difference in lightness, 0..=255, between a lone pixel and the
    /// color around it for `orphan_pixel` to report it. A lone pixel that stands out
    /// more, such as an eye or a glint, is taken as deliberate. Default 32.
    pub orphan_contrast: u8,
}

impl Default for LintOptions {
    fn default() -> Self {
        Self {
            allow: Vec::new(),
            orphan_contrast: 32,
        }
    }
}

/// The kinds of definitions that `lint=` can be given to.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Kind {
    Palette,
    Sprite,
    Animation,
    Tileset,
    Tilemap,
    Font,
}

/// Where definitions are in the text, and what lint they allow: what a [`Document`]
/// keeps of its text for [`Document::lint`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Sources {
    /// The name of each file of the parse; the first, the text itself, is empty.
    pub files: Vec<String>,
    /// The codes that `lint allow` allows in each file.
    pub file_allows: Vec<Vec<LintCode>>,
    pub defs: Vec<Def>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Def {
    pub kind: Kind,
    pub name: String,
    pub pos: Position,
    pub allow: Vec<LintCode>,
    /// The sprites it is built from.
    pub deps: Vec<String>,
    pub detail: Detail,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Detail {
    Other,
    /// A sprite with rows: for each layer of rows, bottom first, each row's first
    /// character and its text.
    Drawn(Vec<Vec<(Position, String)>>),
    /// A texture, and the keys of its ramp.
    Texture(Vec<char>),
    /// A palette: the palette it copies, and the keys it writes, at their positions.
    Palette {
        from: Option<String>,
        keys: Vec<(char, Position)>,
    },
}

impl Sources {
    fn def(&self, kind: Kind, name: &str) -> Option<&Def> {
        self.defs.iter().find(|d| d.kind == kind && d.name == name)
    }

    /// Whether the text allows `code` for the definition: by its own `lint=`, its
    /// file's `lint allow`, or those of the sprites it is built from.
    fn allows(&self, kind: Kind, name: &str, code: LintCode) -> bool {
        let mut seen = HashSet::new();
        let mut stack = vec![(kind, name)];
        while let Some((kind, name)) = stack.pop() {
            if !seen.insert((kind, name)) {
                continue;
            }
            let Some(def) = self.def(kind, name) else {
                continue;
            };
            let file = self.file_allows.get(def.pos.file as usize);
            if def.allow.contains(&code) || file.is_some_and(|f| f.contains(&code)) {
                return true;
            }
            stack.extend(def.deps.iter().map(|d| (Kind::Sprite, d.as_str())));
        }
        false
    }
}

/// Runs the checks; see [`Document::lint`].
pub(crate) fn run(document: &Document, options: &LintOptions, limits: &Limits) -> Vec<Warning> {
    let sources = &document.sources;
    let mut warnings = Vec::new();
    for sprite in &document.sprites {
        let Some(def) = sources.def(Kind::Sprite, &sprite.name) else {
            continue;
        };
        if let Detail::Drawn(rows) = &def.detail {
            orphan_pixels(sprite, def, rows, options, &mut warnings);
        }
    }
    unused_keys(document, &mut warnings);

    // Allowed codes, and repeats at one place (a pixel that two sprites share).
    let mut seen = HashSet::new();
    warnings.retain(|(kind, name, w): &(Kind, String, Warning)| {
        !options.allow.contains(&w.code)
            && !sources.allows(*kind, name, w.code)
            && (w.position.is_none() || seen.insert((w.code, w.position)))
    });
    let mut warnings: Vec<Warning> = warnings.into_iter().map(|(_, _, w)| w).collect();
    warnings.sort_by(|a, b| {
        (a.position.is_none(), a.position, a.code).cmp(&(b.position.is_none(), b.position, b.code))
    });
    if warnings.len() > limits.max_warnings {
        let total = warnings.len();
        warnings.truncate(limits.max_warnings);
        warnings.push(Warning {
            code: LintCode::Truncated,
            message: format!(
                "{total} warnings, more than the limit of {}; the rest are left out",
                limits.max_warnings
            ),
            position: None,
            file: None,
            sprite: None,
            pixels: Vec::new(),
        });
    }
    for w in &mut warnings {
        if let Some(p) = w.position.filter(|p| p.file > 0) {
            w.file = sources.files.get(p.file as usize).cloned();
        }
    }
    warnings
}

/// A warning, with the definition whose `lint=` can allow it.
type Found = (Kind, String, Warning);

/// Lightness, 0..=255, by the luma of Rec. 601.
fn luma(c: Color) -> i32 {
    (299 * c.r as i32 + 587 * c.g as i32 + 114 * c.b as i32) / 1000
}

/// `orphan_pixel`: in a drawn sprite of at least 3×3, a visible pixel whose four
/// neighbours share one other color, close to its own in lightness, with none of its
/// color touching it diagonally. A visible pixel with nothing visible around it at
/// all is reported too.
fn orphan_pixels(
    sprite: &Sprite,
    def: &Def,
    rows: &[Vec<(Position, String)>],
    options: &LintOptions,
    out: &mut Vec<Found>,
) {
    let (w, h) = (sprite.width as i64, sprite.height as i64);
    if w < 3 || h < 3 {
        return;
    }
    let image = sprite.render();
    // All fully transparent pixels are one color.
    let at = |x: i64, y: i64| -> Color {
        if x < 0 || y < 0 || x >= w || y >= h {
            return Color::TRANSPARENT;
        }
        let c = image
            .pixel(x as u32, y as u32)
            .unwrap_or(Color::TRANSPARENT);
        if c.a == 0 { Color::TRANSPARENT } else { c }
    };
    for y in 0..h {
        for x in 0..w {
            let c = at(x, y);
            if c.a == 0 {
                continue;
            }
            let sides = [at(x - 1, y), at(x + 1, y), at(x, y - 1), at(x, y + 1)];
            let corners = [
                at(x - 1, y - 1),
                at(x + 1, y - 1),
                at(x - 1, y + 1),
                at(x + 1, y + 1),
            ];
            let around = sides[0];
            if sides.iter().any(|&s| s != around) || around == c {
                continue;
            }
            let message = if around.a == 0 {
                if corners.iter().any(|k| k.a > 0) {
                    continue;
                }
                format!(
                    "A lone pixel in sprite `{}` at ({x}, {y}), touching nothing",
                    sprite.name
                )
            } else {
                let contrast = (luma(c) - luma(around)).abs();
                if corners.contains(&c) || contrast > options.orphan_contrast as i32 {
                    continue;
                }
                format!(
                    "A lone pixel in sprite `{}` at ({x}, {y}), close in lightness to the \
                     color around it, reads as noise",
                    sprite.name
                )
            };
            let position = pixel_position(sprite, rows, x as u32, y as u32).unwrap_or(def.pos);
            out.push((
                Kind::Sprite,
                sprite.name.clone(),
                Warning {
                    code: LintCode::OrphanPixel,
                    message,
                    position: Some(position),
                    file: None,
                    sprite: Some(sprite.name.clone()),
                    pixels: vec![(x as u32, y as u32)],
                },
            ));
        }
    }
}

/// The character in the text that draws pixel `(x, y)`: in the topmost layer of rows
/// whose key there is visible.
fn pixel_position(
    sprite: &Sprite,
    rows: &[Vec<(Position, String)>],
    x: u32,
    y: u32,
) -> Option<Position> {
    let lookup = crate::sprite::ColorLookup::new(&sprite.palette);
    rows.iter().rev().find_map(|layer| {
        let (start, text) = layer.get(y as usize)?;
        let key = text.chars().nth(x as usize)?;
        (key != EMPTY && lookup.get(key).a > 0)
            .then(|| Position::in_file(start.file, start.line, start.column + x as usize))
    })
}

/// `unused_key`: a key, as a palette writes it, that no sprite, frame, tile or
/// texture ramp uses through that palette or a palette copied from it.
fn unused_keys(document: &Document, out: &mut Vec<Found>) {
    let sources = &document.sources;
    let palettes: HashMap<&str, &Def> = sources
        .defs
        .iter()
        .filter(|d| d.kind == Kind::Palette)
        .rev()
        .map(|d| (d.name.as_str(), d))
        .collect();
    let mut used: HashSet<(&str, char)> = HashSet::new();
    let mut mark = |palette: &str, key: char| {
        // The key's color comes from the nearest palette along `from=` that writes it.
        let mut name = palette;
        for _ in 0..=palettes.len() {
            let Some(def) = palettes.get(name) else {
                return;
            };
            let Detail::Palette { from, keys } = &def.detail else {
                return;
            };
            if keys.iter().any(|&(k, _)| k == key) {
                used.insert((def.name.as_str(), key));
                return;
            }
            match from {
                Some(from) => name = from,
                None => return,
            }
        }
    };
    let tiles = document.tilesets.iter().flat_map(|t| &t.tiles);
    for sprite in document
        .sprites
        .iter()
        .chain(&document.frame_sprites)
        .chain(tiles)
    {
        let mut keys: Vec<char> = sprite
            .layers
            .iter()
            .flat_map(|l| l.keys.iter().copied())
            .collect();
        keys.sort_unstable();
        keys.dedup();
        for key in keys.into_iter().filter(|&k| k != EMPTY) {
            mark(&sprite.palette.name, key);
        }
    }
    for def in &sources.defs {
        if let Detail::Texture(ramp) = &def.detail {
            if let Some(sprite) = document.sprites.iter().find(|s| s.name == def.name) {
                for &key in ramp {
                    mark(&sprite.palette.name, key);
                }
            }
        }
    }
    for def in palettes.values() {
        let Detail::Palette { keys, .. } = &def.detail else {
            continue;
        };
        for &(key, pos) in keys {
            if !used.contains(&(def.name.as_str(), key)) {
                out.push((
                    Kind::Palette,
                    def.name.clone(),
                    Warning {
                        code: LintCode::UnusedKey,
                        message: format!("Nothing uses key `{key}` of palette `{}`", def.name),
                        position: Some(pos),
                        file: None,
                        sprite: None,
                        pixels: Vec::new(),
                    },
                ));
            }
        }
    }
}
