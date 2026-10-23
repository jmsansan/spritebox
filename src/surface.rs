//! Palette-key painting: operations replace keys, never blend intermediate colors.
use crate::scene::{Budget, scene_seed};
use crate::sprite::EMPTY;
use crate::{Color, Dither, Error, Image, Limits, Noise, Palette, Result};
use std::collections::HashSet;
use std::sync::Arc;

/// A filled primitive, with signed pixel coordinates and half-open box bounds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Primitive {
    Rect { at: (i32, i32), size: (u32, u32) },
    Line { from: (i32, i32), to: (i32, i32) },
    Polygon { points: Vec<(i32, i32)> },
    Ellipse { at: (i32, i32), size: (u32, u32) },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Axis {
    X,
    Y,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GradientMode {
    Bands,
    Blend,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GradientStop {
    pub position: u32,
    pub key: char,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gradient {
    pub axis: Axis,
    pub mode: GradientMode,
    pub dither: Dither,
    pub stops: Vec<GradientStop>,
}
/// A profile uses unnormalized wrapped value noise. Heights are y boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileOptions {
    pub key: char,
    pub baseline: u32,
    pub amplitude: u32,
    pub cell: u32,
    pub octaves: u32,
    pub roughness: u32,
    pub seed: u64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Profile {
    name: String,
    heights: Arc<Vec<u32>>,
}
impl Profile {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn heights(&self) -> &[u32] {
        &self.heights
    }
}
/// An immutable finite key grid, separate from sprites and atlas regions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Surface {
    pub(crate) commands: usize,
    pub(crate) used_keys: Vec<char>,
    pub(crate) name: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) palette: Palette,
    pub(crate) keys: Arc<Vec<char>>,
    pub(crate) profiles: Vec<Profile>,
    pub(crate) seed: u64,
}
impl Surface {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub fn palette(&self) -> &Palette {
        &self.palette
    }
    pub fn key(&self, x: u32, y: u32) -> Option<char> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(self.keys[y as usize * self.width as usize + x as usize]).filter(|&k| k != EMPTY)
    }
    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }
    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }
    pub fn render(&self) -> Result<Image> {
        self.render_with(&Limits::default())
    }
    pub fn render_with(&self, limits: &Limits) -> Result<Image> {
        let mut b = Budget::new(limits);
        b.pixels(self.storage())?;
        b.pixels(self.width as u64 * self.height as u64)?;
        b.work(self.width as u64 * self.height as u64)?;
        let mut image = Image::new_with(self.width, self.height, limits)?;
        let lookup = crate::sprite::ColorLookup::new(&self.palette);
        for y in 0..self.height {
            for x in 0..self.width {
                image.set_pixel(
                    x,
                    y,
                    self.key(x, y)
                        .map(|k| lookup.get(k))
                        .unwrap_or(Color::TRANSPARENT),
                );
            }
        }
        Ok(image)
    }
    pub fn builder(
        name: &str,
        width: u32,
        height: u32,
        palette: &Palette,
        seed: u64,
        limits: &Limits,
    ) -> Result<SurfaceBuilder> {
        let mut budget = Budget::new(limits);
        let surface = Self::new(name, width, height, palette, seed, limits, &mut budget)?;
        Ok(SurfaceBuilder {
            surface,
            names: HashSet::new(),
            limits: limits.clone(),
            budget,
        })
    }
    pub(crate) fn new(
        name: &str,
        width: u32,
        height: u32,
        palette: &Palette,
        seed: u64,
        limits: &Limits,
        budget: &mut Budget,
    ) -> Result<Self> {
        crate::background_builder::check_name(name)?;
        if width == 0 || height == 0 {
            return Err(Error::invalid("Surface dimensions must be positive"));
        }
        limits.check_image(width, height)?;
        budget.pixels(width as u64 * height as u64)?;
        Ok(Self {
            commands: 0,
            used_keys: Vec::new(),
            name: name.into(),
            width,
            height,
            palette: palette.clone(),
            seed,
            keys: Arc::new(vec![EMPTY; width as usize * height as usize]),
            profiles: Vec::new(),
        })
    }
    pub(crate) fn storage(&self) -> u64 {
        self.keys.len() as u64
            + self
                .profiles
                .iter()
                .map(|p| p.heights.len() as u64)
                .sum::<u64>()
    }
    pub(crate) fn check_key(&self, key: char) -> Result<()> {
        if self.palette.color(key).is_none() {
            return Err(Error::new(
                crate::ErrorCode::UnknownName,
                format!("Unknown surface palette key `{key}`"),
            ));
        }
        Ok(())
    }
    fn put(&mut self, x: i64, y: i64, key: char) {
        if x >= 0 && y >= 0 && x < self.width as i64 && y < self.height as i64 {
            Arc::make_mut(&mut self.keys)[y as usize * self.width as usize + x as usize] = key;
        }
    }
    pub(crate) fn paint(
        &mut self,
        primitive: &Primitive,
        key: char,
        limits: &Limits,
        b: &mut Budget,
    ) -> Result<()> {
        self.check_key(key)?;
        self.commands += 1 + if let Primitive::Polygon { points } = primitive {
            points.len()
        } else {
            0
        };
        if !self.used_keys.contains(&key) {
            self.used_keys.push(key);
        }
        match primitive {
            Primitive::Rect { at, size } | Primitive::Ellipse { at, size } => {
                let (w, h) = *size;
                if w == 0 || h == 0 {
                    return Err(Error::invalid("Primitive dimensions must be positive"));
                }
                limits.check_image(w, h)?;
                let (l, t, r, d) = self.bounds(
                    at.0 as i64,
                    at.1 as i64,
                    at.0 as i64 + w as i64,
                    at.1 as i64 + h as i64,
                );
                b.work((r - l) as u64 * (d - t) as u64)?;
                for y in t..d {
                    for x in l..r {
                        let dx = 2 * (x - at.0 as i64) as i128 + 1 - w as i128;
                        let dy = 2 * (y - at.1 as i64) as i128 + 1 - h as i128;
                        if matches!(primitive, Primitive::Rect { .. })
                            || dx * dx * h as i128 * h as i128 + dy * dy * w as i128 * w as i128
                                <= w as i128 * w as i128 * h as i128 * h as i128
                        {
                            self.put(x, y, key);
                        }
                    }
                }
            }
            Primitive::Line { from, to } => {
                let (a, z) = if from <= to {
                    (*from, *to)
                } else {
                    (*to, *from)
                };
                let (dx, dy) = (z.0 as i64 - a.0 as i64, z.1 as i64 - a.1 as i64);
                let horizontal = dx.abs() >= dy.abs();
                let (major, minor, start, side) = if horizontal {
                    (dx, dy, a.0 as i64, self.width)
                } else {
                    (dy, dx, a.1 as i64, self.height)
                };
                let n = major.abs();
                if n == 0 {
                    b.work(1)?;
                    self.put(a.0 as i64, a.1 as i64, key);
                    return Ok(());
                }
                let (lo, hi) = if major >= 0 {
                    ((-start).max(0), (side as i64 - 1 - start).min(n))
                } else {
                    ((start - side as i64 + 1).max(0), start.min(n))
                };
                if lo > hi {
                    return Ok(());
                }
                b.work((hi - lo + 1) as u64)?;
                for i in lo..=hi {
                    let off = ((2 * i as i128 * minor.abs() as i128 + n as i128) / (2 * n as i128))
                        as i64
                        * minor.signum();
                    let (x, y) = if horizontal {
                        (a.0 as i64 + i * major.signum(), a.1 as i64 + off)
                    } else {
                        (a.0 as i64 + off, a.1 as i64 + i * major.signum())
                    };
                    self.put(x, y, key);
                }
            }
            Primitive::Polygon { points } => {
                validate_polygon(points, b).map_err(|(_, e)| e)?;
                let l = points.iter().map(|p| p.0 as i64).min().unwrap();
                let r = points.iter().map(|p| p.0 as i64).max().unwrap();
                let t = points.iter().map(|p| p.1 as i64).min().unwrap();
                let d = points.iter().map(|p| p.1 as i64).max().unwrap();
                let (l, t, r, d) = self.bounds(l, t, r, d);
                b.work(
                    ((r - l) as u64 * (d - t) as u64)
                        .checked_mul(points.len() as u64 + 1)
                        .ok_or_else(|| Error::limit("Polygon work overflow"))?,
                )?;
                for y in t..d {
                    for x in l..r {
                        let (px, py) = (2 * x as i128 + 1, 2 * y as i128 + 1);
                        let mut inside = false;
                        for (a, z) in edges(points) {
                            let (a, z) = if a.1 < z.1 { (a, z) } else { (z, a) };
                            if py >= 2 * a.1 as i128 && py < 2 * z.1 as i128 {
                                let lhs = (px - 2 * a.0 as i128) * (z.1 as i128 - a.1 as i128);
                                let rhs = (py - 2 * a.1 as i128) * (z.0 as i128 - a.0 as i128);
                                if lhs < rhs {
                                    inside = !inside;
                                }
                            }
                        }
                        if inside {
                            self.put(x, y, key);
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn bounds(&self, l: i64, t: i64, r: i64, b: i64) -> (i64, i64, i64, i64) {
        (
            l.clamp(0, self.width as i64),
            t.clamp(0, self.height as i64),
            r.clamp(0, self.width as i64),
            b.clamp(0, self.height as i64),
        )
    }
    pub(crate) fn gradient(&mut self, g: &Gradient, b: &mut Budget) -> Result<()> {
        self.commands += 1 + g.stops.len();
        let length = match g.axis {
            Axis::X => self.width,
            Axis::Y => self.height,
        };
        if g.stops.is_empty()
            || g.stops.iter().any(|s| s.position >= length)
            || g.stops.windows(2).any(|s| s[0].position >= s[1].position)
        {
            return Err(Error::invalid(
                "Gradient stops must be nonempty, increasing and in bounds",
            ));
        }
        if !matches!(g.dither, Dither::None | Dither::Bayer(2 | 4 | 8))
            || (g.mode == GradientMode::Bands && g.dither != Dither::None)
        {
            return Err(Error::invalid("Invalid gradient mode/dither combination"));
        }
        for stop in &g.stops {
            self.check_key(stop.key)?;
            if !self.used_keys.contains(&stop.key) {
                self.used_keys.push(stop.key);
            }
        }
        b.work(self.width as u64 * self.height as u64)?;
        for y in 0..self.height {
            for x in 0..self.width {
                let v = match g.axis {
                    Axis::X => x,
                    Axis::Y => y,
                };
                let i = g
                    .stops
                    .partition_point(|s| s.position <= v)
                    .saturating_sub(1);
                let a = g.stops[i];
                let key =
                    if g.mode == GradientMode::Blend && v > a.position && i + 1 < g.stops.len() {
                        let z = g.stops[i + 1];
                        let value = ((v - a.position) as u64 * 65535
                            / (z.position - a.position) as u64)
                            as u16;
                        if g.dither.pick(value, x, y, 2) == 0 {
                            a.key
                        } else {
                            z.key
                        }
                    } else {
                        a.key
                    };
                self.put(x as i64, y as i64, key);
            }
        }
        Ok(())
    }
    pub(crate) fn add_profile(
        &mut self,
        name: &str,
        p: &ProfileOptions,
        b: &mut Budget,
    ) -> Result<()> {
        self.check_key(p.key)?;
        self.commands += 1;
        if !self.used_keys.contains(&p.key) {
            self.used_keys.push(p.key);
        }
        if p.baseline > self.height || p.amplitude > p.baseline {
            return Err(Error::invalid(
                "Profile needs amplitude <= baseline <= surface height",
            ));
        }
        let noise = Noise {
            width: self.width,
            height: p.cell,
            cell: p.cell,
            octaves: p.octaves,
            roughness: p.roughness,
            seed: scene_seed(&["profile", &self.name, name], &[self.seed, p.seed]),
        };
        noise.check()?;
        b.pixels(self.width as u64)?;
        b.work(self.width as u64 * p.octaves as u64)?;
        let heights: Vec<u32> = (0..self.width)
            .map(|x| {
                p.baseline - (p.amplitude as u64 * noise.value(x as i64, 0) as u64 / 65535) as u32
            })
            .collect();
        let visits = heights.iter().map(|h| (self.height - h) as u64).sum();
        b.work(visits)?;
        for (x, &height) in heights.iter().enumerate() {
            for y in height..self.height {
                self.put(x as i64, y as i64, p.key);
            }
        }
        self.profiles.push(Profile {
            name: name.into(),
            heights: Arc::new(heights),
        });
        Ok(())
    }
}
/// Validated mutable authoring path; [`finish`](Self::finish) returns immutable keys.
pub struct SurfaceBuilder {
    surface: Surface,
    names: HashSet<String>,
    limits: Limits,
    budget: Budget,
}
impl SurfaceBuilder {
    fn name(&mut self, name: &str, records: usize) -> Result<()> {
        crate::background_builder::check_name(name)?;
        if name.is_empty() || !self.names.insert(name.into()) {
            return Err(Error::new(
                crate::ErrorCode::DuplicateName,
                "Duplicate or empty surface operation name",
            ));
        }
        self.budget.commands(1 + records)
    }
    pub fn paint(&mut self, name: &str, primitive: &Primitive, key: char) -> Result<&mut Self> {
        self.name(
            name,
            if let Primitive::Polygon { points } = primitive {
                points.len()
            } else {
                0
            },
        )?;
        self.surface
            .paint(primitive, key, &self.limits, &mut self.budget)?;
        Ok(self)
    }
    pub fn gradient(&mut self, name: &str, gradient: &Gradient) -> Result<&mut Self> {
        self.name(name, gradient.stops.len())?;
        self.surface.gradient(gradient, &mut self.budget)?;
        Ok(self)
    }
    pub fn profile(&mut self, name: &str, profile: &ProfileOptions) -> Result<&mut Self> {
        self.name(name, 0)?;
        self.surface.add_profile(name, profile, &mut self.budget)?;
        Ok(self)
    }
    pub fn finish(self) -> Surface {
        self.surface
    }
}
fn edges(p: &[(i32, i32)]) -> impl Iterator<Item = ((i32, i32), (i32, i32))> + '_ {
    p.iter()
        .copied()
        .zip(p.iter().copied().cycle().skip(1))
        .take(p.len())
}
fn cross(a: (i32, i32), b: (i32, i32), c: (i32, i32)) -> i128 {
    (b.0 as i128 - a.0 as i128) * (c.1 as i128 - a.1 as i128)
        - (b.1 as i128 - a.1 as i128) * (c.0 as i128 - a.0 as i128)
}
fn on(a: (i32, i32), b: (i32, i32), p: (i32, i32)) -> bool {
    cross(a, b, p) == 0
        && p.0 >= a.0.min(b.0)
        && p.0 <= a.0.max(b.0)
        && p.1 >= a.1.min(b.1)
        && p.1 <= a.1.max(b.1)
}
pub(crate) fn validate_polygon(
    p: &[(i32, i32)],
    b: &mut Budget,
) -> std::result::Result<(), (usize, Error)> {
    if p.len() < 3 {
        return Err((
            0,
            Error::invalid("Polygon needs at least three distinct vertices"),
        ));
    }
    let mut seen = HashSet::new();
    for (i, &v) in p.iter().enumerate() {
        b.work(1).map_err(|e| (i, e))?;
        if !seen.insert(v) {
            return Err((i, Error::invalid("Polygon vertices must be distinct")));
        }
    }
    let area: i128 = edges(p)
        .map(|(a, z)| a.0 as i128 * z.1 as i128 - a.1 as i128 * z.0 as i128)
        .sum();
    if area == 0 {
        return Err((0, Error::invalid("Polygon area must be nonzero")));
    }
    for (i, (a, z)) in edges(p).enumerate() {
        for (j, (c, d)) in edges(p).enumerate().skip(i + 1) {
            b.work(1).map_err(|e| (j, e))?;
            if j == i + 1 || (i == 0 && j + 1 == p.len()) {
                let (outer, shared, other) = if j == i + 1 { (a, z, d) } else { (z, a, c) };
                if cross(outer, shared, other) == 0
                    && (on(outer, shared, other) || on(shared, other, outer))
                {
                    return Err((j, Error::invalid("Polygon edges overlap")));
                }
                continue;
            }
            let (ac, ad, ca, cz) = (
                cross(a, z, c),
                cross(a, z, d),
                cross(c, d, a),
                cross(c, d, z),
            );
            if (ac.signum() != ad.signum() && ca.signum() != cz.signum())
                || on(a, z, c)
                || on(a, z, d)
                || on(c, d, a)
                || on(c, d, z)
            {
                return Err((j, Error::invalid("Polygon must not intersect itself")));
            }
        }
    }
    Ok(())
}
