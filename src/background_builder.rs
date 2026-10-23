//! Typed scene authoring, using the same validation and generation as the parser.
use crate::background::{ScatterContext, check_rect, scatter};
use crate::scene::Budget;
use crate::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct BackgroundBuilder<'a> {
    doc: &'a Document,
    background: Background,
    limits: Limits,
    budget: Budget,
    sprites: HashMap<String, Arc<Sprite>>,
    operations: HashSet<String>,
}
impl Background {
    pub fn builder<'a>(
        doc: &'a Document,
        name: &str,
        size: (u32, u32),
        seed: u64,
        limits: &Limits,
    ) -> Result<BackgroundBuilder<'a>> {
        check_name(name)?;
        check_size(size, limits)?;
        let mut budget = Budget::new(limits);
        budget.pixels(doc.scene_storage())?;
        budget.commands(doc.scene_commands())?;
        Ok(BackgroundBuilder {
            doc,
            background: Background {
                commands: 0,
                name: name.into(),
                width: size.0,
                height: size.1,
                seed,
                layers: Vec::new(),
            },
            limits: limits.clone(),
            budget,
            sprites: HashMap::new(),
            operations: HashSet::new(),
        })
    }
}
pub(crate) fn check_name(name: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
    {
        return Err(Error::invalid(
            "Scene name must contain ASCII letters, digits, underscores or hyphens",
        ));
    }
    Ok(())
}
fn check_size(size: (u32, u32), limits: &Limits) -> Result<()> {
    if size.0 == 0 || size.1 == 0 {
        return Err(Error::invalid("Scene size must be positive"));
    }
    limits.check_image(size.0, size.1)
}
impl BackgroundBuilder<'_> {
    pub fn layer(
        &mut self,
        name: &str,
        size: (u32, u32),
        options: LayerOptions,
    ) -> Result<&mut Self> {
        check_name(name)?;
        check_size(size, &self.limits)?;
        if size.0 > i32::MAX as u32 || size.1 > i32::MAX as u32 {
            return Err(Error::limit("Layer coordinates exceed signed range"));
        }
        if self.background.layers.len() >= self.limits.max_layers {
            return Err(Error::limit("Too many background layers"));
        }
        if self.background.layers.iter().any(|l| l.name == name) {
            return Err(Error::new(
                ErrorCode::DuplicateName,
                "Duplicate background layer",
            ));
        }
        if let Some(clip) = options.clip {
            check_rect(clip, size)?;
        }
        self.budget.commands(1)?;
        self.background.commands += 1;
        self.operations.clear();
        self.background.layers.push(BackgroundLayer {
            name: name.into(),
            width: size.0,
            height: size.1,
            options,
            items: Vec::new(),
            position: Position::new(1, 1),
        });
        Ok(self)
    }
    fn resolve(&mut self, source: &SourceRef) -> Result<(SourceRef, Option<Arc<Sprite>>)> {
        match source {
            SourceRef::Sprite(name) => {
                let canonical = crate::text::scene_chain_name(name, &self.limits)?;
                if let Some(s) = self.sprites.get(&canonical) {
                    return Ok((SourceRef::Sprite(canonical), Some(s.clone())));
                }
                let (name, sprite) =
                    crate::text::scene_chain(name, self.doc, &self.limits, &mut self.budget)?;
                let sprite = Arc::new(sprite);
                self.sprites.insert(name.clone(), sprite.clone());
                Ok((SourceRef::Sprite(name), Some(sprite)))
            }
            _ => Ok((source.clone(), None)),
        }
    }
    pub fn place(&mut self, source: SourceRef, options: PlacementOptions) -> Result<&mut Self> {
        if self.background.layers.is_empty() {
            return Err(Error::invalid("Add a layer before placing sources"));
        }
        self.budget.commands(1)?;
        self.background.commands += 1;
        let (source, sprite) = self.resolve(&source)?;
        let placement = BackgroundPlacement {
            source,
            sprite,
            options,
            position: Position::new(1, 1),
        };
        placement.crop(placement.view(self.doc)?.size()?)?;
        self.budget.pixels(1)?;
        self.background
            .layers
            .last_mut()
            .unwrap()
            .items
            .push(placement);
        Ok(self)
    }
    pub fn scatter(
        &mut self,
        name: &str,
        options: &ScatterOptions,
        variants: &[WeightedVariant],
    ) -> Result<&mut Self> {
        check_name(name)?;
        if !self.operations.insert(name.into()) {
            return Err(Error::new(
                ErrorCode::DuplicateName,
                "Duplicate layer operation",
            ));
        }
        self.budget.commands(
            variants
                .len()
                .checked_add(1)
                .ok_or_else(|| Error::limit("Scene command overflow"))?,
        )?;
        self.background.commands += 1 + variants.len();
        let mut resolved = Vec::new();
        for variant in variants {
            let (source, sprite) = self.resolve(&variant.source)?;
            resolved.push((
                WeightedVariant {
                    source,
                    weight: variant.weight,
                },
                sprite,
            ));
        }
        let layer = self
            .background
            .layers
            .last()
            .ok_or_else(|| Error::invalid("Add a layer before scattering"))?;
        let items = scatter(
            self.doc,
            ScatterContext {
                background: &self.background.name,
                layer,
                operation: name,
                owner_seed: self.background.seed,
                position: Position::new(1, 1),
            },
            options,
            &resolved,
            &mut self.budget,
        )?;
        self.background
            .layers
            .last_mut()
            .unwrap()
            .items
            .extend(items);
        Ok(self)
    }
    pub fn finish(self) -> Background {
        self.background
    }
}
impl Document {
    pub fn with_surface(&self, mut surface: Surface, limits: &Limits) -> Result<Document> {
        self.check_new_definition(limits)?;
        if self.surface(surface.name()).is_some() {
            return Err(Error::new(ErrorCode::DuplicateName, "Duplicate surface"));
        }
        limits.check_image(surface.width, surface.height)?;
        let palette = self.palette(surface.palette.name()).ok_or_else(|| {
            Error::new(
                ErrorCode::UnknownName,
                "Surface palette must exist in receiver document",
            )
        })?;
        if surface
            .keys
            .iter()
            .any(|&k| k != crate::sprite::EMPTY && palette.color(k).is_none())
        {
            return Err(Error::new(
                ErrorCode::UnknownName,
                "Surface key is absent from receiver palette",
            ));
        }
        surface.palette = palette.clone();
        let mut b = Budget::new(limits);
        b.pixels(self.scene_storage())?;
        b.pixels(surface.storage())?;
        b.commands(
            self.scene_commands()
                .checked_add(surface.commands)
                .ok_or_else(|| Error::limit("Scene command overflow"))?,
        )?;
        let mut doc = self.clone();
        doc.surfaces.push(surface);
        Ok(doc)
    }
    pub fn with_background(&self, background: Background, limits: &Limits) -> Result<Document> {
        self.check_new_definition(limits)?;
        if self.background(background.name()).is_some() {
            return Err(Error::new(ErrorCode::DuplicateName, "Duplicate background"));
        }
        limits.check_image(background.width, background.height)?;
        if background.layers.len() > limits.max_layers {
            return Err(Error::limit("Too many background layers"));
        }
        for layer in &background.layers {
            limits.check_image(layer.width, layer.height)?;
            if let Some(clip) = layer.options.clip {
                check_rect(clip, layer.size())?;
            }
            for p in &layer.items {
                p.crop(p.view(self)?.size()?)?;
                if let Some(s) = &p.sprite {
                    s.effect_size(s.width, s.height, s.layers.len(), limits)?;
                    let palette = self.palette(s.palette.name()).ok_or_else(|| {
                        Error::new(
                            ErrorCode::UnknownName,
                            "Background sprite palette is absent from receiver",
                        )
                    })?;
                    if let Some(key) = s.missing_key(palette) {
                        return Err(Error::new(
                            ErrorCode::UnknownName,
                            format!("Background key `{key}` is absent from receiver palette"),
                        ));
                    }
                }
            }
        }
        let mut b = Budget::new(limits);
        let mut shared = HashSet::new();
        b.pixels(self.scene_storage_seen(&mut shared))?;
        b.pixels(background.storage(&mut shared))?;
        b.commands(
            self.scene_commands()
                .checked_add(background.commands)
                .ok_or_else(|| Error::limit("Scene command overflow"))?,
        )?;
        let mut doc = self.clone();
        doc.backgrounds.push(background);
        Ok(doc)
    }
    fn check_new_definition(&self, limits: &Limits) -> Result<()> {
        let count = self.palettes.len()
            + self.sprites.len()
            + self.animations.len()
            + self.tilesets.len()
            + self.tilemaps.len()
            + self.fonts.len()
            + self.groups.len()
            + self.screens.len()
            + self.themes.len()
            + self.states.len()
            + self.icon_sets.len()
            + self.surfaces.len()
            + self.backgrounds.len();
        if count >= limits.max_definitions {
            Err(Error::limit("More than the definition limit"))
        } else {
            Ok(())
        }
    }
}
