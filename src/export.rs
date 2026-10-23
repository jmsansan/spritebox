//! Everything a game needs from a document: the atlas, a texture for each tileset,
//! the tilemaps, and their metadata in one JSON file.

use crate::atlas::write_header;
use crate::json::{Writer, list};
use crate::{
    Atlas, AtlasOptions, Document, Error, Image, Limits, Result, Scheme, Tilemap, TtfOptions,
};

/// How a document is exported.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExportOptions {
    pub atlas: AtlasOptions,
    pub backgrounds: crate::BackgroundExportOptions,
}

/// A tileset's texture and what the metadata says of it.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct TilesetExport {
    pub name: String,
    /// The texture's file name: `tileset-NAME.png`.
    pub file: String,
    /// Tile 1 at the top left, then row by row, `columns` a row, each with `padding`
    /// pixels around it.
    pub image: Image,
    pub tile_width: u32,
    pub tile_height: u32,
    pub columns: u32,
    /// Pixels around each tile, as [`AtlasOptions::padding`]: tiles are
    /// `2 * padding` apart and `padding` from the edges.
    pub padding: u32,
    /// The tiles' names in index order: `tiles[0]` is tile 1. A terrain's variants
    /// are `TERRAIN/VARIANT`.
    pub tiles: Vec<String>,
    /// Each terrain's name, scheme and first tile index.
    pub terrains: Vec<(String, Scheme, u32)>,
}

impl TilesetExport {
    /// Where tile `index`, from 1, is in [`TilesetExport::image`], without its padding.
    pub fn tile_rect(&self, index: u32) -> Option<crate::Rect> {
        let i = index
            .checked_sub(1)
            .filter(|&i| (i as usize) < self.tiles.len())?;
        let (cw, ch) = (
            self.tile_width + 2 * self.padding,
            self.tile_height + 2 * self.padding,
        );
        Some(crate::Rect {
            x: self.padding + i % self.columns * cw,
            y: self.padding + i / self.columns * ch,
            width: self.tile_width,
            height: self.tile_height,
        })
    }
}

/// A font as a TrueType file.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct FontExport {
    pub name: String,
    /// The file's name: `font-NAME.ttf`.
    pub file: String,
    pub ttf: Vec<u8>,
    pub height: u32,
    pub baseline: u32,
    pub line_height: u32,
    /// TrueType units for each pixel; the em is `height * units_per_pixel`.
    pub units_per_pixel: u32,
}

/// The files of a document: see [`Export::files`].
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Export {
    pub(crate) scene_work: u64,
    pub backgrounds: Vec<crate::BackgroundExport>,
    pub themes: Vec<crate::ThemeExport>,
    pub atlas: Atlas,
    pub tilesets: Vec<TilesetExport>,
    pub tilemaps: Vec<Tilemap>,
    pub fonts: Vec<FontExport>,
}

/// The name of the atlas texture in exported files.
pub const ATLAS_FILE: &str = "atlas.png";
/// The name of the metadata file.
pub const METADATA_FILE: &str = "spritebox.json";

pub(crate) fn preflight_paths(
    doc: &Document,
    options: &ExportOptions,
    limits: &Limits,
    all_themes: bool,
) -> Result<()> {
    if let Some(name) = &options.backgrounds.selection {
        if doc.background(name).is_none() {
            return Err(Error::new(
                crate::ErrorCode::UnknownName,
                format!("Unknown background `{name}`"),
            ));
        }
    }
    let mut seen = std::collections::HashSet::new();
    let mut add = |name: String| -> Result<()> {
        if seen.len() >= limits.max_export_files {
            return Err(Error::limit("More than the export file limit"));
        }
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(Error::invalid(format!(
                "Output filenames collide at `{name}`"
            )));
        }
        Ok(())
    };
    add(ATLAS_FILE.into())?;
    add(METADATA_FILE.into())?;
    for t in &doc.tilesets {
        add(format!("tileset-{}.png", t.name))?;
    }
    for f in &doc.fonts {
        add(format!("font-{}.ttf", f.name))?;
    }
    let bgs = crate::background_export::file_names(doc, &options.backgrounds);
    for name in &bgs {
        add(name.clone())?;
    }
    if all_themes {
        for (i, t) in doc.themes.iter().enumerate() {
            add(format!("atlas-{}.png", t.name))?;
            for set in &doc.tilesets {
                add(format!("tileset-{}-{}.png", set.name, t.name))?;
            }
            for name in &bgs {
                add(format!(
                    "{}-theme-{i:04}.png",
                    name.strip_suffix(".png").unwrap()
                ))?;
            }
        }
    }
    Ok(())
}

impl Export {
    /// Planned relative paths without encoding any image data.
    pub fn file_names(&self) -> Vec<String> {
        let mut names = vec![ATLAS_FILE.into()];
        names.extend(self.tilesets.iter().map(|t| t.file.clone()));
        names.extend(self.fonts.iter().map(|f| f.file.clone()));
        let backgrounds = |b: &[crate::BackgroundExport]| {
            b.iter()
                .flat_map(|b| {
                    b.flat
                        .iter()
                        .map(|f| f.file.clone())
                        .chain(b.layers.iter().map(|l| l.file.clone()))
                })
                .collect::<Vec<_>>()
        };
        names.extend(backgrounds(&self.backgrounds));
        for t in &self.themes {
            names.push(t.file.clone());
            names.extend(t.tilesets.iter().map(|t| t.file.clone()));
            names.extend(backgrounds(&t.backgrounds));
        }
        names.push(METADATA_FILE.into());
        names
    }
    /// Image cells retained by this export, useful when sharing a scene budget
    /// with a preview or another in-memory output operation.
    pub fn image_pixels(&self) -> u64 {
        let bg = |b: &[crate::BackgroundExport]| {
            b.iter()
                .map(|b| {
                    b.flat
                        .as_ref()
                        .map_or(0, |f| f.image.width() as u64 * f.image.height() as u64)
                        + b.layers
                            .iter()
                            .map(|l| l.image.width() as u64 * l.image.height() as u64)
                            .sum::<u64>()
                })
                .sum::<u64>()
        };
        self.atlas.image.width() as u64 * self.atlas.image.height() as u64
            + self
                .tilesets
                .iter()
                .map(|t| t.image.width() as u64 * t.image.height() as u64)
                .sum::<u64>()
            + bg(&self.backgrounds)
            + self
                .themes
                .iter()
                .map(|t| {
                    t.image.width() as u64 * t.image.height() as u64
                        + t.tilesets
                            .iter()
                            .map(|t| t.image.width() as u64 * t.image.height() as u64)
                            .sum::<u64>()
                        + bg(&t.backgrounds)
                })
                .sum::<u64>()
    }
    /// Scene work already consumed while constructing this export.
    pub fn scene_work(&self) -> u64 {
        self.scene_work
    }
    /// The metadata as JSON: the atlas and animations, as [`Atlas::to_json`] writes
    /// them, then `tilesets` and `tilemaps`. See the README for the form.
    pub fn to_json(&self) -> String {
        let mut w = Writer::new();
        w.object(None);
        write_header(&mut w);
        self.atlas.write_json(&mut w, ATLAS_FILE);
        if !self.themes.is_empty() {
            w.object(Some("themes"));
            for theme in &self.themes {
                w.object(Some(&theme.name));
                w.string(Some("atlas"), &theme.file);
                w.object(Some("tilesets"));
                for tileset in &theme.tilesets {
                    w.string(Some(&tileset.name), &tileset.file);
                }
                w.close('}');
                crate::background_export::write_json(&mut w, &theme.backgrounds);
                w.close('}');
            }
            w.close('}');
        }
        w.object(Some("tilesets"));
        for t in &self.tilesets {
            w.object(Some(&t.name));
            w.string(Some("image"), &t.file);
            w.raw(Some("size"), &list(&[t.image.width(), t.image.height()]));
            w.raw(Some("tile_size"), &list(&[t.tile_width, t.tile_height]));
            w.raw(Some("columns"), &t.columns.to_string());
            w.raw(Some("padding"), &t.padding.to_string());
            w.array(Some("tiles"));
            for tile in &t.tiles {
                w.string(None, tile);
            }
            w.close(']');
            w.object(Some("terrains"));
            for (name, scheme, first) in &t.terrains {
                w.raw(
                    Some(name),
                    &format!(
                        "{{\"variants\": {}, \"first\": {first}}}",
                        scheme.variants()
                    ),
                );
            }
            w.close('}');
            w.close('}');
        }
        w.close('}');
        w.object(Some("tilemaps"));
        for map in &self.tilemaps {
            w.object(Some(&map.name));
            w.string(Some("tileset"), &map.tileset);
            w.raw(Some("size"), &list(&[map.width, map.height]));
            w.array(Some("tiles"));
            for row in map.tiles.chunks(map.width.max(1) as usize) {
                w.raw(None, &list(row));
            }
            w.close(']');
            w.raw(Some("solid"), &list(&map.solid));
            w.close('}');
        }
        w.close('}');
        w.object(Some("fonts"));
        for font in &self.fonts {
            w.object(Some(&font.name));
            w.string(Some("file"), &font.file);
            w.raw(Some("height"), &font.height.to_string());
            w.raw(Some("baseline"), &font.baseline.to_string());
            w.raw(Some("line_height"), &font.line_height.to_string());
            w.raw(Some("units_per_pixel"), &font.units_per_pixel.to_string());
            w.close('}');
        }
        w.close('}');
        crate::background_export::write_json(&mut w, &self.backgrounds);
        w.close('}');
        w.finish()
    }

    /// Every file, as `(name, bytes)`: `atlas.png`, then `tileset-NAME.png` for each
    /// tileset, `font-NAME.ttf` for each font, then `spritebox.json`.
    pub fn files(&self) -> Vec<(String, Vec<u8>)> {
        let mut files = vec![(ATLAS_FILE.to_owned(), crate::png::encode(&self.atlas.image))];
        for t in &self.tilesets {
            files.push((t.file.clone(), crate::png::encode(&t.image)));
        }
        for f in &self.fonts {
            files.push((f.file.clone(), f.ttf.clone()));
        }
        crate::background_export::add_files(&mut files, &self.backgrounds);
        for theme in &self.themes {
            files.push((theme.file.clone(), crate::png::encode(&theme.image)));
            for tileset in &theme.tilesets {
                files.push((tileset.file.clone(), crate::png::encode(&tileset.image)));
            }
            crate::background_export::add_files(&mut files, &theme.backgrounds);
        }
        files.push((METADATA_FILE.to_owned(), self.to_json().into_bytes()));
        files
    }
}

pub(crate) fn build(
    document: &Document,
    options: &ExportOptions,
    limits: &Limits,
) -> Result<Export> {
    let mut budget = crate::scene::Budget::new(limits);
    budget.pixels(document.scene_storage())?;
    build_copies(document, options, limits, 1, &mut budget)
}

pub(crate) fn build_copies(
    document: &Document,
    options: &ExportOptions,
    limits: &Limits,
    copies: u64,
    scene_budget: &mut crate::scene::Budget,
) -> Result<Export> {
    preflight_paths(document, options, limits, false)?;
    let mut names = vec![ATLAS_FILE.to_owned(), METADATA_FILE.to_owned()];
    names.extend(
        document
            .tilesets
            .iter()
            .map(|t| format!("tileset-{}.png", t.name)),
    );
    names.extend(
        document
            .fonts
            .iter()
            .map(|t| format!("font-{}.ttf", t.name)),
    );
    names.extend(crate::background_export::file_names(
        document,
        &options.backgrounds,
    ));
    let mut unique = std::collections::HashSet::new();
    for name in &names {
        if !unique.insert(name.to_ascii_lowercase()) {
            return Err(Error::invalid(format!(
                "Output filenames collide at `{name}`"
            )));
        }
    }
    if names.len() > limits.max_export_files {
        return Err(Error::limit("More than the export file limit"));
    }
    let atlas = document.atlas_with(&options.atlas, limits)?;
    let pad = options.atlas.padding;
    let max = options.atlas.max_side;
    let mut pixels = atlas.image.width() as u64 * atlas.image.height() as u64;
    for tileset in &document.tilesets {
        let (w, h) = tileset.padded_size(pad);
        pixels = pixels
            .checked_add(
                w.checked_mul(h)
                    .ok_or_else(|| Error::limit("Export image size overflow"))?,
            )
            .ok_or_else(|| Error::limit("Export image budget overflow"))?;
    }
    if pixels
        .checked_mul(copies)
        .is_none_or(|n| n > limits.max_image_pixels)
    {
        return Err(Error::limit(
            "The exported images exceed the aggregate pixel limit",
        ));
    }
    let files = (document.tilesets.len() as u64 + 1)
        .checked_mul(copies)
        .and_then(|n| n.checked_add(document.fonts.len() as u64 + 1))
        .ok_or_else(|| Error::limit("Export file count overflow"))?;
    if files > limits.max_export_files as u64 {
        return Err(Error::limit("More than the export file limit"));
    }
    if !document.backgrounds.is_empty() || !document.surfaces.is_empty() {
        scene_budget.pixels(pixels)?;
    }
    let backgrounds =
        crate::background_export::build(document, &options.backgrounds, limits, scene_budget)?;
    let mut tilesets = Vec::with_capacity(document.tilesets.len());
    for t in &document.tilesets {
        let (w, h) = t.padded_size(pad);
        if w > max as u64 || h > max as u64 {
            return Err(Error::limit(format!(
                "Tileset `{}` is {w}x{h} texels with padding {pad}, more than the largest side of {max}; \
                 raise `max_side` or use fewer columns",
                t.name
            )));
        }
        Image::check_size(w as u32, h as u32, limits)?;
        tilesets.push(TilesetExport {
            name: t.name.clone(),
            file: format!("tileset-{}.png", t.name),
            image: t.render_padded(pad, options.atlas.extrude),
            tile_width: t.tile_width,
            tile_height: t.tile_height,
            columns: t.columns,
            padding: pad,
            tiles: t.tiles.iter().map(|s| s.name.clone()).collect(),
            terrains: t
                .terrains
                .iter()
                .map(|r| (r.name.clone(), r.scheme, r.first))
                .collect(),
        });
    }
    let fonts = document
        .fonts
        .iter()
        .map(|f| {
            Ok(FontExport {
                name: f.name.clone(),
                file: format!("font-{}.ttf", f.name),
                ttf: f.to_ttf(&TtfOptions::default())?,
                height: f.height,
                baseline: f.baseline,
                line_height: f.line_height,
                units_per_pixel: f.units_per_pixel(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Export {
        scene_work: limits.max_scene_work - scene_budget.work,
        backgrounds,
        themes: Vec::new(),
        atlas,
        tilesets,
        tilemaps: document.tilemaps.clone(),
        fonts,
    })
}
