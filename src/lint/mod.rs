//! Lint: warnings for art that is valid but does not follow pixel-art practice.

mod lines;
mod palette;
mod sets;
mod shading;

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fmt;
use std::str::FromStr;

use crate::{Document, Error, ErrorCode, Limits, Position, Sprite};

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
    /// Steps along a silhouette's edge whose lengths jump, such as `1, 1, 3, 1, 1`,
    /// where a clean line keeps one length or changes it gradually.
    Jaggy,
    /// An outline two pixels thick at an L corner, where one diagonal step would do.
    DoubleOutline,
    /// A lone translucent pixel in an otherwise opaque sprite: a leftover of smoothing.
    StrayAlpha,
    /// Two keys of a palette, both used, whose colors are too close to tell apart.
    NearDuplicateColor,
    /// A ramp whose hue does not change from end to end: shades that only change in
    /// brightness look dull.
    FlatRamp,
    /// Neighbouring keys of a ramp too close in lightness to tell apart.
    LowContrast,
    /// Shading that darkens toward the edges on every side, with no direction of
    /// light.
    PillowShading,
    /// A line of a shade one pixel thick between a lighter and a darker one: shading
    /// stacked in parallel lines.
    Banding,
    /// A sprite drawn at 2×, 3× or 4× among sprites drawn at 1×.
    Mixel,
    /// More colors than a sprite of its size reads well with.
    TooManyColors,
    /// A sprite of a set (an animation's frames, a group) of another size than most.
    InconsistentSize,
    /// A sprite of a set with no outline, or another, when most have one; or with
    /// one when most have none.
    InconsistentOutline,
    /// A sprite of a set drawn with a palette that is not a copy of the one most use.
    InconsistentPalette,
    /// A sprite of a set drawn at another pixel scale than most.
    InconsistentScale,
    /// A palette key that nothing uses.
    UnusedKey,
}

impl LintCode {
    /// Every code, in order.
    pub const ALL: [LintCode; 16] = [
        LintCode::OrphanPixel,
        LintCode::Jaggy,
        LintCode::DoubleOutline,
        LintCode::StrayAlpha,
        LintCode::PillowShading,
        LintCode::Banding,
        LintCode::Mixel,
        LintCode::TooManyColors,
        LintCode::InconsistentSize,
        LintCode::InconsistentOutline,
        LintCode::InconsistentPalette,
        LintCode::InconsistentScale,
        LintCode::NearDuplicateColor,
        LintCode::FlatRamp,
        LintCode::LowContrast,
        LintCode::UnusedKey,
    ];

    /// The code's stable name in `snake_case`, such as `"orphan_pixel"`.
    pub fn as_str(self) -> &'static str {
        match self {
            LintCode::OrphanPixel => "orphan_pixel",
            LintCode::Jaggy => "jaggy",
            LintCode::DoubleOutline => "double_outline",
            LintCode::StrayAlpha => "stray_alpha",
            LintCode::PillowShading => "pillow_shading",
            LintCode::Banding => "banding",
            LintCode::Mixel => "mixel",
            LintCode::TooManyColors => "too_many_colors",
            LintCode::InconsistentSize => "inconsistent_size",
            LintCode::InconsistentOutline => "inconsistent_outline",
            LintCode::InconsistentPalette => "inconsistent_palette",
            LintCode::InconsistentScale => "inconsistent_scale",
            LintCode::NearDuplicateColor => "near_duplicate_color",
            LintCode::FlatRamp => "flat_ramp",
            LintCode::LowContrast => "low_contrast",
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
    /// How near, in percent of the sprite's shorter side, the light must sit to the
    /// middle of the fill for `pillow_shading`. Default 10.
    pub pillow_offset: u32,
    /// The shortest line of one shade that `banding` reports, in pixels. Default 4.
    pub banding_length: usize,
    /// The most colors for `too_many_colors` in a sprite under 32×32. Default 8.
    pub max_colors_small: usize,
    /// The most colors for `too_many_colors` in any sprite. Default 16.
    pub max_colors: usize,
    /// The smallest OKLab distance, in ten-thousandths, between two used keys of a
    /// palette before `near_duplicate_color` reports them. Default 300 (0.03).
    pub min_color_distance: u32,
    /// The smallest step of OKLab lightness, in ten-thousandths, between neighbouring
    /// keys of a ramp before `low_contrast` reports them. Default 600 (0.06).
    pub min_ramp_step: u32,
}

impl Default for LintOptions {
    fn default() -> Self {
        Self {
            allow: Vec::new(),
            orphan_contrast: 32,
            pillow_offset: 10,
            banding_length: 4,
            max_colors_small: 8,
            max_colors: 16,
            min_color_distance: 300,
            min_ramp_step: 600,
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
    Group,
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
    /// Palettes and the keys that sprites used from them midway through chains, such
    /// as `q` in `s:swap(q):outline(k):swap(p)`.
    pub palette_uses: std::collections::BTreeSet<(String, char)>,
    /// Sprites that belong together.
    pub sets: Vec<Set>,
}

/// Sprites that belong together: an animation's frames, a group's sprites, or a
/// tileset's tiles, each by the name of its sprite in the document, at the place
/// that names it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Set {
    pub kind: Kind,
    pub name: String,
    pub members: Vec<Member>,
}

/// A sprite of a set: its name in the document, the place that names it, and the
/// sprite it is made from, whose `lint=` reaches it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Member {
    pub name: String,
    pub pos: Position,
    pub source: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Def {
    pub kind: Kind,
    pub name: String,
    pub pos: Position,
    pub allow: Vec<LintCode>,
    /// What it is built from: the sprites of a sprite, the palette a palette copies.
    pub deps: Vec<(Kind, String)>,
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
        /// Its own ramps.
        ramps: Vec<RampSource>,
    },
}

/// A ramp as a palette writes it: its name, the name's position, and its keys at
/// their positions.
pub(crate) type RampSource = (String, Position, Vec<(char, Position)>);

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
        self.allows_where(index, kind, name, code, true)
    }

    /// [`Sources::allows`], with the file's `lint allow` only when `file`.
    fn allows_where(
        &self,
        index: &Index,
        kind: Kind,
        name: &str,
        code: LintCode,
        file: bool,
    ) -> bool {
        if let Some(def) = index.get(&(kind, name)).filter(|_| file) {
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
            stack.extend(def.deps.iter().map(|(kind, name)| (*kind, name.as_str())))
        }
        false
    }
}

/// Collects warnings: drops those allowed, counts the rest, and keeps only the first
/// `limit` in order, so that memory stays bounded however many there are.
pub(super) struct Sink<'s> {
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
pub(super) struct Found<'f> {
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

    /// Whether `code` is allowed for the definition: by the options or the text.
    pub(super) fn allowed(&mut self, code: LintCode, kind: Kind, name: &str) -> bool {
        if self.options.allow.contains(&code) {
            return true;
        }
        let known = self
            .allowed
            .get(&(kind, code))
            .and_then(|names| names.get(name))
            .copied();
        known.unwrap_or_else(|| {
            let allowed = self.sources.allows(&self.index, kind, name, code);
            self.allowed
                .entry((kind, code))
                .or_default()
                .insert(name.to_owned(), allowed);
            allowed
        })
    }

    /// Whether `code` is allowed for a sprite by the options or by `lint=` on it or
    /// on what it is made from; not by its file's `lint allow`, which stays in that
    /// file.
    pub(super) fn allowed_by_definition(&self, code: LintCode, kind: Kind, name: &str) -> bool {
        self.options.allow.contains(&code)
            || self
                .sources
                .allows_where(&self.index, kind, name, code, false)
    }

    pub(super) fn push(&mut self, found: Found, message: impl FnOnce() -> String) {
        let code = found.code;
        if self.allowed(code, found.kind, found.name) {
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
    // Each drawn sprite's pixel scale, for `mixel`.
    let mut scales = Vec::new();
    for sprite in &document.sprites {
        let Some(def) = index.get(&(Kind::Sprite, sprite.name.as_str())) else {
            continue;
        };
        if let Detail::Drawn(layers) = &def.detail {
            let canvas = lines::Canvas::new(sprite);
            let report = lines::Report {
                sprite,
                def,
                layers,
            };
            lines::check(&canvas, &report, options, &mut sink);
            shading::check(&canvas, &report, options, &mut sink);
            scales.push((shading::scale(&canvas), report));
        }
    }
    shading::mixels(&scales, &mut sink);
    sets::check(document, &index, &mut sink);
    palette::check(document, &index, options, &mut sink);
    sink.finish()
}

/// The character in the text that draws pixel `(x, y)`, when the layer seen there is
/// written as rows; `None` when it comes from another sprite.
pub(super) fn pixel_position(
    sprite: &Sprite,
    layers: &[DrawnLayer],
    x: u32,
    y: u32,
) -> Option<Position> {
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
