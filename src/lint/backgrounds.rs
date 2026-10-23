use super::{Found, Kind, LintCode, Sink};
use crate::{Document, Rect};
pub(super) fn check(doc: &Document, sink: &mut Sink) {
    for bg in &doc.backgrounds {
        for layer in &bg.layers {
            for item in &layer.items {
                let Ok(view) = item.view(doc) else {
                    continue;
                };
                let Ok(size) = view.size() else {
                    continue;
                };
                let Ok(crop) = item.crop(size) else {
                    continue;
                };
                let (x, y) = item.top_left(crop);
                let clip = layer.options.clip.unwrap_or(Rect {
                    x: 0,
                    y: 0,
                    width: layer.width,
                    height: layer.height,
                });
                if x + crop.width as i64 <= clip.x as i64
                    || y + crop.height as i64 <= clip.y as i64
                    || x >= clip.x as i64 + clip.width as i64
                    || y >= clip.y as i64 + clip.height as i64
                {
                    sink.push(
                        Found {
                            code: LintCode::BackgroundClipped,
                            kind: Kind::Background,
                            name: &bg.name,
                            position: Some(item.position),
                            sprite: Some(&bg.name),
                            pixels: Vec::new(),
                        },
                        || {
                            format!(
                                "Placement is fully clipped by background `{}` layer `{}`",
                                bg.name, layer.name
                            )
                        },
                    );
                }
            }
        }
    }
}
