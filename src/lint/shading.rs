//! Shading and scale: pillow shading, banding, mixed pixel sizes and too many colors,
//! in the sprites drawn with rows.

use std::collections::VecDeque;

use super::lines::{AROUND, Canvas, Report};
use super::{LintCode, LintOptions, Sink};
use crate::Color;
use crate::oklab::Lab;

/// Runs the shading checks on a sprite drawn with rows.
pub(super) fn check(canvas: &Canvas, report: &Report, options: &LintOptions, sink: &mut Sink) {
    too_many_colors(canvas, report, options, sink);
    pillow_shading(canvas, report, options, sink);
    banding(canvas, report, options, sink);
}

/// The lightness of a color, 0 to 10000.
fn lightness(c: Color) -> i64 {
    Lab::from_color(c).l as i64
}

/// `too_many_colors`: more visible colors than `max_colors_small` in a sprite under
/// 32×32, or than `max_colors` at any size.
fn too_many_colors(canvas: &Canvas, report: &Report, options: &LintOptions, sink: &mut Sink) {
    let mut colors: Vec<Color> = (0..canvas.h)
        .flat_map(|y| (0..canvas.w).map(move |x| (x, y)))
        .map(|(x, y)| canvas.at(x, y))
        .filter(|c| c.a > 0)
        .collect();
    colors.sort_by_key(|c| c.to_array());
    colors.dedup();
    let small = canvas.w < 32 && canvas.h < 32;
    let limit = if small {
        options.max_colors_small
    } else {
        options.max_colors
    };
    if colors.len() <= limit {
        return;
    }
    let name = &report.sprite.name;
    let count = colors.len();
    report.push(sink, LintCode::TooManyColors, Vec::new(), || {
        let size = if small {
            "a sprite under 32x32"
        } else {
            "a sprite"
        };
        format!(
            "Sprite `{name}` shows {count} colors, more than {limit} for {size}; fewer \
             colors read better at small sizes"
        )
    });
}

/// `pillow_shading`: the fill (the pixels inside the outline) gets darker toward its
/// edge on every side, with no direction of light. The centroid of its lighter half
/// is within `pillow_offset` percent of the shorter side from the centroid of the
/// whole fill, and lightness rises with the distance from the edge. Only in sprites
/// of at least 8×8, with a silhouette, three colors in the fill, and not panels.
fn pillow_shading(canvas: &Canvas, report: &Report, options: &LintOptions, sink: &mut Sink) {
    let sprite = report.sprite;
    let (w, h) = (canvas.w, canvas.h);
    if w < 8 || h < 8 || sprite.slice.is_some() || !canvas.has_silhouette() {
        return;
    }
    let outline = canvas.outline_key(sprite);
    let fill = |x: i64, y: i64| {
        x >= 0
            && y >= 0
            && x < w
            && y < h
            && canvas.visible(x, y)
            && sprite.key(x as u32, y as u32) != outline
    };
    // Distance from the edge of the fill, in steps across or down.
    let index = |x: i64, y: i64| (y * w + x) as usize;
    let mut distance = vec![u32::MAX; (w * h) as usize];
    let mut queue = VecDeque::new();
    for y in 0..h {
        for x in 0..w {
            if !fill(x, y) {
                continue;
            }
            let edge = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .iter()
                .any(|&(dx, dy)| !fill(x + dx, y + dy));
            if edge {
                distance[index(x, y)] = 1;
                queue.push_back((x, y));
            }
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        let d = distance[index(x, y)];
        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
            let (nx, ny) = (x + dx, y + dy);
            if fill(nx, ny) && distance[index(nx, ny)] == u32::MAX {
                distance[index(nx, ny)] = d + 1;
                queue.push_back((nx, ny));
            }
        }
    }
    // Each fill pixel: its place, lightness and distance from the edge.
    let mut pixels: Vec<(i64, i64, i64, i64)> = Vec::new();
    let mut colors: Vec<Color> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if fill(x, y) {
                let c = canvas.at(x, y);
                if !colors.contains(&c) {
                    colors.push(c);
                }
                pixels.push((x, y, lightness(c), distance[index(x, y)] as i64));
            }
        }
    }
    if colors.len() < 3 || pixels.len() < 16 {
        return;
    }
    // Lightness must rise with the distance from the edge: a correlation of at least
    // one half.
    let n = pixels.len() as i128;
    let (mut sl, mut sd, mut sll, mut sdd, mut sld) = (0i128, 0i128, 0i128, 0i128, 0i128);
    for &(_, _, l, d) in &pixels {
        let (l, d) = (l as i128, d as i128);
        sl += l;
        sd += d;
        sll += l * l;
        sdd += d * d;
        sld += l * d;
    }
    let covariance = n * sld - sl * sd;
    let spread = (n * sll - sl * sl) * (n * sdd - sd * sd);
    if covariance <= 0 || 4 * covariance * covariance < spread {
        return;
    }
    // And the light must sit in the middle.
    let (sx, sy) = pixels.iter().fold((0i128, 0i128), |(a, b), p| {
        (a + p.0 as i128, b + p.1 as i128)
    });
    let mut lighter = pixels.clone();
    lighter.sort_by_key(|&(x, y, l, _)| (-l, y, x));
    lighter.truncate(pixels.len() / 2);
    let m = lighter.len() as i128;
    let (lx, ly) = lighter.iter().fold((0i128, 0i128), |(a, b), p| {
        (a + p.0 as i128, b + p.1 as i128)
    });
    // |lighter/m - all/n| ≤ offset% × side, multiplied through by 100·m·n.
    let (dx, dy) = (lx * n - sx * m, ly * n - sy * m);
    let side = w.min(h) as i128;
    let reach = options.pillow_offset as i128 * side * m * n;
    if (dx * dx + dy * dy) * 10000 > reach * reach {
        return;
    }
    report.push(sink, LintCode::PillowShading, Vec::new(), || {
        format!(
            "Sprite `{}` is pillow-shaded: it darkens toward its edges on every side, \
             with no direction of light",
            sprite.name
        )
    });
}

/// `banding`: a line one pixel thick, at least `banding_length` pixels long, of a
/// color between the colors on its two sides in lightness: shades stacked in
/// parallel lines rather than shaped. The outline is not a shade, on either side.
fn banding(canvas: &Canvas, report: &Report, options: &LintOptions, sink: &mut Sink) {
    let (w, h) = (canvas.w, canvas.h);
    let index = |x: i64, y: i64| (y * w + x) as usize;
    let outline = canvas.outline_key(report.sprite);
    let shade = |x: i64, y: i64| {
        let inside = x >= 0 && y >= 0 && x < w && y < h;
        let c = canvas.at(x, y);
        let outlined =
            inside && outline.is_some() && report.sprite.key(x as u32, y as u32) == outline;
        (c.a > 0 && !outlined).then_some(c)
    };
    // Whether a pixel is a step of a ramp across or down: between two other shades,
    // one lighter and one darker.
    let step = |x: i64, y: i64| -> bool {
        let Some(c) = shade(x, y) else {
            return false;
        };
        let l = lightness(c);
        [((-1, 0), (1, 0)), ((0, -1), (0, 1))]
            .iter()
            .any(|&((ax, ay), (bx, by))| {
                let (Some(a), Some(b)) = (shade(x + ax, y + ay), shade(x + bx, y + by)) else {
                    return false;
                };
                if a == c || b == c || a == b {
                    return false;
                }
                let (la, lb) = (lightness(a), lightness(b));
                (la < l && l < lb) || (la > l && l > lb)
            })
    };
    let mut steps = vec![false; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            steps[index(x, y)] = step(x, y);
        }
    }
    let mut seen = vec![false; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            if !steps[index(x, y)] || seen[index(x, y)] {
                continue;
            }
            // The line: steps of the same color, touching.
            let color = canvas.at(x, y);
            let mut line = Vec::new();
            let mut stack = vec![(x, y)];
            seen[index(x, y)] = true;
            while let Some((px, py)) = stack.pop() {
                line.push((px as u32, py as u32));
                for (dx, dy) in AROUND {
                    let (nx, ny) = (px + dx, py + dy);
                    if nx >= 0
                        && ny >= 0
                        && nx < w
                        && ny < h
                        && !seen[index(nx, ny)]
                        && steps[index(nx, ny)]
                        && canvas.at(nx, ny) == color
                    {
                        seen[index(nx, ny)] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            if line.len() < options.banding_length {
                continue;
            }
            line.sort_by_key(|&(x, y)| (y, x));
            let (fx, fy) = line[0];
            let length = line.len();
            let name = &report.sprite.name;
            report.push(sink, LintCode::Banding, line, || {
                format!(
                    "A band one pixel thick in sprite `{name}`, {length} pixels from ({fx}, \
                     {fy}), between a lighter and a darker shade: shading stacked in lines"
                )
            });
        }
    }
}

/// The pixel scale a sprite looks drawn at: the greatest common divisor of the
/// lengths of its runs of one color, across and down, when it is 1 to 4; `None` for
/// sprites with fewer than three colors, transparent included, or a larger divisor,
/// which say nothing of scale.
pub(super) fn scale(canvas: &Canvas) -> Option<i64> {
    let mut colors: Vec<Color> = Vec::new();
    let mut divisor = 0i64;
    let mut runs = |pixels: &mut dyn Iterator<Item = Color>| {
        let mut previous: Option<Color> = None;
        let mut length = 0i64;
        for c in pixels {
            if !colors.contains(&c) {
                colors.push(c);
            }
            if previous == Some(c) {
                length += 1;
                continue;
            }
            if previous.is_some() {
                divisor = gcd(divisor, length);
            }
            previous = Some(c);
            length = 1;
        }
        divisor = gcd(divisor, length);
    };
    for y in 0..canvas.h {
        runs(&mut (0..canvas.w).map(|x| canvas.at(x, y)));
    }
    for x in 0..canvas.w {
        runs(&mut (0..canvas.h).map(|y| canvas.at(x, y)));
    }
    (colors.len() >= 3 && (1..=4).contains(&divisor)).then_some(divisor)
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// `mixel`: a sprite drawn at 2×, 3× or 4× (every run of one color a multiple of
/// it) in a document whose other sprites include some drawn at 1×.
pub(super) fn mixels(scales: &[(Option<i64>, Report)], sink: &mut Sink) {
    if !scales.iter().any(|(s, _)| *s == Some(1)) {
        return;
    }
    for (scale, report) in scales {
        let Some(scale) = scale.filter(|&s| s > 1) else {
            continue;
        };
        let name = &report.sprite.name;
        report.push(sink, LintCode::Mixel, Vec::new(), || {
            format!(
                "Sprite `{name}` looks drawn at {scale}x: every run of one color is a \
                 multiple of {scale} pixels, while other sprites are drawn at 1x"
            )
        });
    }
}
