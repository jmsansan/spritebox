use super::chain;
use super::lex::{Node, Token};
use super::sprite::{Body, SpriteDef};
use super::{Header, Parsed, Parser, SetDef, check_name};
use crate::lint::Kind;
use crate::{ErrorCode, IconSet, Sprite, StateSet};

pub(super) struct WidgetDef<'a> {
    pub name: Token<'a>,
    pub kind: Kind,
    pub members: Vec<(Token<'a>, Token<'a>)>,
    pub outline: Option<Token<'a>>,
    pub margin: Option<Token<'a>>,
}

impl<'a> Parser<'a, '_> {
    fn generated(&mut self, at: Token<'a>) -> Parsed<Token<'a>> {
        if self.definitions_seen + self.extra_definitions >= self.limits.max_definitions {
            return Err(at.error(
                ErrorCode::LimitExceeded,
                "Generated widget sprites exceed the definition limit",
            ));
        }
        self.extra_definitions += 1;
        let text = self.generated_names.get(&at.pos).ok_or_else(|| {
            at.error(
                ErrorCode::LimitExceeded,
                "Widget names exceed the definition or input byte limit",
            )
        })?;
        Ok(Token { text, pos: at.pos })
    }

    pub(super) fn states(&mut self, node: &Node<'a>) -> Parsed<()> {
        let mut header = Header::new(node, "states")?;
        if let Some(t) = header.args.first() {
            return Err(t.error(ErrorCode::Syntax, "Unexpected state-set argument"));
        }
        let name = header.name;
        let lint = header.option("lint");
        header.finish("states", "lint=")?;
        self.lint_option(Kind::States, name, lint)?;
        if node.children.first().map(|n| n.keyword()) != Some("normal") {
            return Err(name.error(
                ErrorCode::InvalidArgument,
                "A state set needs `normal` first",
            ));
        }
        let mut members: Vec<(Token, Token)> = Vec::new();
        for child in &node.children {
            let tokens = &child.line.tokens;
            let state = tokens[0];
            check_name(state)?;
            if tokens.len() != 2 || !child.children.is_empty() {
                return Err(state.error(ErrorCode::Syntax, "Expected `STATE SPRITE_CHAIN`"));
            }
            if members.iter().any(|(s, _)| s.text == state.text) {
                return Err(state.error(ErrorCode::DuplicateName, "Duplicate widget state"));
            }
            let chain = chain::parse(tokens[1], self.limits)?;
            let generated = self.generated(state)?;
            self.sprites.push(SpriteDef {
                name: generated,
                slice: None,
                body: Body::Derived(chain),
            });
            members.push((state, generated));
        }
        self.widgets.push(WidgetDef {
            name,
            kind: Kind::States,
            members,
            outline: None,
            margin: None,
        });
        Ok(())
    }

    pub(super) fn icons(&mut self, node: &Node<'a>) -> Parsed<()> {
        let mut header = Header::new(node, "icons")?;
        if header.args.len() != 1 {
            return Err(header
                .name
                .error(ErrorCode::Syntax, "Expected `icons NAME WxH`"));
        }
        let size = header.args[0];
        let (width, height) = super::size(size, self.limits.max_sprite_side)?;
        let palette = header.option("palette");
        let outline = header.option("outline");
        let margin = header.option("margin");
        if let Some(t) = outline {
            if t.text.chars().count() != 1 {
                return Err(t.error(ErrorCode::Syntax, "Expected one outline palette key"));
            }
        }
        if let Some(t) = margin {
            if t.text
                .parse::<u32>()
                .ok()
                .filter(|v| *v <= width / 2 && *v <= height / 2)
                .is_none()
            {
                return Err(t.error(
                    ErrorCode::InvalidArgument,
                    "Icon margin does not fit the icon size",
                ));
            }
        }
        let name = header.name;
        let lint = header.option("lint");
        header.finish("icons", "palette=, outline=, margin= or lint=")?;
        self.lint_option(Kind::Icons, name, lint)?;
        if node.children.is_empty() {
            return Err(name.error(
                ErrorCode::InvalidArgument,
                "An icon set needs at least one icon",
            ));
        }
        let mut members: Vec<(Token, Token)> = Vec::new();
        for child in &node.children {
            let tokens = &child.line.tokens;
            if tokens.len() < 2
                || tokens[0].text != "icon"
                || tokens[2..].iter().any(|t| !t.text.starts_with("lint="))
            {
                return Err(
                    tokens[0].error(ErrorCode::Syntax, "Expected `icon NAME` and its pixel rows")
                );
            }
            check_name(tokens[1])?;
            if members.iter().any(|(s, _)| s.text == tokens[1].text) {
                return Err(tokens[1].error(ErrorCode::DuplicateName, "Duplicate icon name"));
            }
            let generated = self.generated(tokens[1])?;
            self.lint_option(Kind::Sprite, generated, lint)?;
            let mut sprite = child.clone();
            sprite.line.tokens = vec![
                Token {
                    text: "sprite",
                    pos: tokens[0].pos,
                },
                generated,
                size,
            ];
            if let Some(p) = palette {
                let original = node
                    .line
                    .tokens
                    .iter()
                    .find(|t| t.text.strip_prefix("palette=") == Some(p.text))
                    .unwrap();
                sprite.line.tokens.push(*original);
            }
            sprite.line.tokens.extend_from_slice(&tokens[2..]);
            self.sprite(&sprite)?;
            members.push((tokens[1], generated));
        }
        self.widgets.push(WidgetDef {
            name,
            kind: Kind::Icons,
            members,
            outline,
            margin,
        });
        Ok(())
    }

    pub(super) fn resolve_widgets(&mut self, sprites: &[Sprite]) -> (Vec<StateSet>, Vec<IconSet>) {
        let mut states = Vec::new();
        let mut icons = Vec::new();
        let mut seen = Vec::new();
        for def in &self.widgets {
            if seen.contains(&(def.kind, def.name.text)) {
                self.errors.push(
                    def.name
                        .error(ErrorCode::DuplicateName, "Duplicate widget set"),
                );
                continue;
            }
            seen.push((def.kind, def.name.text));
            let mut normal = None;
            let mut valid = true;
            for &(member, generated) in &def.members {
                let Some(sprite) = sprites.iter().find(|s| s.name == generated.text) else {
                    valid = false;
                    continue;
                };
                if def.kind == Kind::States {
                    if let Some((w, h, slice)) = normal {
                        if (sprite.width, sprite.height) != (w, h) {
                            self.errors.push(member.error(
                                ErrorCode::SizeMismatch,
                                "Widget states must have the same size as normal",
                            ));
                            valid = false;
                        }
                        if sprite.slice != slice {
                            self.errors.push(member.error(
                                ErrorCode::InvalidArgument,
                                "Widget states must have matching panel insets",
                            ));
                            valid = false;
                        }
                    } else {
                        normal = Some((sprite.width, sprite.height, sprite.slice));
                    }
                } else {
                    let image = sprite.render();
                    if let Some(t) = def.outline {
                        let key = t.text.chars().next().unwrap();
                        if sprite.palette.color(key).is_none() {
                            self.errors
                                .push(t.error(ErrorCode::UnknownName, "Unknown icon outline key"));
                            valid = false;
                            continue;
                        }
                        let opaque = |x: i64, y: i64| {
                            x >= 0
                                && y >= 0
                                && image.pixel(x as u32, y as u32).is_some_and(|c| c.a != 0)
                        };
                        let mut boundary = false;
                        for y in 0..sprite.height {
                            for x in 0..sprite.width {
                                if opaque(x as i64, y as i64)
                                    && [(-1, 0), (1, 0), (0, -1), (0, 1)]
                                        .iter()
                                        .any(|&(dx, dy)| !opaque(x as i64 + dx, y as i64 + dy))
                                {
                                    boundary = true;
                                    if sprite.key(x, y) != Some(key) {
                                        valid = false;
                                        self.errors.push(member.error(
                                            ErrorCode::InvalidArgument,
                                            "Icon does not have its declared outline",
                                        ));
                                        break;
                                    }
                                }
                            }
                        }
                        if !boundary {
                            valid = false;
                            self.errors.push(member.error(
                                ErrorCode::InvalidArgument,
                                "An outlined icon cannot be empty",
                            ));
                        }
                    }
                    if let Some(t) = def.margin {
                        let margin = t.text.parse::<u32>().unwrap();
                        if (0..sprite.height).any(|y| {
                            (0..sprite.width).any(|x| {
                                (x < margin
                                    || y < margin
                                    || x >= sprite.width - margin
                                    || y >= sprite.height - margin)
                                    && image.pixel(x, y).is_some_and(|c| c.a != 0)
                            })
                        }) {
                            self.errors.push(member.error(
                                ErrorCode::InvalidArgument,
                                "Icon margin must be transparent",
                            ));
                            valid = false;
                        }
                    }
                }
            }
            if valid {
                if def.kind == Kind::States {
                    states.push(StateSet {
                        name: def.name.text.into(),
                        states: def
                            .members
                            .iter()
                            .map(|(s, g)| (s.text.into(), g.text.into()))
                            .collect(),
                    });
                } else {
                    icons.push(IconSet {
                        name: def.name.text.into(),
                        members: def.members.iter().map(|(_, g)| g.text.into()).collect(),
                    });
                }
            }
        }
        (states, icons)
    }

    pub(super) fn widget_sets(&self) -> Vec<SetDef<'a>> {
        self.widgets
            .iter()
            .map(|def| {
                (
                    def.kind,
                    def.name,
                    def.members
                        .iter()
                        .map(|(_, g)| crate::lint::Member {
                            name: g.text.into(),
                            pos: g.pos,
                            sources: vec![g.text.into()],
                        })
                        .collect(),
                )
            })
            .collect()
    }
}
