//! Finite native layers sampled with exact integer camera transforms.
use crate::scene::{Budget, random, scene_seed};
use crate::{
    Color, Document, Error, ErrorCode, Image, Limits, Position, Rect, Result, Sprite, Surface,
    Tilemap, Tileset,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ratio {
    num: i32,
    den: u32,
}
impl Ratio {
    pub fn new(num: i32, den: u32) -> Result<Self> {
        if den == 0 {
            return Err(Error::invalid("Parallax denominator must be positive"));
        }
        let (mut a, mut b) = (num.unsigned_abs(), den);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        let divisor = a.max(1);
        Ok(Self {
            num: (num as i64 / divisor as i64) as i32,
            den: den / divisor,
        })
    }
    pub fn numerator(self) -> i32 {
        self.num
    }
    pub fn denominator(self) -> u32 {
        self.den
    }
    pub fn offset(self, camera: i32) -> i64 {
        (camera as i64 * self.num as i64).div_euclid(self.den as i64)
    }
}
impl Default for Ratio {
    fn default() -> Self {
        Self { num: 1, den: 1 }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Repeat {
    #[default]
    None,
    X,
    Y,
    Both,
}
impl Repeat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::X => "x",
            Self::Y => "y",
            Self::Both => "both",
        }
    }
    pub(crate) fn x(self) -> bool {
        matches!(self, Self::X | Self::Both)
    }
    pub(crate) fn y(self) -> bool {
        matches!(self, Self::Y | Self::Both)
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Anchor {
    #[default]
    TopLeft,
    BottomLeft,
    Center,
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum SourceRef {
    Sprite(String),
    Surface(String),
    Tilemap(String),
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BackgroundRenderOptions {
    pub camera: (i32, i32),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerOptions {
    pub origin: (i32, i32),
    pub repeat: Repeat,
    pub parallax: (Ratio, Ratio),
    pub clip: Option<Rect>,
}
impl Default for LayerOptions {
    fn default() -> Self {
        Self {
            origin: (0, 0),
            repeat: Repeat::None,
            parallax: (Ratio::default(), Ratio::default()),
            clip: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlacementOptions {
    pub at: (i32, i32),
    pub anchor: Anchor,
    pub crop: Option<Rect>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackgroundPlacement {
    pub(crate) source: SourceRef,
    pub(crate) sprite: Option<Arc<Sprite>>,
    pub(crate) options: PlacementOptions,
    pub(crate) position: Position,
}
impl BackgroundPlacement {
    pub fn source(&self) -> &SourceRef {
        &self.source
    }
    pub fn options(&self) -> PlacementOptions {
        self.options
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackgroundLayer {
    pub(crate) name: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) options: LayerOptions,
    pub(crate) items: Vec<BackgroundPlacement>,
    pub(crate) position: Position,
}
impl BackgroundLayer {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub fn options(&self) -> &LayerOptions {
        &self.options
    }
    pub fn placements(&self) -> &[BackgroundPlacement] {
        &self.items
    }
    /// Map a viewport pixel to this layer's native coordinates. Transparent margins
    /// are part of the repeat period. Negative fractional shifts use floor.
    pub fn sample(&self, x: u32, y: u32, camera: (i32, i32)) -> Option<(u32, u32)> {
        let u = x as i64 - self.options.origin.0 as i64 + self.options.parallax.0.offset(camera.0);
        let v = y as i64 - self.options.origin.1 as i64 + self.options.parallax.1.offset(camera.1);
        let axis = |v: i64, n: u32, repeat: bool| {
            if repeat {
                Some(v.rem_euclid(n as i64) as u32)
            } else {
                (v >= 0 && v < n as i64).then_some(v as u32)
            }
        };
        Some((
            axis(u, self.width, self.options.repeat.x())?,
            axis(v, self.height, self.options.repeat.y())?,
        ))
    }
    pub(crate) fn native(&self, doc: &Document, limits: &Limits, b: &mut Budget) -> Result<Image> {
        limits.check_image(self.width, self.height)?;
        b.pixels(self.width as u64 * self.height as u64)?;
        let mut image = Image::new_with(self.width, self.height, limits)?;
        for item in &self.items {
            let source = item.view(doc)?;
            let lookup = crate::sprite::ColorLookup::new(source.palette(doc));
            let crop = item.crop(source.size()?)?;
            let (x, y) = item.top_left(crop);
            let (l, t, r, d) = crate::screen::placement_bounds(
                image.width(),
                image.height(),
                crop.width,
                crop.height,
                x,
                y,
            );
            b.work(
                ((r - l) as u64 * (d - t) as u64)
                    .checked_mul(source.cost())
                    .ok_or_else(|| Error::limit("Source work overflow"))?,
            )?;
            for py in t..d {
                for px in l..r {
                    let color =
                        source.pixel(&lookup, crop.x + (px - x) as u32, crop.y + (py - y) as u32);
                    let below = image.pixel(px as u32, py as u32).unwrap();
                    image.set_pixel(px as u32, py as u32, color.over(below));
                }
            }
        }
        if let Some(clip) = self.options.clip {
            b.work(self.width as u64 * self.height as u64)?;
            for y in 0..self.height {
                for x in 0..self.width {
                    if x < clip.x
                        || y < clip.y
                        || x >= clip.x + clip.width
                        || y >= clip.y + clip.height
                    {
                        image.set_pixel(x, y, Color::TRANSPARENT);
                    }
                }
            }
        }
        Ok(image)
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
#[doc=include_str!("../docs/backgrounds.md")]
pub struct Background {
    pub(crate) commands: usize,
    pub(crate) name: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) layers: Vec<BackgroundLayer>,
    pub(crate) seed: u64,
}
impl Background {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub fn layers(&self) -> &[BackgroundLayer] {
        &self.layers
    }
    pub fn render(&self, doc: &Document, options: &BackgroundRenderOptions) -> Result<Image> {
        self.render_with(doc, options, &Limits::default())
    }
    pub fn render_with(
        &self,
        doc: &Document,
        options: &BackgroundRenderOptions,
        limits: &Limits,
    ) -> Result<Image> {
        let mut b = Budget::new(limits);
        let mut shared = std::collections::HashSet::new();
        b.pixels(doc.scene_storage_seen(&mut shared))?;
        if !doc.backgrounds.iter().any(|bg| std::ptr::eq(bg, self)) {
            b.pixels(self.storage(&mut shared))?;
        }
        let layers = self.native_layers(doc, limits, &mut b)?;
        self.flatten(&layers, options, limits, &mut b)
    }
    pub(crate) fn native_layers(
        &self,
        doc: &Document,
        limits: &Limits,
        b: &mut Budget,
    ) -> Result<Vec<Image>> {
        if self.layers.len() > limits.max_layers {
            return Err(Error::limit("Too many background layers"));
        }
        self.layers
            .iter()
            .map(|l| l.native(doc, limits, b))
            .collect()
    }
    pub(crate) fn storage(&self, sprites: &mut std::collections::HashSet<*const Sprite>) -> u64 {
        self.layers
            .iter()
            .map(|l| {
                l.items.len() as u64
                    + l.items
                        .iter()
                        .filter_map(|p| p.sprite.as_ref())
                        .filter(|s| sprites.insert(Arc::as_ptr(s)))
                        .map(|s| s.width as u64 * s.height as u64 * s.layers.len() as u64)
                        .sum::<u64>()
            })
            .sum()
    }
    pub(crate) fn flatten(
        &self,
        layers: &[Image],
        options: &BackgroundRenderOptions,
        limits: &Limits,
        b: &mut Budget,
    ) -> Result<Image> {
        b.pixels(self.width as u64 * self.height as u64)?;
        let mut image = Image::new_with(self.width, self.height, limits)?;
        b.work(
            (self.width as u64 * self.height as u64)
                .checked_mul(self.layers.len() as u64)
                .ok_or_else(|| Error::limit("View work overflow"))?,
        )?;
        for (layer, native) in self.layers.iter().zip(layers) {
            for y in 0..self.height {
                for x in 0..self.width {
                    if let Some((u, v)) = layer.sample(x, y, options.camera) {
                        let above = native.pixel(u, v).unwrap();
                        let below = image.pixel(x, y).unwrap();
                        image.set_pixel(x, y, above.over(below));
                    }
                }
            }
        }
        Ok(image)
    }
}
impl Document {
    pub fn surfaces(&self) -> &[Surface] {
        &self.surfaces
    }
    pub fn surface(&self, name: &str) -> Option<&Surface> {
        self.surfaces.iter().find(|s| s.name() == name)
    }
    pub fn backgrounds(&self) -> &[Background] {
        &self.backgrounds
    }
    pub fn background(&self, name: &str) -> Option<&Background> {
        self.backgrounds.iter().find(|s| s.name() == name)
    }
    pub(crate) fn scene_storage(&self) -> u64 {
        self.scene_storage_seen(&mut std::collections::HashSet::new())
    }
    pub(crate) fn scene_commands(&self) -> usize {
        self.surfaces.iter().map(|s| s.commands).sum::<usize>()
            + self.backgrounds.iter().map(|b| b.commands).sum::<usize>()
    }
    pub(crate) fn scene_storage_seen(
        &self,
        sprites: &mut std::collections::HashSet<*const Sprite>,
    ) -> u64 {
        self.surfaces.iter().map(|s| s.storage()).sum::<u64>()
            + self
                .backgrounds
                .iter()
                .map(|b| b.storage(sprites))
                .sum::<u64>()
    }
}
pub(crate) enum SourceView<'a> {
    Sprite(&'a Sprite),
    Surface(&'a Surface),
    Tilemap(&'a Tilemap, &'a Tileset),
}
impl SourceView<'_> {
    pub fn size(&self) -> Result<(u32, u32)> {
        match self {
            Self::Sprite(s) => Ok((s.width, s.height)),
            Self::Surface(s) => Ok(s.size()),
            Self::Tilemap(m, t) => Ok((
                m.width
                    .checked_mul(t.tile_width)
                    .ok_or_else(|| Error::limit("Map width overflow"))?,
                m.height
                    .checked_mul(t.tile_height)
                    .ok_or_else(|| Error::limit("Map height overflow"))?,
            )),
        }
    }
    fn cost(&self) -> u64 {
        match self {
            Self::Sprite(s) => s.layers.len() as u64,
            Self::Surface(_) => 1,
            Self::Tilemap(_, t) => t
                .tiles
                .iter()
                .map(|s| s.layers.len() as u64)
                .max()
                .unwrap_or(1),
        }
    }
    fn palette<'a>(&'a self, doc: &'a Document) -> &'a crate::Palette {
        let p = match self {
            Self::Sprite(s) => &s.palette,
            Self::Surface(s) => &s.palette,
            Self::Tilemap(_, t) => &t.palette,
        };
        doc.palette(p.name()).unwrap_or(p)
    }
    fn pixel(&self, lookup: &crate::sprite::ColorLookup<'_>, x: u32, y: u32) -> Color {
        match self {
            Self::Sprite(s) => sprite_pixel(s, lookup, x, y),
            Self::Surface(s) => s
                .key(x, y)
                .map(|k| lookup.get(k))
                .unwrap_or(Color::TRANSPARENT),
            Self::Tilemap(m, t) => m
                .tile(x / t.tile_width, y / t.tile_height)
                .and_then(|i| i.checked_sub(1))
                .and_then(|i| t.tiles.get(i as usize))
                .map(|s| sprite_pixel(s, lookup, x % t.tile_width, y % t.tile_height))
                .unwrap_or(Color::TRANSPARENT),
        }
    }
}
fn sprite_pixel(s: &Sprite, lookup: &crate::sprite::ColorLookup<'_>, x: u32, y: u32) -> Color {
    let i = y as usize * s.width as usize + x as usize;
    s.layers
        .iter()
        .fold(Color::TRANSPARENT, |c, l| lookup.get(l.keys[i]).over(c))
}
impl BackgroundPlacement {
    pub(crate) fn view<'a>(&'a self, doc: &'a Document) -> Result<SourceView<'a>> {
        let missing = || Error::new(ErrorCode::UnknownName, "Unknown background source");
        match &self.source {
            SourceRef::Sprite(name) => self
                .sprite
                .as_deref()
                .or_else(|| doc.sprite(name))
                .map(SourceView::Sprite)
                .ok_or_else(missing),
            SourceRef::Surface(name) => doc
                .surface(name)
                .map(SourceView::Surface)
                .ok_or_else(missing),
            SourceRef::Tilemap(name) => {
                let m = doc.tilemap(name).ok_or_else(missing)?;
                let t = doc.tileset(m.tileset()).ok_or_else(missing)?;
                Ok(SourceView::Tilemap(m, t))
            }
        }
    }
    pub(crate) fn crop(&self, size: (u32, u32)) -> Result<Rect> {
        let crop = self.options.crop.unwrap_or(Rect {
            x: 0,
            y: 0,
            width: size.0,
            height: size.1,
        });
        check_rect(crop, size)?;
        Ok(crop)
    }
    pub(crate) fn top_left(&self, crop: Rect) -> (i64, i64) {
        let (x, y) = (self.options.at.0 as i64, self.options.at.1 as i64);
        match self.options.anchor {
            Anchor::TopLeft => (x, y),
            Anchor::BottomLeft => (x, y - crop.height as i64),
            Anchor::Center => (x - crop.width as i64 / 2, y - crop.height as i64 / 2),
        }
    }
}
pub(crate) fn check_rect(rect: Rect, size: (u32, u32)) -> Result<()> {
    if rect.width == 0
        || rect.height == 0
        || rect.x as u64 + rect.width as u64 > size.0 as u64
        || rect.y as u64 + rect.height as u64 > size.1 as u64
    {
        return Err(Error::invalid(
            "Crop/clip must be a positive in-bounds rectangle",
        ));
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScatterAmount {
    Count(u64),
    Density { num: u32, den: u32 },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScatterOptions {
    pub amount: ScatterAmount,
    pub spacing: u32,
    pub attempts: Option<u64>,
    pub seed: u64,
    pub mask: Option<(String, char)>,
    pub baseline: Option<(String, String)>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeightedVariant {
    pub source: SourceRef,
    pub weight: u32,
}
pub(crate) struct ScatterContext<'a> {
    pub background: &'a str,
    pub layer: &'a BackgroundLayer,
    pub operation: &'a str,
    pub owner_seed: u64,
    pub position: Position,
}
pub(crate) fn scatter(
    doc: &Document,
    c: ScatterContext<'_>,
    options: &ScatterOptions,
    variants: &[(WeightedVariant, Option<Arc<Sprite>>)],
    b: &mut Budget,
) -> Result<Vec<BackgroundPlacement>> {
    let layer = c.layer;
    let mask = options
        .mask
        .as_ref()
        .map(|(name, key)| {
            let s = doc
                .surface(name)
                .ok_or_else(|| Error::new(ErrorCode::UnknownName, "Unknown scatter mask"))?;
            s.check_key(*key)?;
            Ok::<_, Error>((s, *key))
        })
        .transpose()?;
    let baseline = options
        .baseline
        .as_ref()
        .map(|(name, profile)| {
            let s = doc
                .surface(name)
                .ok_or_else(|| Error::new(ErrorCode::UnknownName, "Unknown baseline surface"))?;
            let p = s
                .profile(profile)
                .ok_or_else(|| Error::new(ErrorCode::UnknownName, "Unknown baseline profile"))?;
            Ok::<_, Error>((s, p))
        })
        .transpose()?;
    if mask.is_some_and(|(s, _)| s.size() != layer.size())
        || baseline.is_some_and(|(s, _)| s.size() != layer.size())
    {
        return Err(Error::new(
            ErrorCode::SizeMismatch,
            "Scatter mask/profile must match native layer size",
        ));
    }
    if variants.is_empty()
        || variants
            .iter()
            .any(|(v, _)| v.weight == 0 || !matches!(v.source, SourceRef::Sprite(_)))
    {
        return Err(Error::invalid(
            "Scatter needs positive weighted sprite variants",
        ));
    }
    let weights = variants.iter().try_fold(0u64, |sum, (v, _)| {
        sum.checked_add(v.weight as u64)
            .ok_or_else(|| Error::limit("Variant weight overflow"))
    })?;
    let y_at = |x: u32, y: u32| baseline.map_or(y, |(_, p)| p.heights()[x as usize]);
    let eligible =
        |x: u32, y: u32| mask.is_none_or(|(s, k)| s.key(x, y.min(layer.height - 1)) == Some(k));
    let count = match options.amount {
        ScatterAmount::Count(n) => n,
        ScatterAmount::Density { num, den } => {
            if den == 0 || num > den {
                return Err(Error::invalid(
                    "Scatter density must be in 0..=1 with positive denominator",
                ));
            }
            let rows = if baseline.is_some() { 1 } else { layer.height };
            b.work(layer.width as u64 * rows as u64)?;
            let mut n = 0u64;
            for y in 0..rows {
                for x in 0..layer.width {
                    n += u64::from(eligible(x, y_at(x, y)));
                }
            }
            ((n as u128 * num as u128) / den as u128) as u64
        }
    };
    if options.attempts == Some(0) {
        return Err(Error::invalid("Scatter attempts must be positive"));
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    let attempts = options.attempts.unwrap_or(
        count
            .checked_mul(32)
            .ok_or_else(|| Error::limit("Scatter attempts overflow"))?
            .max(64),
    );
    // Reserve the exact retained record budget before allocating, even if geometric generation fails.
    if count > isize::MAX as u64 / std::mem::size_of::<BackgroundPlacement>() as u64 {
        return Err(Error::limit(
            "Scatter records exceed platform allocation limit",
        ));
    }
    b.pixels(count)?;
    let seed = scene_seed(
        &["scatter", c.background, &layer.name, c.operation],
        &[c.owner_seed, options.seed],
    );
    let mut items: Vec<BackgroundPlacement> = Vec::new();
    for attempt in 0..attempts {
        b.attempts(1)?;
        b.work(1)?;
        let x = random(seed, attempt, 0, layer.width as u64) as u32;
        let y = y_at(x, random(seed, attempt, 1, layer.height as u64) as u32);
        let mut draw = random(seed, attempt, 2, weights);
        let (variant, sprite) = variants
            .iter()
            .find(|(v, _)| {
                if draw < v.weight as u64 {
                    true
                } else {
                    draw -= v.weight as u64;
                    false
                }
            })
            .unwrap();
        if !eligible(x, y) {
            continue;
        }
        let mut close = false;
        for prior in &items {
            b.work(1)?;
            let (mut dx, mut dy) = (
                (prior.options.at.0 as i64 - x as i64).unsigned_abs(),
                (prior.options.at.1 as i64 - y as i64).unsigned_abs(),
            );
            if layer.options.repeat.x() {
                dx = dx.min(layer.width as u64 - dx);
            }
            if layer.options.repeat.y() {
                dy = dy.min(layer.height as u64 - dy);
            }
            if (dx == 0 && dy == 0)
                || dx * dx + dy * dy < (options.spacing as u64) * (options.spacing as u64)
            {
                close = true;
                break;
            }
        }
        if close {
            continue;
        }
        items.push(BackgroundPlacement {
            source: variant.source.clone(),
            sprite: sprite.clone(),
            options: PlacementOptions {
                at: (x as i32, y as i32),
                anchor: Anchor::BottomLeft,
                crop: None,
            },
            position: c.position,
        });
        if items.len() as u64 == count {
            return Ok(items);
        }
    }
    Err(Error::invalid(format!(
        "Scatter target {count}, accepted {}, attempts {attempts}; increase attempts, reduce count/spacing, or widen the mask",
        items.len()
    )))
}
