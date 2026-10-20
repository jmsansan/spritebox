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
        })
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidArgument);
    let limits = Limits {
        max_image_side: 16,
        ..Limits::default()
    };
    let err = doc
        .atlas_with(&AtlasOptions::default(), &limits)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::LimitExceeded);
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
