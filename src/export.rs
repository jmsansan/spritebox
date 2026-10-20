//! Everything a game needs from a document: the atlas, a texture for each tileset,
//! the tilemaps, and their metadata in one JSON file.

use crate::atlas::write_header;
use crate::json::{Writer, list};
use crate::{Atlas, AtlasOptions, Document, Image, Limits, Result, Scheme, Tilemap};

/// How a document is exported.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExportOptions {
    pub atlas: AtlasOptions,
}

/// A tileset's texture and what the metadata says of it.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct TilesetExport {
    pub name: String,
    /// The texture's file name: `tileset-NAME.png`.
    pub file: String,
    /// Tile 1 at the top left, then row by row, `columns` a row, with no spacing.
    pub image: Image,
    pub tile_width: u32,
    pub tile_height: u32,
    pub columns: u32,
    /// The tiles' names in index order: `tiles[0]` is tile 1. A terrain's variants
    /// are `TERRAIN/VARIANT`.
    pub tiles: Vec<String>,
    /// Each terrain's name, scheme and first tile index.
    pub terrains: Vec<(String, Scheme, u32)>,
}

/// The files of a document: see [`Export::files`].
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Export {
    pub atlas: Atlas,
    pub tilesets: Vec<TilesetExport>,
    pub tilemaps: Vec<Tilemap>,
}

/// The name of the atlas texture in exported files.
pub const ATLAS_FILE: &str = "atlas.png";
/// The name of the metadata file.
pub const METADATA_FILE: &str = "spritebox.json";

impl Export {
    /// The metadata as JSON: the atlas and animations, as [`Atlas::to_json`] writes
    /// them, then `tilesets` and `tilemaps`. See the README for the form.
    pub fn to_json(&self) -> String {
        let mut w = Writer::new();
        w.object(None);
        write_header(&mut w);
        self.atlas.write_json(&mut w, ATLAS_FILE);
        w.object(Some("tilesets"));
        for t in &self.tilesets {
            w.object(Some(&t.name));
            w.string(Some("image"), &t.file);
            w.raw(Some("size"), &list(&[t.image.width(), t.image.height()]));
            w.raw(Some("tile_size"), &list(&[t.tile_width, t.tile_height]));
            w.raw(Some("columns"), &t.columns.to_string());
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
        w.close('}');
        w.finish()
    }

    /// Every file, as `(name, bytes)`: `atlas.png`, then `tileset-NAME.png` for each
    /// tileset, then `spritebox.json`.
    pub fn files(&self) -> Vec<(String, Vec<u8>)> {
        let mut files = vec![(ATLAS_FILE.to_owned(), crate::png::encode(&self.atlas.image))];
        for t in &self.tilesets {
            files.push((t.file.clone(), crate::png::encode(&t.image)));
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
    let atlas = document.atlas_with(&options.atlas, limits)?;
    let tilesets = document
        .tilesets
        .iter()
        .map(|t| TilesetExport {
            name: t.name.clone(),
            file: format!("tileset-{}.png", t.name),
            image: t.render(),
            tile_width: t.tile_width,
            tile_height: t.tile_height,
            columns: t.columns,
            tiles: t.tiles.iter().map(|s| s.name.clone()).collect(),
            terrains: t
                .terrains
                .iter()
                .map(|r| (r.name.clone(), r.scheme, r.first))
                .collect(),
        })
        .collect();
    Ok(Export {
        atlas,
        tilesets,
        tilemaps: document.tilemaps.clone(),
    })
}
