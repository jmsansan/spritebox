//! Pixel fonts: `font NAME height=H` and a `glyph` for each character.

use std::collections::HashMap;

use super::lex::{Node, Token};
use super::{Header, Parsed, Parser, duplicate};
use crate::font::{Font, Glyph};
use crate::{Error, ErrorCode, Position};

impl<'a> Parser<'a, '_> {
    pub(super) fn font(&mut self, node: &Node<'a>) -> Parsed<()> {
        let mut header = Header::new(node, "font")?;
        if let Some(extra) = header.args.first() {
            return Err(extra.error(
                ErrorCode::Syntax,
                format!(
                    "Unexpected `{}`; write the font's metrics as `height=7`",
                    extra.text
                ),
            ));
        }
        let max = self.limits.max_sprite_side;
        let height_token = header.option("height").ok_or_else(|| {
            header
                .name
                .end_error("A font needs `height=H`, the rows of its glyphs".to_owned())
        })?;
        let height = number(height_token, 1, max, "Height")?;
        let baseline = match header.option("baseline") {
            Some(t) => number(t, 0, height, "The baseline")?,
            None => height,
        };
        let line_height = match header.option("line_height") {
            Some(t) => number(t, 1, max, "The line height")?,
            None => height + 1,
        };
        let spacing = match header.option("spacing") {
            Some(t) => number(t, 0, max, "Spacing")?,
            None => 1,
        };
        let space = match header.option("space") {
            Some(t) => Some(number(t, 0, max, "The space's advance")?),
            None => None,
        };
        let name = header.name;
        header.finish(
            "font",
            "`height=`, `baseline=`, `line_height=`, `spacing=` or `space=`",
        )?;

        let mut glyphs: Vec<Glyph> = Vec::new();
        let mut seen: HashMap<char, Position> = HashMap::new();
        let mut complete = true;
        for (i, child) in node.children.iter().enumerate() {
            if i == self.limits.max_glyphs {
                self.errors.push(child.line.first().error(
                    ErrorCode::LimitExceeded,
                    format!("More than the limit of {} glyphs", self.limits.max_glyphs),
                ));
                complete = false;
                break;
            }
            match self.glyph(child, height, spacing) {
                Ok((glyph, token)) => {
                    if let Some(&first) = seen.get(&glyph.ch) {
                        self.errors.push(duplicate(token, "Glyph", first));
                        complete = false;
                    } else {
                        seen.insert(glyph.ch, token.pos);
                        glyphs.push(glyph);
                    }
                }
                Err(e) => {
                    self.errors.push(e);
                    complete = false;
                }
            }
        }
        if !glyphs.iter().any(|g| g.ch == ' ') {
            glyphs.push(Glyph {
                ch: ' ',
                width: 0,
                advance: space.unwrap_or(height.div_ceil(2)),
                pixels: Vec::new(),
            });
        } else if let Some(space) = space {
            self.errors.push(Error::at(
                name.pos,
                ErrorCode::DuplicateName,
                format!(
                    "Font `{}` has a glyph for the space, so `space={space}` is not needed",
                    name.text
                ),
            ));
            complete = false;
        }
        if complete {
            self.fonts.push((
                name,
                Font {
                    name: name.text.to_owned(),
                    height,
                    baseline,
                    line_height,
                    glyphs,
                },
            ));
        } else {
            self.failed_fonts.push(name);
        }
        Ok(())
    }

    /// `glyph CHAR [advance=N]` and its rows of `#` and `.`.
    fn glyph(&mut self, node: &Node<'a>, height: u32, spacing: u32) -> Parsed<(Glyph, Token<'a>)> {
        let tokens = &node.line.tokens;
        if tokens[0].text != "glyph" {
            return Err(tokens[0].error(
                ErrorCode::Syntax,
                format!("Unknown font entry `{}`; expected `glyph`", tokens[0].text),
            ));
        }
        let token = *tokens.get(1).ok_or_else(|| {
            tokens[0]
                .end_error("Expected the glyph's character, such as `A` or `U+0041`".to_owned())
        })?;
        let ch = character(token)?;
        let mut advance = None;
        for &extra in &tokens[2..] {
            match extra.text.strip_prefix("advance=") {
                Some(value) if advance.is_none() => {
                    let value_token = Token {
                        text: value,
                        pos: extra.at("advance=".len()),
                    };
                    advance = Some(number(
                        value_token,
                        0,
                        self.limits.max_sprite_side,
                        "Advance",
                    )?);
                }
                _ => {
                    return Err(extra.error(
                        ErrorCode::Syntax,
                        format!("Unexpected `{}`; expected `advance=N`", extra.text),
                    ));
                }
            }
        }
        let what = format!("glyph `{}`", token.text);
        let mut width = None;
        let mut pixels = Vec::new();
        for child in &node.children {
            let row = child.line.first();
            if child.line.tokens.len() > 1 || !child.children.is_empty() {
                return Err(row.error(
                    ErrorCode::Syntax,
                    "A row of a glyph is one word of `#` and `.`",
                ));
            }
            if let Some((x, c)) = row
                .text
                .chars()
                .enumerate()
                .find(|&(_, c)| c != '#' && c != '.')
            {
                return Err(Error::at(
                    row.at(x),
                    ErrorCode::Syntax,
                    format!("A glyph's pixels are `#` (ink) or `.` (none), not `{c}`"),
                ));
            }
            let count = row.text.chars().count() as u32;
            match width {
                None => width = Some(count),
                Some(w) if w != count => {
                    let pos = if count < w {
                        row.end()
                    } else {
                        row.at(w as usize)
                    };
                    return Err(Error::at(
                        pos,
                        ErrorCode::SizeMismatch,
                        format!("Row has {count} pixels; the first row of {what} has {w}"),
                    ));
                }
                Some(_) => {}
            }
            pixels.extend(row.text.chars().map(|c| c == '#'));
        }
        let rows = node.children.len() as u32;
        if rows != 0 && rows != height {
            let pos = match node.children.get(height as usize) {
                Some(extra) => extra.line.first().pos,
                None => token.end(),
            };
            return Err(Error::at(
                pos,
                ErrorCode::SizeMismatch,
                format!(
                    "{} has {rows} rows; the font's height is {height}",
                    capitalize(&what)
                ),
            ));
        }
        if width.is_some_and(|w| w > self.limits.max_sprite_side) {
            return Err(token.error(
                ErrorCode::LimitExceeded,
                format!(
                    "{} is wider than the limit of {}",
                    capitalize(&what),
                    self.limits.max_sprite_side
                ),
            ));
        }
        let width = width.unwrap_or(0);
        Ok((
            Glyph {
                ch,
                width,
                advance: advance.unwrap_or(if width == 0 { spacing } else { width + spacing }),
                pixels,
            },
            token,
        ))
    }

    pub(super) fn resolve_fonts(&mut self) -> Vec<Font> {
        let mut out: Vec<Font> = Vec::new();
        let mut seen: HashMap<&str, Position> = HashMap::new();
        for name in &self.failed_fonts {
            seen.entry(name.text).or_insert(name.pos);
        }
        let mut names: Vec<(Token, Font)> = std::mem::take(&mut self.fonts);
        names.sort_by_key(|(t, _)| t.pos);
        for (token, font) in names {
            if let Some(&first) = seen.get(token.text) {
                if first != token.pos {
                    self.errors.push(duplicate(token, "Font", first));
                }
                continue;
            }
            seen.insert(token.text, token.pos);
            out.push(font);
        }
        out
    }
}

/// A glyph's character: itself, or `U+` and 4 to 6 hexadecimal digits.
fn character(token: Token) -> Parsed<char> {
    let mut chars = token.text.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Ok(c);
    }
    let invalid = || {
        token.error(
            ErrorCode::Syntax,
            format!(
                "Expected one character or `U+` and its code, such as `U+003B` for `;`, not `{}`",
                token.text
            ),
        )
    };
    let hex = token.text.strip_prefix("U+").ok_or_else(invalid)?;
    if !(4..=6).contains(&hex.len()) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(invalid());
    }
    u32::from_str_radix(hex, 16)
        .ok()
        .and_then(char::from_u32)
        .ok_or_else(|| {
            token.error(
                ErrorCode::InvalidArgument,
                format!("{} is not a character", token.text),
            )
        })
}

fn number(token: Token, min: u32, max: u32, what: &str) -> Parsed<u32> {
    if token.text.is_empty() || !token.text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(token.error(
            ErrorCode::Syntax,
            format!("Expected a whole number, not `{}`", token.text),
        ));
    }
    match token.text.parse::<u64>() {
        Ok(v) if (min as u64..=max as u64).contains(&v) => Ok(v as u32),
        _ => Err(token.error(
            ErrorCode::InvalidArgument,
            format!("{what} must be {min}..={max}, not {}", token.text),
        )),
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
