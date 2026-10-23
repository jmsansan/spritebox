//! Fixed-position UI compositions, rendered independently of the atlas.
use crate::{Color, Document, Error, Image, Limits, Position, Result, Sprite};

/// A sprite chain placed on a screen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScreenItem {
    pub(crate) sprite: Sprite,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) position: Position,
}
impl ScreenItem {
    pub fn sprite(&self) -> &Sprite {
        &self.sprite
    }
    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }
}

/// A screen with a finite pixel canvas and ordered sprite placements.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Screen {
    pub(crate) name: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) background: Option<(String, char)>,
    pub(crate) grid: Option<u32>,
    pub(crate) items: Vec<ScreenItem>,
    pub(crate) position: Position,
}
impl Screen {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub fn items(&self) -> &[ScreenItem] {
        &self.items
    }
    pub fn grid(&self) -> Option<u32> {
        self.grid
    }
    pub(crate) fn canvas(&self, document: &Document, limits: &Limits) -> Result<Image> {
        let mut image = Image::new_with(self.width, self.height, limits)?;
        let color = self
            .background
            .as_ref()
            .and_then(|(p, k)| document.palette(p)?.color(*k))
            .unwrap_or(Color::TRANSPARENT);
        for y in 0..self.height {
            for x in 0..self.width {
                image.set_pixel(x, y, color);
            }
        }
        Ok(image)
    }
    pub(crate) fn render(&self, document: &Document, limits: &Limits) -> Result<Image> {
        let mut image = self.canvas(document, limits)?;
        for item in &self.items {
            image.over_from(&item.sprite.render(), item.x as i64, item.y as i64);
        }
        Ok(image)
    }
}
impl Document {
    pub fn screens(&self) -> &[Screen] {
        &self.screens
    }
    /// Render a named screen within the default limits.
    pub fn screen(&self, name: &str) -> Option<Image> {
        self.screen_with(name, &Limits::default()).ok().flatten()
    }
    /// Render a screen with explicit limits. Unknown names return `Ok(None)`.
    pub fn screen_with(&self, name: &str, limits: &Limits) -> Result<Option<Image>> {
        self.screens
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.render(self, limits))
            .transpose()
    }
}
impl Image {
    /// Draw source-over at signed coordinates, clipping to this image.
    pub fn over_from(&mut self, source: &Image, x: i64, y: i64) {
        let (left, top, right, bottom) = placement_bounds(
            self.width(),
            self.height(),
            source.width(),
            source.height(),
            x,
            y,
        );
        for ty in top..bottom {
            for tx in left..right {
                let color = source.pixel((tx - x) as u32, (ty - y) as u32).unwrap();
                let below = self.pixel(tx as u32, ty as u32).unwrap();
                self.set_pixel(tx as u32, ty as u32, color.over(below));
            }
        }
    }
    /// Nearest-neighbour integer scaling within explicit image limits.
    pub fn scaled_with(&self, scale: u32, limits: &Limits) -> Result<Image> {
        if !(1..=64).contains(&scale) {
            return Err(Error::invalid("Scale must be 1..=64"));
        }
        let w = self
            .width()
            .checked_mul(scale)
            .ok_or_else(|| Error::limit("Scaled width overflow"))?;
        let h = self
            .height()
            .checked_mul(scale)
            .ok_or_else(|| Error::limit("Scaled height overflow"))?;
        let mut image = Image::new_with(w, h, limits)?;
        for y in 0..h {
            for x in 0..w {
                image.set_pixel(x, y, self.pixel(x / scale, y / scale).unwrap());
            }
        }
        Ok(image)
    }
}

pub(crate) fn placement_bounds(
    width: u32,
    height: u32,
    source_width: u32,
    source_height: u32,
    x: i64,
    y: i64,
) -> (i64, i64, i64, i64) {
    (
        x.clamp(0, width as i64),
        y.clamp(0, height as i64),
        x.saturating_add(source_width as i64).clamp(0, width as i64),
        y.saturating_add(source_height as i64)
            .clamp(0, height as i64),
    )
}
