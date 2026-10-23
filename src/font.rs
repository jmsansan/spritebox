//! Pixel fonts: glyphs as grids of pixels, text drawn with them, and TrueType export
//! whose outlines are the pixels as squares.

use std::collections::BTreeMap;

use crate::{Color, Error, Image, Limits, Result};

/// One glyph: a grid of pixels as tall as its font, and an advance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Glyph {
    pub(crate) ch: char,
    pub(crate) width: u32,
    pub(crate) advance: u32,
    /// Row by row from the top, `width * font height` of them.
    pub(crate) pixels: Vec<bool>,
}

impl Glyph {
    pub fn char(&self) -> char {
        self.ch
    }
    /// Width of the grid in pixels; 0 for a blank glyph such as a space.
    pub fn width(&self) -> u32 {
        self.width
    }
    /// How far the pen moves after the glyph, in pixels.
    pub fn advance(&self) -> u32 {
        self.advance
    }
    /// Whether the pixel at `(x, y)` is inked, `y` from the top of the grid.
    pub fn pixel(&self, x: u32, y: u32) -> bool {
        x < self.width
            && self
                .pixels
                .get((y * self.width + x) as usize)
                .copied()
                .unwrap_or(false)
    }
}

/// A pixel font: glyphs of one height, with a baseline and a line height.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Font {
    pub(crate) name: String,
    pub(crate) height: u32,
    pub(crate) baseline: u32,
    pub(crate) line_height: u32,
    pub(crate) glyphs: Vec<Glyph>,
    pub(crate) kerning: Vec<KernPair>,
}

/// An adjustment in pixels between two glyphs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernPair {
    pub left: char,
    pub right: char,
    pub adjustment: i32,
}

impl Font {
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Rows of every glyph's grid.
    pub fn height(&self) -> u32 {
        self.height
    }
    /// Rows of the grid above the baseline; the rest descend below it.
    pub fn baseline(&self) -> u32 {
        self.baseline
    }
    /// Pixels from one baseline to the next.
    pub fn line_height(&self) -> u32 {
        self.line_height
    }
    /// The glyphs, in the order written; a space is added when none is written.
    pub fn glyphs(&self) -> &[Glyph] {
        &self.glyphs
    }
    pub fn glyph(&self, ch: char) -> Option<&Glyph> {
        self.glyphs.iter().find(|g| g.ch == ch)
    }

    /// Authored kerning pairs, ordered by left and right character.
    pub fn kerning(&self) -> &[KernPair] {
        &self.kerning
    }

    /// Pixel adjustment before `right` when it follows `left`.
    pub fn kern(&self, left: char, right: char) -> i32 {
        self.kerning
            .binary_search_by_key(&(left, right), |k| (k.left, k.right))
            .ok()
            .map_or(0, |i| self.kerning[i].adjustment)
    }

    /// The size of `text` drawn by [`Font::render`], in pixels.
    pub fn measure(&self, text: &str) -> (u32, u32) {
        let mut width = 0u64;
        let mut lines = 0u64;
        for line in text.split('\n') {
            lines += 1;
            let mut previous = None;
            let mut w = 0u64;
            let mut right = 0u64;
            for c in line.chars() {
                if let Some(g) = self.glyph(c).or_else(|| self.glyph('?')) {
                    let adjustment = previous.map_or(0, |p| self.kern(p, g.ch));
                    w = (w as i64 + adjustment as i64).max(0) as u64;
                    right = right.max(w + g.width as u64);
                    w += g.advance as u64;
                    right = right.max(w);
                    previous = Some(g.ch);
                }
            }
            width = width.max(right);
        }
        let height = (lines - 1) * self.line_height as u64 + self.height as u64;
        (
            width.min(u32::MAX as u64) as u32,
            height.min(u32::MAX as u64) as u32,
        )
    }

    /// Draws `text` in `color` on a transparent image, within the default [`Limits`].
    /// `\n` starts a new line; characters without a glyph are drawn as `?`, or left
    /// out when the font has no `?`.
    pub fn render(&self, text: &str, color: Color) -> Result<Image> {
        self.render_with(text, color, &Limits::default())
    }

    /// [`Font::render`] within `limits`.
    pub fn render_with(&self, text: &str, color: Color, limits: &Limits) -> Result<Image> {
        let (w, h) = self.measure(text);
        let mut image = Image::new_with(w.max(1), h.max(1), limits)?;
        for (row, line) in text.split('\n').enumerate() {
            let top = row as u32 * self.line_height;
            let mut pen = 0u32;
            let mut previous = None;
            for c in line.chars() {
                let Some(glyph) = self.glyph(c).or_else(|| self.glyph('?')) else {
                    continue;
                };
                let adjustment = previous.map_or(0, |p| self.kern(p, glyph.ch));
                pen = (pen as i64 + adjustment as i64).max(0) as u32;
                for y in 0..self.height {
                    for x in 0..glyph.width {
                        if glyph.pixel(x, y) {
                            image.set_pixel(pen + x, top + y, color);
                        }
                    }
                }
                pen += glyph.advance;
                previous = Some(glyph.ch);
            }
        }
        Ok(image)
    }

    /// The font as a TrueType file. Each glyph's outline is its pixels as squares,
    /// merged where they touch. See [`TtfOptions`].
    pub fn to_ttf(&self, options: &TtfOptions) -> Result<Vec<u8>> {
        crate::ttf::write(self, options)
    }

    /// The outline of `ch`'s glyph, in pixels: closed contours of corner points, `x`
    /// right and `y` up from the baseline. Outer contours run clockwise and holes
    /// counter-clockwise, so ink is always on the right; pixels that touch only at a
    /// corner make separate contours. TrueType export scales these by
    /// [`Font::units_per_pixel`].
    pub fn outline(&self, ch: char) -> Option<Vec<Vec<(i32, i32)>>> {
        self.glyph(ch)
            .map(|g| outline(g, self.height, self.baseline))
    }

    /// Font units per pixel in TrueType export: 128, or fewer when the font is taller
    /// than 128 pixels or has glyphs wider than 250, so that the em, which is the glyph
    /// height, stays within 16384 units and every coordinate within 32000.
    pub fn units_per_pixel(&self) -> u32 {
        let widest = self
            .glyphs
            .iter()
            .map(|g| g.width.max(g.advance))
            .max()
            .unwrap_or(1)
            .max(1);
        (16384 / self.height.max(1))
            .min(32000 / widest)
            .clamp(1, 128)
    }
}

/// How a font is written as TrueType.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtfOptions {
    /// The family name in the `name` table; the font's own name when `None`.
    pub family: Option<String>,
    /// The version in the `name` and `head` tables, as `major.minor`. Default 1.0.
    pub version: (u16, u16),
}

impl Default for TtfOptions {
    fn default() -> Self {
        Self {
            family: None,
            version: (1, 0),
        }
    }
}

/// The outline of a glyph's pixels (see [`Font::outline`]): closed contours of points in pixel units, `x`
/// right and `y` up from the baseline. Outer contours run clockwise and holes
/// counter-clockwise, so the inked side is always on the right; corners only, no
/// points in the middle of straight edges. Pixels that touch only at a corner make
/// separate contours.
pub(crate) fn outline(glyph: &Glyph, font_height: u32, baseline: u32) -> Vec<Vec<(i32, i32)>> {
    let filled = |x: i32, row: i32| -> bool {
        x >= 0 && row >= 0 && (row as u32) < font_height && glyph.pixel(x as u32, row as u32)
    };
    // y of a row's bottom edge; rows grow downward, y grows upward.
    let bottom = |row: i32| baseline as i32 - row - 1;
    // Directed edges, inked side on the right, keyed by their start.
    let mut edges: BTreeMap<(i32, i32), Vec<(i32, i32)>> = BTreeMap::new();
    let mut add = |from: (i32, i32), to: (i32, i32)| edges.entry(from).or_default().push(to);
    for row in 0..font_height as i32 {
        for x in 0..glyph.width as i32 {
            if !filled(x, row) {
                continue;
            }
            let (y0, y1) = (bottom(row), bottom(row) + 1);
            if !filled(x, row - 1) {
                add((x, y1), (x + 1, y1));
            }
            if !filled(x + 1, row) {
                add((x + 1, y1), (x + 1, y0));
            }
            if !filled(x, row + 1) {
                add((x + 1, y0), (x, y0));
            }
            if !filled(x - 1, row) {
                add((x, y0), (x, y1));
            }
        }
    }
    let mut contours = Vec::new();
    // Start each contour at its highest, then leftmost, unused point.
    while let Some(start) = edges
        .iter()
        .filter(|(_, ends)| !ends.is_empty())
        .map(|(&p, _)| p)
        .min_by_key(|&(x, y)| (-y, x))
    {
        let mut points = vec![start];
        let mut at = start;
        let mut direction = (0, 0);
        loop {
            let ends = edges.get_mut(&at).filter(|e| !e.is_empty());
            let Some(ends) = ends else { break };
            // At a corner where two edges leave, turn right: pixels that touch only
            // at that corner stay in separate contours.
            let pick = |to: &(i32, i32)| {
                let d = (to.0 - at.0, to.1 - at.1);
                let right = (direction.1, -direction.0);
                if d == right {
                    0
                } else if d == direction {
                    1
                } else {
                    2
                }
            };
            let index = (0..ends.len()).min_by_key(|&i| pick(&ends[i])).unwrap_or(0);
            let to = ends.swap_remove(index);
            direction = (to.0 - at.0, to.1 - at.1);
            at = to;
            if at == start {
                break;
            }
            points.push(at);
        }
        contours.push(simplify(points));
    }
    contours
}

/// Removes points in the middle of straight runs of a closed contour.
fn simplify(points: Vec<(i32, i32)>) -> Vec<(i32, i32)> {
    let n = points.len();
    (0..n)
        .filter(|&i| {
            let (a, b, c) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
            (b.0 - a.0) * (c.1 - b.1) != (b.1 - a.1) * (c.0 - b.0)
        })
        .map(|i| points[i])
        .collect()
}

impl Font {
    /// Checks the font's invariants; the parser guarantees them for parsed fonts.
    pub(crate) fn validate(&self) -> Result<()> {
        if self.height == 0 || self.baseline > self.height || self.line_height == 0 {
            return Err(Error::invalid(
                "A font needs a height, a baseline and a line height",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(rows: &[&str]) -> Glyph {
        Glyph {
            ch: 'x',
            width: rows[0].len() as u32,
            advance: rows[0].len() as u32 + 1,
            pixels: rows
                .iter()
                .flat_map(|r| r.chars().map(|c| c == '#'))
                .collect(),
        }
    }

    #[test]
    fn outlines() {
        // A square: one clockwise contour from the top left.
        let square = glyph(&["##", "##"]);
        assert_eq!(
            outline(&square, 2, 2),
            vec![vec![(0, 2), (2, 2), (2, 0), (0, 0)]]
        );
        // A ring: an outer contour and a counter-clockwise hole.
        let ring = glyph(&["###", "#.#", "###"]);
        let c = outline(&ring, 3, 3);
        assert_eq!(c[0], [(0, 3), (3, 3), (3, 0), (0, 0)]);
        assert_eq!(c[1], [(1, 2), (1, 1), (2, 1), (2, 2)]);
        // Diagonal neighbours stay separate.
        let diagonal = glyph(&["#.", ".#"]);
        let c = outline(&diagonal, 2, 1);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0], [(0, 1), (1, 1), (1, 0), (0, 0)]);
        assert_eq!(c[1], [(1, 0), (2, 0), (2, -1), (1, -1)]);
        // An L, with descent below the baseline.
        let l = glyph(&["#.", "##"]);
        assert_eq!(
            outline(&l, 2, 1),
            vec![vec![(0, 1), (1, 1), (1, 0), (2, 0), (2, -1), (0, -1)]]
        );
        assert!(outline(&glyph(&[".."]), 1, 1).is_empty());
    }
}
