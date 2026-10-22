//! Lint: warnings for art that is valid but does not follow pixel-art practice.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
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
}

impl LintCode {
    /// Every code, in order.
    pub const ALL: [LintCode; 2] = [LintCode::OrphanPixel, LintCode::UnusedKey];

    /// The code's stable name in `snake_case`, such as `"orphan_pixel"`.
    pub fn as_str(self) -> &'static str {
        match self {
            LintCode::OrphanPixel => "orphan_pixel",
            LintCode::UnusedKey => "unused_key",
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

/// What [`Document::lint`] found: the first warnings, and how many of each code there
/// are in all.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Lint {
    /// The warnings, ordered by file, then position, then code: all of them, or the
    /// first [`Limits::max_warnings`].
    pub warnings: Vec<Warning>,
    /// Each code found, in the order of [`LintCode::ALL`], with how many warnings it
    /// has, those beyond the limit included.
    pub counts: Vec<(LintCode, usize)>,
}

impl Lint {
    /// How many warnings there are, those beyond the limit included.
    pub fn total(&self) -> usize {
        self.counts.iter().map(|&(_, n)| n).sum()
    }
    /// How many warnings of `code` there are, those beyond the limit included.
    pub fn count(&self, code: LintCode) -> usize {
        self.counts
            .iter()
            .find(|&&(c, _)| c == code)
            .map_or(0, |&(_, n)| n)
    }
    /// How many warnings were left out of [`Lint::warnings`] by the limit.
    pub fn omitted(&self) -> usize {
        self.total() - self.warnings.len()
    }
    /// Whether there are no warnings at all.
    pub fn is_clean(&self) -> bool {
        self.counts.is_empty()
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
    /// A sprite with a size: its layers as written, bottom first.
    Drawn(Vec<DrawnLayer>),
    /// A texture, and the keys of its ramp.
    Texture(Vec<char>),
    /// A palette: the palette it copies, and the keys it writes, at their positions.
    Palette {
        from: Option<String>,
        keys: Vec<(char, Position)>,
    },
}

/// A layer as written in a sprite: rows, or a sprite it is made from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DrawnLayer {
    /// How many of the sprite's layers it makes: one for rows, as many as its sprite
    /// has for a sprite.
    pub layers: usize,
    /// The first character of each row, for rows.
    pub rows: Option<Vec<Position>>,
}

/// The definitions by kind and name, the first of each.
type Index<'s> = HashMap<(Kind, &'s str), &'s Def>;

impl Sources {
    fn index(&self) -> Index<'_> {
        let mut index = HashMap::new();
        for def in &self.defs {
            index.entry((def.kind, def.name.as_str())).or_insert(def);
        }
        index
    }

    /// Whether the text allows `code` for the definition: by its own `lint=` or its
    /// file's `lint allow`, or by the `lint=` of the sprites it is built from. A
    /// file's `lint allow` stays in that file, even for sprites built from its own.
    fn allows(&self, index: &Index, kind: Kind, name: &str, code: LintCode) -> bool {
        if let Some(def) = index.get(&(kind, name)) {
            let file = self.file_allows.get(def.pos.file as usize);
            if file.is_some_and(|f| f.contains(&code)) {
                return true;
            }
        }
        let mut seen = HashSet::new();
        let mut stack = vec![(kind, name)];
        while let Some((kind, name)) = stack.pop() {
            if !seen.insert((kind, name)) {
                continue;
            }
            let Some(def) = index.get(&(kind, name)) else {
                continue;
            };
            if def.allow.contains(&code) {
                return true;
            }
            stack.extend(def.deps.iter().map(|d| (Kind::Sprite, d.as_str())))
        }
        false
    }
}

/// Collects warnings: drops those allowed, counts the rest, and keeps only the first
/// `limit` in order, so that memory stays bounded however many there are.
struct Sink<'s> {
    sources: &'s Sources,
    index: Index<'s>,
    options: &'s LintOptions,
    limit: usize,
    /// Whether each (kind, code, name) is allowed, once worked out.
    allowed: HashMap<(Kind, LintCode), HashMap<String, bool>>,
    counts: [usize; LintCode::ALL.len()],
    /// The first warnings so far, with the last of them on top.
    kept: BinaryHeap<Kept>,
    found: usize,
}

/// A kept warning, ordered by file, position and code, then by when it was found.
struct Kept {
    key: (bool, Option<Position>, LintCode, usize),
    warning: Warning,
}

impl PartialEq for Kept {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}
impl Eq for Kept {}
impl PartialOrd for Kept {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Kept {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key.cmp(&other.key)
    }
}

/// A warning about to be reported, built only if it is kept.
struct Found<'f> {
    code: LintCode,
    /// The definition whose `lint=` can allow it.
    kind: Kind,
    name: &'f str,
    position: Option<Position>,
    sprite: Option<&'f str>,
    pixels: Vec<(u32, u32)>,
}

impl<'s> Sink<'s> {
    fn new(sources: &'s Sources, options: &'s LintOptions, limit: usize) -> Self {
        Self {
            sources,
            index: sources.index(),
            options,
            limit,
            allowed: HashMap::new(),
            counts: [0; LintCode::ALL.len()],
            kept: BinaryHeap::new(),
            found: 0,
        }
    }

    fn push(&mut self, found: Found, message: impl FnOnce() -> String) {
        let code = found.code;
        if self.options.allow.contains(&code) {
            return;
        }
        let known = self
            .allowed
            .get(&(found.kind, code))
            .and_then(|names| names.get(found.name))
            .copied();
        let allowed = known.unwrap_or_else(|| {
            let allowed = self
                .sources
                .allows(&self.index, found.kind, found.name, code);
            self.allowed
                .entry((found.kind, code))
                .or_default()
                .insert(found.name.to_owned(), allowed);
            allowed
        });
        if allowed {
            return;
        }
        if let Some(i) = LintCode::ALL.iter().position(|&c| c == code) {
            self.counts[i] += 1;
        }
        let key = (found.position.is_none(), found.position, code, self.found);
        self.found += 1;
        let full = self.kept.len() >= self.limit;
        if full && self.kept.peek().is_none_or(|last| key >= last.key) {
            return;
        }
        let file = found
            .position
            .filter(|p| p.file > 0)
            .and_then(|p| self.sources.files.get(p.file as usize).cloned());
        self.kept.push(Kept {
            key,
            warning: Warning {
                code,
                message: message(),
                position: found.position,
                file,
                sprite: found.sprite.map(str::to_owned),
                pixels: found.pixels,
            },
        });
        if self.kept.len() > self.limit {
            self.kept.pop();
        }
    }

    fn finish(self) -> Lint {
        Lint {
            warnings: self
                .kept
                .into_sorted_vec()
                .into_iter()
                .map(|k| k.warning)
                .collect(),
            counts: LintCode::ALL
                .into_iter()
                .zip(self.counts)
                .filter(|&(_, n)| n > 0)
                .collect(),
        }
    }
}

/// Runs the checks; see [`Document::lint`].
pub(crate) fn run(document: &Document, options: &LintOptions, limits: &Limits) -> Lint {
    let sources = &document.sources;
    let mut sink = Sink::new(sources, options, limits.max_warnings);
    let index = sources.index();
    for sprite in &document.sprites {
        let Some(def) = index.get(&(Kind::Sprite, sprite.name.as_str())) else {
            continue;
        };
        if let Detail::Drawn(layers) = &def.detail {
            orphan_pixels(sprite, def, layers, options, &mut sink);
        }
    }
    unused_keys(document, &index, &mut sink);
    sink.finish()
}

/// Lightness, 0..=255, by the luma of Rec. 601.
fn luma(c: Color) -> i32 {
    (299 * c.r as i32 + 587 * c.g as i32 + 114 * c.b as i32) / 1000
}

/// `orphan_pixel`: in a sprite of at least 3×3 with a size, a visible pixel whose four
/// neighbours share one other color, close to its own in lightness, with none of its
/// color touching it diagonally. A visible pixel with nothing visible around it at
/// all is reported too.
fn orphan_pixels(
    sprite: &Sprite,
    def: &Def,
    layers: &[DrawnLayer],
    options: &LintOptions,
    sink: &mut Sink,
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
            let isolated = around.a == 0;
            if isolated {
                if corners.iter().any(|k| k.a > 0) {
                    continue;
                }
            } else {
                let contrast = (luma(c) - luma(around)).abs();
                if corners.contains(&c) || contrast > options.orphan_contrast as i32 {
                    continue;
                }
            }
            let (x, y) = (x as u32, y as u32);
            let found = Found {
                code: LintCode::OrphanPixel,
                kind: Kind::Sprite,
                name: &sprite.name,
                position: Some(pixel_position(sprite, layers, x, y).unwrap_or(def.pos)),
                sprite: Some(&sprite.name),
                pixels: vec![(x, y)],
            };
            sink.push(found, || {
                if isolated {
                    format!(
                        "A lone pixel in sprite `{}` at ({x}, {y}), touching nothing",
                        sprite.name
                    )
                } else {
                    format!(
                        "A lone pixel in sprite `{}` at ({x}, {y}), close in lightness to the \
                         color around it, reads as noise",
                        sprite.name
                    )
                }
            });
        }
    }
}

/// The character in the text that draws pixel `(x, y)`, when the layer seen there is
/// written as rows; `None` when it comes from another sprite.
fn pixel_position(sprite: &Sprite, layers: &[DrawnLayer], x: u32, y: u32) -> Option<Position> {
    let seen = sprite.top_layer(x, y)?;
    let mut first = 0;
    for layer in layers {
        if seen < first + layer.layers {
            let start = layer.rows.as_ref()?.get(y as usize)?;
            return Some(Position::in_file(
                start.file,
                start.line,
                start.column + x as usize,
            ));
        }
        first += layer.layers;
    }
    None
}

/// `unused_key`: a key, as a palette writes it, that no sprite, frame, tile or
/// texture ramp uses through that palette or a palette copied from it.
fn unused_keys(document: &Document, index: &Index, sink: &mut Sink) {
    let sources = &document.sources;
    let mut used: HashSet<(&str, char)> = HashSet::new();
    let mut mark = |palette: &str, key: char| {
        // The key's color comes from the nearest palette along `from=` that writes it.
        let mut name = palette;
        for _ in 0..=sources.defs.len() {
            let Some(def) = index.get(&(Kind::Palette, name)) else {
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
    for def in &sources.defs {
        let Detail::Palette { keys, .. } = &def.detail else {
            continue;
        };
        // A palette defined twice is an error; only the first is in the document.
        if !index
            .get(&(Kind::Palette, def.name.as_str()))
            .is_some_and(|first| std::ptr::eq(*first, def))
        {
            continue;
        }
        for &(key, pos) in keys {
            if used.contains(&(def.name.as_str(), key)) {
                continue;
            }
            let found = Found {
                code: LintCode::UnusedKey,
                kind: Kind::Palette,
                name: &def.name,
                position: Some(pos),
                sprite: None,
                pixels: Vec::new(),
            };
            sink.push(found, || {
                format!("Nothing uses key `{key}` of palette `{}`", def.name)
            });
        }
    }
}
