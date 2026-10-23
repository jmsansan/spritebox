//! Palette substitutions that retain sprite geometry and atlas placement.
use crate::{
    Color, Document, Error, ErrorCode, Export, ExportOptions, Image, Limits, Result, Sprite,
    TilesetExport,
};

/// A named set of color replacements for one base palette.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Theme {
    pub(crate) name: String,
    pub(crate) palette: String,
    pub(crate) colors: Vec<(char, Color)>,
}
impl Theme {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn palette(&self) -> &str {
        &self.palette
    }
    pub fn colors(&self) -> &[(char, Color)] {
        &self.colors
    }
}

/// Additional images for a theme. All use the base export's region metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeExport {
    pub name: String,
    pub file: String,
    pub image: Image,
    pub tilesets: Vec<TilesetExport>,
}

impl Document {
    pub fn themes(&self) -> &[Theme] {
        &self.themes
    }

    /// Apply a theme to retained palette keys. Geometry and atlas rectangles stay
    /// fixed, including when the theme changes alpha. Calling this again starts
    /// from the original base palettes, rather than accumulating substitutions.
    pub fn with_theme(&self, name: &str) -> Result<Document> {
        let theme =
            self.themes.iter().find(|t| t.name == name).ok_or_else(|| {
                Error::new(ErrorCode::UnknownName, format!("Unknown theme `{name}`"))
            })?;
        let mut doc = self.clone();
        doc.palettes = self.theme_bases.clone();
        let palette = doc
            .palettes
            .iter_mut()
            .find(|p| p.name == theme.palette)
            .ok_or_else(|| Error::new(ErrorCode::UnknownName, "Unknown theme palette"))?;
        for &(key, color) in &theme.colors {
            if let Some((_, c)) = palette.colors.iter_mut().find(|(k, _)| *k == key) {
                *c = color;
            }
        }
        doc.recolor_palettes();
        Ok(doc)
    }

    pub(crate) fn recolor_palettes(&mut self) {
        let palettes = &self.palettes;
        let apply = |sprite: &mut Sprite| {
            if let Some(p) = palettes.iter().find(|p| p.name == sprite.palette.name) {
                sprite.palette = p.clone();
            }
        };
        for sprite in self.sprites.iter_mut().chain(self.frame_sprites.iter_mut()) {
            apply(sprite);
        }
        for screen in &mut self.screens {
            for item in &mut screen.items {
                apply(&mut item.sprite);
            }
        }
        for tileset in &mut self.tilesets {
            if let Some(p) = palettes.iter().find(|p| p.name == tileset.palette.name) {
                tileset.palette = p.clone();
            }
            for sprite in &mut tileset.tiles {
                apply(sprite);
            }
        }
    }

    /// Export the base colors and one atlas/tileset image per theme.
    pub fn export_all_themes(&self, options: &ExportOptions) -> Result<Export> {
        self.export_all_themes_with(options, &Limits::default())
    }

    /// [`Document::export_all_themes`] within an aggregate image budget.
    pub fn export_all_themes_with(
        &self,
        options: &ExportOptions,
        limits: &Limits,
    ) -> Result<Export> {
        let mut base = self.clone();
        if !self.themes.is_empty() {
            base.palettes = self.theme_bases.clone();
            base.recolor_palettes();
        }
        let copies = self.themes.len() as u64 + 1;
        let mut result = crate::export::build_copies(&base, options, limits, copies)?;
        for theme in &self.themes {
            let themed = self.with_theme(&theme.name)?.export_with(options, limits)?;
            let mut tilesets = themed.tilesets;
            for tileset in &mut tilesets {
                tileset.file = format!("tileset-{}-{}.png", tileset.name, theme.name);
            }
            result.themes.push(ThemeExport {
                name: theme.name.clone(),
                file: format!("atlas-{}.png", theme.name),
                image: themed.atlas.image,
                tilesets,
            });
        }
        let mut names = std::collections::HashSet::new();
        for (name, _) in result.files() {
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(Error::invalid(format!(
                    "Themed output filenames collide at `{name}`"
                )));
            }
        }
        Ok(result)
    }
}

pub(crate) fn palette_signature(sprite: &Sprite) -> Vec<u8> {
    let mut key = Vec::new();
    key.extend_from_slice(&(sprite.palette.name.len() as u64).to_le_bytes());
    key.extend_from_slice(sprite.palette.name.as_bytes());
    for layer in &sprite.layers {
        key.extend_from_slice(&(layer.keys.len() as u64).to_le_bytes());
        for &c in &layer.keys {
            key.extend_from_slice(&(c as u32).to_le_bytes());
        }
    }
    key
}
