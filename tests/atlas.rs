//! Atlas packing and its metadata.

use spritebox::{AtlasOptions, Color, Document, ErrorCode, Image, Limits, Rect, png};

const SHOWCASE: &str = include_str!("../examples/showcase.spritebox");

fn crop(image: &Image, r: Rect) -> Vec<Option<Color>> {
    let mut out = Vec::new();
    for y in r.y..r.y + r.height {
        for x in r.x..r.x + r.width {
            out.push(image.pixel(x, y));
        }
    }
    out
}

fn inside(r: Rect, x: u32, y: u32, margin: u32) -> bool {
    x + margin >= r.x
        && x < r.x + r.width + margin
        && y + margin >= r.y
        && y < r.y + r.height + margin
}

#[test]
fn regions_hold_the_sprites() {
    let doc: Document = SHOWCASE.parse().unwrap();
    for padding in [0, 1, 3] {
        let atlas = doc
            .atlas(&AtlasOptions {
                padding,
                extrude: false,
                ..AtlasOptions::default()
            })
            .unwrap();
        let names: Vec<&str> = atlas.regions.iter().map(|r| r.name.as_str()).collect();
        let expected: Vec<&str> = doc
            .sprites()
            .iter()
            .chain(doc.frame_sprites())
            .map(|s| s.name())
            .collect();
        assert_eq!(names, expected);
        for region in &atlas.regions {
            let sprite = doc.sprite(&region.name).unwrap().render();
            assert_eq!(region.rect.width, sprite.width());
            assert_eq!(region.rect.height, sprite.height());
            assert_eq!(
                crop(&atlas.image, region.rect),
                crop(
                    &sprite,
                    Rect {
                        x: 0,
                        y: 0,
                        width: sprite.width(),
                        height: sprite.height()
                    }
                ),
                "{}",
                region.name
            );
            assert!(region.rect.x >= padding && region.rect.y >= padding);
            assert!(region.rect.x + region.rect.width + padding <= atlas.image.width());
            assert!(region.rect.y + region.rect.height + padding <= atlas.image.height());
        }
        // Distinct rectangles keep 2 * padding apart: their padded areas never meet.
        let mut rects: Vec<Rect> = atlas.regions.iter().map(|r| r.rect).collect();
        rects.sort_by_key(|r| (r.x, r.y));
        rects.dedup();
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                let apart = a.x + a.width + 2 * padding <= b.x
                    || b.x + b.width + 2 * padding <= a.x
                    || a.y + a.height + 2 * padding <= b.y
                    || b.y + b.height + 2 * padding <= a.y;
                assert!(apart, "{a:?} {b:?} with padding {padding}");
            }
        }
        // Pixels outside every rectangle are transparent.
        for y in 0..atlas.image.height() {
            for x in 0..atlas.image.width() {
                if !rects.iter().any(|&r| inside(r, x, y, 0)) {
                    assert_eq!(
                        atlas.image.pixel(x, y),
                        Some(Color::TRANSPARENT),
                        "({x}, {y})"
                    );
                }
            }
        }
    }
}

#[test]
fn identical_sprites_share_a_rectangle() {
    let doc: Document = SHOWCASE.parse().unwrap();
    let atlas = doc.atlas(&AtlasOptions::default()).unwrap();
    assert_eq!(
        atlas.region("sprout_night"),
        atlas.region("sprout:swap(dusk)")
    );
    assert_ne!(atlas.region("sprout"), atlas.region("sprout_left"));
    assert_eq!(atlas.region("nothing"), None);
}

#[test]
fn animations_carry_rectangles_and_ticks() {
    let doc: Document = SHOWCASE.parse().unwrap();
    let atlas = doc.atlas(&AtlasOptions::default()).unwrap();
    let spin = atlas
        .animations
        .iter()
        .find(|a| a.name == "coin_spin")
        .unwrap();
    assert!(spin.looping);
    let frames: Vec<(&str, u32)> = spin
        .frames
        .iter()
        .map(|f| (f.sprite.as_str(), f.ticks))
        .collect();
    assert_eq!(
        frames,
        [
            ("coin", 6),
            ("coin_edge", 6),
            ("coin_thin", 6),
            ("coin_edge:flip_x", 6)
        ]
    );
    for frame in &spin.frames {
        assert_eq!(Some(frame.rect), atlas.region(&frame.sprite));
    }
    let hurt = atlas
        .animations
        .iter()
        .find(|a| a.name == "sprout_hurt")
        .unwrap();
    assert!(!hurt.looping);
}

#[test]
fn extrusion_copies_edges_into_the_padding() {
    let doc: Document = "palette p\n  a #ff0000\n  b #0000ff\nsprite s 2x2\n  ab\n  ba\n"
        .parse()
        .unwrap();
    let atlas = doc
        .atlas(&AtlasOptions {
            padding: 2,
            extrude: true,
            ..AtlasOptions::default()
        })
        .unwrap();
    let r = atlas.region("s").unwrap();
    assert_eq!(
        (r.x, r.y, atlas.image.width(), atlas.image.height()),
        (2, 2, 6, 6)
    );
    let red = Some(Color::rgb(255, 0, 0));
    let blue = Some(Color::rgb(0, 0, 255));
    let rows: Vec<Vec<Option<Color>>> = (0..6)
        .map(|y| (0..6).map(|x| atlas.image.pixel(x, y)).collect())
        .collect();
    assert_eq!(rows[0], [red, red, red, blue, blue, blue]);
    assert_eq!(rows[2], [red, red, red, blue, blue, blue]);
    assert_eq!(rows[3], [blue, blue, blue, red, red, red]);
    assert_eq!(rows[5], [blue, blue, blue, red, red, red]);
}

#[test]
fn json_metadata() {
    let doc: Document = "
palette p
  .  transparent
  a  #ff0000
sprite one 2x1
  a.
sprite two = one:flip_x
animation blink once
  one         3
  one:flip_y  2
"
    .parse()
    .unwrap();
    let atlas = doc.atlas(&AtlasOptions::default()).unwrap();
    let r = |name: &str| {
        let r = atlas.region(name).unwrap();
        format!("[{}, {}, {}, {}]", r.x, r.y, r.width, r.height)
    };
    let expected = format!(
        r#"{{
  "format": "spritebox",
  "version": 1,
  "atlas": {{
    "image": "atlas.png",
    "size": [{w}, {h}],
    "padding": 1,
    "sprites": {{
      "one": {one},
      "two": {two},
      "one:flip_y": {one}
    }}
  }},
  "animations": {{
    "blink": {{
      "loop": false,
      "frames": [
        {{"sprite": "one", "rect": {one}, "ticks": 3}},
        {{"sprite": "one:flip_y", "rect": {one}, "ticks": 2}}
      ]
    }}
  }}
}}
"#,
        w = atlas.image.width(),
        h = atlas.image.height(),
        one = r("one"),
        two = r("two"),
    );
    assert_eq!(atlas.to_json("atlas.png"), expected);
    assert_eq!(
        atlas.region("one"),
        atlas.region("one:flip_y"),
        "a 1-row sprite flipped vertically is the same"
    );
}

#[test]
fn empty_documents_give_an_empty_atlas() {
    let atlas = Document::parse("")
        .unwrap()
        .atlas(&AtlasOptions::default())
        .unwrap();
    assert_eq!((atlas.image.width(), atlas.image.height()), (1, 1));
    assert!(atlas.regions.is_empty());
    assert!(atlas.to_json("a.png").contains("\"sprites\": {}"));
}

#[test]
fn deterministic() {
    let make = || {
        let doc: Document = SHOWCASE.parse().unwrap();
        let atlas = doc.atlas(&AtlasOptions::default()).unwrap();
        (png::encode(&atlas.image), atlas.to_json("atlas.png"))
    };
    assert_eq!(make(), make());
}

#[test]
fn options_and_limits() {
    let doc: Document = SHOWCASE.parse().unwrap();
    let err = doc
        .atlas(&AtlasOptions {
            padding: 65,
            extrude: false,
            ..AtlasOptions::default()
        })
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArgument);
    // A largest side beyond the image limits is not a valid option.
    let limits = Limits {
        max_image_side: 16,
        ..Limits::default()
    };
    let err = doc
        .atlas_with(&AtlasOptions::default(), &limits)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArgument);
    for max_side in [0, 9000] {
        let options = AtlasOptions {
            max_side,
            ..AtlasOptions::default()
        };
        assert_eq!(
            doc.atlas(&options).unwrap_err().code,
            ErrorCode::InvalidArgument
        );
    }
}

#[test]
fn rgba_bytes_are_exactly_as_documented() {
    // Straight (not premultiplied) sRGB RGBA8, rows top first, tightly packed.
    let doc: Document = "palette p
  .  transparent
  r  #ff0000
  h  #0000ff80
  w  #ffffff
sprite s 2x2
  r.
  hw
"
    .parse()
    .unwrap();
    let image = doc.sprite("s").unwrap().render();
    assert_eq!(
        image.pixels(),
        [
            255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 255, 128, 255, 255, 255, 255
        ]
    );
    // In the atlas, the region holds the same bytes.
    let atlas = doc.atlas(&AtlasOptions::default()).unwrap();
    let r = atlas.region("s").unwrap();
    let mut bytes = Vec::new();
    for y in r.y..r.y + 2 {
        for x in r.x..r.x + 2 {
            bytes.extend(atlas.image.pixel(x, y).unwrap().to_array());
        }
    }
    assert_eq!(bytes, image.pixels());
}

#[test]
fn every_sprite_border_is_extruded_by_default() {
    let doc: Document = SHOWCASE.parse().unwrap();
    for padding in [1, 2] {
        let options = AtlasOptions {
            padding,
            ..AtlasOptions::default()
        };
        assert!(options.extrude);
        let atlas = doc.atlas(&options).unwrap();
        for region in &atlas.regions {
            let r = region.rect;
            let p = padding as i64;
            for y in r.y as i64 - p..(r.y + r.height) as i64 + p {
                for x in r.x as i64 - p..(r.x + r.width) as i64 + p {
                    let cx = x.clamp(r.x as i64, (r.x + r.width - 1) as i64) as u32;
                    let cy = y.clamp(r.y as i64, (r.y + r.height - 1) as i64) as u32;
                    assert_eq!(
                        atlas.image.pixel(x as u32, y as u32),
                        atlas.image.pixel(cx, cy),
                        "{} at ({x}, {y})",
                        region.name
                    );
                }
            }
        }
    }
}

#[test]
fn tileset_textures_are_padded_and_extruded() {
    let doc: Document = include_str!("../examples/tiles.spritebox").parse().unwrap();
    for (padding, extrude) in [(0, true), (1, true), (2, false)] {
        let options = spritebox::ExportOptions {
            atlas: AtlasOptions {
                padding,
                extrude,
                ..AtlasOptions::default()
            },
        };
        let export = doc.export(&options).unwrap();
        let t = &export.tilesets[0];
        let tileset = doc.tileset("ground").unwrap();
        assert_eq!(t.padding, padding);
        let cell = 8 + 2 * padding;
        assert_eq!((t.image.width(), t.image.height()), (8 * cell, 9 * cell));
        for index in 1..=tileset.tile_count() {
            let r = t.tile_rect(index).unwrap();
            assert_eq!(
                (r.x, r.y),
                (
                    padding + (index - 1) % 8 * cell,
                    padding + (index - 1) / 8 * cell
                )
            );
            let tile = tileset.tile(index).unwrap().render();
            let p = padding as i64;
            for y in -p..8 + p {
                for x in -p..8 + p {
                    let inside = (0..8).contains(&x) && (0..8).contains(&y);
                    let expected = if inside || extrude {
                        tile.pixel(x.clamp(0, 7) as u32, y.clamp(0, 7) as u32)
                    } else {
                        Some(Color::TRANSPARENT)
                    };
                    let got = t
                        .image
                        .pixel((r.x as i64 + x) as u32, (r.y as i64 + y) as u32);
                    assert_eq!(got, expected, "tile {index} at ({x}, {y})");
                }
            }
        }
        assert_eq!(t.tile_rect(0), None);
        assert_eq!(t.tile_rect(tileset.tile_count() + 1), None);
    }
}

#[test]
fn art_too_large_for_the_bound_is_an_error() {
    let doc: Document = SHOWCASE.parse().unwrap();
    let options = AtlasOptions {
        max_side: 20,
        ..AtlasOptions::default()
    };
    let err = doc.atlas(&options).unwrap_err();
    assert_eq!(err.code, ErrorCode::LimitExceeded);
    assert!(err.message.contains("20x20"), "{}", err.message);
    // The default bound is 2048, the largest side every WebGPU device supports.
    assert_eq!(AtlasOptions::default().max_side, 2048);
    let mut text = String::from("palette p\n  a #ff0000\n");
    for i in 0..5 {
        text.push_str(&format!("sprite s{i} 1000x1\n  {}\n", "a".repeat(1000)));
    }
    let wide: Document = text.parse().unwrap();
    assert_eq!(
        wide.atlas(&AtlasOptions::default()).unwrap().image.width(),
        1002
    );
    let options = AtlasOptions {
        max_side: 1001,
        ..AtlasOptions::default()
    };
    assert_eq!(
        wide.atlas(&options).unwrap_err().code,
        ErrorCode::LimitExceeded
    );
    // Tileset textures obey the bound too.
    let tiles: Document = include_str!("../examples/tiles.spritebox").parse().unwrap();
    let export = spritebox::ExportOptions {
        atlas: AtlasOptions {
            max_side: 64,
            ..AtlasOptions::default()
        },
    };
    let err = tiles.export(&export).unwrap_err();
    assert_eq!(err.code, ErrorCode::LimitExceeded);
    assert!(err.message.contains("Tileset `ground`"), "{}", err.message);
}

#[test]
fn packs_many_sprites_tightly() {
    let mut text = String::from("palette p\n  a #ff0000\n  b #00ff00\n");
    for i in 0..60 {
        let (w, h) = (3 + i % 13, 2 + (i * 7) % 11);
        text.push_str(&format!("sprite s{i} {w}x{h}\n"));
        for y in 0..h {
            let row: String = (0..w)
                .map(|x| if (x + y + i) % 3 == 0 { 'a' } else { 'b' })
                .collect();
            text.push_str(&format!("  {row}\n"));
        }
    }
    let doc: Document = text.parse().unwrap();
    let atlas = doc
        .atlas(&AtlasOptions {
            padding: 0,
            extrude: false,
            ..AtlasOptions::default()
        })
        .unwrap();
    let used: u64 = atlas
        .regions
        .iter()
        .map(|r| r.rect.width as u64 * r.rect.height as u64)
        .sum();
    let total = atlas.image.width() as u64 * atlas.image.height() as u64;
    assert!(total < used * 13 / 10, "{total} pixels for {used}");
}
