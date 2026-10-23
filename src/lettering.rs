//! Palette-key text sprites with wrapping and alignment.
use crate::sprite::{EMPTY, Layer};
use crate::{Error, ErrorCode, Font, Image, Limits, Palette, Result, Sprite};

/// Horizontal alignment within the text sprite's width.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// Layout of a static text sprite.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TextOptions {
    pub width: Option<u32>,
    pub align: Align,
    pub line_height: Option<u32>,
}

impl Font {
    /// Draw text into a palette-key sprite, rejecting characters without glyphs.
    pub fn sprite(
        &self,
        text: &str,
        palette: &Palette,
        key: char,
        options: &TextOptions,
    ) -> Result<Sprite> {
        self.sprite_with(text, palette, key, options, &Limits::default())
    }

    /// [`Font::sprite`] with explicit limits. Wrapping happens at spaces; an
    /// overlong word is an error. Explicit leading spaces remain in each line.
    pub fn sprite_with(
        &self,
        text: &str,
        palette: &Palette,
        key: char,
        options: &TextOptions,
        limits: &Limits,
    ) -> Result<Sprite> {
        if limits.max_layers == 0 {
            return Err(Error::limit("Text sprites need one layer"));
        }
        if text.len() > limits.max_text_bytes {
            return Err(Error::limit("Text exceeds the input byte limit"));
        }
        if palette.color(key).is_none() {
            return Err(Error::new(
                ErrorCode::UnknownName,
                format!("Unknown text color `{key}`"),
            ));
        }
        let line_height = options.line_height.unwrap_or(self.line_height);
        if line_height == 0 || line_height > limits.max_sprite_side || options.width == Some(0) {
            return Err(Error::invalid(
                "Text width and line height must be positive and within the sprite limits",
            ));
        }
        for c in text.chars().filter(|c| *c != '\n') {
            if self.glyph(c).is_none() {
                return Err(Error::new(
                    ErrorCode::UnknownName,
                    format!("Font `{}` has no glyph for `{c}`", self.name),
                ));
            }
        }
        let mut lines: Vec<Vec<char>> = Vec::new();
        for raw in text.split('\n') {
            let chars: Vec<char> = raw.chars().collect();
            let mut start = 0;
            loop {
                let rest = &chars[start..];
                if let Some(width) = options.width {
                    let fit = self.fitting_prefix(rest, width as u64);
                    if fit < rest.len() {
                        let cut = if rest.get(fit) == Some(&' ') {
                            Some(fit)
                        } else {
                            rest[..fit].iter().rposition(|c| *c == ' ')
                        };
                        let Some(cut) = cut else {
                            return Err(Error::invalid(format!(
                                "A word is wider than the text width of {width}"
                            )));
                        };
                        let mut end = cut;
                        while end > 0 && rest[end - 1] == ' ' {
                            end -= 1;
                        }
                        lines.push(rest[..end].to_vec());
                        if lines.len() as u64 * line_height as u64 > limits.max_sprite_side as u64 {
                            return Err(Error::limit(
                                "Wrapped text exceeds the sprite height limit",
                            ));
                        }
                        start += cut + 1;
                        while start < chars.len() && chars[start] == ' ' {
                            start += 1;
                        }
                        continue;
                    }
                }
                lines.push(rest.to_vec());
                break;
            }
        }
        let natural = lines
            .iter()
            .map(|l| self.layout_width(l))
            .max()
            .unwrap_or(0);
        let width = options.width.map(u64::from).unwrap_or(natural).max(1);
        let height = (lines.len() as u64 - 1)
            .checked_mul(line_height as u64)
            .and_then(|v| v.checked_add(self.height as u64))
            .ok_or_else(|| Error::limit("Text height overflow"))?;
        if width > limits.max_sprite_side as u64 || height > limits.max_sprite_side as u64 {
            return Err(Error::limit("Text exceeds the sprite size limit"));
        }
        Image::check_size(width as u32, height as u32, limits)?;
        let mut keys = vec![EMPTY; (width * height) as usize];
        for (row, line) in lines.iter().enumerate() {
            let content = self.layout_width(line);
            let offset = match options.align {
                Align::Left => 0,
                Align::Center => (width - content) / 2,
                Align::Right => width - content,
            };
            let mut pen = offset as i64;
            let mut previous = None;
            for &c in line {
                let glyph = self.glyph(c).unwrap();
                pen += previous.map_or(0, |p| self.kern(p, c)) as i64;
                for y in 0..self.height {
                    for x in 0..glyph.width {
                        let tx = pen + x as i64;
                        let ty = row as u64 * line_height as u64 + y as u64;
                        if glyph.pixel(x, y) && tx >= 0 && (tx as u64) < width && ty < height {
                            keys[ty as usize * width as usize + tx as usize] = key;
                        }
                    }
                }
                pen += glyph.advance as i64;
                previous = Some(c);
            }
        }
        Ok(Sprite {
            name: String::new(),
            width: width as u32,
            height: height as u32,
            palette: palette.clone(),
            layers: vec![Layer {
                text: true,
                name: Some("text".into()),
                keys,
            }],
            slice: None,
        })
    }

    fn layout_width(&self, chars: &[char]) -> u64 {
        let mut pen = 0i64;
        let mut right = 0i64;
        let mut previous = None;
        for &c in chars {
            if let Some(glyph) = self.glyph(c) {
                pen += previous.map_or(0, |p| self.kern(p, c)) as i64;
                right = right.max(pen + glyph.width as i64);
                pen += glyph.advance as i64;
                right = right.max(pen);
                previous = Some(c);
            }
        }
        right.max(0) as u64
    }

    fn fitting_prefix(&self, chars: &[char], width: u64) -> usize {
        let mut pen = 0i64;
        let mut right = 0i64;
        let mut previous = None;
        for (i, &c) in chars.iter().enumerate() {
            let glyph = self.glyph(c).unwrap();
            pen += previous.map_or(0, |p| self.kern(p, c)) as i64;
            right = right.max(pen + glyph.width as i64);
            pen += glyph.advance as i64;
            right = right.max(pen);
            if right as u64 > width {
                return i;
            }
            previous = Some(c);
        }
        chars.len()
    }
}
