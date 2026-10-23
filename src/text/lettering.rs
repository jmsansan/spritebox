use super::lex::{Node, Token};
use super::sprite::{Body, SpriteDef};
use super::{Header, Parsed, Parser};
use crate::{Align, Error, ErrorCode, Font, Palette, Position, Sprite, TextOptions};

pub(super) struct TextDef<'a> {
    pub font: Token<'a>,
    pub palette: Option<Token<'a>>,
    pub color: Token<'a>,
    pub options: TextOptions,
    pub lines: Vec<Vec<(char, Position)>>,
}

impl<'a> Parser<'a, '_> {
    pub(super) fn text_sprite(&mut self, node: &Node<'a>) -> Parsed<()> {
        let mut header = Header::new(node, "text")?;
        if let Some(t) = header.args.first() {
            return Err(t.error(ErrorCode::Syntax, "Expected font= and color="));
        }
        let font = header
            .option("font")
            .ok_or_else(|| header.name.error(ErrorCode::Syntax, "Text needs font="))?;
        let color = header
            .option("color")
            .ok_or_else(|| header.name.error(ErrorCode::Syntax, "Text needs color="))?;
        let palette = header.option("palette");
        let number = |t: Token| -> Parsed<u32> {
            t.text
                .parse::<u32>()
                .ok()
                .filter(|v| *v > 0 && *v <= self.limits.max_sprite_side)
                .ok_or_else(|| {
                    t.error(
                        ErrorCode::InvalidArgument,
                        "Text size must be positive and within sprite limits",
                    )
                })
        };
        let width = header.option("width").map(number).transpose()?;
        let line_height = header.option("line_height").map(number).transpose()?;
        let align = match header.option("align") {
            None => Align::Left,
            Some(t) => match t.text {
                "left" => Align::Left,
                "center" => Align::Center,
                "right" => Align::Right,
                _ => {
                    return Err(
                        t.error(ErrorCode::InvalidArgument, "Expected left, center or right")
                    );
                }
            },
        };
        let name = header.name;
        let lint = header.option("lint");
        header.finish(
            "text",
            "font=, color=, palette=, width=, align=, line_height= or lint=",
        )?;
        self.lint_option(crate::lint::Kind::Sprite, name, lint)?;
        let indent = node
            .children
            .iter()
            .filter(|c| !c.line.raw.trim().is_empty())
            .map(|c| c.line.indent)
            .min()
            .ok_or_else(|| name.error(ErrorCode::Syntax, "Text needs an indented line"))?;
        let mut lines = Vec::new();
        for child in &node.children {
            let raw = child.line.raw.get(indent..).unwrap_or("").trim_end();
            let mut line = Vec::new();
            let mut offset = 0;
            for word in raw.split_inclusive(' ') {
                let token = word.trim_end_matches(' ');
                let escaped = token.strip_prefix("U+").filter(|hex| {
                    (4..=6).contains(&hex.len()) && hex.bytes().all(|b| b.is_ascii_hexdigit())
                });
                if let Some(hex) = escaped {
                    let c = u32::from_str_radix(hex, 16)
                        .ok()
                        .and_then(char::from_u32)
                        .ok_or_else(|| {
                            Error::at(
                                Position::in_file(
                                    name.pos.file,
                                    child.line.first().pos.line,
                                    indent + offset + 1,
                                ),
                                ErrorCode::InvalidArgument,
                                "Invalid text character",
                            )
                        })?;
                    line.push((
                        c,
                        Position::in_file(
                            name.pos.file,
                            child.line.first().pos.line,
                            indent + offset + 1,
                        ),
                    ));
                    for (i, _) in word[token.len()..].chars().enumerate() {
                        line.push((
                            ' ',
                            Position::in_file(
                                name.pos.file,
                                child.line.first().pos.line,
                                indent + offset + token.chars().count() + i + 1,
                            ),
                        ));
                    }
                } else {
                    for (i, c) in word.chars().enumerate() {
                        line.push((
                            c,
                            Position::in_file(
                                name.pos.file,
                                child.line.first().pos.line,
                                indent + offset + i + 1,
                            ),
                        ));
                    }
                }
                offset += word.chars().count();
            }
            lines.push(line);
        }
        self.sprites.push(SpriteDef {
            name,
            slice: None,
            body: Body::Text(TextDef {
                font,
                palette,
                color,
                options: TextOptions {
                    width,
                    align,
                    line_height,
                },
                lines,
            }),
        });
        Ok(())
    }

    pub(super) fn build_text(
        &mut self,
        name: Token,
        def: &TextDef,
        fonts: &[Font],
        palettes: &[Palette],
    ) -> Option<(Sprite, usize)> {
        let font = match fonts.iter().find(|f| f.name() == def.font.text) {
            Some(f) => f,
            None => {
                self.errors
                    .push(def.font.error(ErrorCode::UnknownName, "Unknown text font"));
                return None;
            }
        };
        let palette = self.sprite_palette(name, def.palette, palettes)?;
        let mut color = def.color.text.chars();
        let key = color.next().unwrap_or(' ');
        if color.next().is_some() || palette.color(key).is_none() {
            self.errors.push(
                def.color
                    .error(ErrorCode::UnknownName, "Unknown text palette key"),
            );
            return None;
        }
        for line in &def.lines {
            for &(c, pos) in line {
                if font.glyph(c).is_none() {
                    self.errors.push(Error::at(
                        pos,
                        ErrorCode::UnknownName,
                        format!("Font `{}` has no glyph for `{c}`", font.name()),
                    ));
                    return None;
                }
            }
        }
        let text = def
            .lines
            .iter()
            .map(|l| l.iter().map(|(c, _)| c).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        match font.sprite_with(&text, palette, key, &def.options, self.limits) {
            Ok(mut sprite) => {
                sprite.name = name.text.to_owned();
                Some((sprite, 0))
            }
            Err(e) => {
                self.errors.push(name.error(e.code, e.message));
                None
            }
        }
    }
}
