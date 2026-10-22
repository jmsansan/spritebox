use crate::{Color, Error, ErrorCode, Image, Limits, Result};

/// A named set of colors, each under a one-character key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Palette {
    pub(crate) name: String,
    pub(crate) colors: Vec<(char, Color)>,
    pub(crate) ramps: Vec<Ramp>,
}

/// A ramp: the keys of a palette that shade one material, dark to light.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ramp {
    pub(crate) name: String,
    pub(crate) keys: Vec<char>,
}

impl Ramp {
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The keys, darkest first.
    pub fn keys(&self) -> &[char] {
        &self.keys
    }
}

impl Palette {
    pub fn name(&self) -> &str {
        &self.name
    }
    /// The keys and their colors, inherited keys first, in the order written.
    pub fn colors(&self) -> &[(char, Color)] {
        &self.colors
    }
    /// The ramps, inherited ones first, in the order written.
    pub fn ramps(&self) -> &[Ramp] {
        &self.ramps
    }
    pub fn ramp(&self, name: &str) -> Option<&Ramp> {
        self.ramps.iter().find(|r| r.name == name)
    }
    /// The color of `key`, if the palette has it.
    pub fn color(&self, key: char) -> Option<Color> {
        self.colors.iter().find(|(k, _)| *k == key).map(|&(_, c)| c)
    }
}

/// Marks a pixel that no key covers in a layer.
pub(crate) const EMPTY: char = '\0';

/// The borders of a nine-slice panel, in pixels from each edge: the corners keep their
/// size while the edges and the middle stretch.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Insets {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl Insets {
    /// Whether the insets fit in a sprite of `width` × `height`.
    pub fn fits(self, width: u32, height: u32) -> bool {
        self.left as u64 + self.right as u64 <= width as u64
            && self.top as u64 + self.bottom as u64 <= height as u64
    }
}

/// One layer of a sprite: a key for each pixel, row by row, or [`EMPTY`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Layer {
    pub name: Option<String>,
    pub keys: Vec<char>,
}

/// A sprite: a grid of palette keys in one or more layers, and the palette that
/// colors them.
///
/// The transforms (`flip_x`, `rotate`, `with_palette`, `outline` and so on) return
/// new sprites named with the modifier chain of the text format, such as
/// `coin:flip_x:outline(k)`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sprite {
    pub(crate) name: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) palette: Palette,
    pub(crate) layers: Vec<Layer>,
    pub(crate) slice: Option<Insets>,
}

impl Sprite {
    /// The borders of a nine-slice panel, when the sprite is one (`slice=`). Flips,
    /// rotations and padding carry them along; an offset drops them.
    pub fn slice(&self) -> Option<Insets> {
        self.slice
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    /// The palette that colors the sprite's keys.
    pub fn palette(&self) -> &Palette {
        &self.palette
    }
    /// The names of the layers, bottom first; `None` for a sprite written without
    /// layers.
    pub fn layer_names(&self) -> impl Iterator<Item = Option<&str>> {
        self.layers.iter().map(|l| l.name.as_deref())
    }
    /// The key seen at `(x, y)`: the topmost key whose color is not fully transparent,
    /// or else the topmost key. `None` where no layer covers the pixel, or outside the
    /// sprite.
    pub fn key(&self, x: u32, y: u32) -> Option<char> {
        let layer = self.top_layer(x, y)?;
        Some(self.layers[layer].keys[y as usize * self.width as usize + x as usize])
    }

    /// The keys that the layers use, each once, in order.
    pub(crate) fn keys(&self) -> Vec<char> {
        let mut keys: Vec<char> = self
            .layers
            .iter()
            .flat_map(|l| l.keys.iter().copied())
            .filter(|&k| k != EMPTY)
            .collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    }

    /// The index of the layer whose key [`Sprite::key`] sees at `(x, y)`.
    pub(crate) fn top_layer(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = y as usize * self.width as usize + x as usize;
        let lookup = ColorLookup::new(&self.palette);
        let mut covering = self
            .layers
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, layer)| layer.keys[i] != EMPTY);
        let (top, _) = covering.clone().next()?;
        Some(
            covering
                .find(|(_, layer)| lookup.get(layer.keys[i]).a > 0)
                .map_or(top, |(n, _)| n),
        )
    }

    /// Renders the sprite to RGBA8: each layer's keys are colored by the palette and
    /// the layers are drawn over each other in order with [`Color::over`]. Pixels that
    /// no layer covers are transparent.
    pub fn render(&self) -> Image {
        let lookup = ColorLookup::new(&self.palette);
        let mut image = Image::blank(self.width, self.height);
        let w = self.width as usize;
        for layer in &self.layers {
            for (i, &key) in layer.keys.iter().enumerate() {
                if key == EMPTY {
                    continue;
                }
                let (x, y) = ((i % w) as u32, (i / w) as u32);
                if let Some(below) = image.pixel(x, y) {
                    image.set_pixel(x, y, lookup.get(key).over(below));
                }
            }
        }
        image
    }

    fn derive(
        &self,
        op: &str,
        width: u32,
        height: u32,
        slice: Option<Insets>,
        map: impl Fn(u32, u32) -> Option<(u32, u32)>,
    ) -> Sprite {
        let layers = self
            .layers
            .iter()
            .map(|layer| {
                let mut keys = Vec::with_capacity(width as usize * height as usize);
                for y in 0..height {
                    for x in 0..width {
                        keys.push(match map(x, y) {
                            Some((sx, sy)) => {
                                layer.keys[sy as usize * self.width as usize + sx as usize]
                            }
                            None => EMPTY,
                        });
                    }
                }
                Layer {
                    name: layer.name.clone(),
                    keys,
                }
            })
            .collect();
        Sprite {
            name: format!("{}:{op}", self.name),
            width,
            height,
            palette: self.palette.clone(),
            layers,
            slice,
        }
    }

    /// The sprite mirrored left to right (`:flip_x`).
    pub fn flip_x(&self) -> Sprite {
        let w = self.width;
        let slice = self.slice.map(|i| Insets {
            left: i.right,
            right: i.left,
            ..i
        });
        self.derive("flip_x", w, self.height, slice, |x, y| Some((w - 1 - x, y)))
    }

    /// The sprite mirrored top to bottom (`:flip_y`).
    pub fn flip_y(&self) -> Sprite {
        let h = self.height;
        let slice = self.slice.map(|i| Insets {
            top: i.bottom,
            bottom: i.top,
            ..i
        });
        self.derive("flip_y", self.width, h, slice, |x, y| Some((x, h - 1 - y)))
    }

    /// The sprite rotated clockwise by `quarter_turns` × 90 degrees (`:rot90`,
    /// `:rot180`, `:rot270`). A quarter turn swaps the width and the height.
    pub fn rotate(&self, quarter_turns: u32) -> Sprite {
        let (w, h) = (self.width, self.height);
        // Each quarter turn clockwise moves the left border to the top.
        let turn = |i: Insets| Insets {
            left: i.bottom,
            top: i.left,
            right: i.top,
            bottom: i.right,
        };
        let mut slice = self.slice;
        for _ in 0..quarter_turns % 4 {
            slice = slice.map(turn);
        }
        match quarter_turns % 4 {
            0 => self.derive("rot0", w, h, slice, |x, y| Some((x, y))),
            1 => self.derive("rot90", h, w, slice, |x, y| Some((y, h - 1 - x))),
            2 => self.derive("rot180", w, h, slice, |x, y| Some((w - 1 - x, h - 1 - y))),
            _ => self.derive("rot270", h, w, slice, |x, y| Some((w - 1 - y, x))),
        }
    }

    /// The sprite's pixels moved right by `dx` and down by `dy` within the same size
    /// (`:offset(dx,dy)`). Pixels moved out are lost, and the pixels uncovered are
    /// empty.
    pub fn offset(&self, dx: i32, dy: i32) -> Sprite {
        let (w, h) = (self.width as i64, self.height as i64);
        self.derive(
            &format!("offset({dx},{dy})"),
            self.width,
            self.height,
            None,
            |x, y| {
                let (sx, sy) = (x as i64 - dx as i64, y as i64 - dy as i64);
                (sx >= 0 && sy >= 0 && sx < w && sy < h).then_some((sx as u32, sy as u32))
            },
        )
    }

    /// The sprite with `n` empty pixels added on every side (`:pad(n)`), within the
    /// default [`Limits::max_sprite_side`].
    pub fn pad(&self, n: u32) -> Result<Sprite> {
        self.pad_within(n, Limits::default().max_sprite_side)
    }

    pub(crate) fn pad_within(&self, n: u32, max_side: u32) -> Result<Sprite> {
        let grow = |side: u32| {
            side.checked_add(n.saturating_mul(2))
                .filter(|&s| s <= max_side)
        };
        let (Some(width), Some(height)) = (grow(self.width), grow(self.height)) else {
            return Err(Error::limit(format!(
                "Padding by {n} makes a sprite larger than the limit of {max_side} pixels a side"
            )));
        };
        let (w, h) = (self.width, self.height);
        let slice = self.slice.map(|i| Insets {
            left: i.left + n,
            top: i.top + n,
            right: i.right + n,
            bottom: i.bottom + n,
        });
        Ok(
            self.derive(&format!("pad({n})"), width, height, slice, |x, y| {
                (x >= n && y >= n && x - n < w && y - n < h).then(|| (x - n, y - n))
            }),
        )
    }

    /// The same keys colored by `palette` (`:swap(palette)`), which must have every
    /// key that the sprite uses.
    pub fn with_palette(&self, palette: &Palette) -> Result<Sprite> {
        if let Some(missing) = self.missing_key(palette) {
            return Err(Error::new(
                ErrorCode::UnknownName,
                format!(
                    "Palette `{}` has no key `{missing}`, which sprite `{}` uses",
                    palette.name, self.name
                ),
            ));
        }
        let mut sprite = self.clone();
        sprite.name = format!("{}:swap({})", self.name, palette.name);
        sprite.palette = palette.clone();
        Ok(sprite)
    }

    /// The first key of the sprite, in reading order, that `palette` lacks.
    pub(crate) fn missing_key(&self, palette: &Palette) -> Option<char> {
        let lookup = ColorLookup::new(palette);
        let mut checked: Vec<char> = Vec::new();
        for i in 0..self.width as usize * self.height as usize {
            for layer in &self.layers {
                let key = layer.keys[i];
                if key == EMPTY || checked.contains(&key) {
                    continue;
                }
                if lookup.find(key).is_none() {
                    return Some(key);
                }
                checked.push(key);
            }
        }
        None
    }

    /// The sprite with a one-pixel outline of `key` (`:outline(key)`): on every pixel
    /// that is fully transparent and shares an edge with a pixel that is not. The
    /// outline is a new bottom layer named `outline`. The size does not change, so
    /// pad the sprite first when its shape touches the edges.
    pub fn outline(&self, key: char) -> Result<Sprite> {
        if self.palette.color(key).is_none() {
            return Err(Error::new(
                ErrorCode::UnknownName,
                format!("Key `{key}` is not in palette `{}`", self.palette.name),
            ));
        }
        let image = self.render();
        let (w, h) = (self.width as i64, self.height as i64);
        let alpha = |x: i64, y: i64| -> u8 {
            if x < 0 || y < 0 || x >= w || y >= h {
                0
            } else {
                image.pixel(x as u32, y as u32).map_or(0, |c| c.a)
            }
        };
        let mut keys = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                let edge = alpha(x, y) == 0
                    && [(-1, 0), (1, 0), (0, -1), (0, 1)]
                        .iter()
                        .any(|&(dx, dy)| alpha(x + dx, y + dy) > 0);
                keys.push(if edge { key } else { EMPTY });
            }
        }
        let mut sprite = self.clone();
        sprite.name = format!("{}:outline({key})", self.name);
        sprite.layers.insert(
            0,
            Layer {
                name: Some("outline".into()),
                keys,
            },
        );
        Ok(sprite)
    }
}

/// Colors of palette keys, with a table for ASCII keys.
pub(crate) struct ColorLookup<'a> {
    palette: &'a Palette,
    ascii: [Option<Color>; 128],
}

impl<'a> ColorLookup<'a> {
    pub(crate) fn new(palette: &'a Palette) -> Self {
        let mut ascii = [None; 128];
        for &(key, color) in palette.colors.iter().rev() {
            if key.is_ascii() {
                ascii[key as usize] = Some(color);
            }
        }
        Self { palette, ascii }
    }
    pub(crate) fn find(&self, key: char) -> Option<Color> {
        if key.is_ascii() {
            self.ascii[key as usize]
        } else {
            self.palette.color(key)
        }
    }
    pub(crate) fn get(&self, key: char) -> Color {
        self.find(key).unwrap_or(Color::TRANSPARENT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite(rows: &[&str]) -> Sprite {
        let palette = Palette {
            name: "p".into(),
            colors: vec![
                ('.', Color::TRANSPARENT),
                ('a', Color::rgb(255, 0, 0)),
                ('b', Color::rgb(0, 255, 0)),
                ('k', Color::BLACK),
            ],
            ramps: Vec::new(),
        };
        Sprite {
            name: "s".into(),
            width: rows[0].len() as u32,
            height: rows.len() as u32,
            palette,
            layers: vec![Layer {
                name: None,
                keys: rows.iter().flat_map(|r| r.chars()).collect(),
            }],
            slice: None,
        }
    }

    fn rows(sprite: &Sprite) -> Vec<String> {
        (0..sprite.height)
            .map(|y| {
                (0..sprite.width)
                    .map(|x| sprite.key(x, y).unwrap_or(' '))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn transforms() {
        let s = sprite(&["ab.", "..b"]);
        assert_eq!(rows(&s.flip_x()), vec![".ba", "b.."]);
        assert_eq!(rows(&s.flip_y()), vec!["..b", "ab."]);
        assert_eq!(rows(&s.rotate(1)), vec![".a", ".b", "b."]);
        assert_eq!(rows(&s.rotate(2)), vec!["b..", ".ba"]);
        assert_eq!(rows(&s.rotate(3)), vec![".b", "b.", "a."]);
        assert_eq!(s.rotate(4).layers, s.layers);
        assert_eq!(rows(&s.offset(1, 1)), vec!["   ", " ab"]);
        assert_eq!(rows(&s.offset(-2, 0)), vec![".  ", "b  "]);
        assert_eq!(
            rows(&s.pad(1).unwrap()),
            vec!["     ", " ab. ", " ..b ", "     "]
        );
        assert!(s.pad(600).is_err());
        assert_eq!(s.flip_x().rotate(1).name(), "s:flip_x:rot90");
        assert_eq!(s.offset(-1, 2).name(), "s:offset(-1,2)");
    }

    #[test]
    fn outlines_and_palettes() {
        let s = sprite(&["....", ".a..", "...."]);
        let o = s.outline('k').unwrap();
        assert_eq!(rows(&o), vec![".k..", "kak.", ".k.."]);
        assert_eq!(o.name(), "s:outline(k)");
        assert_eq!(o.layer_names().collect::<Vec<_>>(), [Some("outline"), None]);
        assert_eq!(s.outline('z').unwrap_err().code, ErrorCode::UnknownName);

        let night = Palette {
            name: "night".into(),
            colors: vec![('.', Color::TRANSPARENT), ('a', Color::rgb(1, 2, 3))],
            ramps: Vec::new(),
        };
        let swapped = s.with_palette(&night).unwrap();
        assert_eq!(swapped.render().pixel(1, 1), Some(Color::rgb(1, 2, 3)));
        assert_eq!(swapped.name(), "s:swap(night)");
        let err = sprite(&["ab"]).with_palette(&night).unwrap_err();
        assert_eq!(err.code, ErrorCode::UnknownName);
        assert!(err.message.contains("no key `b`"), "{}", err.message);
    }
}
