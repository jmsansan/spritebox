//! Palettes: unused keys, colors too close, and ramps that are flat or too close in
//! lightness.

use std::collections::{HashMap, HashSet};

use super::{Detail, Found, Index, Kind, LintCode, LintOptions, RampSource, Sink};
use crate::oklab::Lab;
use crate::{Color, Document, Position};

/// Runs the palette checks: unused keys where each palette writes them, and colors
/// and ramps in each palette as it resolves, with what it copies.
pub(super) fn check(document: &Document, index: &Index, options: &LintOptions, sink: &mut Sink) {
    let used = used_keys(document, index);
    let found = |code, name, position| Found {
        code,
        kind: Kind::Palette,
        name,
        position: Some(position),
        sprite: None,
        pixels: Vec::new(),
    };
    // `unused_key`, where each palette writes the key.
    for def in &document.sources.defs {
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
        let name = def.name.as_str();
        for &(key, pos) in keys {
            if !used.contains(&(name, key)) {
                sink.push(found(LintCode::UnusedKey, name, pos), || {
                    format!("Nothing uses key `{key}` of palette `{name}`")
                });
            }
        }
    }
    // The rest in each palette with the colors it ends with, at the lines that write
    // the keys and ramps concerned. A problem a copy inherits unchanged, at the same
    // place with the same colors, is reported once: for the palette it comes from,
    // as palettes are checked copied ones first, and a copy's `lint=` allows what it
    // copies.
    let direct = direct_uses(document);
    let mut reported: HashSet<(LintCode, Position, [u8; 4], [u8; 4])> = HashSet::new();
    let mut palettes: Vec<&crate::Palette> = document.palettes.iter().collect();
    palettes.sort_by_key(|p| copies(index, &p.name));
    for palette in palettes {
        let name = palette.name.as_str();
        let uses = direct.get(name);
        // `near_duplicate_color`: visible colors of keys used with this palette,
        // alike in alpha.
        let visible: Vec<(char, Color)> = palette
            .colors
            .iter()
            .filter(|&&(k, c)| c.a > 0 && uses.is_some_and(|u| u.contains(&k)))
            .copied()
            .collect();
        for (j, &(second, b)) in visible.iter().enumerate() {
            let close = visible[..j].iter().find(|&&(_, a)| {
                a.a.abs_diff(b.a) <= 32
                    && Lab::from_color(a).distance(Lab::from_color(b))
                        < options.min_color_distance as i64
            });
            let Some(&(first, a)) = close else { continue };
            // At the later of the two lines that write them.
            let places = [owner(index, name, first), owner(index, name, second)];
            let Some((_, pos)) = places.into_iter().flatten().max_by_key(|p| p.1) else {
                continue;
            };
            // Recorded only when not allowed here, so that it is still reported for
            // a palette that does not allow it.
            if sink.allowed(LintCode::NearDuplicateColor, Kind::Palette, name)
                || !reported.insert((
                    LintCode::NearDuplicateColor,
                    pos,
                    a.to_array(),
                    b.to_array(),
                ))
            {
                continue;
            }
            let d = Lab::from_color(a).distance(Lab::from_color(b));
            sink.push(found(LintCode::NearDuplicateColor, name, pos), || {
                format!(
                    "Keys `{first}` and `{second}` of palette `{name}` are almost the same \
                     color, {} apart in OKLab; one would do",
                    decimal(d)
                )
            });
        }
        for ramp in &palette.ramps {
            let Some((_, (_, pos, keys))) = ramp_source(index, name, &ramp.name) else {
                continue;
            };
            let colors: Vec<Color> = ramp.keys.iter().filter_map(|&k| palette.color(k)).collect();
            let labs: Vec<Lab> = colors.iter().map(|&c| Lab::from_color(c)).collect();
            if labs.len() != keys.len() || labs.len() < 2 {
                continue;
            }
            let ramp = ramp.name.as_str();
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
                let ends = (colors[0].to_array(), colors[colors.len() - 1].to_array());
                if dot > 0
                    && dot * dot * 1_000_000 >= 992_404 * lengths
                    && !sink.allowed(LintCode::FlatRamp, Kind::Palette, name)
                    && reported.insert((LintCode::FlatRamp, *pos, ends.0, ends.1))
                {
                    sink.push(found(LintCode::FlatRamp, name, *pos), || {
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
                let ((a, _), (b, at)) = (keys[i], keys[i + 1]);
                let pair = (colors[i].to_array(), colors[i + 1].to_array());
                if step >= options.min_ramp_step as i64
                    || sink.allowed(LintCode::LowContrast, Kind::Palette, name)
                    || !reported.insert((LintCode::LowContrast, at, pair.0, pair.1))
                {
                    continue;
                }
                sink.push(found(LintCode::LowContrast, name, at), || {
                    format!(
                        "Keys `{a}` and `{b}` of ramp `{ramp}` in palette `{name}` differ by \
                         {} in lightness, too little to tell apart",
                        decimal(step)
                    )
                });
            }
        }
    }
}

/// How many palettes `palette` copies through, along `from=`.
fn copies(index: &Index, palette: &str) -> usize {
    let mut name = palette;
    for depth in 0..=index.len() {
        let detail = index.get(&(Kind::Palette, name)).map(|d| &d.detail);
        let Some(Detail::Palette {
            from: Some(from), ..
        }) = detail
        else {
            return depth;
        };
        name = from;
    }
    index.len()
}

/// The palette that writes `key` for palette `palette`, along `from=`, and where.
fn owner<'d>(index: &Index<'d>, palette: &str, key: char) -> Option<(&'d str, Position)> {
    let mut name = palette;
    for _ in 0..=index.len() {
        let def = index.get(&(Kind::Palette, name))?;
        let Detail::Palette { from, keys, .. } = &def.detail else {
            return None;
        };
        if let Some(&(_, pos)) = keys.iter().find(|&&(k, _)| k == key) {
            return Some((def.name.as_str(), pos));
        }
        name = from.as_deref()?;
    }
    None
}

/// The palette that writes ramp `ramp` for palette `palette`, along `from=`, and the
/// ramp as written.
fn ramp_source<'d>(
    index: &Index<'d>,
    palette: &str,
    ramp: &str,
) -> Option<(&'d str, &'d RampSource)> {
    let mut name = palette;
    for _ in 0..=index.len() {
        let def = index.get(&(Kind::Palette, name))?;
        let Detail::Palette { from, ramps, .. } = &def.detail else {
            return None;
        };
        if let Some(source) = ramps.iter().find(|r| r.0 == ramp) {
            return Some((def.name.as_str(), source));
        }
        name = from.as_deref()?;
    }
    None
}

/// The keys used with each palette directly: by sprites, frames and tiles drawn with
/// it, texture ramps, and steps of chains.
fn direct_uses(document: &Document) -> HashMap<&str, HashSet<char>> {
    let mut uses: HashMap<&str, HashSet<char>> = HashMap::new();
    let tiles = document.tilesets.iter().flat_map(|t| &t.tiles);
    for sprite in document
        .sprites
        .iter()
        .chain(&document.frame_sprites)
        .chain(tiles)
    {
        uses.entry(&sprite.palette.name)
            .or_default()
            .extend(sprite.keys());
    }
    for (palette, key) in &document.sources.palette_uses {
        uses.entry(palette).or_default().insert(*key);
    }
    for def in &document.sources.defs {
        if let Detail::Texture(ramp) = &def.detail {
            if let Some(sprite) = document.sprites.iter().find(|s| s.name == def.name) {
                uses.entry(&sprite.palette.name)
                    .or_default()
                    .extend(ramp.iter().copied());
            }
        }
    }
    uses
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
