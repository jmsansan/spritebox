//! `texture NAME WxH`: seeded noise drawn with a ramp of palette keys.

use super::lex::{Node, Token};
use super::sprite::{Body, SpriteDef};
use super::{Header, Parsed, Parser, check_name, size};
use crate::sprite::Layer;
use crate::{Dither, ErrorCode, Noise, Palette, Sprite};

impl<'a> Parser<'a, '_> {
    pub(super) fn texture(&mut self, node: &Node<'a>) -> Parsed<()> {
        let mut header = Header::new(node, "texture")?;
        let size_token = *header.args.first().ok_or_else(|| {
            header
                .name
                .end_error("Expected the texture's size, such as `16x16`".to_owned())
        })?;
        if let Some(extra) = header.args.get(1) {
            return Err(extra.error(
                ErrorCode::Syntax,
                format!("Unexpected `{}` after the texture's size", extra.text),
            ));
        }
        let (width, height) = size(size_token, self.limits.max_sprite_side)?;
        let palette = header.option("palette");
        if let Some(palette) = palette {
            check_name(palette)?;
        }
        let seed = match header.option("seed") {
            Some(t) => t
                .text
                .parse::<u64>()
                .ok()
                .filter(|_| t.text.bytes().all(|b| b.is_ascii_digit()))
                .ok_or_else(|| {
                    t.error(
                        ErrorCode::InvalidArgument,
                        format!(
                            "A seed is a whole number 0..={}, not `{}`",
                            u64::MAX,
                            t.text
                        ),
                    )
                })?,
            None => 0,
        };
        let name = header.name;
        header.finish("texture", "`palette=` or `seed=`")?;

        let mut noise: Option<Noise> = None;
        let mut ramp: Option<Vec<Token<'a>>> = None;
        let mut dither: Option<Dither> = None;
        for child in &node.children {
            let tokens = &child.line.tokens;
            let keyword = tokens[0];
            if let Some(grandchild) = child.children.first() {
                return Err(grandchild
                    .line
                    .first()
                    .error(ErrorCode::Syntax, "Unexpected indented line"));
            }
            let duplicate = match keyword.text {
                "noise" => noise.is_some(),
                "ramp" => ramp.is_some(),
                "dither" => dither.is_some(),
                _ => false,
            };
            if duplicate {
                return Err(keyword.error(
                    ErrorCode::DuplicateName,
                    format!("`{}` is given twice", keyword.text),
                ));
            }
            match keyword.text {
                "noise" => {
                    let mut cell = None;
                    let mut octaves = 1;
                    let mut roughness = 50;
                    for &t in &tokens[1..] {
                        let (key, value) = t.text.split_once('=').ok_or_else(|| {
                            t.error(
                                ErrorCode::Syntax,
                                format!(
                                    "Expected `cell=`, `octaves=` or `roughness=`, not `{}`",
                                    t.text
                                ),
                            )
                        })?;
                        let value_token = Token {
                            text: value,
                            pos: t.at(key.chars().count() + 1),
                        };
                        let number = value
                            .parse::<u32>()
                            .ok()
                            .filter(|_| value.bytes().all(|b| b.is_ascii_digit()))
                            .ok_or_else(|| {
                                value_token.error(
                                    ErrorCode::Syntax,
                                    format!("Expected a whole number, not `{value}`"),
                                )
                            })?;
                        match key {
                            "cell" => cell = Some((number, value_token)),
                            "octaves" => octaves = number,
                            "roughness" => roughness = number,
                            _ => {
                                return Err(t.error(
                                    ErrorCode::Syntax,
                                    format!("Unknown noise option `{key}`; expected `cell=`, `octaves=` or `roughness=`"),
                                ));
                            }
                        }
                    }
                    let (cell, cell_token) = cell.ok_or_else(|| {
                        keyword.end_error(
                            "Noise needs `cell=N`, the pixels between its lattice points"
                                .to_owned(),
                        )
                    })?;
                    let candidate = Noise {
                        width,
                        height,
                        cell,
                        octaves,
                        roughness,
                        seed,
                    };
                    candidate
                        .check()
                        .map_err(|e| cell_token.error(e.code, e.message))?;
                    noise = Some(candidate);
                }
                "ramp" => {
                    if tokens.len() < 2 {
                        return Err(keyword.end_error(
                            "A ramp needs palette keys, from low noise to high".to_owned(),
                        ));
                    }
                    for t in &tokens[1..] {
                        if t.text.chars().count() != 1 {
                            return Err(t.error(
                                ErrorCode::Syntax,
                                format!(
                                    "A ramp lists palette keys one by one, with spaces, not `{}`",
                                    t.text
                                ),
                            ));
                        }
                    }
                    ramp = Some(tokens[1..].to_vec());
                }
                "dither" => {
                    let value = *tokens.get(1).ok_or_else(|| {
                        keyword
                            .end_error("Expected `none`, `bayer2`, `bayer4` or `bayer8`".to_owned())
                    })?;
                    if let Some(extra) = tokens.get(2) {
                        return Err(
                            extra.error(ErrorCode::Syntax, format!("Unexpected `{}`", extra.text))
                        );
                    }
                    dither = Some(match value.text {
                        "none" => Dither::None,
                        "bayer2" => Dither::Bayer(2),
                        "bayer4" => Dither::Bayer(4),
                        "bayer8" => Dither::Bayer(8),
                        other => {
                            return Err(value.error(
                                ErrorCode::Syntax,
                                format!(
                                    "Expected `none`, `bayer2`, `bayer4` or `bayer8`, not `{other}`"
                                ),
                            ));
                        }
                    });
                }
                other => {
                    return Err(keyword.error(
                        ErrorCode::Syntax,
                        format!(
                            "Unknown texture line `{other}`; expected `noise`, `ramp` or `dither`"
                        ),
                    ));
                }
            }
        }
        let noise = noise.ok_or_else(|| {
            name.error(
                ErrorCode::Syntax,
                format!("Texture `{}` needs a `noise` line", name.text),
            )
        })?;
        let ramp = ramp.ok_or_else(|| {
            name.error(
                ErrorCode::Syntax,
                format!("Texture `{}` needs a `ramp` line", name.text),
            )
        })?;
        self.sprites.push(SpriteDef {
            name,
            slice: None,
            body: Body::Texture {
                palette,
                noise,
                ramp,
                dither: dither.unwrap_or_default(),
            },
        });
        Ok(())
    }

    /// Draws a texture whose ramp keys are checked against its palette.
    pub(super) fn texture_sprite(
        &mut self,
        name: Token,
        palette: Option<Token>,
        noise: &Noise,
        ramp: &[Token],
        dither: Dither,
        palettes: &[Palette],
    ) -> Option<(Sprite, usize)> {
        let palette = self.sprite_palette(name, palette, palettes)?;
        let mut keys = Vec::with_capacity(ramp.len());
        for t in ramp {
            let key = t.text.chars().next().unwrap_or(' ');
            if palette.color(key).is_none() {
                self.errors.push(t.error(
                    ErrorCode::UnknownName,
                    format!("Key `{key}` is not in palette `{}`", palette.name),
                ));
                return None;
            }
            keys.push(key);
        }
        let values = noise.stretched();
        let mut pixels = Vec::with_capacity(values.len());
        for (i, &value) in values.iter().enumerate() {
            let (x, y) = (i as u32 % noise.width, i as u32 / noise.width);
            pixels.push(keys[dither.pick(value, x, y, keys.len())]);
        }
        Some((
            Sprite {
                name: name.text.to_owned(),
                width: noise.width,
                height: noise.height,
                palette: palette.clone(),
                layers: vec![Layer {
                    name: None,
                    keys: pixels,
                }],
                slice: None,
            },
            0,
        ))
    }
}
