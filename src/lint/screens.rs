use super::{Found, Kind, LintCode, LintOptions, Sink};
use crate::sprite::EMPTY;
use crate::{Document, Limits, Screen, ScreenItem};

fn report(sink: &mut Sink, code: LintCode, screen: &Screen, item: &ScreenItem, message: &str) {
    sink.push(
        Found {
            code,
            kind: Kind::Screen,
            name: &screen.name,
            position: Some(item.position),
            sprite: Some(&item.sprite.name),
            pixels: Vec::new(),
        },
        || message.into(),
    );
}

pub(super) fn check(document: &Document, options: &LintOptions, limits: &Limits, sink: &mut Sink) {
    for screen in &document.screens {
        let Ok(mut image) = screen.canvas(document, limits) else {
            continue;
        };
        let mut occupied = if options.screen_overlap {
            vec![false; screen.width as usize * screen.height as usize]
        } else {
            Vec::new()
        };
        for item in &screen.items {
            let (x, y) = (item.x as i64, item.y as i64);
            let sprite = &item.sprite;
            if x < 0
                || y < 0
                || x + sprite.width as i64 > screen.width as i64
                || y + sprite.height as i64 > screen.height as i64
            {
                report(
                    sink,
                    LintCode::ScreenBounds,
                    screen,
                    item,
                    "The item falls partly or wholly outside the screen",
                );
            }
            if screen
                .grid
                .is_some_and(|g| x % g as i64 != 0 || y % g as i64 != 0)
            {
                report(
                    sink,
                    LintCode::OffGrid,
                    screen,
                    item,
                    "The item's position is not on the screen grid",
                );
            }
            let source = sprite.render();
            let mut overlap = false;
            let mut low_contrast = false;
            for sy in 0..sprite.height {
                for sx in 0..sprite.width {
                    let (tx, ty) = (x + sx as i64, y + sy as i64);
                    if tx < 0 || ty < 0 || tx >= screen.width as i64 || ty >= screen.height as i64 {
                        continue;
                    }
                    let color = source.pixel(sx, sy).unwrap();
                    let index = ty as usize * screen.width as usize + tx as usize;
                    if options.screen_overlap && color.a == 255 {
                        overlap |= occupied[index];
                        occupied[index] = true;
                    }
                    let mut text = false;
                    let mut backing = crate::Color::TRANSPARENT;
                    let pixel = sy as usize * sprite.width as usize + sx as usize;
                    for layer in &sprite.layers {
                        let key = layer.keys[pixel];
                        if key == EMPTY {
                            continue;
                        }
                        let layer_color = sprite
                            .palette
                            .color(key)
                            .unwrap_or(crate::Color::TRANSPARENT);
                        let alpha = layer_color.a;
                        if !layer.text {
                            backing = layer_color.over(backing);
                        }
                        if layer.text && alpha != 0 {
                            text = true;
                        } else if alpha == 255 {
                            text = false;
                        }
                    }
                    if text && !low_contrast {
                        let screen_below = image.pixel(tx as u32, ty as u32).unwrap();
                        let below = backing.over(screen_below);
                        if below.a != 0 {
                            let a = crate::oklab::Lab::from_color(color.over(screen_below)).l;
                            let b = crate::oklab::Lab::from_color(below).l;
                            low_contrast = a.abs_diff(b) < options.min_text_contrast;
                        }
                    }
                }
            }
            if overlap {
                report(
                    sink,
                    LintCode::ScreenOverlap,
                    screen,
                    item,
                    "Opaque pixels of screen items overlap",
                );
            }
            if low_contrast {
                report(
                    sink,
                    LintCode::LowContrast,
                    screen,
                    item,
                    "Text has too little lightness contrast against the pixels below it",
                );
            }
            image.over_from(&source, x, y);
        }
    }
}
