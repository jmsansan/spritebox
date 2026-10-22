//! Tilesets with autotiled terrains, and tilemaps.

use std::collections::{BTreeSet, HashMap};

use super::chain;
use super::lex::{Node, Token};
use super::sprite::Source;
use super::{Header, Parsed, Parser, SetDef, check_name, duplicate, size};
use crate::lint::Kind;
use crate::sprite::Layer;
use crate::tiles::{Piece, Scheme, Terrain, Tilemap, Tileset, assemble};
use crate::{Error, ErrorCode, Palette, Position, Sprite};

pub(crate) struct TilesetDef<'a> {
    name: Token<'a>,
    width: u32,
    height: u32,
    palette: Option<Token<'a>>,
    columns: Option<u32>,
    entries: Vec<Entry<'a>>,
    complete: bool,
}

enum Entry<'a> {
    Tile {
        name: Token<'a>,
        source: Source<'a>,
    },
    Terrain {
        name: Token<'a>,
        scheme: Scheme,
        /// Each piece with its name's token.
        pieces: Vec<(Piece, Token<'a>, Source<'a>)>,
    },
}

pub(crate) struct TilemapDef<'a> {
    name: Token<'a>,
    tileset: Option<Token<'a>>,
    outside_connects: bool,
    legend: Vec<LegendEntry<'a>>,
    /// The keys of every legend line, those with errors included.
    keys: Vec<char>,
    has_legend: bool,
    rows: Vec<Token<'a>>,
    complete: bool,
}

struct LegendEntry<'a> {
    key: char,
    key_token: Token<'a>,
    /// `empty`, a tile's name, a terrain's name, or a terrain's variant `NAME/N`.
    target: Token<'a>,
    solid: bool,
}

/// What a legend key stands for, once resolved.
#[derive(Clone, Copy)]
enum Cell {
    Empty,
    Tile(u32),
    /// Autotiled: the terrain's index in the tileset.
    Terrain(usize),
}

impl<'a> Parser<'a, '_> {
    /// Each tileset's tiles, at their names, for lint.
    pub(super) fn tileset_sets(&self) -> Vec<SetDef<'a>> {
        self.tilesets
            .iter()
            .map(|t| {
                // A terrain's variants are made from its pieces, at its name.
                let tiles = t
                    .entries
                    .iter()
                    .flat_map(|e| match e {
                        Entry::Tile { name, source } => vec![crate::lint::Member {
                            name: name.text.to_owned(),
                            pos: name.pos,
                            source: match source {
                                Source::Chain(chain) => Some(chain.base.text.to_owned()),
                                Source::Rows(_) => None,
                            },
                        }],
                        Entry::Terrain { name, scheme, .. } => (0..scheme.variants())
                            .map(|v| crate::lint::Member {
                                name: format!("{}/{v}", name.text),
                                pos: name.pos,
                                source: None,
                            })
                            .collect(),
                    })
                    .collect();
                (Kind::Tileset, t.name, tiles)
            })
            .collect()
    }

    pub(super) fn tileset(&mut self, node: &Node<'a>) -> Parsed<()> {
        let mut header = Header::new(node, "tileset")?;
        let size_token = *header.args.first().ok_or_else(|| {
            header
                .name
                .end_error("Expected the tile size, such as `16x16`".to_owned())
        })?;
        if let Some(extra) = header.args.get(1) {
            return Err(extra.error(
                ErrorCode::Syntax,
                format!("Unexpected `{}` after the tile size", extra.text),
            ));
        }
        let (width, height) = size(size_token, self.limits.max_sprite_side)?;
        let palette = header.option("palette");
        if let Some(palette) = palette {
            check_name(palette)?;
        }
        let columns = match header.option("columns") {
            Some(token) => Some(whole(token, 1, 4096, "Columns")?),
            None => None,
        };
        let name = header.name;
        let lint = header.option("lint");
        header.finish("tileset", "`palette=`, `columns=` or `lint=`")?;
        self.lint_option(crate::lint::Kind::Tileset, name, lint)?;
        let mut entries: Vec<Entry<'a>> = Vec::new();
        let mut complete = true;
        let mut names: HashMap<&str, Position> = HashMap::new();
        for child in &node.children {
            let entry = match child.keyword() {
                "tile" => self.tile(child, width, height),
                "terrain" => self.terrain(child, width, height),
                other => Err(child.line.first().error(
                    ErrorCode::Syntax,
                    format!("Unknown tileset entry `{other}`; expected `tile` or `terrain`"),
                )),
            };
            match entry {
                Ok((entry, ok)) => {
                    complete &= ok;
                    let entry_name = match &entry {
                        Entry::Tile { name, .. } | Entry::Terrain { name, .. } => *name,
                    };
                    if let Some(&first) = names.get(entry_name.text) {
                        self.errors.push(duplicate(entry_name, "Tile", first));
                        complete = false;
                    } else {
                        names.insert(entry_name.text, entry_name.pos);
                    }
                    entries.push(entry);
                }
                Err(e) => {
                    self.errors.push(e);
                    complete = false;
                }
            }
        }
        self.tilesets.push(TilesetDef {
            name,
            width,
            height,
            palette,
            columns,
            entries,
            complete,
        });
        Ok(())
    }

    /// `tile NAME` with rows under it, or `tile NAME = CHAIN`.
    fn tile(&mut self, node: &Node<'a>, width: u32, height: u32) -> Parsed<(Entry<'a>, bool)> {
        let tokens = &node.line.tokens;
        let name = *tokens
            .get(1)
            .ok_or_else(|| tokens[0].end_error("Expected the tile's name".to_owned()))?;
        check_name(name)?;
        let (source, ok) = self.source(
            node,
            &tokens[2..],
            name,
            width,
            height,
            &format!("tile `{}`", name.text),
        )?;
        Ok((Entry::Tile { name, source }, ok))
    }

    /// `terrain NAME 16|47` with its pieces under it.
    fn terrain(&mut self, node: &Node<'a>, width: u32, height: u32) -> Parsed<(Entry<'a>, bool)> {
        let tokens = &node.line.tokens;
        let name = *tokens
            .get(1)
            .ok_or_else(|| tokens[0].end_error("Expected the terrain's name".to_owned()))?;
        check_name(name)?;
        let scheme_token = *tokens.get(2).ok_or_else(|| {
            name.end_error("Expected `16` or `47` variants after the terrain's name".to_owned())
        })?;
        let scheme = match scheme_token.text {
            "16" => Scheme::Edges,
            "47" => Scheme::Blob,
            other => {
                return Err(scheme_token.error(
                    ErrorCode::Syntax,
                    format!("Expected `16` or `47` variants, not `{other}`"),
                ));
            }
        };
        if let Some(extra) = tokens.get(3) {
            return Err(extra.error(ErrorCode::Syntax, format!("Unexpected `{}`", extra.text)));
        }
        if width % 2 != 0 || height % 2 != 0 {
            return Err(scheme_token.error(
                ErrorCode::InvalidArgument,
                format!(
                    "Autotiling builds tiles from quarters, so it needs an even tile size, not {width}x{height}"
                ),
            ));
        }
        let mut pieces: Vec<(Piece, Token<'a>, Source<'a>)> = Vec::new();
        let mut ok = true;
        for child in &node.children {
            let piece_token = child.line.first();
            let Some(&piece) = Piece::ALL.iter().find(|p| p.name() == piece_token.text) else {
                self.errors.push(piece_token.error(
                    ErrorCode::Syntax,
                    format!(
                        "Unknown piece `{}`; expected `island`, `h_strip`, `v_strip`, `inner` or `fill`",
                        piece_token.text
                    ),
                ));
                ok = false;
                continue;
            };
            if piece == Piece::Inner && scheme == Scheme::Edges {
                self.errors.push(piece_token.error(
                    ErrorCode::Syntax,
                    "A 16-variant terrain has no `inner` piece; it is for 47 variants",
                ));
                ok = false;
                continue;
            }
            if let Some((_, first, _)) = pieces.iter().find(|(p, _, _)| *p == piece) {
                self.errors.push(duplicate(piece_token, "Piece", first.pos));
                ok = false;
                continue;
            }
            let what = format!("piece `{}` of terrain `{}`", piece_token.text, name.text);
            match self.source(
                child,
                &child.line.tokens[1..],
                piece_token,
                width,
                height,
                &what,
            ) {
                Ok((source, rows_ok)) => {
                    ok &= rows_ok;
                    pieces.push((piece, piece_token, source));
                }
                Err(e) => {
                    self.errors.push(e);
                    ok = false;
                }
            }
        }
        let needed: Vec<Piece> = Piece::ALL
            .into_iter()
            .filter(|&p| p != Piece::Inner || scheme == Scheme::Blob)
            .collect();
        let missing: Vec<&str> = needed
            .iter()
            .filter(|p| !pieces.iter().any(|(q, _, _)| q == *p))
            .map(|p| p.name())
            .collect();
        if ok && !missing.is_empty() {
            return Err(name.error(
                ErrorCode::Syntax,
                format!(
                    "Terrain `{}` needs the pieces {}; missing {}",
                    name.text,
                    needed
                        .iter()
                        .map(|p| format!("`{}`", p.name()))
                        .collect::<Vec<_>>()
                        .join(", "),
                    missing
                        .iter()
                        .map(|p| format!("`{p}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
        Ok((
            Entry::Terrain {
                name,
                scheme,
                pieces,
            },
            ok,
        ))
    }

    /// The rows under `node`, or the chain after `=` in `rest`.
    fn source(
        &mut self,
        node: &Node<'a>,
        rest: &[Token<'a>],
        name: Token<'a>,
        width: u32,
        height: u32,
        what: &str,
    ) -> Parsed<(Source<'a>, bool)> {
        match rest {
            [] => {
                let (rows, ok) = self.rows(node, name.end(), width, height, what);
                Ok((Source::Rows(rows), ok))
            }
            [eq, chain_token] if eq.text == "=" => {
                if let Some(child) = node.children.first() {
                    return Err(child.line.first().error(
                        ErrorCode::Syntax,
                        format!("{what} is made from a sprite, so it has no rows"),
                    ));
                }
                Ok((
                    Source::Chain(chain::parse(*chain_token, self.limits)?),
                    true,
                ))
            }
            [eq] if eq.text == "=" => {
                Err(eq.end_error("Expected a sprite and its modifiers after `=`".to_owned()))
            }
            [extra, ..] => {
                Err(extra.error(ErrorCode::Syntax, format!("Unexpected `{}`", extra.text)))
            }
        }
    }

    pub(super) fn tilemap(&mut self, node: &Node<'a>) -> Parsed<()> {
        let mut header = Header::new(node, "tilemap")?;
        if let Some(extra) = header.args.first() {
            return Err(extra.error(
                ErrorCode::Syntax,
                format!("Unexpected `{}` after the tilemap's name", extra.text),
            ));
        }
        let tileset = header.option("tileset");
        if let Some(tileset) = tileset {
            check_name(tileset)?;
        }
        let outside_connects = match header.option("outside") {
            None => true,
            Some(t) if t.text == "same" => true,
            Some(t) if t.text == "empty" => false,
            Some(t) => {
                return Err(t.error(
                    ErrorCode::Syntax,
                    format!(
                        "Expected `outside=same` or `outside=empty`, not `{}`",
                        t.text
                    ),
                ));
            }
        };
        let name = header.name;
        let lint = header.option("lint");
        header.finish("tilemap", "`tileset=`, `outside=` or `lint=`")?;
        self.lint_option(crate::lint::Kind::Tilemap, name, lint)?;
        let mut legend = Vec::new();
        let mut keys = Vec::new();
        let mut rows = Vec::new();
        let mut complete = true;
        let (mut seen_legend, mut seen_map) = (None::<Position>, None::<Position>);
        for child in &node.children {
            let keyword = child.line.first();
            if let Some(extra) = child.line.tokens.get(1) {
                self.errors
                    .push(extra.error(ErrorCode::Syntax, format!("Unexpected `{}`", extra.text)));
                complete = false;
                continue;
            }
            match keyword.text {
                "legend" => {
                    if let Some(first) = seen_legend {
                        self.errors.push(duplicate(keyword, "Block", first));
                        complete = false;
                        continue;
                    }
                    seen_legend = Some(keyword.pos);
                    for entry in &child.children {
                        keys.extend(entry.line.first().text.chars().next());
                        match legend_entry(entry) {
                            Ok(e) => {
                                if let Some(first) =
                                    legend.iter().find(|f: &&LegendEntry| f.key == e.key)
                                {
                                    self.errors.push(Error::at(
                                        e.key_token.pos,
                                        ErrorCode::DuplicateName,
                                        format!(
                                            "Key `{}` is already in the legend on line {}",
                                            e.key, first.key_token.pos.line
                                        ),
                                    ));
                                    complete = false;
                                } else {
                                    legend.push(e);
                                }
                            }
                            Err(e) => {
                                self.errors.push(e);
                                complete = false;
                            }
                        }
                    }
                }
                "map" => {
                    if let Some(first) = seen_map {
                        self.errors.push(duplicate(keyword, "Block", first));
                        complete = false;
                        continue;
                    }
                    seen_map = Some(keyword.pos);
                    for row in &child.children {
                        if row.line.tokens.len() > 1 || !row.children.is_empty() {
                            self.errors.push(row.line.first().error(
                                ErrorCode::Syntax,
                                "A row of a map is one word of legend keys, with no spaces",
                            ));
                            complete = false;
                            continue;
                        }
                        rows.push(row.line.first());
                    }
                    if child.children.is_empty() {
                        self.errors.push(
                            keyword
                                .end_error("Expected rows of legend keys under `map`".to_owned()),
                        );
                        complete = false;
                    }
                }
                other => {
                    self.errors.push(keyword.error(
                        ErrorCode::Syntax,
                        format!("Unknown tilemap block `{other}`; expected `legend` or `map`"),
                    ));
                    complete = false;
                }
            }
        }
        for (block, seen) in [("legend", seen_legend), ("map", seen_map)] {
            if seen.is_none() {
                self.errors.push(name.error(
                    ErrorCode::Syntax,
                    format!("Tilemap `{}` needs a `{block}` block", name.text),
                ));
                complete = false;
            }
        }
        // Rows of one width.
        let width = rows.first().map_or(0, |r| r.text.chars().count());
        for row in &rows {
            let count = row.text.chars().count();
            if count != width {
                let pos = if count < width {
                    row.end()
                } else {
                    row.at(width)
                };
                self.errors.push(Error::at(
                    pos,
                    ErrorCode::SizeMismatch,
                    format!("Row has {count} tiles; the first row of the map has {width}"),
                ));
                complete = false;
            }
        }
        let max = self.limits.max_map_side as usize;
        if width > max || rows.len() > max {
            let at = if width > max {
                rows[0].at(max)
            } else {
                rows[max].pos
            };
            self.errors.push(Error::at(
                at,
                ErrorCode::LimitExceeded,
                format!("The map is more than the limit of {max} tiles a side"),
            ));
            complete = false;
        }
        self.tilemaps.push(TilemapDef {
            name,
            tileset,
            outside_connects,
            legend,
            keys,
            has_legend: seen_legend.is_some(),
            rows,
            complete,
        });
        Ok(())
    }

    /// Resolves the tilesets, whose tiles may be made from `sprites`.
    pub(super) fn resolve_tilesets(
        &mut self,
        palettes: &[Palette],
        sprites: &[Sprite],
    ) -> Vec<Tileset> {
        let defs = std::mem::take(&mut self.tilesets);
        let mut out = Vec::new();
        let mut seen: HashMap<&str, Position> = HashMap::new();
        for def in &defs {
            if let Some(&first) = seen.get(def.name.text) {
                self.errors.push(duplicate(def.name, "Tileset", first));
                continue;
            }
            seen.insert(def.name.text, def.name.pos);
            let palette = self.sprite_palette(def.name, def.palette, palettes);
            let mut tiles: Vec<Sprite> = Vec::new();
            let mut terrains = Vec::new();
            let mut ok = def.complete && palette.is_some();
            for entry in &def.entries {
                match entry {
                    Entry::Tile { name, source } => {
                        let Some(palette) = palette else { continue };
                        match self.tile_sprite(
                            name.text.to_owned(),
                            source,
                            def,
                            palette,
                            palettes,
                            sprites,
                        ) {
                            Some(tile) => tiles.push(tile),
                            None => ok = false,
                        }
                    }
                    Entry::Terrain {
                        name,
                        scheme,
                        pieces,
                    } => {
                        let Some(palette) = palette else { continue };
                        let mut built: Vec<Option<Sprite>> = vec![None; 5];
                        for (piece, token, source) in pieces {
                            let slot = Piece::ALL.iter().position(|p| p == piece).unwrap_or(0);
                            let label = format!("{}:{}", name.text, token.text);
                            built[slot] =
                                self.tile_sprite(label, source, def, palette, palettes, sprites);
                        }
                        // A 16-variant terrain never uses `inner`; stand in `fill`.
                        if *scheme == Scheme::Edges {
                            built[3] = built[4].clone();
                        }
                        let Some(pieces) = built.into_iter().collect::<Option<Vec<Sprite>>>()
                        else {
                            ok = false;
                            continue;
                        };
                        let Ok(pieces) = <[Sprite; 5]>::try_from(pieces) else {
                            ok = false;
                            continue;
                        };
                        terrains.push(Terrain {
                            name: name.text.to_owned(),
                            scheme: *scheme,
                            first: tiles.len() as u32 + 1,
                        });
                        for variant in 0..scheme.variants() {
                            let tile_name = format!("{}/{variant}", name.text);
                            tiles.push(assemble(&pieces, *scheme, scheme.mask(variant), tile_name));
                        }
                    }
                }
            }
            if tiles.len() > self.limits.max_tiles {
                self.errors.push(def.name.error(
                    ErrorCode::LimitExceeded,
                    format!(
                        "Tileset `{}` has {} tiles, more than the limit of {}",
                        def.name.text,
                        tiles.len(),
                        self.limits.max_tiles
                    ),
                ));
                continue;
            }
            let (Some(palette), true) = (palette, ok) else {
                continue;
            };
            let columns = def.columns.unwrap_or(8).min(tiles.len().max(1) as u32);
            let tileset = Tileset {
                name: def.name.text.to_owned(),
                tile_width: def.width,
                tile_height: def.height,
                columns,
                palette: palette.clone(),
                tiles,
                terrains,
            };
            let (w, h) = tileset.image_size();
            if let Err(e) = crate::Image::check_size(w, h, self.limits) {
                self.errors.push(def.name.error(
                    e.code,
                    format!("Tileset `{}`: {}", def.name.text, e.message),
                ));
                continue;
            }
            out.push(tileset);
        }
        self.tilesets = defs;
        out
    }

    /// A tile or piece from rows or a chain, checked against the tileset.
    fn tile_sprite(
        &mut self,
        name: String,
        source: &Source<'a>,
        def: &TilesetDef<'a>,
        palette: &Palette,
        palettes: &[Palette],
        sprites: &[Sprite],
    ) -> Option<Sprite> {
        match source {
            Source::Rows(rows) => {
                let keys = self.keys(rows, palette)?;
                // Rows with errors, which were reported, leave the tile short.
                if keys.len() != def.width as usize * def.height as usize {
                    return None;
                }
                Some(Sprite {
                    name,
                    width: def.width,
                    height: def.height,
                    palette: palette.clone(),
                    layers: vec![Layer { name: None, keys }],
                    slice: None,
                })
            }
            Source::Chain(chain) => {
                let Some(base) = sprites.iter().find(|s| s.name == chain.base.text) else {
                    if !self.sprites.iter().any(|d| d.name.text == chain.base.text) {
                        self.errors.push(chain.base.error(
                            ErrorCode::UnknownName,
                            format!("Unknown sprite `{}`", chain.base.text),
                        ));
                    }
                    return None;
                };
                let mut sprite =
                    match chain.apply(base, palettes, self.limits, &mut self.palette_uses) {
                        Ok(s) => s,
                        Err(e) => {
                            self.errors.push(e);
                            return None;
                        }
                    };
                if (sprite.width, sprite.height) != (def.width, def.height) {
                    self.errors.push(chain.base.error(
                        ErrorCode::SizeMismatch,
                        format!(
                            "`{}` is {}x{}; the tiles of `{}` are {}x{}",
                            chain.canonical(),
                            sprite.width,
                            sprite.height,
                            def.name.text,
                            def.width,
                            def.height
                        ),
                    ));
                    return None;
                }
                if sprite.palette.name != palette.name {
                    self.errors.push(chain.base.error(
                        ErrorCode::InvalidArgument,
                        format!(
                            "`{}` uses palette `{}`; tileset `{}` uses `{}`",
                            chain.canonical(),
                            sprite.palette.name,
                            def.name.text,
                            palette.name
                        ),
                    ));
                    return None;
                }
                sprite.name = name;
                sprite.slice = None;
                Some(sprite)
            }
        }
    }

    pub(super) fn resolve_tilemaps(&mut self, tilesets: &[Tileset]) -> Vec<Tilemap> {
        let defs = std::mem::take(&mut self.tilemaps);
        let mut out = Vec::new();
        let mut seen: HashMap<&str, Position> = HashMap::new();
        for def in &defs {
            if let Some(&first) = seen.get(def.name.text) {
                self.errors.push(duplicate(def.name, "Tilemap", first));
                continue;
            }
            seen.insert(def.name.text, def.name.pos);
            let tileset = match def.tileset {
                Some(token) => {
                    let found = tilesets.iter().find(|t| t.name == token.text);
                    if found.is_none() && !self.tilesets.iter().any(|t| t.name.text == token.text) {
                        self.errors.push(token.error(
                            ErrorCode::UnknownName,
                            format!("Unknown tileset `{}`", token.text),
                        ));
                    }
                    found
                }
                None if self.tilesets.len() == 1 => tilesets.first(),
                None => {
                    self.errors.push(def.name.error(
                        ErrorCode::Syntax,
                        format!(
                            "Tilemap `{}` needs `tileset=NAME`: the file defines {} tilesets",
                            def.name.text,
                            self.tilesets.len()
                        ),
                    ));
                    None
                }
            };
            let Some(tileset) = tileset else { continue };
            if let Some(map) = self.build_tilemap(def, tileset) {
                out.push(map);
            }
        }
        self.tilemaps = defs;
        out
    }

    fn build_tilemap(&mut self, def: &TilemapDef<'a>, tileset: &Tileset) -> Option<Tilemap> {
        if !def.has_legend {
            return None;
        }
        // Resolve the legend.
        let mut cells: Vec<(char, Cell, bool)> = Vec::new();
        let mut ok = def.complete;
        for entry in &def.legend {
            let target = entry.target.text;
            let cell = if target == "empty" {
                if entry.solid {
                    self.errors.push(
                        entry
                            .target
                            .error(ErrorCode::InvalidArgument, "An empty cell cannot be solid"),
                    );
                    ok = false;
                    continue;
                }
                Cell::Empty
            } else if let Some(t) = tileset.terrains.iter().position(|t| t.name == target) {
                Cell::Terrain(t)
            } else if let Some(index) = tileset.index(target) {
                Cell::Tile(index)
            } else {
                self.errors.push(entry.target.error(
                    ErrorCode::UnknownName,
                    format!(
                        "Tileset `{}` has no tile or terrain `{target}`",
                        tileset.name
                    ),
                ));
                ok = false;
                continue;
            };
            cells.push((entry.key, cell, entry.solid));
        }
        // Which indices are solid, and which are not; an index may not be both.
        let mut solid = BTreeSet::new();
        let mut not_solid = BTreeSet::new();
        for (entry, &(_, cell, is_solid)) in def.legend.iter().zip(&cells) {
            let indices: Vec<u32> = match cell {
                Cell::Empty => continue,
                Cell::Tile(i) => vec![i],
                Cell::Terrain(t) => {
                    let terrain = &tileset.terrains[t];
                    (terrain.first..terrain.first + terrain.scheme.variants()).collect()
                }
            };
            let (this, other) = if is_solid {
                (&mut solid, &not_solid)
            } else {
                (&mut not_solid, &solid)
            };
            if indices.iter().any(|i| other.contains(i)) {
                self.errors.push(entry.target.error(
                    ErrorCode::InvalidArgument,
                    format!(
                        "`{}` is solid under one key of the legend and not under another",
                        entry.target.text
                    ),
                ));
                ok = false;
            }
            this.extend(indices);
        }
        // Each map cell's legend entry.
        let width = def.rows.first().map_or(0, |r| r.text.chars().count());
        let height = def.rows.len();
        let mut grid: Vec<Option<Cell>> = Vec::with_capacity(width * height);
        for row in &def.rows {
            let mut reported = false;
            for (x, key) in row.text.chars().enumerate() {
                match cells.iter().find(|(k, _, _)| *k == key) {
                    Some(&(_, cell, _)) => grid.push(Some(cell)),
                    None => {
                        // Keys of legend lines with errors were reported there.
                        if !reported && !def.keys.contains(&key) {
                            self.errors.push(Error::at(
                                row.at(x),
                                ErrorCode::UnknownName,
                                format!(
                                    "Key `{key}` is not in the legend of tilemap `{}`",
                                    def.name.text
                                ),
                            ));
                            reported = true;
                        }
                        ok = false;
                        grid.push(None);
                    }
                }
            }
        }
        if !ok || grid.len() != width * height || width == 0 {
            return None;
        }
        // Autotile: each terrain cell's variant from its neighbours of the same terrain.
        let terrain_at = |x: i64, y: i64| -> Option<usize> {
            if x < 0 || y < 0 || x >= width as i64 || y >= height as i64 {
                return None;
            }
            match grid[y as usize * width + x as usize] {
                Some(Cell::Terrain(t)) => Some(t),
                Some(Cell::Tile(i)) => tileset
                    .terrains
                    .iter()
                    .position(|t| (t.first..t.first + t.scheme.variants()).contains(&i)),
                _ => None,
            }
        };
        let inside = |x: i64, y: i64| x >= 0 && y >= 0 && x < width as i64 && y < height as i64;
        use crate::tiles::neighbours::*;
        const AROUND: [(i64, i64, u8); 8] = [
            (0, -1, N),
            (1, -1, NE),
            (1, 0, E),
            (1, 1, SE),
            (0, 1, S),
            (-1, 1, SW),
            (-1, 0, W),
            (-1, -1, NW),
        ];
        let mut tiles = Vec::with_capacity(width * height);
        for y in 0..height as i64 {
            for x in 0..width as i64 {
                let index = match grid[y as usize * width + x as usize] {
                    Some(Cell::Tile(i)) => i,
                    Some(Cell::Terrain(t)) => {
                        let mut mask = 0u8;
                        for &(dx, dy, bit) in &AROUND {
                            let (nx, ny) = (x + dx, y + dy);
                            let same = if inside(nx, ny) {
                                terrain_at(nx, ny) == Some(t)
                            } else {
                                def.outside_connects
                            };
                            if same {
                                mask |= bit;
                            }
                        }
                        tileset.terrains[t].index(mask)
                    }
                    _ => 0,
                };
                tiles.push(index);
            }
        }
        Some(Tilemap {
            name: def.name.text.to_owned(),
            tileset: tileset.name.clone(),
            width: width as u32,
            height: height as u32,
            tiles,
            solid: solid.into_iter().collect(),
        })
    }
}

fn legend_entry<'a>(node: &Node<'a>) -> Parsed<LegendEntry<'a>> {
    let tokens = &node.line.tokens;
    let key_token = tokens[0];
    if let Some(child) = node.children.first() {
        return Err(child.line.first().error(
            ErrorCode::Syntax,
            "Unexpected indented line under a legend entry",
        ));
    }
    let mut chars = key_token.text.chars();
    let key = chars.next().unwrap_or(' ');
    if chars.next().is_some() {
        return Err(key_token.error(
            ErrorCode::Syntax,
            format!(
                "A legend key is one character, not `{}`; write `KEY TILE [solid]`",
                key_token.text
            ),
        ));
    }
    if key.is_control() {
        return Err(key_token.error(
            ErrorCode::InvalidArgument,
            format!("`{key}` cannot be a legend key"),
        ));
    }
    let target = *tokens.get(1).ok_or_else(|| {
        key_token.end_error(format!(
            "Expected a tile, a terrain or `empty` after `{key}`"
        ))
    })?;
    let solid = match tokens.get(2) {
        None => false,
        Some(t) if t.text == "solid" => true,
        Some(t) => {
            return Err(t.error(
                ErrorCode::Syntax,
                format!("Expected `solid`, not `{}`", t.text),
            ));
        }
    };
    if let Some(extra) = tokens.get(3) {
        return Err(extra.error(ErrorCode::Syntax, format!("Unexpected `{}`", extra.text)));
    }
    let name_ok = target
        .text
        .split_once('/')
        .map_or(target.text, |(terrain, variant)| {
            if variant.is_empty() || !variant.bytes().all(|b| b.is_ascii_digit()) {
                ""
            } else {
                terrain
            }
        });
    check_name(Token {
        text: name_ok,
        pos: target.pos,
    })
    .map_err(|_| {
        target.error(
            ErrorCode::Syntax,
            format!(
                "Expected a tile, a terrain, `TERRAIN/VARIANT` or `empty`, not `{}`",
                target.text
            ),
        )
    })?;
    Ok(LegendEntry {
        key,
        key_token,
        target,
        solid,
    })
}

fn whole(token: Token, min: u32, max: u32, what: &str) -> Parsed<u32> {
    if token.text.is_empty() || !token.text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(token.error(
            ErrorCode::Syntax,
            format!("Expected a whole number, not `{}`", token.text),
        ));
    }
    match token.text.parse::<u32>() {
        Ok(v) if (min..=max).contains(&v) => Ok(v),
        _ => Err(token.error(
            ErrorCode::InvalidArgument,
            format!("{what} must be {min}..={max}, not {}", token.text),
        )),
    }
}
