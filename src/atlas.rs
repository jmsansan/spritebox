//! Atlases: every sprite and animation frame packed into one texture, with the
//! rectangle of each and the animations' frames and ticks.

use std::collections::HashMap;

use crate::json::{Writer, list, quote};
use crate::{Document, Error, Image, Limits, Result};

/// How sprites are packed into an atlas.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AtlasOptions {
    /// Pixels around each sprite or tile, 0..=64: neighbors are `2 * padding` apart
    /// and `padding` from the edges. Default 1. Use 0 for textures sampled only with
    /// nearest filtering.
    pub padding: u32,
    /// Fill the padding with copies of each sprite's edge pixels, so that linear
    /// filtering and mipmaps do not mix in transparent texels or neighbors. With
    /// `false`, the padding is transparent. Default true.
    pub extrude: bool,
    /// The largest width or height of a texture, 1..=[`Limits::max_image_side`].
    /// Default 2048, the largest side that every WebGPU device supports. Art that does
    /// not fit is an error, never a second texture.
    pub max_side: u32,
}

impl Default for AtlasOptions {
    fn default() -> Self {
        Self {
            padding: 1,
            extrude: true,
            max_side: 2048,
        }
    }
}

impl AtlasOptions {
    pub(crate) fn check(&self, limits: &Limits) -> Result<()> {
        if self.padding > 64 {
            return Err(Error::invalid("Atlas padding must be 0..=64"));
        }
        if self.max_side == 0 || self.max_side > limits.max_image_side {
            return Err(Error::invalid(format!(
                "The atlas's largest side must be 1..={}",
                limits.max_image_side
            )));
        }
        Ok(())
    }
}

/// A rectangle of a texture in texels, from its top-left corner.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// `[x, y, width, height]`, as the JSON metadata writes it.
    pub fn to_array(self) -> [u32; 4] {
        [self.x, self.y, self.width, self.height]
    }
}

/// A sprite's place in an atlas.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Region {
    /// A sprite's name, or the modifier chain of an animation frame's sprite.
    pub name: String,
    pub rect: Rect,
}

/// An animation, with the atlas rectangle of each frame.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct AtlasAnimation {
    pub name: String,
    pub looping: bool,
    pub frames: Vec<AtlasFrame>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct AtlasFrame {
    /// The frame's sprite: a name or a modifier chain, as in [`Region::name`].
    pub sprite: String,
    pub rect: Rect,
    pub ticks: u32,
}

/// Every sprite of a document in one texture, and where each one is.
///
/// Sprites with identical pixels share one rectangle.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Atlas {
    pub image: Image,
    pub padding: u32,
    /// Named sprites in the order defined, then animation frames' sprites in the
    /// order first used.
    pub regions: Vec<Region>,
    pub animations: Vec<AtlasAnimation>,
}

impl Atlas {
    /// The rectangle of a sprite or frame sprite.
    pub fn region(&self, name: &str) -> Option<Rect> {
        self.regions.iter().find(|r| r.name == name).map(|r| r.rect)
    }

    /// The metadata as JSON, naming the atlas image file `image`. See the README for
    /// the form.
    pub fn to_json(&self, image: &str) -> String {
        let mut w = Writer::new();
        w.object(None);
        write_header(&mut w);
        self.write_json(&mut w, image);
        w.close('}');
        w.finish()
    }

    /// Writes the `atlas` and `animations` members.
    pub(crate) fn write_json(&self, w: &mut Writer, image: &str) {
        w.object(Some("atlas"));
        w.string(Some("image"), image);
        w.raw(
            Some("size"),
            &list(&[self.image.width(), self.image.height()]),
        );
        w.raw(Some("padding"), &self.padding.to_string());
        w.object(Some("sprites"));
        for region in &self.regions {
            w.raw(Some(&region.name), &list(&region.rect.to_array()));
        }
        w.close('}');
        w.close('}');
        w.object(Some("animations"));
        for animation in &self.animations {
            w.object(Some(&animation.name));
            w.raw(
                Some("loop"),
                if animation.looping { "true" } else { "false" },
            );
            w.array(Some("frames"));
            for frame in &animation.frames {
                w.raw(
                    None,
                    &format!(
                        "{{\"sprite\": {}, \"rect\": {}, \"ticks\": {}}}",
                        quote(&frame.sprite),
                        list(&frame.rect.to_array()),
                        frame.ticks
                    ),
                );
            }
            w.close(']');
            w.close('}');
        }
        w.close('}');
    }
}

pub(crate) fn write_header(w: &mut Writer) {
    w.string(Some("format"), "spritebox");
    w.raw(Some("version"), "1");
}

/// Packs the sprites of `document`.
pub(crate) fn build(document: &Document, options: &AtlasOptions, limits: &Limits) -> Result<Atlas> {
    options.check(limits)?;
    let sprites: Vec<_> = document
        .sprites
        .iter()
        .chain(&document.frame_sprites)
        .collect();
    // Identical images are packed once.
    let mut unique: Vec<Image> = Vec::new();
    let mut index_of: HashMap<Vec<u8>, Vec<usize>> = HashMap::new();
    let mut slot = Vec::with_capacity(sprites.len());
    for sprite in &sprites {
        let image = sprite.render();
        let candidates = index_of.entry(image.pixels().to_vec()).or_default();
        let found = candidates
            .iter()
            .copied()
            .find(|&u| unique[u].width() == image.width() && unique[u].height() == image.height());
        let u = match found {
            Some(u) => u,
            None => {
                candidates.push(unique.len());
                unique.push(image);
                unique.len() - 1
            }
        };
        slot.push(u);
    }
    let pad = options.padding;
    let sizes: Vec<(u32, u32)> = unique
        .iter()
        .map(|i| (i.width() + 2 * pad, i.height() + 2 * pad))
        .collect();
    let max = options.max_side;
    let (positions, width, height) = pack(&sizes, max).ok_or_else(|| {
        Error::limit(format!(
            "The sprites do not fit in an atlas of {max}x{max} texels with padding {pad}; \
             raise `max_side` or split the art"
        ))
    })?;
    let (width, height) = (width.max(1), height.max(1));
    Image::check_size(width, height, limits)?;
    let mut image = Image::blank(width, height);
    for (sprite, &(x, y)) in unique.iter().zip(&positions) {
        place(&mut image, sprite, x + pad, y + pad, pad, options.extrude);
    }
    let rect = |u: usize| Rect {
        x: positions[u].0 + pad,
        y: positions[u].1 + pad,
        width: unique[u].width(),
        height: unique[u].height(),
    };
    let regions: Vec<Region> = sprites
        .iter()
        .zip(&slot)
        .map(|(s, &u)| Region {
            name: s.name.clone(),
            rect: rect(u),
        })
        .collect();
    let find = |name: &str| {
        regions
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.rect)
            .unwrap_or_default()
    };
    let animations = document
        .animations
        .iter()
        .map(|a| AtlasAnimation {
            name: a.name.clone(),
            looping: a.looping,
            frames: a
                .frames
                .iter()
                .map(|f| AtlasFrame {
                    sprite: f.sprite.clone(),
                    rect: find(&f.sprite),
                    ticks: f.ticks,
                })
                .collect(),
        })
        .collect();
    Ok(Atlas {
        image,
        padding: pad,
        regions,
        animations,
    })
}

/// Copies `sprite` to `(x, y)`, and its edge pixels outward into the padding when
/// `extrude` is set.
pub(crate) fn place(image: &mut Image, sprite: &Image, x: u32, y: u32, pad: u32, extrude: bool) {
    image.copy_from(sprite, x as i64, y as i64);
    if extrude && pad > 0 {
        extrude_edges(image, sprite, x, y, pad);
    }
}

/// Copies the edge pixels of the sprite at `(x, y)` outward into its padding.
fn extrude_edges(image: &mut Image, sprite: &Image, x: u32, y: u32, pad: u32) {
    let (w, h) = (sprite.width() as i64, sprite.height() as i64);
    for py in -(pad as i64)..h + pad as i64 {
        for px in -(pad as i64)..w + pad as i64 {
            if (0..w).contains(&px) && (0..h).contains(&py) {
                continue;
            }
            let (cx, cy) = (px.clamp(0, w - 1) as u32, py.clamp(0, h - 1) as u32);
            if let Some(color) = sprite.pixel(cx, cy) {
                image.set_pixel((x as i64 + px) as u32, (y as i64 + py) as u32, color);
            }
        }
    }
}

/// Top-left corners of packed rectangles, and the width and height they use.
type Packing = (Vec<(u32, u32)>, u32, u32);

/// Packs rectangles of `sizes` within `max_side` a side; returns their top-left
/// corners and the size used, or `None` when they do not fit.
///
/// Rectangles go tallest first into a skyline, each where its top edge is lowest and
/// then leftmost. Several widths are tried, and the one with the smallest area wins,
/// then the squarest, then the narrowest. The result depends only on `sizes`.
fn pack(sizes: &[(u32, u32)], max_side: u32) -> Option<Packing> {
    if sizes.is_empty() {
        return Some((Vec::new(), 0, 0));
    }
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|&i| {
        (
            std::cmp::Reverse(sizes[i].1),
            std::cmp::Reverse(sizes[i].0),
            i,
        )
    });
    let widest = sizes.iter().map(|s| s.0).max().unwrap_or(1) as u64;
    let area: u64 = sizes.iter().map(|&(w, h)| w as u64 * h as u64).sum();
    let side = area.isqrt() + 1;
    let mut widths: Vec<u64> = [side, side * 5 / 4, side * 3 / 2, side * 2]
        .iter()
        .map(|&w| w.max(widest))
        .collect();
    let mut p = widest.next_power_of_two();
    while p <= side * 2 {
        widths.push(p);
        p *= 2;
    }
    widths.push(max_side as u64);
    widths.retain(|&w| w >= widest && w <= max_side as u64);
    widths.sort_unstable();
    widths.dedup();
    let mut best: Option<Packing> = None;
    for width in widths {
        let placed = skyline(sizes, &order, width as u32);
        let (w, h) = placed
            .iter()
            .zip(sizes)
            .fold((0, 0), |(mw, mh), (&(x, y), &(sw, sh))| {
                (mw.max(x + sw), mh.max(y + sh))
            });
        if h > max_side {
            continue;
        }
        let score = |w: u32, h: u32| (w as u64 * h as u64, w.max(h), w);
        if best.as_ref().is_none_or(|b| score(w, h) < score(b.1, b.2)) {
            best = Some((placed, w, h));
        }
    }
    best
}

fn skyline(sizes: &[(u32, u32)], order: &[usize], width: u32) -> Vec<(u32, u32)> {
    // Segments of the skyline as (x, y, width), left to right, covering 0..width.
    let mut segments: Vec<(u32, u32, u32)> = vec![(0, 0, width)];
    let mut placed = vec![(0, 0); sizes.len()];
    for &i in order {
        let (w, h) = sizes[i];
        let mut best: Option<(u32, u32, usize)> = None;
        for start in 0..segments.len() {
            let x = segments[start].0;
            if x as u64 + w as u64 > width as u64 {
                break;
            }
            let mut y = 0;
            let mut covered = 0u64;
            for seg in &segments[start..] {
                if covered >= w as u64 {
                    break;
                }
                y = y.max(seg.1);
                covered += seg.2 as u64;
            }
            if best.is_none_or(|(by, bx, _)| (y + h, x) < (by + h, bx)) {
                best = Some((y, x, start));
            }
        }
        let (y, x, start) = best.unwrap_or((0, 0, 0));
        placed[i] = (x, y);
        // Replace the covered part of the skyline with the new top edge.
        let end = x + w;
        let mut next = Vec::with_capacity(segments.len() + 2);
        next.extend_from_slice(&segments[..start]);
        next.push((x, y + h, w));
        for &(sx, sy, sw) in &segments[start..] {
            let s_end = sx + sw;
            if s_end <= end {
                continue;
            }
            if sx < end {
                next.push((end, sy, s_end - end));
            } else {
                next.push((sx, sy, sw));
            }
        }
        // Merge neighbors of equal height.
        segments.clear();
        for seg in next {
            match segments.last_mut() {
                Some(last) if last.1 == seg.1 => last.2 += seg.2,
                _ => segments.push(seg),
            }
        }
    }
    placed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(a: (u32, u32, u32, u32), b: (u32, u32, u32, u32)) -> bool {
        a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
    }

    #[test]
    fn packs_without_overlap() {
        let mut sizes = Vec::new();
        let mut x = 7u32;
        for _ in 0..200 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            sizes.push((1 + (x >> 16) % 40, 1 + (x >> 8) % 30));
        }
        let (placed, w, h) = pack(&sizes, 4096).unwrap();
        let rects: Vec<_> = placed
            .iter()
            .zip(&sizes)
            .map(|(&(x, y), &(sw, sh))| (x, y, sw, sh))
            .collect();
        for (i, a) in rects.iter().enumerate() {
            assert!(a.0 + a.2 <= w && a.1 + a.3 <= h);
            for b in &rects[i + 1..] {
                assert!(!overlaps(*a, *b), "{a:?} {b:?}");
            }
        }
        let area: u64 = sizes.iter().map(|&(w, h)| w as u64 * h as u64).sum();
        let used = w as u64 * h as u64;
        assert!(used < area * 13 / 10, "{used} for {area}");
        assert_eq!(pack(&sizes, 4096), Some((placed, w, h)), "deterministic");
    }

    #[test]
    fn packs_equal_squares_into_a_square() {
        let (_, w, h) = pack(&[(10, 10); 16], 4096).unwrap();
        assert_eq!((w, h), (40, 40));
        assert_eq!(pack(&[(3, 5)], 4096).unwrap().1, 3);
        // Within a bound that forces a narrower, taller layout, or none at all.
        let (_, w, h) = pack(&[(10, 10); 16], 20).unwrap_or_default();
        assert_eq!((w, h), (0, 0), "16 squares of 10 do not fit in 20x20");
        let (_, w, h) = pack(&[(10, 10); 4], 20).unwrap();
        assert_eq!((w, h), (20, 20));
        assert!(pack(&[(30, 1)], 20).is_none());
    }
}
