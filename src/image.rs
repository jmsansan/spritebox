use std::fmt;
use std::str::FromStr;

use crate::{Error, Limits, Result};

/// A color: sRGB-encoded red, green and blue, and an alpha that is not premultiplied.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Fully transparent black, the color of pixels that nothing covers.
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    pub const fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
    pub const fn from_array([r, g, b, a]: [u8; 4]) -> Self {
        Self { r, g, b, a }
    }

    /// `self` drawn over `below` with the "over" operator, on sRGB-encoded values and
    /// in integer arithmetic, so that the result is the same on every platform.
    pub fn over(self, below: Color) -> Color {
        if self.a == 255 || below.a == 0 {
            return self;
        }
        if self.a == 0 {
            return below;
        }
        let sa = self.a as u32;
        // Alpha of `below` that shows through, scaled by 255 * 255.
        let da = below.a as u32 * (255 - sa);
        let out = sa * 255 + da;
        let channel = |s: u8, d: u8| ((s as u32 * sa * 255 + d as u32 * da + out / 2) / out) as u8;
        Color {
            r: channel(self.r, below.r),
            g: channel(self.g, below.g),
            b: channel(self.b, below.b),
            a: ((out + 127) / 255) as u8,
        }
    }
}

impl fmt::Display for Color {
    /// `#rrggbb` for an opaque color, `#rrggbbaa` otherwise.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)?;
        if self.a != 255 {
            write!(f, "{:02x}", self.a)?;
        }
        Ok(())
    }
}

impl FromStr for Color {
    type Err = Error;
    /// Parses `#rrggbb`, `#rrggbbaa` or `transparent`.
    fn from_str(text: &str) -> Result<Self> {
        if text == "transparent" {
            return Ok(Color::TRANSPARENT);
        }
        let hex = text
            .strip_prefix('#')
            .filter(|h| (h.len() == 6 || h.len() == 8) && h.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| {
                Error::invalid(format!(
                    "Color `{text}` must be `#rrggbb`, `#rrggbbaa` or `transparent`"
                ))
            })?;
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
        let a = if hex.len() == 8 { byte(6) } else { 255 };
        Ok(Color::rgba(byte(0), byte(2), byte(4), a))
    }
}

/// An RGBA8 image: sRGB-encoded, not premultiplied, tightly packed rows, top row first.
///
/// That is the layout that GPU texture uploads and most engines take, such as Diamon's
/// `TextureStore::register`.
#[derive(Clone, Eq, PartialEq)]
pub struct Image {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl fmt::Debug for Image {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Image({}x{})", self.width, self.height)
    }
}

impl Image {
    /// A transparent image. Both sides must be at least 1 and within the default
    /// [`Limits`].
    pub fn new(width: u32, height: u32) -> Result<Self> {
        Self::new_with(width, height, &Limits::default())
    }

    /// A transparent image within `limits`.
    pub fn new_with(width: u32, height: u32, limits: &Limits) -> Result<Self> {
        Self::check_size(width, height, limits)?;
        Ok(Self::blank(width, height))
    }

    /// An image of `pixels`, which must hold `width * height * 4` bytes.
    pub fn from_pixels(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(Error::invalid("An image needs at least one pixel a side"));
        }
        if pixels.len() as u64 != width as u64 * height as u64 * 4 {
            return Err(Error::invalid(format!(
                "An image of {width}x{height} needs {} bytes of RGBA8, not {}",
                width as u64 * height as u64 * 4,
                pixels.len()
            )));
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub(crate) fn check_size(width: u32, height: u32, limits: &Limits) -> Result<()> {
        if width == 0 || height == 0 {
            return Err(Error::invalid("An image needs at least one pixel a side"));
        }
        limits.check_image(width, height)
    }

    /// A transparent image whose size the caller has checked.
    pub(crate) fn blank(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    /// The RGBA8 bytes, `width * height * 4` of them.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }

    /// The color at `(x, y)`, or `None` outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<Color> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        Some(Color::from_array([
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]))
    }

    /// Sets the color at `(x, y)`. Outside the image, does nothing.
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x < self.width && y < self.height {
            let i = (y as usize * self.width as usize + x as usize) * 4;
            self.pixels[i..i + 4].copy_from_slice(&color.to_array());
        }
    }

    /// Draws `color` over the pixel at `(x, y)`, which may be outside the image.
    pub(crate) fn blend_pixel(&mut self, x: i64, y: i64, color: Color) {
        if x < 0 || y < 0 || x >= self.width as i64 || y >= self.height as i64 {
            return;
        }
        let (x, y) = (x as u32, y as u32);
        if let Some(below) = self.pixel(x, y) {
            self.set_pixel(x, y, color.over(below));
        }
    }

    /// Fills a rectangle, clipped to the image, with `color` drawn over what is there.
    pub(crate) fn fill_rect(&mut self, x: i64, y: i64, width: u32, height: u32, color: Color) {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + width as i64).min(self.width as i64);
        let y1 = (y + height as i64).min(self.height as i64);
        for py in y0..y1 {
            for px in x0..x1 {
                self.blend_pixel(px, py, color);
            }
        }
    }

    /// Copies `source` with its top-left corner at `(x, y)`, replacing pixels and
    /// clipping to this image.
    pub fn copy_from(&mut self, source: &Image, x: i64, y: i64) {
        for sy in 0..source.height {
            let ty = y + sy as i64;
            if ty < 0 || ty >= self.height as i64 {
                continue;
            }
            let x0 = x.max(0);
            let x1 = (x + source.width as i64).min(self.width as i64);
            if x0 >= x1 {
                return;
            }
            let from = (sy as usize * source.width as usize + (x0 - x) as usize) * 4;
            let to = (ty as usize * self.width as usize + x0 as usize) * 4;
            let len = (x1 - x0) as usize * 4;
            self.pixels[to..to + len].copy_from_slice(&source.pixels[from..from + len]);
        }
    }

    /// Encodes the image as a PNG file. See [`png::encode`](crate::png::encode).
    pub fn to_png(&self) -> Vec<u8> {
        crate::png::encode(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_parse_and_print() {
        assert_eq!(
            "#1a1c2c".parse::<Color>().unwrap(),
            Color::rgb(0x1a, 0x1c, 0x2c)
        );
        assert_eq!(
            "#FF000080".parse::<Color>().unwrap(),
            Color::rgba(255, 0, 0, 128)
        );
        assert_eq!("transparent".parse::<Color>().unwrap(), Color::TRANSPARENT);
        for bad in ["", "#", "#fff", "#12345g", "1a1c2c", "#1a1c2c0", "#+1a1c2c"] {
            assert!(bad.parse::<Color>().is_err(), "{bad}");
        }
        assert_eq!(Color::rgb(1, 2, 255).to_string(), "#0102ff");
        assert_eq!(Color::rgba(1, 2, 255, 0).to_string(), "#0102ff00");
    }

    #[test]
    fn over_matches_the_operator() {
        let red = Color::rgb(255, 0, 0);
        let half_blue = Color::rgba(0, 0, 255, 128);
        assert_eq!(red.over(Color::WHITE), red);
        assert_eq!(Color::TRANSPARENT.over(red), red);
        assert_eq!(half_blue.over(Color::TRANSPARENT), half_blue);
        assert_eq!(half_blue.over(red), Color::rgba(127, 0, 128, 255));
        let a = Color::rgba(200, 100, 0, 100).over(Color::rgba(0, 100, 200, 100));
        // Alpha: 100 + 100 * 155 / 255 = 160.8; colors weighted 100 : 60.8.
        assert_eq!(a, Color::rgba(124, 100, 76, 161));
    }

    #[test]
    fn copies_clip() {
        let mut target = Image::new(4, 4).unwrap();
        let mut source = Image::new(3, 3).unwrap();
        source.set_pixel(0, 0, Color::WHITE);
        source.set_pixel(2, 2, Color::BLACK);
        target.copy_from(&source, -2, 2);
        assert_eq!(target.pixel(0, 4), None);
        assert_eq!(target.pixel(0, 2), Some(Color::TRANSPARENT));
        target.copy_from(&source, 2, -2);
        assert_eq!(target.pixel(2, 0), Some(Color::TRANSPARENT));
        assert_eq!(target.pixel(3, 0), Some(Color::TRANSPARENT));
        target.copy_from(&source, 1, 1);
        assert_eq!(target.pixel(1, 1), Some(Color::WHITE));
        assert_eq!(target.pixel(3, 3), Some(Color::BLACK));
    }
}
