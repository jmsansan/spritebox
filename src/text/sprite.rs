//! Sprites: grids of keys in layers, and sprites derived through modifier chains.

use std::collections::HashMap;

use super::chain::{self, Chain};
use super::lex::{Node, Token};
use super::{Header, Parsed, Parser, check_name, duplicate, size};
use crate::sprite::Layer;
use crate::{Error, ErrorCode, Palette, Position, Sprite};

/// A sprite as written, before names and keys are resolved.
pub(crate) struct SpriteDef<'a> {
    pub name: Token<'a>,
    pub body: Body<'a>,
}

pub(crate) enum Body<'a> {
    Grid {
        width: u32,
        height: u32,
        palette: Option<Token<'a>>,
        layers: Vec<LayerDef<'a>>,
        /// False when some row or layer had an error, which was reported.
        complete: bool,
    },
    Derived(Chain<'a>),
}

pub(crate) struct LayerDef<'a> {
    pub name: Option<Token<'a>>,
    pub source: Source<'a>,
}

pub(crate) enum Source<'a> {
    /// Rows of the right width.
    Rows(Vec<Token<'a>>),
    Chain(Chain<'a>),
}

impl SpriteDef<'_> {
    /// The sprites that this one is built from.
    fn dependencies(&self) -> Vec<Token<'_>> {
        match &self.body {
            Body::Derived(chain) => vec![chain.base],
            Body::Grid { layers, .. } => layers
                .iter()
                .filter_map(|l| match &l.source {
                    Source::Chain(chain) => Some(chain.base),
                    Source::Rows(_) => None,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Unvisited,
    InProgress,
    /// Resolved, with its depth of derivation; or failed, with an error reported.
    Done(Option<usize>),
}

impl<'a> Parser<'a, '_> {
    pub(super) fn sprite(&mut self, node: &Node<'a>) -> Parsed<()> {
        let tokens = &node.line.tokens;
        if tokens.get(2).is_some_and(|t| t.text == "=") {
            return self.derived_sprite(node);
        }
        let mut header = Header::new(node, "sprite")?;
        let size_token = *header.args.first().ok_or_else(|| {
            header
                .name
                .end_error("Expected the sprite's size, such as `16x16`, or `= CHAIN`".to_owned())
        })?;
        if let Some(extra) = header.args.get(1) {
            return Err(extra.error(
                ErrorCode::Syntax,
                format!("Unexpected `{}` after the sprite's size", extra.text),
            ));
        }
        let (width, height) = size(size_token, self.limits.max_sprite_side)?;
        let palette = header.option("palette");
        if let Some(palette) = palette {
            check_name(palette)?;
        }
        let name = header.name;
        header.finish("sprite", "`palette=`")?;

        let layered = node
            .children
            .iter()
            .any(|c| c.keyword() == "layer" && c.line.tokens.len() > 1);
        let mut complete = true;
        let mut layers = Vec::new();
        if !layered {
            let (rows, ok) = self.rows(
                node,
                size_token.pos,
                width,
                height,
                &format!("sprite `{}`", name.text),
            );
            complete = ok;
            layers.push(LayerDef {
                name: None,
                source: Source::Rows(rows),
            });
        }
        for child in node.children.iter().filter(|_| layered) {
            match self.layer(child, width, height) {
                Ok((layer, ok)) => {
                    complete &= ok;
                    let named = layer.name.map(|t| t.text);
                    if let Some(first) = layers
                        .iter()
                        .find(|l: &&LayerDef| l.name.map(|t| t.text) == named)
                    {
                        let first: Position = first.name.map(|t| t.pos).unwrap_or_default();
                        self.errors
                            .push(duplicate(layer.name.unwrap_or(name), "Layer", first));
                        complete = false;
                    }
                    layers.push(layer);
                }
                Err(e) => {
                    self.errors.push(e);
                    complete = false;
                }
            }
        }
        if layers.len() > self.limits.max_layers {
            let at = node.children[self.limits.max_layers].line.first();
            self.errors.push(at.error(
                ErrorCode::LimitExceeded,
                format!("More than the limit of {} layers", self.limits.max_layers),
            ));
            complete = false;
        }
        self.sprites.push(SpriteDef {
            name,
            body: Body::Grid {
                width,
                height,
                palette,
                layers,
                complete,
            },
        });
        Ok(())
    }

    fn derived_sprite(&mut self, node: &Node<'a>) -> Parsed<()> {
        let tokens = &node.line.tokens;
        let name = tokens[1];
        check_name(name)?;
        let chain_token = *tokens.get(3).ok_or_else(|| {
            tokens[2].end_error("Expected a sprite and its modifiers after `=`".to_owned())
        })?;
        if let Some(extra) = tokens.get(4) {
            return Err(extra.error(
                ErrorCode::Syntax,
                format!(
                    "Unexpected `{}`; write the modifiers without spaces, such as `coin:flip_x`",
                    extra.text
                ),
            ));
        }
        if let Some(child) = node.children.first() {
            return Err(child.line.first().error(
                ErrorCode::Syntax,
                "A derived sprite has no rows; draw a sprite with a size instead",
            ));
        }
        let chain = chain::parse(chain_token, self.limits)?;
        self.sprites.push(SpriteDef {
            name,
            body: Body::Derived(chain),
        });
        Ok(())
    }

    /// One `layer NAME` with rows under it, or `layer NAME = CHAIN`.
    fn layer(&mut self, node: &Node<'a>, width: u32, height: u32) -> Parsed<(LayerDef<'a>, bool)> {
        let tokens = &node.line.tokens;
        if tokens[0].text != "layer" || tokens.len() < 2 {
            return Err(tokens[0].error(
                ErrorCode::Syntax,
                "In a sprite with layers, every line under it is `layer NAME`",
            ));
        }
        let name = tokens[1];
        check_name(name)?;
        if tokens.get(2).is_some_and(|t| t.text == "=") {
            let chain_token = *tokens.get(3).ok_or_else(|| {
                tokens[2].end_error("Expected a sprite and its modifiers after `=`".to_owned())
            })?;
            if let Some(extra) = tokens.get(4) {
                return Err(extra.error(ErrorCode::Syntax, format!("Unexpected `{}`", extra.text)));
            }
            if let Some(child) = node.children.first() {
                return Err(child
                    .line
                    .first()
                    .error(ErrorCode::Syntax, "A layer made from a sprite has no rows"));
            }
            let chain = chain::parse(chain_token, self.limits)?;
            return Ok((
                LayerDef {
                    name: Some(name),
                    source: Source::Chain(chain),
                },
                true,
            ));
        }
        if let Some(extra) = tokens.get(2) {
            return Err(extra.error(
                ErrorCode::Syntax,
                format!("Unexpected `{}` after the layer's name", extra.text),
            ));
        }
        let (rows, ok) = self.rows(
            node,
            name.end(),
            width,
            height,
            &format!("layer `{}`", name.text),
        );
        Ok((
            LayerDef {
                name: Some(name),
                source: Source::Rows(rows),
            },
            ok,
        ))
    }

    /// The rows under `node`, checked against the size; too few rows are reported at
    /// `missing_at`. Returns the rows of the right width, and whether all were right.
    pub(super) fn rows(
        &mut self,
        node: &Node<'a>,
        missing_at: Position,
        width: u32,
        height: u32,
        what: &str,
    ) -> (Vec<Token<'a>>, bool) {
        let mut rows = Vec::new();
        let mut ok = true;
        for child in &node.children {
            let line = &child.line;
            if line.tokens.len() > 1 || !child.children.is_empty() {
                self.errors.push(line.first().error(
                    ErrorCode::Syntax,
                    "A row is one word of palette keys, with no spaces",
                ));
                ok = false;
                continue;
            }
            let row = line.first();
            let count = row.text.chars().count();
            if count != width as usize {
                let (pos, message) = if count < width as usize {
                    (
                        row.end(),
                        format!("Row has only {count} pixels; {what} is {width} wide"),
                    )
                } else {
                    (
                        row.at(width as usize),
                        format!("Row has {count} pixels; {what} is {width} wide"),
                    )
                };
                self.errors
                    .push(Error::at(pos, ErrorCode::SizeMismatch, message));
                ok = false;
                continue;
            }
            rows.push(row);
        }
        let written = node.children.len();
        if written != height as usize {
            let pos = match node.children.get(height as usize) {
                Some(extra) => extra.line.first().pos,
                None => missing_at,
            };
            self.errors.push(Error::at(
                pos,
                ErrorCode::SizeMismatch,
                format!(
                    "{} has {written} rows; its size says {height}",
                    capitalize(what)
                ),
            ));
            ok = false;
        }
        (rows, ok)
    }

    /// Resolves every sprite, each after the sprites it is built from. Returns the
    /// sprites that resolved, in definition order, with the positions of their names.
    pub(super) fn resolve_sprites(&mut self, palettes: &[Palette]) -> Vec<(Sprite, Position)> {
        let defs = std::mem::take(&mut self.sprites);
        let mut index: HashMap<&str, usize> = HashMap::new();
        let mut usable = vec![true; defs.len()];
        for (i, def) in defs.iter().enumerate() {
            match index.get(def.name.text) {
                Some(&first) => {
                    self.errors
                        .push(duplicate(def.name, "Sprite", defs[first].name.pos));
                    usable[i] = false;
                }
                None => {
                    index.insert(def.name.text, i);
                }
            }
        }
        let mut state = vec![State::Unvisited; defs.len()];
        let mut built: Vec<Option<Sprite>> = vec![None; defs.len()];
        for (root, _) in usable.iter().enumerate().filter(|(_, u)| **u) {
            let mut stack = vec![(root, false)];
            while let Some((i, expanded)) = stack.pop() {
                if expanded {
                    let depth = self.build_sprite(&defs[i], &index, &state, &mut built, palettes);
                    state[i] = State::Done(depth);
                    continue;
                }
                if state[i] != State::Unvisited {
                    continue;
                }
                state[i] = State::InProgress;
                stack.push((i, true));
                for dependency in defs[i].dependencies() {
                    let Some(&j) = index.get(dependency.text) else {
                        continue;
                    };
                    match state[j] {
                        State::Unvisited => stack.push((j, false)),
                        State::InProgress => {
                            self.errors.push(dependency.error(
                                ErrorCode::Cycle,
                                format!(
                                    "Sprite `{}` is built from itself through `{}`",
                                    defs[i].name.text, dependency.text
                                ),
                            ));
                            // Break the circle; sprites built from it fail quietly.
                            state[j] = State::Done(None);
                        }
                        State::Done(_) => {}
                    }
                }
            }
        }
        let resolved = defs
            .iter()
            .zip(built)
            .filter_map(|(def, sprite)| sprite.map(|s| (s, def.name.pos)))
            .collect();
        self.sprites = defs;
        resolved
    }

    /// Builds one sprite whose dependencies are done. Returns its depth, or `None`
    /// when it fails.
    fn build_sprite(
        &mut self,
        def: &SpriteDef<'a>,
        index: &HashMap<&str, usize>,
        state: &[State],
        built: &mut [Option<Sprite>],
        palettes: &[Palette],
    ) -> Option<usize> {
        // The sprite a chain starts from, and the depth of the chain's result.
        let base = |this: &mut Self, chain: &Chain<'a>| -> Option<(Sprite, usize)> {
            let Some(&j) = index.get(chain.base.text) else {
                this.errors.push(chain.base.error(
                    ErrorCode::UnknownName,
                    format!("Unknown sprite `{}`", chain.base.text),
                ));
                return None;
            };
            let State::Done(Some(depth)) = state[j] else {
                return None;
            };
            let depth = depth + 1;
            if depth > this.limits.max_nesting {
                this.errors.push(chain.base.error(
                    ErrorCode::LimitExceeded,
                    format!(
                        "Sprites built from sprites more than the limit of {} deep",
                        this.limits.max_nesting
                    ),
                ));
                return None;
            }
            let sprite = built[j].clone()?;
            match chain.apply(&sprite, palettes, this.limits) {
                Ok(sprite) => Some((sprite, depth)),
                Err(e) => {
                    this.errors.push(e);
                    None
                }
            }
        };
        let i = index.get(def.name.text).copied()?;
        let result = match &def.body {
            Body::Derived(chain) => base(self, chain).map(|(mut sprite, depth)| {
                sprite.name = def.name.text.to_owned();
                (sprite, depth)
            }),
            Body::Grid {
                width,
                height,
                palette,
                layers,
                complete,
            } => {
                let palette = self.sprite_palette(def.name, *palette, palettes);
                let mut out = Vec::new();
                let mut depth = 0;
                let mut ok = *complete && palette.is_some();
                for layer in layers {
                    let layer_name = layer.name.map(|t| t.text.to_owned());
                    match &layer.source {
                        Source::Rows(rows) => {
                            let Some(palette) = palette else { continue };
                            match self.keys(rows, palette) {
                                Some(keys) => out.push(Layer {
                                    name: layer_name,
                                    keys,
                                }),
                                None => ok = false,
                            }
                        }
                        Source::Chain(chain) => {
                            let Some((sprite, d)) = base(self, chain) else {
                                ok = false;
                                continue;
                            };
                            depth = depth.max(d);
                            let token = layer.name.unwrap_or(def.name);
                            if (sprite.width, sprite.height) != (*width, *height) {
                                self.errors.push(chain.base.error(
                                    ErrorCode::SizeMismatch,
                                    format!(
                                        "Layer `{}` is {}x{}; sprite `{}` is {width}x{height}",
                                        token.text, sprite.width, sprite.height, def.name.text
                                    ),
                                ));
                                ok = false;
                                continue;
                            }
                            if let Some(p) = palette {
                                if sprite.palette.name != p.name {
                                    self.errors.push(chain.base.error(
                                        ErrorCode::InvalidArgument,
                                        format!(
                                            "Layer `{}` uses palette `{}`; sprite `{}` uses `{}`",
                                            token.text, sprite.palette.name, def.name.text, p.name
                                        ),
                                    ));
                                    ok = false;
                                    continue;
                                }
                            }
                            for mut l in sprite.layers {
                                l.name = layer_name.clone();
                                out.push(l);
                            }
                        }
                    }
                }
                if out.len() > self.limits.max_layers && ok {
                    self.errors.push(def.name.error(
                        ErrorCode::LimitExceeded,
                        format!(
                            "Sprite `{}` has {} layers, more than the limit of {}",
                            def.name.text,
                            out.len(),
                            self.limits.max_layers
                        ),
                    ));
                    ok = false;
                }
                match (ok, palette) {
                    (true, Some(palette)) => Some((
                        Sprite {
                            name: def.name.text.to_owned(),
                            width: *width,
                            height: *height,
                            palette: palette.clone(),
                            layers: out,
                        },
                        depth,
                    )),
                    _ => None,
                }
            }
        };
        let (sprite, depth) = result?;
        built[i] = Some(sprite);
        Some(depth)
    }

    pub(super) fn sprite_palette<'p>(
        &mut self,
        name: Token,
        palette: Option<Token>,
        palettes: &'p [Palette],
    ) -> Option<&'p Palette> {
        match palette {
            Some(token) => {
                let found = palettes.iter().find(|p| p.name == token.text);
                if found.is_none() && !self.palettes.iter().any(|p| p.name.text == token.text) {
                    self.errors.push(token.error(
                        ErrorCode::UnknownName,
                        format!("Unknown palette `{}`", token.text),
                    ));
                }
                found
            }
            None if self.palettes.len() == 1 => palettes.first(),
            None => {
                self.errors.push(name.error(
                    ErrorCode::Syntax,
                    format!(
                        "Sprite `{}` needs `palette=NAME`: the file defines {} palettes",
                        name.text,
                        self.palettes.len()
                    ),
                ));
                None
            }
        }
    }

    /// The keys of `rows`, or `None` after reporting the first unknown key of each
    /// row.
    pub(super) fn keys(&mut self, rows: &[Token], palette: &Palette) -> Option<Vec<char>> {
        let lookup = crate::sprite::ColorLookup::new(palette);
        let mut keys = Vec::with_capacity(rows.iter().map(|r| r.text.len()).sum());
        let mut ok = true;
        for row in rows {
            for (x, key) in row.text.chars().enumerate() {
                if lookup.find(key).is_none() {
                    self.errors.push(Error::at(
                        row.at(x),
                        ErrorCode::UnknownName,
                        format!("Key `{key}` is not in palette `{}`", palette.name),
                    ));
                    ok = false;
                    break;
                }
                keys.push(key);
            }
        }
        ok.then_some(keys)
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
