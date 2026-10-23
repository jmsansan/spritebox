use super::chain::{self, Chain};
use super::lex::{Node, Token};
use super::{Header, Parsed, Parser};
use crate::{ErrorCode, Palette, Screen, ScreenItem, Sprite};
pub(super) struct ScreenDef<'a> {
    pub name: Token<'a>,
    width: u32,
    height: u32,
    palette: Option<Token<'a>>,
    background: Option<Token<'a>>,
    grid: Option<u32>,
    items: Vec<(Chain<'a>, i32, i32)>,
}

impl<'a> Parser<'a, '_> {
    pub(super) fn screen(&mut self, node: &Node<'a>) -> Parsed<()> {
        let mut header = Header::new(node, "screen")?;
        if header.args.len() != 1 {
            return Err(header
                .name
                .error(ErrorCode::Syntax, "Expected `screen NAME WxH`"));
        }
        let (width, height) = super::size(header.args[0], self.limits.max_image_side)?;
        self.limits
            .check_image(width, height)
            .map_err(|e| header.args[0].error(e.code, e.message))?;
        let palette = header.option("palette");
        let background = header.option("background");
        let grid = header
            .option("grid")
            .map(|t| {
                t.text
                    .parse::<u32>()
                    .ok()
                    .filter(|n| *n > 0 && *n <= self.limits.max_image_side)
                    .ok_or_else(|| {
                        t.error(
                            ErrorCode::InvalidArgument,
                            "Screen grid must be positive and within image limits",
                        )
                    })
            })
            .transpose()?;
        let name = header.name;
        let lint = header.option("lint");
        header.finish("screen", "palette=, background=, grid= or lint=")?;
        self.lint_option(crate::lint::Kind::Screen, name, lint)?;
        let mut items = Vec::new();
        for child in &node.children {
            let tokens = &child.line.tokens;
            if items.len() >= self.limits.max_screen_items {
                return Err(
                    tokens[0].error(ErrorCode::LimitExceeded, "More than the screen item limit")
                );
            }
            if tokens.len() != 3 || tokens[1].text != "at" || !child.children.is_empty() {
                return Err(tokens[0].error(ErrorCode::Syntax, "Expected `SPRITE_CHAIN at X,Y`"));
            }
            let chain = chain::parse(tokens[0], self.limits)?;
            let Some((x, y)) = tokens[2].text.split_once(',') else {
                return Err(tokens[2].error(ErrorCode::Syntax, "Expected signed X,Y coordinates"));
            };
            let number = |s: &str| {
                s.parse::<i32>().map_err(|_| {
                    tokens[2].error(
                        ErrorCode::InvalidArgument,
                        "Screen coordinates must fit signed 32-bit integers",
                    )
                })
            };
            items.push((chain, number(x)?, number(y)?));
        }
        self.screens.push(ScreenDef {
            name,
            width,
            height,
            palette,
            background,
            grid,
            items,
        });
        Ok(())
    }

    pub(super) fn resolve_screens(
        &mut self,
        palettes: &[Palette],
        sprites: &[Sprite],
        budget: &mut u64,
    ) -> Vec<Screen> {
        let defs = std::mem::take(&mut self.screens);
        let mut screens: Vec<Screen> = Vec::new();
        for def in &defs {
            if screens.iter().any(|s| s.name == def.name.text) {
                self.errors
                    .push(def.name.error(ErrorCode::DuplicateName, "Duplicate screen"));
                continue;
            }
            let mut background = None;
            let mut valid = true;
            if let Some(token) = def.background {
                if let Some(palette) = self.sprite_palette(def.name, def.palette, palettes) {
                    let mut chars = token.text.chars();
                    let key = chars.next().unwrap_or(' ');
                    if chars.next().is_some() || palette.color(key).is_none() {
                        self.errors.push(
                            token.error(ErrorCode::UnknownName, "Unknown screen background key"),
                        );
                        valid = false;
                    } else {
                        self.palette_uses.insert((palette.name.clone(), key));
                        background = Some((palette.name.clone(), key));
                    }
                } else {
                    valid = false;
                }
            } else if let Some(p) = def.palette {
                if !palettes.iter().any(|v| v.name == p.text) {
                    self.errors
                        .push(p.error(ErrorCode::UnknownName, "Unknown screen palette"));
                    valid = false;
                }
            }
            let pixels = def.width as u64 * def.height as u64;
            if pixels > *budget {
                self.errors.push(def.name.error(
                    ErrorCode::LimitExceeded,
                    "Screens exceed the aggregate pixel limit",
                ));
                continue;
            }
            *budget -= pixels;
            let mut items = Vec::new();
            for (chain, x, y) in &def.items {
                let Some(base) = sprites.iter().find(|s| s.name == chain.base.text) else {
                    self.errors.push(
                        chain
                            .base
                            .error(ErrorCode::UnknownName, "Unknown screen sprite"),
                    );
                    valid = false;
                    continue;
                };
                match chain.apply(base, palettes, self.limits, &mut self.palette_uses) {
                    Ok(sprite) => {
                        let pixels =
                            sprite.width as u64 * sprite.height as u64 * sprite.layers.len() as u64;
                        if pixels > *budget {
                            self.errors.push(chain.base.error(
                                ErrorCode::LimitExceeded,
                                "Screen placements exceed the aggregate pixel limit",
                            ));
                            valid = false;
                            break;
                        }
                        *budget -= pixels;
                        items.push(ScreenItem {
                            sprite,
                            x: *x,
                            y: *y,
                            position: chain.base.pos,
                        });
                    }
                    Err(e) => {
                        self.errors.push(e);
                        valid = false;
                    }
                }
            }
            if valid {
                screens.push(Screen {
                    name: def.name.text.into(),
                    width: def.width,
                    height: def.height,
                    background,
                    grid: def.grid,
                    items,
                    position: def.name.pos,
                });
            }
        }
        self.screens = defs;
        screens
    }
}
