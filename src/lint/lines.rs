//! Line work: lone pixels, uneven steps, doubled outlines and stray alpha, in the
//! sprites drawn with rows.

use super::{Def, DrawnLayer, Found, Kind, LintCode, LintOptions, Sink, pixel_position};
use crate::{Color, Image, Sprite};

/// A sprite's rendered pixels, read with fully transparent pixels as one color and
/// beyond the edges as transparent.
struct Canvas {
    image: Image,
    w: i64,
    h: i64,
}

impl Canvas {
    fn at(&self, x: i64, y: i64) -> Color {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return Color::TRANSPARENT;
        }
        let c = self
            .image
            .pixel(x as u32, y as u32)
            .unwrap_or(Color::TRANSPARENT);
        if c.a == 0 { Color::TRANSPARENT } else { c }
    }
    fn visible(&self, x: i64, y: i64) -> bool {
        self.at(x, y).a > 0
    }
}

/// The 8 neighbours of a pixel, as offsets.
const AROUND: [(i64, i64); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// Runs the line-work checks on a sprite drawn with rows, of at least 3×3.
pub(super) fn check(
    sprite: &Sprite,
    def: &Def,
    layers: &[DrawnLayer],
    options: &LintOptions,
    sink: &mut Sink,
) {
    if sprite.width < 3 || sprite.height < 3 {
        return;
    }
    let canvas = Canvas {
        image: sprite.render(),
        w: sprite.width as i64,
        h: sprite.height as i64,
    };
    let report = Report {
        sprite,
        def,
        layers,
    };
    orphan_pixels(&canvas, &report, options, sink);
    jaggies(&canvas, &report, sink);
    double_outlines(&canvas, &report, sink);
    stray_alpha(&canvas, &report, sink);
}

/// What a warning about a sprite's pixels needs.
struct Report<'r> {
    sprite: &'r Sprite,
    def: &'r Def,
    layers: &'r [DrawnLayer],
}

impl Report<'_> {
    /// Reports `pixels`, at the character of the first.
    fn push(
        &self,
        sink: &mut Sink,
        code: LintCode,
        pixels: Vec<(u32, u32)>,
        message: impl FnOnce() -> String,
    ) {
        let (x, y) = pixels[0];
        let position = pixel_position(self.sprite, self.layers, x, y).unwrap_or(self.def.pos);
        let found = Found {
            code,
            kind: Kind::Sprite,
            name: &self.sprite.name,
            position: Some(position),
            sprite: Some(&self.sprite.name),
            pixels,
        };
        sink.push(found, message);
    }
}

/// Lightness, 0..=255, by the luma of Rec. 601.
fn luma(c: Color) -> i32 {
    (299 * c.r as i32 + 587 * c.g as i32 + 114 * c.b as i32) / 1000
}

/// `orphan_pixel`: a visible pixel whose four neighbours share one other color, close
/// to its own in lightness, with none of its color touching it diagonally. A visible
/// pixel with nothing visible around it at all is reported too.
fn orphan_pixels(canvas: &Canvas, report: &Report, options: &LintOptions, sink: &mut Sink) {
    let name = &report.sprite.name;
    for y in 0..canvas.h {
        for x in 0..canvas.w {
            let c = canvas.at(x, y);
            if c.a == 0 {
                continue;
            }
            let at = |(dx, dy): (i64, i64)| canvas.at(x + dx, y + dy);
            let sides = [(-1, 0), (1, 0), (0, -1), (0, 1)].map(at);
            let corners = [(-1, -1), (1, -1), (-1, 1), (1, 1)].map(at);
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
            report.push(sink, LintCode::OrphanPixel, vec![(x, y)], || {
                if isolated {
                    format!("A lone pixel in sprite `{name}` at ({x}, {y}), touching nothing")
                } else {
                    format!(
                        "A lone pixel in sprite `{name}` at ({x}, {y}), close in lightness to \
                         the color around it, reads as noise"
                    )
                }
            });
        }
    }
}

/// A run of a profile: `len` places from `start` at one `value`.
#[derive(Clone, Copy)]
struct Run {
    start: i64,
    len: i64,
    value: i64,
}

/// `jaggy`: along each side of the silhouette, a stretch of steps one pixel apart
/// where one step is at least two longer, or two shorter, than the steps on both sides
/// of it: `1, 1, 3, 1, 1` rather than `1, 1, 1, 1` or a curve's `1, 2, 3`. The first
/// and last steps of a stretch, which corners cut short, are not judged.
fn jaggies(canvas: &Canvas, report: &Report, sink: &mut Sink) {
    let name = &report.sprite.name;
    let mut reported: std::collections::HashSet<Vec<(u32, u32)>> = Default::default();
    // Each side: the first visible pixel of each column from the top or the bottom, or
    // of each row from the left or the right.
    for side in 0..4 {
        let (count, depth) = if side < 2 {
            (canvas.w, canvas.h)
        } else {
            (canvas.h, canvas.w)
        };
        let edge = |i: i64| -> Option<i64> {
            let mut steps = 0..depth;
            let mut find = |d: i64| {
                let d = if side % 2 == 0 { d } else { depth - 1 - d };
                let (x, y) = if side < 2 { (i, d) } else { (d, i) };
                canvas.visible(x, y).then_some(d)
            };
            steps.find_map(&mut find)
        };
        // Pixel `(x, y)` of a run's place `i`.
        let pixel = |i: i64, value: i64| -> (u32, u32) {
            if side < 2 {
                (i as u32, value as u32)
            } else {
                (value as u32, i as u32)
            }
        };
        let mut runs: Vec<Run> = Vec::new();
        for i in 0..count {
            let value = edge(i);
            match (value, runs.last_mut()) {
                (Some(v), Some(last)) if last.value == v && last.start + last.len == i => {
                    last.len += 1;
                }
                (Some(v), _) => runs.push(Run {
                    start: i,
                    len: 1,
                    value: v,
                }),
                (None, _) => {}
            }
        }
        // Stretches of runs that each step one pixel the same way.
        let mut first = 0;
        while first < runs.len() {
            let mut last = first;
            let mut direction = 0;
            let mut turns = false;
            while last + 1 < runs.len() {
                let (a, b) = (runs[last], runs[last + 1]);
                let step = b.value - a.value;
                let touching = a.start + a.len == b.start;
                if !touching || step.abs() != 1 {
                    break;
                }
                if direction != 0 && step != direction {
                    turns = true;
                    break;
                }
                direction = step;
                last += 1;
            }
            let stretch = &runs[first..=last];
            // Where the edge turns, the run at the turn ends one stretch and starts the
            // next.
            first = if turns { last } else { last + 1 };
            if stretch.len() < 5 {
                continue;
            }
            let inner = &stretch[1..stretch.len() - 1];
            let odd: Vec<Run> = inner
                .windows(3)
                .filter(|w| {
                    let (before, step, after) = (w[0].len, w[1].len, w[2].len);
                    (step >= before + 2 && step >= after + 2)
                        || (step + 2 <= before && step + 2 <= after)
                })
                .map(|w| w[1])
                .collect();
            if odd.is_empty() {
                continue;
            }
            let pixels: Vec<(u32, u32)> = odd
                .iter()
                .flat_map(|r| (r.start..r.start + r.len).map(move |i| pixel(i, r.value)))
                .collect();
            // A stroke one pixel thin shows the same steps from both sides.
            if !reported.insert(pixels.clone()) {
                continue;
            }
            let steps: Vec<String> = inner.iter().map(|r| r.len.to_string()).collect();
            let (x, y) = pixels[0];
            report.push(sink, LintCode::Jaggy, pixels, || {
                format!(
                    "Uneven steps along the edge of sprite `{name}`: {} pixels long; the step \
                     at ({x}, {y}) breaks the line's rhythm",
                    steps.join(", ")
                )
            });
        }
    }
}

/// `double_outline`: a pixel of the outline that makes an L corner with two others
/// where one diagonal step would do, so that the line is two pixels thick there. The
/// outline is the key of at least half the silhouette's pixels, where those pixels
/// connect. Corners within 2×2 squares of the outline, as in lines two pixels thick
/// across, are left alone, and so are corners with no fill near them: a stroke drawn
/// on its own outlines nothing.
fn double_outlines(canvas: &Canvas, report: &Report, sink: &mut Sink) {
    let sprite = report.sprite;
    let (w, h) = (canvas.w, canvas.h);
    let inside = |x: i64, y: i64| x >= 0 && y >= 0 && x < w && y < h;
    let index = |x: i64, y: i64| (y * w + x) as usize;
    // A sprite with no transparent pixel, such as a tile, has no silhouette.
    if (0..h).all(|y| (0..w).all(|x| canvas.visible(x, y))) {
        return;
    }
    let edge = |x: i64, y: i64| {
        canvas.visible(x, y)
            && [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .iter()
                .any(|&(dx, dy)| !canvas.visible(x + dx, y + dy))
    };
    let key = |x: i64, y: i64| sprite.key(x as u32, y as u32);
    let mut counts: Vec<(char, usize)> = Vec::new();
    let mut total = 0;
    for y in 0..h {
        for x in 0..w {
            if !edge(x, y) {
                continue;
            }
            total += 1;
            let Some(k) = key(x, y) else { continue };
            match counts.iter_mut().find(|(c, _)| *c == k) {
                Some((_, n)) => *n += 1,
                None => counts.push((k, 1)),
            }
        }
    }
    let Some(&(outline, n)) = counts
        .iter()
        .max_by_key(|&&(k, n)| (n, std::cmp::Reverse(k)))
    else {
        return;
    };
    if n * 2 < total {
        return;
    }
    // The outline's pixels: those of its key that connect to its pixels on the edge.
    let mut mask = vec![false; (w * h) as usize];
    let mut stack: Vec<(i64, i64)> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| edge(x, y) && key(x, y) == Some(outline))
        .collect();
    while let Some((x, y)) = stack.pop() {
        if mask[index(x, y)] {
            continue;
        }
        mask[index(x, y)] = true;
        for (dx, dy) in AROUND {
            let (nx, ny) = (x + dx, y + dy);
            if inside(nx, ny)
                && !mask[index(nx, ny)]
                && canvas.visible(nx, ny)
                && key(nx, ny) == Some(outline)
            {
                stack.push((nx, ny));
            }
        }
    }
    let on = |mask: &[bool], x: i64, y: i64| inside(x, y) && mask[index(x, y)];
    for y in 0..h {
        for x in 0..w {
            if !on(&mask, x, y) {
                continue;
            }
            let neighbours: Vec<(i64, i64)> = AROUND
                .iter()
                .map(|&(dx, dy)| (x + dx, y + dy))
                .filter(|&(nx, ny)| on(&mask, nx, ny))
                .collect();
            let across = [(-1, 0), (1, 0)]
                .iter()
                .filter(|&&(dx, dy)| on(&mask, x + dx, y + dy))
                .count();
            let down = [(0, -1), (0, 1)]
                .iter()
                .filter(|&&(dx, dy)| on(&mask, x + dx, y + dy))
                .count();
            // An L: one neighbour across and one up or down, and more besides.
            if neighbours.len() < 3 || across != 1 || down != 1 {
                continue;
            }
            // A line thick on purpose fills 2×2 squares.
            let square = [(-1, -1), (0, -1), (-1, 0), (0, 0)]
                .iter()
                .any(|&(sx, sy)| {
                    [(0, 0), (1, 0), (0, 1), (1, 1)]
                        .iter()
                        .all(|&(dx, dy)| on(&mask, x + sx + dx, y + sy + dy))
                });
            // The fill that the outline goes around, within two pixels.
            let fill = (-2..=2).any(|dy: i64| {
                (-2..=2).any(|dx: i64| {
                    let (nx, ny) = (x + dx, y + dy);
                    canvas.visible(nx, ny) && key(nx, ny) != Some(outline)
                })
            });
            if square || !fill || !connected(&neighbours) {
                continue;
            }
            // Taken out, so the rest of the line is judged without it.
            mask[index(x, y)] = false;
            let (x, y) = (x as u32, y as u32);
            report.push(sink, LintCode::DoubleOutline, vec![(x, y)], || {
                format!(
                    "The outline of sprite `{}` is two pixels thick at ({x}, {y}); one \
                     diagonal step would do",
                    sprite.name
                )
            });
        }
    }
}

/// Whether `pixels` are all connected to each other, through each other alone.
fn connected(pixels: &[(i64, i64)]) -> bool {
    let mut reached = vec![false; pixels.len()];
    let mut stack = vec![0];
    while let Some(i) = stack.pop() {
        if std::mem::replace(&mut reached[i], true) {
            continue;
        }
        let (x, y) = pixels[i];
        for (j, &(nx, ny)) in pixels.iter().enumerate() {
            if !reached[j] && (nx - x).abs() <= 1 && (ny - y).abs() <= 1 {
                stack.push(j);
            }
        }
    }
    reached.iter().all(|&r| r)
}

/// `stray_alpha`: a translucent pixel with no translucent pixel around it, in a
/// sprite whose translucent pixels are fewer than one in eight visible ones: a
/// leftover of anti-aliasing, which blurs when the sprite is scaled. Translucent areas,
/// such as glass or a shadow, are left alone.
fn stray_alpha(canvas: &Canvas, report: &Report, sink: &mut Sink) {
    let translucent = |x: i64, y: i64| {
        let a = canvas.at(x, y).a;
        a > 0 && a < 255
    };
    let (mut visible, mut partial) = (0usize, 0usize);
    for y in 0..canvas.h {
        for x in 0..canvas.w {
            visible += canvas.visible(x, y) as usize;
            partial += translucent(x, y) as usize;
        }
    }
    if partial == 0 || partial * 8 >= visible {
        return;
    }
    let name = &report.sprite.name;
    for y in 0..canvas.h {
        for x in 0..canvas.w {
            if !translucent(x, y) || AROUND.iter().any(|&(dx, dy)| translucent(x + dx, y + dy)) {
                continue;
            }
            let alpha = canvas.at(x, y).a;
            let (x, y) = (x as u32, y as u32);
            report.push(sink, LintCode::StrayAlpha, vec![(x, y)], || {
                format!(
                    "A lone translucent pixel in sprite `{name}` at ({x}, {y}), alpha {alpha}: \
                     a leftover of smoothing that blurs when scaled"
                )
            });
        }
    }
}
