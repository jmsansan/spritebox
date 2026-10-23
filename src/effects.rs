//! Pixel-preserving panel resizing and silhouette shadows.
use crate::sprite::{EMPTY, Layer};
use crate::{Error, ErrorCode, Image, Limits, Result, Sprite};

/// How a panel's edges and centre fill their destination.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Fill {
    /// Nearest-neighbour stretching. Corners retain their original pixels.
    #[default]
    Stretch,
    /// Repeat each edge and the centre from their top-left pixel.
    Tile,
}

/// The silhouette used by [`Sprite::shadow`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Shadow {
    #[default]
    Hard,
    /// Fill the integer steps between the original and the offset silhouette.
    Long,
}

impl Sprite {
    /// Resize a nine-slice or three-slice panel, within default limits.
    pub fn resize(&self, width: u32, height: u32, fill: Fill) -> Result<Sprite> {
        self.resize_with(width, height, fill, &Limits::default())
    }

    /// Resize a panel within explicit limits, retaining its border insets.
    pub fn resize_with(
        &self,
        width: u32,
        height: u32,
        fill: Fill,
        limits: &Limits,
    ) -> Result<Sprite> {
        let inset = self
            .slice
            .ok_or_else(|| Error::invalid("Only a panel with `slice=` can be resized"))?;
        self.effect_size(width, height, self.layers.len(), limits)?;
        let axis = |target: u32, source: u32, before: u32, after: u32, name: &str| -> Result<()> {
            if before == 0 && after == 0 && target != source {
                return Err(Error::invalid(format!(
                    "The {name} of this three-slice panel is fixed at {source}"
                )));
            }
            if target < before + after {
                return Err(Error::invalid(format!(
                    "The target {name} is smaller than the panel borders"
                )));
            }
            if source == before + after && target > before + after {
                return Err(Error::invalid(format!(
                    "The panel has no {name} centre pixels to fill the target"
                )));
            }
            Ok(())
        };
        axis(width, self.width, inset.left, inset.right, "width")?;
        axis(height, self.height, inset.top, inset.bottom, "height")?;
        let map = |p: u32, target: u32, source: u32, before: u32, after: u32| {
            if p < before {
                p
            } else if p >= target - after {
                source - (target - p)
            } else {
                let span = source - before - after;
                before
                    + match fill {
                        Fill::Stretch => {
                            ((p - before) as u64 * span as u64 / (target - before - after) as u64)
                                as u32
                        }
                        Fill::Tile => (p - before) % span,
                    }
            }
        };
        let mode = if fill == Fill::Tile { ",tile" } else { "" };
        Ok(self.derive(
            &format!("size({width}x{height}{mode})"),
            width,
            height,
            Some(inset),
            |x, y| {
                Some((
                    map(x, width, self.width, inset.left, inset.right),
                    map(y, height, self.height, inset.top, inset.bottom),
                ))
            },
        ))
    }

    /// Draw an offset silhouette beneath this sprite. The canvas grows, and panel
    /// insets grow on the same sides. Offsets are limited to -8..=8 pixels.
    pub fn shadow(&self, key: char, dx: i32, dy: i32, mode: Shadow) -> Result<Sprite> {
        self.shadow_with(key, dx, dy, mode, &Limits::default())
    }

    /// Draw a shadow within explicit image, sprite, and layer limits.
    pub fn shadow_with(
        &self,
        key: char,
        dx: i32,
        dy: i32,
        mode: Shadow,
        limits: &Limits,
    ) -> Result<Sprite> {
        if !(-8..=8).contains(&dx) || !(-8..=8).contains(&dy) {
            return Err(Error::invalid("Shadow offsets must be -8..=8"));
        }
        if self.palette.color(key).is_none() {
            return Err(Error::new(
                ErrorCode::UnknownName,
                format!("Unknown shadow key `{key}`"),
            ));
        }
        let width = self
            .width
            .checked_add(dx.unsigned_abs())
            .ok_or_else(|| Error::limit("Shadow width overflow"))?;
        let height = self
            .height
            .checked_add(dy.unsigned_abs())
            .ok_or_else(|| Error::limit("Shadow height overflow"))?;
        self.effect_size(width, height, self.layers.len() + 1, limits)?;
        let (left, top) = ((-dx).max(0) as u32, (-dy).max(0) as u32);
        let inset = self.slice.map(|mut s| {
            s.left += left;
            s.top += top;
            s.right += dx.max(0) as u32;
            s.bottom += dy.max(0) as u32;
            s
        });
        let suffix = if mode == Shadow::Long { ",long" } else { "" };
        let mut sprite = self.derive(
            &format!("shadow({key},{dx},{dy}{suffix})"),
            width,
            height,
            inset,
            |x, y| {
                (x >= left && y >= top && x - left < self.width && y - top < self.height)
                    .then(|| (x - left, y - top))
            },
        );
        let image = self.render();
        let mut keys = vec![EMPTY; width as usize * height as usize];
        let steps = dx.unsigned_abs().max(dy.unsigned_abs()).max(1);
        let first = if mode == Shadow::Long { 0 } else { steps };
        for step in first..=steps {
            let shift =
                |v: i32| ((v.unsigned_abs() * step + steps / 2) / steps) as i32 * v.signum();
            for y in 0..self.height {
                for x in 0..self.width {
                    if image.pixel(x, y).is_some_and(|c| c.a != 0) {
                        let tx = (x as i64 + left as i64 + shift(dx) as i64) as usize;
                        let ty = (y as i64 + top as i64 + shift(dy) as i64) as usize;
                        keys[ty * width as usize + tx] = key;
                    }
                }
            }
        }
        sprite.layers.insert(
            0,
            Layer {
                text: false,
                name: Some("shadow".into()),
                keys,
            },
        );
        Ok(sprite)
    }

    pub(crate) fn effect_size(
        &self,
        width: u32,
        height: u32,
        layers: usize,
        limits: &Limits,
    ) -> Result<()> {
        if width > limits.max_sprite_side
            || height > limits.max_sprite_side
            || layers > limits.max_layers
        {
            return Err(Error::limit(
                "The effect exceeds the sprite size or layer limit",
            ));
        }
        Image::check_size(width, height, limits)?;
        if (width as u64)
            .checked_mul(height as u64)
            .and_then(|n| n.checked_mul(layers as u64))
            .is_none_or(|n| n > limits.max_image_pixels)
        {
            return Err(Error::limit("The effect exceeds the pixel limit"));
        }
        Ok(())
    }
}
