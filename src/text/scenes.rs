//! Scene grammar. Parsing and generation share one document-wide budget.
use super::chain;
use super::lex::{Node, Token};
use super::{Header, Parsed, Parser, check_name, size};
use crate::background::{ScatterContext, check_rect, scatter};
use crate::scene::Budget;
use crate::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub(super) struct SurfaceDef<'a> {
    pub name: Token<'a>,
    size: (u32, u32),
    palette: Option<Token<'a>>,
    seed: u64,
    ops: Vec<SurfaceOp<'a>>,
}
struct SurfaceOp<'a> {
    name: Token<'a>,
    key: Option<Token<'a>>,
    kind: Operation,
    stop_keys: Vec<Token<'a>>,
}
enum Operation {
    Paint(Primitive, char),
    Gradient(Gradient),
    Profile(ProfileOptions),
}
pub(super) struct BackgroundDef<'a> {
    pub name: Token<'a>,
    size: (u32, u32),
    seed: u64,
    layers: Vec<LayerDef<'a>>,
}
struct LayerDef<'a> {
    name: Token<'a>,
    size: (u32, u32),
    options: LayerOptions,
    items: Vec<ItemDef<'a>>,
}
enum ItemDef<'a> {
    Place {
        kind: Token<'a>,
        source: Token<'a>,
        options: PlacementOptions,
        crop_token: Option<Token<'a>>,
    },
    Scatter {
        name: Token<'a>,
        options: ScatterOptions,
        variants: Vec<(Token<'a>, u32)>,
        locations: Vec<Token<'a>>,
    },
}
struct Options<'a> {
    values: Vec<(&'a str, Token<'a>)>,
}
impl<'a> Options<'a> {
    fn new(tokens: &[Token<'a>]) -> Parsed<Self> {
        let mut values = Vec::new();
        for t in tokens {
            let (key, value) = t
                .text
                .split_once('=')
                .ok_or_else(|| t.error(ErrorCode::Syntax, "Expected key=value"))?;
            if values.iter().any(|(k, _)| *k == key) {
                return Err(t.error(ErrorCode::DuplicateName, "Duplicate option"));
            }
            values.push((
                key,
                Token {
                    text: value,
                    pos: t.at(key.chars().count() + 1),
                },
            ));
        }
        Ok(Self { values })
    }
    fn get(&mut self, key: &str) -> Option<Token<'a>> {
        let i = self.values.iter().position(|(k, _)| *k == key)?;
        Some(self.values.remove(i).1)
    }
    fn need(&mut self, key: &str, at: Token<'a>) -> Parsed<Token<'a>> {
        self.get(key)
            .ok_or_else(|| at.error(ErrorCode::Syntax, format!("Missing {key}=")))
    }
    fn finish(self) -> Parsed<()> {
        if let Some((key, t)) = self.values.first() {
            Err(t.error(ErrorCode::Syntax, format!("Unknown scene option `{key}`")))
        } else {
            Ok(())
        }
    }
}
fn integer<T: std::str::FromStr>(t: Token) -> Parsed<T> {
    t.text.parse().map_err(|_| {
        t.error(
            ErrorCode::InvalidArgument,
            "Integer is invalid or out of range",
        )
    })
}
fn pair(t: Token) -> Parsed<(i32, i32)> {
    let (a, b) = t
        .text
        .split_once(',')
        .ok_or_else(|| t.error(ErrorCode::Syntax, "Expected signed X,Y"))?;
    Ok((
        integer(Token { text: a, ..t })?,
        integer(Token {
            text: b,
            pos: t.at(a.chars().count() + 1),
        })?,
    ))
}
fn rect(t: Token) -> Parsed<Rect> {
    let v = t
        .text
        .split(',')
        .map(|v| {
            v.parse::<u32>()
                .map_err(|_| t.error(ErrorCode::InvalidArgument, "Expected unsigned x,y,w,h"))
        })
        .collect::<Parsed<Vec<_>>>()?;
    if v.len() != 4 {
        return Err(t.error(ErrorCode::Syntax, "Expected x,y,w,h"));
    }
    Ok(Rect {
        x: v[0],
        y: v[1],
        width: v[2],
        height: v[3],
    })
}
fn key(t: Token) -> Parsed<char> {
    let mut c = t.text.chars();
    let k = c
        .next()
        .ok_or_else(|| t.error(ErrorCode::Syntax, "Expected one palette key"))?;
    if c.next().is_some() {
        return Err(t.error(ErrorCode::Syntax, "Expected one palette key"));
    }
    Ok(k)
}
fn ratio(t: Token) -> Parsed<Ratio> {
    let (a, b) = t
        .text
        .split_once('/')
        .ok_or_else(|| t.error(ErrorCode::Syntax, "Expected numerator/denominator"))?;
    Ratio::new(
        integer(Token { text: a, ..t })?,
        integer(Token {
            text: b,
            pos: t.at(a.chars().count() + 1),
        })?,
    )
    .map_err(|e| t.error(e.code, e.message))
}
fn reference(t: Token<'_>) -> Parsed<(&str, &str)> {
    let (a, b) = t
        .text
        .split_once('(')
        .and_then(|(a, b)| b.strip_suffix(')').map(|b| (a, b)))
        .ok_or_else(|| t.error(ErrorCode::Syntax, "Expected surface(value)"))?;
    Ok((a, b))
}
fn leaf(node: &Node) -> Parsed<()> {
    if let Some(c) = node.children.first() {
        Err(c
            .line
            .first()
            .error(ErrorCode::Syntax, "Unexpected nested scene line"))
    } else {
        Ok(())
    }
}
impl<'a> Parser<'a, '_> {
    pub(super) fn surface(&mut self, node: &Node<'a>) -> Parsed<()> {
        let mut h = Header::new(node, "surface")?;
        if h.args.len() != 1 {
            return Err(h.name.error(ErrorCode::Syntax, "Expected surface NAME WxH"));
        }
        let dimensions = size(h.args[0], self.limits.max_image_side)?;
        self.limits
            .check_image(dimensions.0, dimensions.1)
            .map_err(|e| h.args[0].error(e.code, e.message))?;
        let palette = h.option("palette");
        let seed = h.option("seed").map(integer).transpose()?.unwrap_or(0);
        let lint = h.option("lint");
        self.lint_option(crate::lint::Kind::Surface, h.name, lint)?;
        let name = h.name;
        h.finish("surface", "palette=, seed= or lint=")?;
        let mut ops = Vec::new();
        let mut names = HashSet::new();
        for child in &node.children {
            self.scene_budget
                .commands(1)
                .map_err(|e| child.line.first().error(e.code, e.message))?;
            let ts = &child.line.tokens;
            let op = *ts
                .get(1)
                .ok_or_else(|| ts[0].error(ErrorCode::Syntax, "Surface operation needs a name"))?;
            check_name(op)?;
            if !names.insert(op.text) {
                return Err(op.error(ErrorCode::DuplicateName, "Duplicate surface operation"));
            }
            let mut points = Vec::new();
            let mut point_tokens = Vec::new();
            let mut stop_keys = Vec::new();
            let mut option_tokens = Vec::new();
            let mut in_points = false;
            for &t in &ts[2..] {
                if let Some(first) = t.text.strip_prefix("points=") {
                    if in_points {
                        return Err(t.error(ErrorCode::DuplicateName, "Duplicate points="));
                    }
                    in_points = true;
                    point_tokens.push(Token {
                        text: first,
                        pos: t.at(7),
                    });
                    points.push(pair(Token {
                        text: first,
                        pos: t.at(7),
                    })?);
                } else if in_points && !t.text.contains('=') {
                    point_tokens.push(t);
                    points.push(pair(t)?);
                } else {
                    option_tokens.push(t);
                }
                if points.len() > self.limits.max_scene_commands {
                    return Err(t.error(ErrorCode::LimitExceeded, "Too many polygon vertices"));
                }
            }
            self.scene_budget
                .commands(points.len())
                .map_err(|e| op.error(e.code, e.message))?;
            let mut o = Options::new(&option_tokens)?;
            let option_at = |name: &str| {
                option_tokens
                    .iter()
                    .find_map(|t| {
                        t.text.strip_prefix(&format!("{name}=")).map(|v| Token {
                            text: v,
                            pos: t.at(name.chars().count() + 1),
                        })
                    })
                    .unwrap_or(op)
            };
            if ts[0].text != "polygon" && !points.is_empty() {
                return Err(
                    point_tokens[0].error(ErrorCode::Syntax, "points= is only valid on polygons")
                );
            }
            let key_token = o.get("key");
            let kind = match ts[0].text {
                "rect" | "ellipse" | "line" | "polygon" => {
                    leaf(child)?;
                    let k = key(key_token
                        .ok_or_else(|| op.error(ErrorCode::Syntax, "Primitive needs key="))?)?;
                    let p = match ts[0].text {
                        "rect" | "ellipse" => {
                            let at = pair(o.need("at", op)?)?;
                            let t = o.need("size", op)?;
                            let dimensions = size(t, self.limits.max_image_side)?;
                            if ts[0].text == "rect" {
                                Primitive::Rect {
                                    at,
                                    size: dimensions,
                                }
                            } else {
                                Primitive::Ellipse {
                                    at,
                                    size: dimensions,
                                }
                            }
                        }
                        "line" => Primitive::Line {
                            from: pair(o.need("from", op)?)?,
                            to: pair(o.need("to", op)?)?,
                        },
                        _ => {
                            crate::surface::validate_polygon(&points, &mut self.scene_budget)
                                .map_err(|(i, e)| {
                                    point_tokens
                                        .get(i)
                                        .copied()
                                        .unwrap_or(op)
                                        .error(e.code, e.message)
                                })?;
                            Primitive::Polygon {
                                points: points.clone(),
                            }
                        }
                    };
                    if ts[0].text != "polygon" && !points.is_empty() {
                        return Err(
                            op.error(ErrorCode::Syntax, "points= is only valid on polygons")
                        );
                    }
                    Operation::Paint(p, k)
                }
                "gradient" => {
                    if key_token.is_some() || !points.is_empty() {
                        return Err(op.error(ErrorCode::Syntax, "Gradient uses stop keys"));
                    }
                    let axis = o.need("axis", op)?;
                    let axis = match axis.text {
                        "x" => Axis::X,
                        "y" => Axis::Y,
                        _ => {
                            return Err(axis.error(ErrorCode::InvalidArgument, "Expected axis=x|y"));
                        }
                    };
                    let mode = o.need("mode", op)?;
                    let mode = match mode.text {
                        "bands" => GradientMode::Bands,
                        "blend" => GradientMode::Blend,
                        _ => {
                            return Err(
                                mode.error(ErrorCode::InvalidArgument, "Expected bands|blend")
                            );
                        }
                    };
                    let dither = match o.get("dither") {
                        None => Dither::None,
                        Some(t) => match t.text {
                            "none" => Dither::None,
                            "bayer2" => Dither::Bayer(2),
                            "bayer4" => Dither::Bayer(4),
                            "bayer8" => Dither::Bayer(8),
                            _ => {
                                return Err(t.error(
                                    ErrorCode::InvalidArgument,
                                    "Expected none|bayer2|bayer4|bayer8",
                                ));
                            }
                        },
                    };
                    let mut stops = Vec::new();
                    for stop in &child.children {
                        self.scene_budget
                            .commands(1)
                            .map_err(|e| stop.line.first().error(e.code, e.message))?;
                        leaf(stop)?;
                        let t = &stop.line.tokens;
                        if t.len() != 3 || t[0].text != "stop" {
                            return Err(t[0].error(ErrorCode::Syntax, "Expected stop POSITION KEY"));
                        }
                        let position = integer(t[1])?;
                        let length = match axis {
                            Axis::X => dimensions.0,
                            Axis::Y => dimensions.1,
                        };
                        if position >= length
                            || stops
                                .last()
                                .is_some_and(|s: &GradientStop| s.position >= position)
                        {
                            return Err(t[1].error(
                                ErrorCode::InvalidArgument,
                                "Gradient stops must be increasing and in bounds",
                            ));
                        }
                        stop_keys.push(t[2]);
                        stops.push(GradientStop {
                            position,
                            key: key(t[2])?,
                        });
                    }
                    if mode == GradientMode::Bands && dither != Dither::None {
                        return Err(option_at("dither")
                            .error(ErrorCode::InvalidArgument, "Bands only accepts dither=none"));
                    }
                    Operation::Gradient(Gradient {
                        axis,
                        mode,
                        dither,
                        stops,
                    })
                }
                "profile" => {
                    leaf(child)?;
                    let p = ProfileOptions {
                        key: key(key_token
                            .ok_or_else(|| op.error(ErrorCode::Syntax, "Profile needs key="))?)?,
                        baseline: integer(o.need("baseline", op)?)?,
                        amplitude: integer(o.need("amplitude", op)?)?,
                        cell: integer(o.need("cell", op)?)?,
                        octaves: o.get("octaves").map(integer).transpose()?.unwrap_or(1),
                        roughness: o.get("roughness").map(integer).transpose()?.unwrap_or(50),
                        seed: o.get("seed").map(integer).transpose()?.unwrap_or(0),
                    };
                    if p.baseline > dimensions.1 {
                        return Err(option_at("baseline").error(
                            ErrorCode::InvalidArgument,
                            "Profile baseline exceeds surface height",
                        ));
                    }
                    if p.amplitude > p.baseline {
                        return Err(option_at("amplitude").error(
                            ErrorCode::InvalidArgument,
                            "Profile amplitude exceeds baseline",
                        ));
                    }
                    let noise = Noise {
                        width: dimensions.0,
                        height: p.cell,
                        cell: p.cell,
                        octaves: p.octaves,
                        roughness: p.roughness,
                        seed: 0,
                    };
                    noise.check().map_err(|e| {
                        let field = if p.cell == 0 || dimensions.0 % p.cell != 0 {
                            "cell"
                        } else if !(1..=8).contains(&p.octaves) || (p.cell >> (p.octaves - 1)) == 0
                        {
                            "octaves"
                        } else if p.roughness > 100 {
                            "roughness"
                        } else {
                            "cell"
                        };
                        option_at(field).error(e.code, e.message)
                    })?;
                    Operation::Profile(p)
                }
                _ => {
                    return Err(ts[0].error(
                        ErrorCode::Syntax,
                        "Expected rect, line, polygon, ellipse, gradient or profile",
                    ));
                }
            };
            o.finish()?;
            ops.push(SurfaceOp {
                name: op,
                key: key_token,
                kind,
                stop_keys,
            });
        }
        self.surfaces.push(SurfaceDef {
            name,
            size: dimensions,
            palette,
            seed,
            ops,
        });
        Ok(())
    }
    pub(super) fn background(&mut self, node: &Node<'a>) -> Parsed<()> {
        let mut h = Header::new(node, "background")?;
        if h.args.len() != 1 {
            return Err(h
                .name
                .error(ErrorCode::Syntax, "Expected background NAME WxH"));
        }
        let dimensions = size(h.args[0], self.limits.max_image_side)?;
        self.limits
            .check_image(dimensions.0, dimensions.1)
            .map_err(|e| h.args[0].error(e.code, e.message))?;
        let seed = h.option("seed").map(integer).transpose()?.unwrap_or(0);
        let name = h.name;
        let lint = h.option("lint");
        self.lint_option(crate::lint::Kind::Background, name, lint)?;
        h.finish("background", "seed= or lint=")?;
        let mut layers = Vec::new();
        let mut names = HashSet::new();
        for child in &node.children {
            self.scene_budget
                .commands(1)
                .map_err(|e| child.line.first().error(e.code, e.message))?;
            if layers.len() >= self.limits.max_layers {
                return Err(child
                    .line
                    .first()
                    .error(ErrorCode::LimitExceeded, "Too many background layers"));
            }
            if child.keyword() != "layer" {
                return Err(child
                    .line
                    .first()
                    .error(ErrorCode::Syntax, "Expected layer"));
            }
            let mut h = Header::new(child, "layer")?;
            let name = h.name;
            if !names.insert(name.text) {
                return Err(name.error(ErrorCode::DuplicateName, "Duplicate background layer"));
            }
            if h.args.len() != 1 {
                return Err(name.error(ErrorCode::Syntax, "Expected layer NAME WxH"));
            }
            let dimensions = size(h.args[0], self.limits.max_image_side)?;
            self.limits
                .check_image(dimensions.0, dimensions.1)
                .map_err(|e| h.args[0].error(e.code, e.message))?;
            if dimensions.0 > i32::MAX as u32 || dimensions.1 > i32::MAX as u32 {
                return Err(name.error(
                    ErrorCode::LimitExceeded,
                    "Layer coordinates exceed signed range",
                ));
            }
            let origin = h.option("at").map(pair).transpose()?.unwrap_or((0, 0));
            let clip = h
                .option("clip")
                .map(|t| {
                    let r = rect(t)?;
                    check_rect(r, dimensions).map_err(|e| t.error(e.code, e.message))?;
                    Ok::<_, Error>(r)
                })
                .transpose()?;
            let repeat = match h.option("repeat") {
                None => Repeat::None,
                Some(t) => match t.text {
                    "none" => Repeat::None,
                    "x" => Repeat::X,
                    "y" => Repeat::Y,
                    "both" => Repeat::Both,
                    _ => return Err(t.error(ErrorCode::InvalidArgument, "Expected none|x|y|both")),
                },
            };
            let parallax = h
                .option("parallax")
                .map(|t| {
                    let (a, b) = t
                        .text
                        .split_once(',')
                        .ok_or_else(|| t.error(ErrorCode::Syntax, "Expected NX/DX,NY/DY"))?;
                    Ok::<_, Error>((
                        ratio(Token { text: a, ..t })?,
                        ratio(Token {
                            text: b,
                            pos: t.at(a.chars().count() + 1),
                        })?,
                    ))
                })
                .transpose()?
                .unwrap_or_default();
            h.finish("layer", "at=, clip=, repeat= or parallax=")?;
            let mut items = Vec::new();
            let mut names = HashSet::new();
            for item in &child.children {
                let ts = &item.line.tokens;
                self.scene_budget
                    .commands(1)
                    .map_err(|e| ts[0].error(e.code, e.message))?;
                match ts[0].text {
                    "place" => {
                        leaf(item)?;
                        if ts.len() < 3 {
                            return Err(ts[0].error(
                                ErrorCode::Syntax,
                                "Expected place sprite|surface|tilemap SOURCE at=X,Y",
                            ));
                        }
                        let mut o = Options::new(&ts[3..])?;
                        let at = pair(o.need("at", ts[0])?)?;
                        let anchor = match o.get("anchor") {
                            None => Anchor::TopLeft,
                            Some(t) => match t.text {
                                "top-left" => Anchor::TopLeft,
                                "bottom-left" => Anchor::BottomLeft,
                                "center" => Anchor::Center,
                                _ => {
                                    return Err(t.error(
                                        ErrorCode::InvalidArgument,
                                        "Expected top-left|bottom-left|center",
                                    ));
                                }
                            },
                        };
                        let crop_token = o.get("crop");
                        let crop = crop_token.map(rect).transpose()?;
                        o.finish()?;
                        items.push(ItemDef::Place {
                            kind: ts[1],
                            source: ts[2],
                            options: PlacementOptions { at, anchor, crop },
                            crop_token,
                        });
                    }
                    "scatter" => {
                        let name = *ts.get(1).ok_or_else(|| {
                            ts[0].error(ErrorCode::Syntax, "Scatter needs a name")
                        })?;
                        check_name(name)?;
                        if !names.insert(name.text) {
                            return Err(
                                name.error(ErrorCode::DuplicateName, "Duplicate layer operation")
                            );
                        }
                        let mut o = Options::new(&ts[2..])?;
                        let count = o.get("count");
                        let density = o.get("density");
                        let amount = match (count, density) {
                            (Some(t), None) => ScatterAmount::Count(integer(t)?),
                            (None, Some(t)) => {
                                let (a, b) = t.text.split_once('/').ok_or_else(|| {
                                    t.error(ErrorCode::Syntax, "Expected density=P/Q")
                                })?;
                                ScatterAmount::Density {
                                    num: integer(Token { text: a, ..t })?,
                                    den: integer(Token {
                                        text: b,
                                        pos: t.at(a.chars().count() + 1),
                                    })?,
                                }
                            }
                            _ => {
                                return Err(name.error(
                                    ErrorCode::InvalidArgument,
                                    "Give exactly one of count= or density=",
                                ));
                            }
                        };
                        if let ScatterAmount::Density { num, den } = amount {
                            if den == 0 || num > den {
                                return Err(density.unwrap().error(
                                    ErrorCode::InvalidArgument,
                                    "Density must be in 0..=1 with positive denominator",
                                ));
                            }
                        }
                        let mask = o
                            .get("mask")
                            .map(|t| {
                                let (s, k) = reference(t)?;
                                Ok::<_, Error>((s.into(), key(Token { text: k, ..t })?))
                            })
                            .transpose()?;
                        let baseline = o
                            .get("baseline")
                            .map(|t| {
                                let (s, p) = reference(t)?;
                                Ok::<_, Error>((s.into(), p.into()))
                            })
                            .transpose()?;
                        let attempts_token = o.get("attempts");
                        let attempts = attempts_token.map(integer).transpose()?;
                        if attempts == Some(0) {
                            return Err(attempts_token.unwrap().error(
                                ErrorCode::InvalidArgument,
                                "Scatter attempts must be positive",
                            ));
                        }
                        let options = ScatterOptions {
                            amount,
                            spacing: o.get("spacing").map(integer).transpose()?.unwrap_or(0),
                            attempts,
                            seed: o.get("seed").map(integer).transpose()?.unwrap_or(0),
                            mask,
                            baseline,
                        };
                        o.finish()?;
                        let mut variants = Vec::new();
                        for v in &item.children {
                            self.scene_budget
                                .commands(1)
                                .map_err(|e| v.line.first().error(e.code, e.message))?;
                            leaf(v)?;
                            let t = &v.line.tokens;
                            if t.len() < 2 || t[0].text != "variant" {
                                return Err(t[0].error(
                                    ErrorCode::Syntax,
                                    "Expected variant SPRITE_CHAIN weight=N",
                                ));
                            }
                            let mut o = Options::new(&t[2..])?;
                            let weight_token = o.get("weight");
                            let weight = weight_token.map(integer).transpose()?.unwrap_or(1);
                            if weight == 0 {
                                return Err(weight_token.unwrap().error(
                                    ErrorCode::InvalidArgument,
                                    "Variant weight must be positive",
                                ));
                            }
                            o.finish()?;
                            variants.push((t[1], weight));
                        }
                        items.push(ItemDef::Scatter {
                            name,
                            options,
                            variants,
                            locations: ts[2..].to_vec(),
                        });
                    }
                    _ => return Err(ts[0].error(ErrorCode::Syntax, "Expected place or scatter")),
                }
            }
            layers.push(LayerDef {
                name,
                size: dimensions,
                options: LayerOptions {
                    origin,
                    repeat,
                    parallax,
                    clip,
                },
                items,
            });
        }
        self.backgrounds.push(BackgroundDef {
            name,
            size: dimensions,
            seed,
            layers,
        });
        Ok(())
    }
    fn scene_sprite(
        &mut self,
        t: Token<'a>,
        doc: &Document,
        cache: &mut HashMap<String, Arc<Sprite>>,
        b: &mut Budget,
    ) -> Parsed<(SourceRef, Option<Arc<Sprite>>)> {
        let chain = chain::parse(t, self.limits)?;
        let canonical = chain.canonical();
        let source = if let Some(s) = cache.get(&canonical) {
            s.clone()
        } else {
            let base = doc.sprite(chain.base.text).ok_or_else(|| {
                chain
                    .base
                    .error(ErrorCode::UnknownName, "Unknown background sprite")
            })?;
            let s =
                chain.apply_scene(base, &doc.palettes, self.limits, &mut self.palette_uses, b)?;
            for k in s.keys() {
                self.palette_uses.insert((s.palette.name.clone(), k));
            }
            let s = Arc::new(s);
            cache.insert(canonical.clone(), s.clone());
            s
        };
        Ok((SourceRef::Sprite(canonical), Some(source)))
    }
    pub(super) fn resolve_scenes(&mut self, doc: &mut Document) {
        let mut budget = self.scene_budget.clone();
        let surfaces = std::mem::take(&mut self.surfaces);
        let mut names = HashSet::new();
        for def in &surfaces {
            let mut build = |this: &mut Self, budget: &mut Budget| -> Parsed<Surface> {
                if !names.insert(def.name.text) {
                    return Err(def
                        .name
                        .error(ErrorCode::DuplicateName, "Duplicate surface"));
                }
                let palette = match def.palette {
                    Some(t) => doc.palette(t.text).ok_or_else(|| {
                        t.error(ErrorCode::UnknownName, "Unknown surface palette")
                    })?,
                    None if doc.palettes.len() == 1 => &doc.palettes[0],
                    None => {
                        return Err(def.name.error(
                            ErrorCode::Syntax,
                            "Surface needs palette=NAME when there is not exactly one palette",
                        ));
                    }
                };
                let mut s = Surface::new(
                    def.name.text,
                    def.size.0,
                    def.size.1,
                    palette,
                    def.seed,
                    this.limits,
                    budget,
                )
                .map_err(|e| def.name.error(e.code, e.message))?;
                for op in &def.ops {
                    if let Some(k) = op.key {
                        s.check_key(key(k)?)
                            .map_err(|e| k.error(e.code, e.message))?;
                    }
                    let result = match &op.kind {
                        Operation::Paint(p, k) => {
                            this.palette_uses.insert((s.palette.name.clone(), *k));
                            s.paint(p, *k, this.limits, budget)
                        }
                        Operation::Gradient(g) => {
                            for k in &op.stop_keys {
                                s.check_key(key(*k)?)
                                    .map_err(|e| k.error(e.code, e.message))?;
                            }
                            for stop in &g.stops {
                                this.palette_uses.insert((s.palette.name.clone(), stop.key));
                            }
                            s.gradient(g, budget)
                        }
                        Operation::Profile(p) => {
                            this.palette_uses.insert((s.palette.name.clone(), p.key));
                            s.add_profile(op.name.text, p, budget)
                        }
                    };
                    result.map_err(|e| op.name.error(e.code, e.message))?;
                }
                Ok(s)
            };
            match build(self, &mut budget) {
                Ok(s) => doc.surfaces.push(s),
                Err(e) => self.errors.push(e),
            }
        }
        self.surfaces = surfaces;
        let backgrounds = std::mem::take(&mut self.backgrounds);
        let mut names = HashSet::new();
        let mut cache = HashMap::new();
        for def in &backgrounds {
            let mut build = |this: &mut Self, budget: &mut Budget| -> Parsed<Background> {
                if !names.insert(def.name.text) {
                    return Err(def
                        .name
                        .error(ErrorCode::DuplicateName, "Duplicate background"));
                }
                let mut bg = Background {
                    commands: def.layers.len()
                        + def
                            .layers
                            .iter()
                            .flat_map(|l| &l.items)
                            .map(|i| match i {
                                ItemDef::Place { .. } => 1,
                                ItemDef::Scatter { variants, .. } => 1 + variants.len(),
                            })
                            .sum::<usize>(),
                    name: def.name.text.into(),
                    width: def.size.0,
                    height: def.size.1,
                    seed: def.seed,
                    layers: Vec::new(),
                };
                for l in &def.layers {
                    let mut layer = BackgroundLayer {
                        name: l.name.text.into(),
                        width: l.size.0,
                        height: l.size.1,
                        options: l.options.clone(),
                        items: Vec::new(),
                        position: l.name.pos,
                    };
                    for item in &l.items {
                        match item {
                            ItemDef::Place {
                                kind,
                                source,
                                options,
                                crop_token,
                            } => {
                                let (source_ref, sprite) = match kind.text {
                                    "sprite" => {
                                        this.scene_sprite(*source, doc, &mut cache, budget)?
                                    }
                                    "surface" => {
                                        check_name(*source)?;
                                        (SourceRef::Surface(source.text.into()), None)
                                    }
                                    "tilemap" => {
                                        check_name(*source)?;
                                        (SourceRef::Tilemap(source.text.into()), None)
                                    }
                                    _ => {
                                        return Err(kind.error(
                                            ErrorCode::Syntax,
                                            "Expected sprite|surface|tilemap",
                                        ));
                                    }
                                };
                                let p = BackgroundPlacement {
                                    source: source_ref,
                                    sprite,
                                    options: *options,
                                    position: source.pos,
                                };
                                let view =
                                    p.view(doc).map_err(|e| source.error(e.code, e.message))?;
                                p.crop(view.size()?).map_err(|e| {
                                    crop_token.unwrap_or(*source).error(e.code, e.message)
                                })?;
                                budget
                                    .pixels(1)
                                    .map_err(|e| source.error(e.code, e.message))?;
                                layer.items.push(p);
                            }
                            ItemDef::Scatter {
                                name,
                                options,
                                variants,
                                locations,
                            } => {
                                let at = |key: &str| {
                                    locations
                                        .iter()
                                        .find_map(|t| {
                                            t.text.strip_prefix(&format!("{key}=")).map(|v| Token {
                                                text: v,
                                                pos: t.at(key.chars().count() + 1),
                                            })
                                        })
                                        .unwrap_or(*name)
                                };
                                if let Some((surface, k)) = &options.mask {
                                    let t = at("mask");
                                    let s = doc.surface(surface).ok_or_else(|| {
                                        t.error(
                                            ErrorCode::UnknownName,
                                            "Unknown scatter mask surface",
                                        )
                                    })?;
                                    s.check_key(*k).map_err(|e| {
                                        Error::at(
                                            t.at(surface.chars().count() + 1),
                                            e.code,
                                            e.message,
                                        )
                                    })?;
                                    if s.size() != layer.size() {
                                        return Err(t.error(
                                            ErrorCode::SizeMismatch,
                                            "Scatter mask must match native layer size",
                                        ));
                                    }
                                }
                                if let Some((surface, profile)) = &options.baseline {
                                    let t = at("baseline");
                                    let s = doc.surface(surface).ok_or_else(|| {
                                        t.error(ErrorCode::UnknownName, "Unknown baseline surface")
                                    })?;
                                    s.profile(profile).ok_or_else(|| {
                                        Error::at(
                                            t.at(surface.chars().count() + 1),
                                            ErrorCode::UnknownName,
                                            "Unknown baseline profile",
                                        )
                                    })?;
                                    if s.size() != layer.size() {
                                        return Err(t.error(
                                            ErrorCode::SizeMismatch,
                                            "Baseline surface must match native layer size",
                                        ));
                                    }
                                }
                                let mut vs = Vec::new();
                                for (t, weight) in variants {
                                    let (source, sprite) =
                                        this.scene_sprite(*t, doc, &mut cache, budget)?;
                                    vs.push((
                                        WeightedVariant {
                                            source,
                                            weight: *weight,
                                        },
                                        sprite,
                                    ));
                                }
                                if let Some((name, k)) = &options.mask {
                                    if let Some(s) = doc.surface(name) {
                                        this.palette_uses.insert((s.palette.name.clone(), *k));
                                    }
                                }
                                let items = scatter(
                                    doc,
                                    ScatterContext {
                                        background: &bg.name,
                                        layer: &layer,
                                        operation: name.text,
                                        owner_seed: def.seed,
                                        position: name.pos,
                                    },
                                    options,
                                    &vs,
                                    budget,
                                )
                                .map_err(|e| name.error(e.code, e.message))?;
                                layer.items.extend(items);
                            }
                        }
                    }
                    bg.layers.push(layer);
                }
                Ok(bg)
            };
            match build(self, &mut budget) {
                Ok(b) => doc.backgrounds.push(b),
                Err(e) => self.errors.push(e),
            }
        }
        self.backgrounds = backgrounds;
        self.scene_budget = budget;
    }
}
