//! The text format: parsing and validation into a [`Document`].

mod animation;
mod chain;
mod font;
mod lex;
mod sprite;
mod tiles;

use std::collections::HashMap;

use crate::{Color, Document, Error, ErrorCode, Limits, Palette, Position, Sprite};
use animation::AnimationDef;
use lex::{Node, Token};
use sprite::SpriteDef;
use tiles::{TilemapDef, TilesetDef};

/// The directives that start a definition.
const DIRECTIVES: &str = "`palette`, `sprite`, `animation`, `tileset`, `tilemap` or `font`";

/// Parses `text`. Returns the document, complete when there are no errors, and the
/// errors ordered by position.
pub(crate) fn parse(text: &str, limits: &Limits) -> (Document, Vec<Error>) {
    let mut errors = Vec::new();
    if text.len() > limits.max_text_bytes {
        errors.push(Error::at(
            Position::new(1, 1),
            ErrorCode::LimitExceeded,
            format!(
                "The text is {} bytes long, more than the limit of {}",
                text.len(),
                limits.max_text_bytes
            ),
        ));
        return (Document::default(), errors);
    }
    let nodes = lex::lex(text, &mut errors);
    let mut parser = Parser {
        limits,
        errors,
        palettes: Vec::new(),
        sprites: Vec::new(),
        animations: Vec::new(),
        tilesets: Vec::new(),
        tilemaps: Vec::new(),
        fonts: Vec::new(),
        failed_fonts: Vec::new(),
    };
    for (count, node) in nodes.iter().enumerate() {
        if count == limits.max_definitions {
            parser.errors.push(node.line.first().error(
                ErrorCode::LimitExceeded,
                format!(
                    "More than the limit of {} definitions",
                    limits.max_definitions
                ),
            ));
            break;
        }
        if let Err(e) = parser.definition(node) {
            parser.errors.push(e);
        }
    }
    let document = parser.resolve();
    let mut errors = parser.errors;
    errors.sort_by_key(|e| e.position);
    (document, errors)
}

type Parsed<T> = std::result::Result<T, Error>;

/// A palette as written, before `from=` is resolved.
struct PaletteDef<'a> {
    name: Token<'a>,
    from: Option<Token<'a>>,
    colors: Vec<(char, Color)>,
}

struct Parser<'a, 'l> {
    limits: &'l Limits,
    errors: Vec<Error>,
    palettes: Vec<PaletteDef<'a>>,
    sprites: Vec<SpriteDef<'a>>,
    animations: Vec<AnimationDef<'a>>,
    tilesets: Vec<TilesetDef<'a>>,
    tilemaps: Vec<TilemapDef<'a>>,
    fonts: Vec<(Token<'a>, crate::Font)>,
    /// Fonts with errors, which were reported, kept so their names are known.
    failed_fonts: Vec<Token<'a>>,
}

/// The words of a definition's first line after its keyword and name: positional
/// arguments and `key=value` options.
struct Header<'a> {
    name: Token<'a>,
    args: Vec<Token<'a>>,
    options: Vec<(Token<'a>, Token<'a>)>,
}

impl<'a> Header<'a> {
    fn new(node: &Node<'a>, what: &str) -> Parsed<Self> {
        let tokens = &node.line.tokens;
        let name = *tokens
            .get(1)
            .ok_or_else(|| tokens[0].end_error(format!("Expected a name for the {what}")))?;
        check_name(name)?;
        let mut args = Vec::new();
        let mut options = Vec::new();
        for &token in &tokens[2..] {
            match token.text.split_once('=') {
                Some((key, value)) if !key.is_empty() => {
                    let key_token = Token {
                        text: key,
                        pos: token.pos,
                    };
                    let value_token = Token {
                        text: value,
                        pos: token.at(key.chars().count() + 1),
                    };
                    if options.iter().any(|(k, _): &(Token, Token)| k.text == key) {
                        return Err(token
                            .error(ErrorCode::Syntax, format!("Option `{key}` is given twice")));
                    }
                    options.push((key_token, value_token));
                }
                _ => {
                    if !options.is_empty() {
                        return Err(token.error(
                            ErrorCode::Syntax,
                            format!("Unexpected `{}` after the options", token.text),
                        ));
                    }
                    args.push(token);
                }
            }
        }
        Ok(Self {
            name,
            args,
            options,
        })
    }

    /// Takes the value of option `key`; [`Header::finish`] rejects the ones not taken.
    fn option(&mut self, key: &str) -> Option<Token<'a>> {
        let index = self.options.iter().position(|(k, _)| k.text == key)?;
        Some(self.options.remove(index).1)
    }

    fn finish(self, what: &str, allowed: &str) -> Parsed<()> {
        if let Some((key, _)) = self.options.first() {
            return Err(key.error(
                ErrorCode::Syntax,
                format!(
                    "Unknown option `{}` for a {what}; expected {allowed}",
                    key.text
                ),
            ));
        }
        Ok(())
    }
}

impl Token<'_> {
    fn end_error(&self, message: String) -> Error {
        Error::at(self.end(), ErrorCode::Syntax, message)
    }
}

fn check_name(token: Token) -> Parsed<()> {
    let valid = token
        .text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid || token.text.is_empty() {
        return Err(token.error(
            ErrorCode::Syntax,
            format!(
                "Name `{}` may only use letters, digits, `_` and `-`",
                token.text
            ),
        ));
    }
    Ok(())
}

/// Characters that cannot be palette keys, besides spaces and control characters.
const RESERVED_KEYS: &[char] = &[';', '(', ')', ',', '='];

/// Parses `WxH`, such as `16x16`.
fn size(token: Token, max: u32) -> Parsed<(u32, u32)> {
    let invalid = || {
        token.error(
            ErrorCode::Syntax,
            format!("Expected a size such as `16x16`, not `{}`", token.text),
        )
    };
    let (w, h) = token.text.split_once('x').ok_or_else(invalid)?;
    let number = |text: &str, offset: usize| -> Parsed<u32> {
        if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid());
        }
        let value: u64 = text.parse().unwrap_or(u64::MAX);
        if value == 0 {
            return Err(Error::at(
                token.at(offset),
                ErrorCode::InvalidArgument,
                "A size must be at least 1",
            ));
        }
        if value > max as u64 {
            return Err(Error::at(
                token.at(offset),
                ErrorCode::LimitExceeded,
                format!("Size {value} is more than the limit of {max}"),
            ));
        }
        Ok(value as u32)
    };
    Ok((number(w, 0)?, number(h, w.len() + 1)?))
}

impl<'a> Parser<'a, '_> {
    fn definition(&mut self, node: &Node<'a>) -> Parsed<()> {
        match node.keyword() {
            "palette" => self.palette(node),
            "sprite" => self.sprite(node),
            "animation" => self.animation(node),
            "tileset" => self.tileset(node),
            "tilemap" => self.tilemap(node),
            "font" => self.font(node),
            other => Err(node.line.first().error(
                ErrorCode::Syntax,
                format!("Unknown directive `{other}`; expected {DIRECTIVES}"),
            )),
        }
    }

    fn palette(&mut self, node: &Node<'a>) -> Parsed<()> {
        let mut header = Header::new(node, "palette")?;
        if let Some(extra) = header.args.first() {
            return Err(extra.error(
                ErrorCode::Syntax,
                format!("Unexpected `{}` after the palette's name", extra.text),
            ));
        }
        let from = header.option("from");
        if let Some(from) = from {
            check_name(from)?;
        }
        let name = header.name;
        header.finish("palette", "`from=`")?;
        let mut colors: Vec<(char, Color)> = Vec::new();
        for child in &node.children {
            match palette_entry(child) {
                Ok((key, color)) => {
                    // A key with a bad color stays defined, so that its uses are not
                    // reported as unknown too.
                    let color = color.unwrap_or_else(|e| {
                        self.errors.push(e);
                        Color::TRANSPARENT
                    });
                    if colors.iter().any(|&(k, _)| k == key) {
                        self.errors.push(child.line.first().error(
                            ErrorCode::DuplicateName,
                            format!("Key `{key}` is defined twice in palette `{}`", name.text),
                        ));
                    } else {
                        colors.push((key, color));
                    }
                }
                Err(e) => self.errors.push(e),
            }
        }
        self.palettes.push(PaletteDef { name, from, colors });
        Ok(())
    }

    /// Resolves names and builds the document, recording errors.
    fn resolve(&mut self) -> Document {
        let palettes = self.resolve_palettes();
        let sprites = self.resolve_sprites(&palettes);
        let mut budget = self.limits.max_image_pixels;
        let sprites = self.spend(sprites, &mut budget);
        let (animations, frame_sprites) = self.resolve_animations(&palettes, &sprites);
        let frame_sprites = self.spend(frame_sprites, &mut budget);
        let tilesets = self.resolve_tilesets(&palettes, &sprites);
        let tilemaps = self.resolve_tilemaps(&tilesets);
        let fonts = self.resolve_fonts();
        Document {
            palettes,
            sprites,
            frame_sprites,
            animations,
            tilesets,
            tilemaps,
            fonts,
        }
    }

    /// Keeps the sprites whose pixels fit in `budget`, and reports the first that does
    /// not.
    fn spend(&mut self, sprites: Vec<(Sprite, Position)>, budget: &mut u64) -> Vec<Sprite> {
        let mut kept = Vec::with_capacity(sprites.len());
        for (sprite, pos) in sprites {
            let pixels = sprite.width as u64 * sprite.height as u64 * sprite.layers.len() as u64;
            if pixels > *budget {
                self.errors.push(Error::at(
                    pos,
                    ErrorCode::LimitExceeded,
                    format!(
                        "The sprites have more than the limit of {} pixels in all",
                        self.limits.max_image_pixels
                    ),
                ));
                *budget = 0;
                break;
            }
            *budget -= pixels;
            kept.push(sprite);
        }
        kept
    }

    /// Resolves `from=`: a palette starts with the colors of the one it names, then
    /// replaces or adds its own.
    fn resolve_palettes(&mut self) -> Vec<Palette> {
        let mut index: HashMap<&str, usize> = HashMap::new();
        let mut usable = vec![true; self.palettes.len()];
        for (i, def) in self.palettes.iter().enumerate() {
            if let Some(&first) = index.get(def.name.text) {
                self.errors.push(duplicate(
                    def.name,
                    "Palette",
                    self.palettes[first].name.pos,
                ));
                usable[i] = false;
            } else {
                index.insert(def.name.text, i);
            }
        }
        let mut resolved: Vec<Option<Palette>> = vec![None; self.palettes.len()];
        // How many `from=` steps lead from each resolved palette to one without.
        let mut depth = vec![0usize; self.palettes.len()];
        for start in 0..self.palettes.len() {
            if !usable[start] || resolved[start].is_some() {
                continue;
            }
            // Follow `from=` to a palette without one, or to one already resolved.
            let mut chain = vec![start];
            let mut failed = false;
            loop {
                let current = &self.palettes[*chain.last().unwrap_or(&start)];
                let Some(from) = current.from else { break };
                let Some(&next) = index.get(from.text) else {
                    self.errors.push(from.error(
                        ErrorCode::UnknownName,
                        format!("Unknown palette `{}`", from.text),
                    ));
                    failed = true;
                    break;
                };
                if chain.contains(&next) {
                    self.errors.push(from.error(
                        ErrorCode::Cycle,
                        format!(
                            "Palette `{}` is copied from itself through `from=`",
                            current.name.text
                        ),
                    ));
                    failed = true;
                    break;
                }
                if resolved[next].is_some() || !usable[next] {
                    chain.push(next);
                    break;
                }
                chain.push(next);
            }
            let last = chain[chain.len() - 1];
            if failed || (!usable[last] && resolved[last].is_none()) {
                for &i in &chain {
                    usable[i] = false;
                }
                continue;
            }
            // Build from the base up.
            let mut base = resolved[last].clone();
            let unresolved = if base.is_some() {
                chain.len() - 1
            } else {
                chain.len()
            };
            let base_level = if base.is_some() { depth[last] + 1 } else { 0 };
            for (n, &i) in chain[..unresolved].iter().enumerate().rev() {
                let def = &self.palettes[i];
                let level = base_level + (unresolved - 1 - n);
                if level > self.limits.max_nesting {
                    let from = def.from.unwrap_or(def.name);
                    self.errors.push(from.error(
                        ErrorCode::LimitExceeded,
                        format!(
                            "Palettes copied from palettes more than the limit of {} deep",
                            self.limits.max_nesting
                        ),
                    ));
                    for &j in &chain[..=n] {
                        usable[j] = false;
                    }
                    break;
                }
                let mut colors = base.take().map(|p| p.colors).unwrap_or_default();
                for &(key, color) in &def.colors {
                    match colors.iter_mut().find(|(k, _)| *k == key) {
                        Some(entry) => entry.1 = color,
                        None => colors.push((key, color)),
                    }
                }
                if colors.len() > self.limits.max_palette_colors {
                    self.errors.push(def.name.error(
                        ErrorCode::LimitExceeded,
                        format!(
                            "Palette `{}` has {} keys, more than the limit of {}",
                            def.name.text,
                            colors.len(),
                            self.limits.max_palette_colors
                        ),
                    ));
                    for &j in &chain[..=n] {
                        usable[j] = false;
                    }
                    break;
                }
                let palette = Palette {
                    name: def.name.text.to_owned(),
                    colors,
                };
                resolved[i] = Some(palette.clone());
                depth[i] = level;
                base = Some(palette);
            }
        }
        resolved.into_iter().flatten().collect()
    }
}

fn duplicate(name: Token, what: &str, first: Position) -> Error {
    name.error(
        ErrorCode::DuplicateName,
        format!(
            "{what} `{}` is already defined on line {}",
            name.text, first.line
        ),
    )
}

/// A palette line's key, and its color or the error in the color.
fn palette_entry(node: &Node) -> Parsed<(char, Parsed<Color>)> {
    let tokens = &node.line.tokens;
    let key_token = tokens[0];
    if let Some(child) = node.children.first() {
        return Err(child.line.first().error(
            ErrorCode::Syntax,
            "Unexpected indented line under a palette color",
        ));
    }
    let mut chars = key_token.text.chars();
    let key = chars.next().unwrap_or(' ');
    if chars.next().is_some() {
        return Err(key_token.error(
            ErrorCode::Syntax,
            format!(
                "A palette key is one character, not `{}`; write `KEY COLOR`",
                key_token.text
            ),
        ));
    }
    if key.is_control() || RESERVED_KEYS.contains(&key) {
        return Err(key_token.error(
            ErrorCode::InvalidArgument,
            format!("`{key}` cannot be a palette key"),
        ));
    }
    let color_token = *tokens
        .get(1)
        .ok_or_else(|| key_token.end_error(format!("Expected a color after key `{key}`")))?;
    if let Some(extra) = tokens.get(2) {
        return Err(extra.error(
            ErrorCode::Syntax,
            format!("Unexpected `{}` after the color", extra.text),
        ));
    }
    let color = color_token
        .text
        .parse()
        .map_err(|e: Error| color_token.error(e.code, e.message));
    Ok((key, color))
}
