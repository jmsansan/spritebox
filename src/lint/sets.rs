//! Sets: sprites that belong together (an animation's frames, a group, a tileset's
//! tiles) and should match in size, outline, palette and pixel scale.

use super::lines::Canvas;
use super::{Detail, Found, Index, Kind, LintCode, Sink, shading};
use crate::{Color, Document, Position, Sprite};

/// Runs the set checks on every set of the document.
pub(super) fn check(document: &Document, index: &Index, sink: &mut Sink) {
    for set in &document.sources.sets {
        // Each sprite once, at the first place that names it.
        let mut members: Vec<(&Sprite, Position)> = Vec::new();
        for (name, pos) in &set.members {
            if members.iter().any(|(s, _)| s.name == *name) {
                continue;
            }
            let sprite = match set.kind {
                Kind::Tileset => document
                    .tileset(&set.name)
                    .and_then(|t| t.tiles.iter().find(|s| s.name == *name)),
                _ => document.sprite(name),
            };
            if let Some(sprite) = sprite {
                members.push((sprite, *pos));
            }
        }
        if members.len() < 2 {
            continue;
        }
        let what = match set.kind {
            Kind::Animation => "animation",
            Kind::Tileset => "tileset",
            _ => "group",
        };
        let report = |sink: &mut Sink, code, i: usize, message: String| {
            let (sprite, pos) = members[i];
            let found = Found {
                code,
                kind: set.kind,
                name: &set.name,
                position: Some(pos),
                sprite: Some(&sprite.name),
                pixels: Vec::new(),
            };
            sink.push(found, || message);
        };
        let set_name = &set.name;
        let canvases: Vec<Canvas> = members.iter().map(|(s, _)| Canvas::new(s)).collect();

        // `inconsistent_size`: tiles are one size by definition.
        if set.kind != Kind::Tileset {
            let sizes: Vec<Option<(u32, u32)>> = members
                .iter()
                .map(|(s, _)| Some((s.width, s.height)))
                .collect();
            let (usual, odd) = minority(&sizes);
            for i in odd {
                let (s, (w, h)) = (members[i].0, (members[i].0.width, members[i].0.height));
                let (uw, uh) = usual.unwrap_or_default();
                report(
                    sink,
                    LintCode::InconsistentSize,
                    i,
                    format!(
                        "Sprite `{}` is {w}x{h}, unlike the {uw}x{uh} of most of {what} \
                         `{set_name}`",
                        s.name
                    ),
                );
            }
        }

        // `inconsistent_outline`: among sprites with a silhouette, an outline in all or
        // none; and among those drawn with one palette, the same color. A palette swap,
        // as for a flash, recolors an outline on purpose.
        let outlines: Vec<Option<Option<Color>>> = canvases
            .iter()
            .map(|c| c.has_silhouette().then(|| c.outline_color()))
            .collect();
        let presence: Vec<Option<bool>> = outlines.iter().map(|o| o.map(|c| c.is_some())).collect();
        let (usual, odd) = minority(&presence);
        for &i in &odd {
            let name = &members[i].0.name;
            let message = if usual == Some(true) {
                format!("Sprite `{name}` has no outline, unlike most of {what} `{set_name}`")
            } else {
                format!("Sprite `{name}` has an outline, unlike most of {what} `{set_name}`")
            };
            report(sink, LintCode::InconsistentOutline, i, message);
        }
        let mut palettes: Vec<&str> = members
            .iter()
            .map(|(s, _)| s.palette.name.as_str())
            .collect();
        palettes.dedup();
        palettes.sort_unstable();
        palettes.dedup();
        for palette in palettes {
            let colors: Vec<Option<Color>> = outlines
                .iter()
                .enumerate()
                .map(|(i, o)| {
                    let same = members[i].0.palette.name == palette && !odd.contains(&i);
                    o.flatten().filter(|_| same)
                })
                .collect();
            let (usual, recolored) = minority(&colors);
            for i in recolored {
                let (name, this) = (&members[i].0.name, colors[i].unwrap_or_default());
                let usual = usual.unwrap_or_default();
                report(
                    sink,
                    LintCode::InconsistentOutline,
                    i,
                    format!(
                        "Sprite `{name}` is outlined in {}, unlike most of {what} `{set_name}` \
                         drawn with palette `{palette}`, outlined in {}",
                        hex(this),
                        hex(usual)
                    ),
                );
            }
        }

        // `inconsistent_palette`: palettes that are not copies of one palette. A
        // tileset's tiles share its palette.
        if set.kind != Kind::Tileset {
            let roots: Vec<Option<String>> = members
                .iter()
                .map(|(s, _)| Some(root(index, &s.palette.name)))
                .collect();
            let (usual, odd) = minority(&roots);
            for i in odd {
                let sprite = members[i].0;
                let usual = usual.clone().unwrap_or_default();
                report(
                    sink,
                    LintCode::InconsistentPalette,
                    i,
                    format!(
                        "Sprite `{}` uses palette `{}`, which is not a copy of `{usual}`, \
                         used by most of {what} `{set_name}`",
                        sprite.name, sprite.palette.name
                    ),
                );
            }
        }

        // `inconsistent_scale`: the pixel scale of `mixel`, among sprites that show
        // one.
        let scales: Vec<Option<i64>> = canvases.iter().map(shading::scale).collect();
        let (usual, odd) = minority(&scales);
        for i in odd {
            let name = &members[i].0.name;
            let (this, usual) = (scales[i].unwrap_or(1), usual.unwrap_or(1));
            report(
                sink,
                LintCode::InconsistentScale,
                i,
                format!(
                    "Sprite `{name}` looks drawn at {this}x, unlike most of {what} \
                     `{set_name}`, drawn at {usual}x"
                ),
            );
        }
    }
}

/// The most common of `values` (ignoring `None`), the earliest on a tie, and the
/// indices of the values that differ from it.
fn minority<T: PartialEq + Clone>(values: &[Option<T>]) -> (Option<T>, Vec<usize>) {
    let mut counts: Vec<(T, usize)> = Vec::new();
    for v in values.iter().flatten() {
        match counts.iter_mut().find(|(c, _)| c == v) {
            Some((_, n)) => *n += 1,
            None => counts.push((v.clone(), 1)),
        }
    }
    let Some(best) = counts.iter().map(|&(_, n)| n).max() else {
        return (None, Vec::new());
    };
    let usual = counts.into_iter().find(|&(_, n)| n == best).map(|(v, _)| v);
    let odd = values
        .iter()
        .enumerate()
        .filter(|(_, v)| v.is_some() && **v != usual)
        .map(|(i, _)| i)
        .collect();
    (usual, odd)
}

/// The palette that `palette` copies at the end of its `from=` chain.
fn root(index: &Index, palette: &str) -> String {
    let mut name = palette;
    for _ in 0..=index.len() {
        let detail = index.get(&(Kind::Palette, name)).map(|d| &d.detail);
        match detail {
            Some(Detail::Palette {
                from: Some(from), ..
            }) => name = from,
            _ => break,
        }
    }
    name.to_owned()
}

/// A color as `#rrggbb`, or `#rrggbbaa` when not opaque.
fn hex(c: Color) -> String {
    if c.a == 255 {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    } else {
        format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)
    }
}
