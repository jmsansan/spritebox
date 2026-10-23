//! Modifier chains: a sprite's name followed by `:modifier`s, such as
//! `coin:flip_x:swap(dusk):outline(k)`.

use super::lex::Token;
use super::{Parsed, RESERVED_KEYS, check_name, check_sprite_name};
use crate::{Error, ErrorCode, Fill, Limits, Palette, Shadow, Sprite};

/// The modifiers, for error messages.
pub(crate) const MODIFIERS: &str = "`flip_x`, `flip_y`, `rot90`, `rot180`, `rot270`, \
    `offset(dx,dy)`, `pad(n)`, `swap(palette)`, `outline(key)`, `size(WxH)` or `shadow(key,dx,dy)`";

#[derive(Clone, Debug)]
pub(crate) enum Op<'a> {
    FlipX,
    FlipY,
    /// Clockwise quarter turns, 1 to 3.
    Rotate(u32),
    Offset(i32, i32),
    Pad(u32),
    Swap(Token<'a>),
    Outline(char),
    Size(u32, u32, Fill),
    Shadow(char, i32, i32, Shadow),
}

#[derive(Clone, Debug)]
pub(crate) struct Chain<'a> {
    pub base: Token<'a>,
    /// Each modifier with the token of its text, for errors.
    pub ops: Vec<(Token<'a>, Op<'a>)>,
}

impl<'a> Chain<'a> {
    /// The chain written without spaces, which names the sprite it builds.
    pub fn canonical(&self) -> String {
        let mut name = self.base.text.to_owned();
        for (_, op) in &self.ops {
            name.push(':');
            match op {
                Op::FlipX => name.push_str("flip_x"),
                Op::FlipY => name.push_str("flip_y"),
                Op::Rotate(q) => name.push_str(&format!("rot{}", q * 90)),
                Op::Offset(dx, dy) => name.push_str(&format!("offset({dx},{dy})")),
                Op::Pad(n) => name.push_str(&format!("pad({n})")),
                Op::Swap(p) => name.push_str(&format!("swap({})", p.text)),
                Op::Outline(k) => name.push_str(&format!("outline({k})")),
                Op::Size(w, h, fill) => name.push_str(&format!(
                    "size({w}x{h}{})",
                    if *fill == Fill::Tile { ",tile" } else { "" }
                )),
                Op::Shadow(k, x, y, mode) => name.push_str(&format!(
                    "shadow({k},{x},{y}{})",
                    if *mode == Shadow::Long { ",long" } else { "" }
                )),
            }
        }
        name
    }

    /// The chain without its trailing `flip_x`, `flip_y` and `rot180`, and whether
    /// those flip the result horizontally and vertically (`rot180` flips both). An
    /// engine flips a sprite when it draws it, so frames state the flips instead of
    /// storing flipped pixels.
    pub fn split_flips(&self) -> (Chain<'a>, bool, bool) {
        let mut chain = self.clone();
        let (mut flip_x, mut flip_y) = (false, false);
        while let Some((_, op)) = chain.ops.last() {
            match op {
                Op::FlipX => flip_x = !flip_x,
                Op::FlipY => flip_y = !flip_y,
                Op::Rotate(2) => {
                    flip_x = !flip_x;
                    flip_y = !flip_y;
                }
                _ => break,
            }
            chain.ops.pop();
        }
        (chain, flip_x, flip_y)
    }

    /// Applies the modifiers to `sprite`, the chain's base. Before each modifier, adds
    /// to `uses` the sprite's palette and keys then: every key that a step can depend
    /// on, though a later step may swap or move it out of the result.
    pub fn apply(
        &self,
        sprite: &Sprite,
        palettes: &[Palette],
        limits: &Limits,
        uses: &mut PaletteUses,
    ) -> Parsed<Sprite> {
        sprite
            .effect_size(sprite.width, sprite.height, sprite.layers.len(), limits)
            .map_err(|e| self.base.error(e.code, e.message))?;
        let mut sprite = sprite.clone();
        for (token, op) in &self.ops {
            let (mut w, mut h, mut layers) = (sprite.width, sprite.height, sprite.layers.len());
            match op {
                Op::Outline(_) => layers += 1,
                Op::Pad(n) => {
                    let twice = n
                        .checked_mul(2)
                        .ok_or_else(|| token.error(ErrorCode::LimitExceeded, "Padding overflow"))?;
                    w = w.checked_add(twice).ok_or_else(|| {
                        token.error(ErrorCode::LimitExceeded, "Padding width overflow")
                    })?;
                    h = h.checked_add(twice).ok_or_else(|| {
                        token.error(ErrorCode::LimitExceeded, "Padding height overflow")
                    })?;
                }
                Op::Size(width, height, _) => {
                    w = *width;
                    h = *height;
                }
                Op::Shadow(_, x, y, _) => {
                    w = w.checked_add(x.unsigned_abs()).ok_or_else(|| {
                        token.error(ErrorCode::LimitExceeded, "Shadow width overflow")
                    })?;
                    h = h.checked_add(y.unsigned_abs()).ok_or_else(|| {
                        token.error(ErrorCode::LimitExceeded, "Shadow height overflow")
                    })?;
                    layers += 1;
                }
                _ => {}
            }
            sprite
                .effect_size(w, h, layers, limits)
                .map_err(|e| token.error(e.code, e.message))?;
            for key in sprite.keys() {
                uses.insert((sprite.palette.name.clone(), key));
            }
            sprite = match op {
                Op::FlipX => sprite.flip_x(),
                Op::FlipY => sprite.flip_y(),
                Op::Rotate(q) => sprite.rotate(*q),
                Op::Offset(dx, dy) => sprite.offset(*dx, *dy),
                Op::Pad(n) => sprite
                    .pad_within(*n, limits.max_sprite_side)
                    .map_err(|e| token.error(e.code, e.message))?,
                Op::Swap(name) => {
                    let palette =
                        palettes
                            .iter()
                            .find(|p| p.name == name.text)
                            .ok_or_else(|| {
                                name.error(
                                    ErrorCode::UnknownName,
                                    format!("Unknown palette `{}`", name.text),
                                )
                            })?;
                    sprite
                        .with_palette(palette)
                        .map_err(|e| name.error(e.code, e.message))?
                }
                Op::Outline(key) => sprite
                    .outline(*key)
                    .map_err(|e| token.error(e.code, e.message))?,
                Op::Size(w, h, fill) => sprite
                    .resize_with(*w, *h, *fill, limits)
                    .map_err(|e| token.error(e.code, e.message))?,
                Op::Shadow(k, x, y, mode) => sprite
                    .shadow_with(*k, *x, *y, *mode, limits)
                    .map_err(|e| token.error(e.code, e.message))?,
            };
        }
        Ok(sprite)
    }
}

/// Palettes and the keys that sprites used from them midway through chains.
pub(crate) type PaletteUses = std::collections::BTreeSet<(String, char)>;

/// Parses a chain from one word.
pub(crate) fn parse<'a>(token: Token<'a>, limits: &Limits) -> Parsed<Chain<'a>> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut start_chars = 0;
    for (chars, (i, c)) in token.text.char_indices().enumerate() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ':' if depth == 0 => {
                parts.push(Token {
                    text: &token.text[start..i],
                    pos: token.at(start_chars),
                });
                start = i + 1;
                start_chars = chars + 1;
            }
            _ => {}
        }
    }
    parts.push(Token {
        text: &token.text[start..],
        pos: token.at(start_chars),
    });
    let base = parts[0];
    if base.text.is_empty() {
        return Err(base.error(ErrorCode::Syntax, "Expected a sprite's name"));
    }
    check_sprite_name(base)?;
    let mut ops = Vec::new();
    for &part in &parts[1..] {
        ops.push((part, modifier(part, limits)?));
    }
    Ok(Chain { base, ops })
}

fn modifier<'a>(part: Token<'a>, limits: &Limits) -> Parsed<Op<'a>> {
    if part.text.is_empty() {
        return Err(part.error(
            ErrorCode::Syntax,
            format!("Expected a modifier after `:`: {MODIFIERS}"),
        ));
    }
    let (name, args) = match part.text.find('(') {
        Some(open) => {
            if !part.text.ends_with(')') {
                return Err(Error::at(
                    part.end(),
                    ErrorCode::Syntax,
                    format!("Expected `)` to close `{}(`", &part.text[..open]),
                ));
            }
            let inner = &part.text[open + 1..part.text.len() - 1];
            let offset = part.text[..open + 1].chars().count();
            (&part.text[..open], Some(arguments(part, inner, offset)))
        }
        None => (part.text, None),
    };
    let no_args = |op: Op<'a>| -> Parsed<Op<'a>> {
        match &args {
            Some(_) => Err(part.error(ErrorCode::Syntax, format!("`{name}` takes no arguments"))),
            None => Ok(op),
        }
    };
    let expect = |count: usize, usage: &str| -> Parsed<Vec<Token<'a>>> {
        match &args {
            Some(list) if list.len() == count && list.iter().all(|a| !a.text.is_empty()) => {
                Ok(list.clone())
            }
            _ => Err(part.error(ErrorCode::Syntax, format!("Expected `{usage}`"))),
        }
    };
    match name {
        "flip_x" => no_args(Op::FlipX),
        "flip_y" => no_args(Op::FlipY),
        "rot90" => no_args(Op::Rotate(1)),
        "rot180" => no_args(Op::Rotate(2)),
        "rot270" => no_args(Op::Rotate(3)),
        "offset" => {
            let a = expect(2, "offset(dx,dy)")?;
            let max = limits.max_sprite_side as i64;
            Ok(Op::Offset(
                integer(a[0], -max, max)? as i32,
                integer(a[1], -max, max)? as i32,
            ))
        }
        "pad" => {
            let a = expect(1, "pad(n)")?;
            Ok(Op::Pad(
                integer(a[0], 1, limits.max_sprite_side as i64)? as u32
            ))
        }
        "swap" => {
            let a = expect(1, "swap(palette)")?;
            check_name(a[0])?;
            Ok(Op::Swap(a[0]))
        }
        "outline" => {
            let a = expect(1, "outline(key)")?;
            let mut chars = a[0].text.chars();
            let key = chars.next().unwrap_or(' ');
            if chars.next().is_some() || RESERVED_KEYS.contains(&key) {
                return Err(a[0].error(
                    ErrorCode::Syntax,
                    format!("Expected one palette key, not `{}`", a[0].text),
                ));
            }
            Ok(Op::Outline(key))
        }
        "size" => {
            let a = args
                .as_ref()
                .ok_or_else(|| part.error(ErrorCode::Syntax, "Expected `size(WxH[,tile])`"))?;
            if a.is_empty() || a.len() > 2 {
                return Err(part.error(ErrorCode::Syntax, "Expected `size(WxH[,tile])`"));
            }
            let (w, h) = super::size(a[0], limits.max_sprite_side)?;
            let fill = match a.get(1).map(|t| t.text) {
                None | Some("stretch") => Fill::Stretch,
                Some("tile") => Fill::Tile,
                _ => return Err(a[1].error(ErrorCode::InvalidArgument, "Expected stretch or tile")),
            };
            Ok(Op::Size(w, h, fill))
        }
        "shadow" => {
            let a = args.as_ref().ok_or_else(|| {
                part.error(
                    ErrorCode::Syntax,
                    "Expected `shadow(key,dx,dy[,hard|long])`",
                )
            })?;
            if !(3..=4).contains(&a.len()) {
                return Err(part.error(
                    ErrorCode::Syntax,
                    "Expected `shadow(key,dx,dy[,hard|long])`",
                ));
            }
            let mut chars = a[0].text.chars();
            let key = chars
                .next()
                .ok_or_else(|| a[0].error(ErrorCode::Syntax, "Expected one palette key"))?;
            if chars.next().is_some() || RESERVED_KEYS.contains(&key) {
                return Err(a[0].error(ErrorCode::Syntax, "Expected one palette key"));
            }
            let mode = match a.get(3).map(|t| t.text) {
                None | Some("hard") => Shadow::Hard,
                Some("long") => Shadow::Long,
                _ => return Err(a[3].error(ErrorCode::InvalidArgument, "Expected hard or long")),
            };
            Ok(Op::Shadow(
                key,
                integer(a[1], -8, 8)? as i32,
                integer(a[2], -8, 8)? as i32,
                mode,
            ))
        }
        _ => Err(part.error(
            ErrorCode::Syntax,
            format!("Unknown modifier `{name}`; expected {MODIFIERS}"),
        )),
    }
}

/// Splits `inner`, which starts `offset` characters into `part`, at commas, and trims
/// each argument.
fn arguments<'a>(part: Token<'a>, inner: &'a str, offset: usize) -> Vec<Token<'a>> {
    let mut out = Vec::new();
    let mut chars = offset;
    for piece in inner.split(',') {
        let leading = piece.len() - piece.trim_start().len();
        let text = piece.trim();
        out.push(Token {
            text,
            pos: part.at(chars + piece[..leading].chars().count()),
        });
        chars += piece.chars().count() + 1;
    }
    out
}

fn integer(token: Token, min: i64, max: i64) -> Parsed<i64> {
    let digits = token.text.strip_prefix('-').unwrap_or(token.text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(token.error(
            ErrorCode::Syntax,
            format!("Expected a whole number, not `{}`", token.text),
        ));
    }
    match token.text.parse::<i64>() {
        Ok(v) if (min..=max).contains(&v) => Ok(v),
        _ => Err(token.error(
            ErrorCode::InvalidArgument,
            format!("`{}` must be {min}..={max}", token.text),
        )),
    }
}
