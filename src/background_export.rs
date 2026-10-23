//! Native layer files and reconstruction metadata, independent of atlas packing.
use crate::json::{Writer, list};
use crate::scene::Budget;
use crate::{
    BackgroundRenderOptions, Document, Error, ErrorCode, Image, LayerOptions, Limits, Result,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BackgroundMode {
    None,
    Flat,
    Layers,
    #[default]
    Both,
}
impl BackgroundMode {
    pub(crate) fn flat(self) -> bool {
        matches!(self, Self::Flat | Self::Both)
    }
    pub(crate) fn layers(self) -> bool {
        matches!(self, Self::Layers | Self::Both)
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BackgroundExportOptions {
    pub mode: BackgroundMode,
    pub selection: Option<String>,
    pub camera: (i32, i32),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackgroundFlatExport {
    pub file: String,
    pub image: Image,
    pub camera: (i32, i32),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackgroundLayerExport {
    pub name: String,
    pub file: String,
    pub image: Image,
    pub options: LayerOptions,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackgroundExport {
    pub name: String,
    pub size: (u32, u32),
    pub flat: Option<BackgroundFlatExport>,
    pub layers: Vec<BackgroundLayerExport>,
}

pub(crate) fn build(
    document: &Document,
    options: &BackgroundExportOptions,
    limits: &Limits,
    b: &mut Budget,
) -> Result<Vec<BackgroundExport>> {
    if let Some(name) = &options.selection {
        if document.background(name).is_none() {
            return Err(Error::new(
                ErrorCode::UnknownName,
                format!("Unknown background `{name}`"),
            ));
        }
    }
    if options.mode == BackgroundMode::None {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for (i, bg) in document
        .backgrounds
        .iter()
        .enumerate()
        .filter(|(_, bg)| options.selection.as_deref().is_none_or(|n| bg.name() == n))
    {
        let natives = bg.native_layers(document, limits, b)?;
        let flat = if options.mode.flat() {
            Some(BackgroundFlatExport {
                file: format!("background-{i:04}.png"),
                image: bg.flatten(
                    &natives,
                    &BackgroundRenderOptions {
                        camera: options.camera,
                    },
                    limits,
                    b,
                )?,
                camera: options.camera,
            })
        } else {
            None
        };
        let mut layers = Vec::new();
        for (j, (layer, image)) in bg.layers.iter().zip(natives).enumerate() {
            if options.mode.layers() {
                layers.push(BackgroundLayerExport {
                    name: layer.name().into(),
                    file: format!("background-{i:04}-layer-{j:04}.png"),
                    image,
                    options: layer.options.clone(),
                });
            } else {
                b.pixels += image.width() as u64 * image.height() as u64;
            }
        }
        result.push(BackgroundExport {
            name: bg.name().into(),
            size: bg.size(),
            flat,
            layers,
        });
    }
    Ok(result)
}
pub(crate) fn file_names(doc: &Document, o: &BackgroundExportOptions) -> Vec<String> {
    let mut names = Vec::new();
    for (i, b) in doc
        .backgrounds
        .iter()
        .enumerate()
        .filter(|(_, b)| o.selection.as_deref().is_none_or(|n| b.name() == n))
    {
        if o.mode.flat() {
            names.push(format!("background-{i:04}.png"));
        }
        if o.mode.layers() {
            for j in 0..b.layers.len() {
                names.push(format!("background-{i:04}-layer-{j:04}.png"));
            }
        }
    }
    names
}
pub(crate) fn write_json(w: &mut Writer, backgrounds: &[BackgroundExport]) {
    if backgrounds.is_empty() {
        return;
    }
    w.object(Some("backgrounds"));
    w.raw(Some("schema"), "1");
    w.string(Some("units"), "pixel");
    w.array(Some("items"));
    for bg in backgrounds {
        w.object(None);
        w.string(Some("name"), &bg.name);
        w.raw(Some("size"), &list(&[bg.size.0, bg.size.1]));
        if let Some(flat) = &bg.flat {
            w.object(Some("flat"));
            w.string(Some("image"), &flat.file);
            w.raw(
                Some("camera"),
                &format!("[{}, {}]", flat.camera.0, flat.camera.1),
            );
            w.close('}');
        }
        if !bg.layers.is_empty() {
            w.array(Some("layers"));
            for layer in &bg.layers {
                w.object(None);
                w.string(Some("name"), &layer.name);
                w.string(Some("image"), &layer.file);
                w.raw(
                    Some("size"),
                    &list(&[layer.image.width(), layer.image.height()]),
                );
                w.raw(
                    Some("origin"),
                    &format!("[{}, {}]", layer.options.origin.0, layer.options.origin.1),
                );
                w.string(Some("repeat"), layer.options.repeat.as_str());
                w.object(Some("parallax"));
                for (axis, r) in [
                    ("x", layer.options.parallax.0),
                    ("y", layer.options.parallax.1),
                ] {
                    w.object(Some(axis));
                    w.raw(Some("num"), &r.numerator().to_string());
                    w.raw(Some("den"), &r.denominator().to_string());
                    w.close('}');
                }
                w.close('}');
                w.close('}');
            }
            w.close(']');
        }
        w.close('}');
    }
    w.close(']');
    w.close('}');
}
pub(crate) fn add_files(files: &mut Vec<(String, Vec<u8>)>, backgrounds: &[BackgroundExport]) {
    for bg in backgrounds {
        if let Some(f) = &bg.flat {
            files.push((f.file.clone(), crate::png::encode(&f.image)));
        }
        for l in &bg.layers {
            files.push((l.file.clone(), crate::png::encode(&l.image)));
        }
    }
}
pub(crate) fn rename_themed(backgrounds: &mut [BackgroundExport], theme: usize) {
    let rename = |file: &mut String| {
        let stem = file.strip_suffix(".png").unwrap();
        *file = format!("{stem}-theme-{theme:04}.png");
    };
    for bg in backgrounds {
        if let Some(f) = &mut bg.flat {
            rename(&mut f.file);
        }
        for l in &mut bg.layers {
            rename(&mut l.file);
        }
    }
}
