//! Animations: frames, each a sprite or a modifier chain, shown for a number of ticks.

use std::collections::HashMap;

use super::chain::{self, Chain};
use super::lex::{Node, Token};
use super::{Parsed, Parser, SetDef, check_name, duplicate};
use crate::lint::Kind;
use crate::{Animation, ErrorCode, Frame, Palette, Position, Sprite};

/// The most ticks one frame may last: more than eighteen minutes at 60 a second.
pub(crate) const MAX_TICKS: u32 = 65535;

pub(crate) struct AnimationDef<'a> {
    name: Token<'a>,
    looping: bool,
    frames: Vec<(Chain<'a>, u32)>,
    complete: bool,
}

impl<'a> Parser<'a, '_> {
    /// Each animation's frames, by the names of their sprites, at their lines, for
    /// lint.
    pub(super) fn animation_sets(&self) -> Vec<SetDef<'a>> {
        self.animations
            .iter()
            .map(|a| {
                let frames = a
                    .frames
                    .iter()
                    .map(|(chain, _)| crate::lint::Member {
                        name: chain.split_flips().0.canonical(),
                        pos: chain.base.pos,
                        sources: vec![chain.base.text.to_owned()],
                    })
                    .collect();
                (Kind::Animation, a.name, frames)
            })
            .collect()
    }

    pub(super) fn animation(&mut self, node: &Node<'a>) -> Parsed<()> {
        let tokens = &node.line.tokens;
        let name = *tokens
            .get(1)
            .ok_or_else(|| tokens[0].end_error("Expected a name for the animation".to_owned()))?;
        check_name(name)?;
        // `loop` or `once`, then `lint=`, both optional.
        let mut rest = &tokens[2..];
        let looping = match rest.first().map(|t| t.text) {
            None | Some("loop") => true,
            Some("once") => false,
            Some(other) if other.starts_with("lint=") => true,
            Some(other) => {
                return Err(tokens[2].error(
                    ErrorCode::Syntax,
                    format!("Expected `loop` or `once`, not `{other}`"),
                ));
            }
        };
        if rest
            .first()
            .is_some_and(|t| t.text == "loop" || t.text == "once")
        {
            rest = &rest[1..];
        }
        let lint = rest.first().and_then(|t| {
            t.text.strip_prefix("lint=").map(|value| Token {
                text: value,
                pos: t.at("lint=".len()),
            })
        });
        if lint.is_some() {
            rest = &rest[1..];
        }
        if let Some(extra) = rest.first() {
            return Err(extra.error(
                ErrorCode::Syntax,
                format!("Unexpected `{}` after `{}`", extra.text, tokens[2].text),
            ));
        }
        self.lint_option(crate::lint::Kind::Animation, name, lint)?;
        if node.children.is_empty() {
            return Err(name.error(
                ErrorCode::Syntax,
                format!(
                    "Animation `{}` has no frames; write one per line, such as `coin 6`",
                    name.text
                ),
            ));
        }
        let mut frames = Vec::new();
        let mut complete = true;
        for (i, child) in node.children.iter().enumerate() {
            if i == self.limits.max_frames {
                self.errors.push(child.line.first().error(
                    ErrorCode::LimitExceeded,
                    format!("More than the limit of {} frames", self.limits.max_frames),
                ));
                complete = false;
                break;
            }
            match self.frame(child) {
                Ok(frame) => frames.push(frame),
                Err(e) => {
                    self.errors.push(e);
                    complete = false;
                }
            }
        }
        self.animations.push(AnimationDef {
            name,
            looping,
            frames,
            complete,
        });
        Ok(())
    }

    fn frame(&mut self, node: &Node<'a>) -> Parsed<(Chain<'a>, u32)> {
        let tokens = &node.line.tokens;
        if let Some(child) = node.children.first() {
            return Err(child.line.first().error(
                ErrorCode::Syntax,
                "Unexpected indented line under an animation frame",
            ));
        }
        let chain = chain::parse(tokens[0], self.limits)?;
        let ticks = *tokens.get(1).ok_or_else(|| {
            tokens[0].end_error(format!(
                "Expected the frame's ticks after `{}`, such as `{} 6`",
                tokens[0].text, tokens[0].text
            ))
        })?;
        if let Some(extra) = tokens.get(2) {
            return Err(extra.error(
                ErrorCode::Syntax,
                format!(
                    "Unexpected `{}`; a frame is a sprite and its ticks, one per line",
                    extra.text
                ),
            ));
        }
        if ticks.text.is_empty() || !ticks.text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ticks.error(
                ErrorCode::Syntax,
                format!("Expected a number of ticks, not `{}`", ticks.text),
            ));
        }
        match ticks.text.parse::<u32>() {
            Ok(t) if (1..=MAX_TICKS).contains(&t) => Ok((chain, t)),
            _ => Err(ticks.error(
                ErrorCode::InvalidArgument,
                format!("Ticks must be 1..={MAX_TICKS}, not {}", ticks.text),
            )),
        }
    }

    /// Resolves the animations' frames. Returns the animations and the sprites that
    /// their chains build, each once, with the position where it was first used.
    pub(super) fn resolve_animations(
        &mut self,
        palettes: &[Palette],
        sprites: &[Sprite],
    ) -> (Vec<Animation>, Vec<(Sprite, Position)>) {
        let defs = std::mem::take(&mut self.animations);
        let mut animations = Vec::new();
        let mut built: Vec<(Sprite, Position)> = Vec::new();
        let mut seen: HashMap<&str, Position> = HashMap::new();
        for def in &defs {
            if let Some(&first) = seen.get(def.name.text) {
                self.errors.push(duplicate(def.name, "Animation", first));
                continue;
            }
            seen.insert(def.name.text, def.name.pos);
            let mut frames = Vec::new();
            let mut ok = def.complete;
            for (written, ticks) in &def.frames {
                let (chain, flip_x, flip_y) = written.split_flips();
                let chain = &chain;
                let name = chain.canonical();
                let known = sprites.iter().any(|s| s.name == name)
                    || built.iter().any(|(s, _)| s.name == name);
                if !known {
                    let Some(base) = sprites.iter().find(|s| s.name == chain.base.text) else {
                        if !self.sprites.iter().any(|d| d.name.text == chain.base.text) {
                            self.errors.push(chain.base.error(
                                ErrorCode::UnknownName,
                                format!("Unknown sprite `{}`", chain.base.text),
                            ));
                        }
                        ok = false;
                        continue;
                    };
                    match chain.apply(base, palettes, self.limits, &mut self.palette_uses) {
                        Ok(mut sprite) => {
                            sprite.name = name.clone();
                            built.push((sprite, chain.base.pos));
                        }
                        Err(e) => {
                            self.errors.push(e);
                            ok = false;
                            continue;
                        }
                    }
                }
                frames.push(Frame {
                    sprite: name,
                    ticks: *ticks,
                    flip_x,
                    flip_y,
                });
            }
            if ok {
                animations.push(Animation {
                    name: def.name.text.to_owned(),
                    frames,
                    looping: def.looping,
                });
            }
        }
        self.animations = defs;
        (animations, built)
    }
}
