//! Palettes: unused keys, colors too close, and ramps that are flat or too close in
//! lightness.

use std::collections::HashSet;

use super::{Detail, Found, Index, Kind, LintCode, LintOptions, Sink};
use crate::Document;
use crate::oklab::Lab;

/// Runs the palette checks on each palette, as it is written.
pub(super) fn check(document: &Document, index: &Index, options: &LintOptions, sink: &mut Sink) {
    let used = used_keys(document, index);
    for def in &document.sources.defs {
        let Detail::Palette { keys, ramps, .. } = &def.detail else {
            continue;
        };
        // A palette defined twice is an error; only the first is in the document.
        if !index
            .get(&(Kind::Palette, def.name.as_str()))
            .is_some_and(|first| std::ptr::eq(*first, def))
        {
            continue;
        }
        let Some(palette) = document.palette(&def.name) else {
            continue;
        };
        let name = def.name.as_str();
        let found = |code, position| Found {
            code,
            kind: Kind::Palette,
            name,
            position: Some(position),
            sprite: None,
            pixels: Vec::new(),
        };
        let used_here = |key: char| used.contains(&(name, key));
        // `unused_key`.
        for &(key, pos) in keys {
            if !used_here(key) {
                sink.push(found(LintCode::UnusedKey, pos), || {
                    format!("Nothing uses key `{key}` of palette `{name}`")
                });
            }
        }
        // `near_duplicate_color`: visible colors of used keys, alike in alpha.
        let visible: Vec<(char, crate::Position, crate::Color)> = keys
            .iter()
            .filter(|&&(k, _)| used_here(k))
            .filter_map(|&(k, pos)| palette.color(k).map(|c| (k, pos, c)))
            .filter(|&(_, _, c)| c.a > 0)
            .collect();
        for (j, &(second, pos, b)) in visible.iter().enumerate() {
            let close = visible[..j].iter().find(|&&(_, _, a)| {
                a.a.abs_diff(b.a) <= 32
                    && Lab::from_color(a).distance(Lab::from_color(b))
                        < options.min_color_distance as i64
            });
            if let Some(&(first, _, a)) = close {
                let d = Lab::from_color(a).distance(Lab::from_color(b));
                sink.push(found(LintCode::NearDuplicateColor, pos), || {
                    format!(
                        "Keys `{first}` and `{second}` of palette `{name}` are almost the same \
                         color, {} apart in OKLab; one would do",
                        decimal(d)
                    )
                });
            }
        }
        for (ramp, pos, keys) in ramps {
            let labs: Vec<Lab> = keys
                .iter()
                .filter_map(|&(k, _)| palette.color(k))
                .map(Lab::from_color)
                .collect();
            if labs.len() != keys.len() || labs.len() < 2 {
                continue;
            }
            // `flat_ramp`: the hue at both ends within 5°, unless an end is grey.
            let (first, last) = (labs[0], labs[labs.len() - 1]);
            if first.chroma() >= 200 && last.chroma() >= 200 {
                let (a1, b1, a2, b2) = (
                    first.a as i128,
                    first.b as i128,
                    last.a as i128,
                    last.b as i128,
                );
                let dot = a1 * a2 + b1 * b2;
                let lengths = (a1 * a1 + b1 * b1) * (a2 * a2 + b2 * b2);
                // cos² 5° = 0.992404.
                if dot > 0 && dot * dot * 1_000_000 >= 992_404 * lengths {
                    sink.push(found(LintCode::FlatRamp, *pos), || {
                        format!(
                            "Ramp `{ramp}` of palette `{name}` keeps one hue from end to end; \
                             turn its highlights toward yellow and its shadows toward blue"
                        )
                    });
                }
            }
            // `low_contrast`: neighbours too close in lightness.
            for (i, pair) in labs.windows(2).enumerate() {
                let step = (pair[1].l - pair[0].l).abs() as i64;
                if step >= options.min_ramp_step as i64 {
                    continue;
                }
                let ((a, _), (b, at)) = (keys[i], keys[i + 1]);
                sink.push(found(LintCode::LowContrast, at), || {
                    format!(
                        "Keys `{a}` and `{b}` of ramp `{ramp}` differ by {} in lightness, too \
                         little to tell apart",
                        decimal(step)
                    )
                });
            }
        }
    }
}

/// Ten-thousandths as a decimal, such as `0.0123`.
fn decimal(value: i64) -> String {
    format!("{}.{:04}", value / 10000, value % 10000)
}

/// The keys, as palettes write them, that a sprite, frame, tile, texture ramp or
/// step of a chain uses, through that palette or a palette copied from it.
fn used_keys<'d>(document: &'d Document, index: &Index<'d>) -> HashSet<(&'d str, char)> {
    let sources = &document.sources;
    let mut used: HashSet<(&str, char)> = HashSet::new();
    let mut mark = |palette: &str, key: char| {
        // The key's color comes from the nearest palette along `from=` that writes it.
        let mut name = palette;
        for _ in 0..=sources.defs.len() {
            let Some(def) = index.get(&(Kind::Palette, name)) else {
                return;
            };
            let Detail::Palette { from, keys, .. } = &def.detail else {
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
        for key in sprite.keys() {
            mark(&sprite.palette.name, key);
        }
    }
    for (palette, key) in &sources.palette_uses {
        mark(palette, *key);
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
    used
}
