//! The metadata a game reads: every value of a small file, exactly.

use spritebox::{AtlasOptions, Document, ErrorCode, ExportOptions, Insets, Limits, check};

const GAME: &str = "
palette p
  .  transparent
  k  #000000
  w  #ffffff
  g  #00ff00

sprite hero 2x3
  .k
  kk
  k.

sprite panel 4x4 slice=1
  kkkk
  kwwk
  kwwk
  kkkk

sprite wide_panel = panel:pad(1)

animation walk loop
  hero         5
  hero:flip_x  5

tileset ground 2x2
  tile block
    gg
    gg

tilemap level
  legend
    .  empty
    #  block  solid
  map
    ...
    #.#
";

#[test]
fn every_value_of_the_metadata() {
    let doc: Document = GAME.parse().unwrap();
    let options = ExportOptions {
        atlas: AtlasOptions {
            padding: 1,
            ..AtlasOptions::default()
        },
    };
    let export = doc.export(&options).unwrap();
    let atlas = &export.atlas;
    let hero = atlas.region("hero").unwrap();
    let panel = atlas.region("panel").unwrap();
    let wide = atlas.region("wide_panel").unwrap();
    // Rectangles are the sprites themselves, without padding, from the atlas's
    // top-left corner with y down.
    assert_eq!((hero.width, hero.height), (2, 3));
    assert_eq!((panel.width, panel.height), (4, 4));
    assert_eq!((wide.width, wide.height), (6, 6));
    for r in [hero, panel, wide] {
        assert!(r.x >= 1 && r.y >= 1);
    }
    let r = |r: spritebox::Rect| format!("[{}, {}, {}, {}]", r.x, r.y, r.width, r.height);
    let expected = format!(
        r#"{{
  "format": "spritebox",
  "version": 1,
  "atlas": {{
    "image": "atlas.png",
    "size": [{w}, {h}],
    "padding": 1,
    "sprites": {{
      "hero": {hero},
      "panel": {panel},
      "wide_panel": {wide}
    }},
    "panels": {{
      "panel": {{"left": 1, "top": 1, "right": 1, "bottom": 1}},
      "wide_panel": {{"left": 2, "top": 2, "right": 2, "bottom": 2}}
    }}
  }},
  "animations": {{
    "walk": {{
      "loop": true,
      "frames": [
        {{"sprite": "hero", "rect": {hero}, "ticks": 5, "flip_x": false, "flip_y": false}},
        {{"sprite": "hero", "rect": {hero}, "ticks": 5, "flip_x": true, "flip_y": false}}
      ]
    }}
  }},
  "tilesets": {{
    "ground": {{
      "image": "tileset-ground.png",
      "size": [4, 4],
      "tile_size": [2, 2],
      "columns": 1,
      "padding": 1,
      "tiles": [
        "block"
      ],
      "terrains": {{}}
    }}
  }},
  "tilemaps": {{
    "level": {{
      "tileset": "ground",
      "size": [3, 2],
      "tiles": [
        [0, 0, 0],
        [1, 0, 1]
      ],
      "solid": [1]
    }}
  }},
  "fonts": {{}}
}}
"#,
        w = atlas.image.width(),
        h = atlas.image.height(),
        hero = r(hero),
        panel = r(panel),
        wide = r(wide),
    );
    assert_eq!(export.to_json(), expected);
    // The atlas holds no flipped copy of the hero.
    assert_eq!(atlas.regions.len(), 3);
    // The pixels under each rectangle are the sprite's.
    let hero_image = doc.sprite("hero").unwrap().render();
    for y in 0..3 {
        for x in 0..2 {
            assert_eq!(
                atlas.image.pixel(hero.x + x, hero.y + y),
                hero_image.pixel(x, y)
            );
        }
    }
}

#[test]
fn panel_insets() {
    let base = "palette p\n  k #000000\n  w #ffffff\nsprite p 6x4 slice=1,2,3,0\n  kkkkkk\n  kwwwwk\n  kwwwwk\n  kkkkkk\n";
    let doc: Document = format!(
        "{base}sprite fx = p:flip_x\nsprite fy = p:flip_y\nsprite r = p:rot90\n\
         sprite padded = p:pad(2)\nsprite moved = p:offset(1,0)\nsprite own = p:offset(1,0) slice=2\n\
         sprite recolored = p:swap(p)\n"
    )
    .parse()
    .unwrap();
    let slice = |name: &str| doc.sprite(name).unwrap().slice();
    let i = |left, top, right, bottom| {
        Some(Insets {
            left,
            top,
            right,
            bottom,
        })
    };
    assert_eq!(slice("p"), i(1, 2, 3, 0));
    assert_eq!(slice("fx"), i(3, 2, 1, 0));
    assert_eq!(slice("fy"), i(1, 0, 3, 2));
    // A quarter turn clockwise moves the left border to the top.
    assert_eq!(slice("r"), i(0, 1, 2, 3));
    assert_eq!(slice("padded"), i(3, 4, 5, 2));
    assert_eq!(slice("moved"), None, "an offset drops the borders");
    assert_eq!(slice("own"), i(2, 2, 2, 2));
    assert_eq!(slice("recolored"), i(1, 2, 3, 0));
    assert!(
        Insets {
            left: 3,
            top: 0,
            right: 3,
            bottom: 0
        }
        .fits(6, 1)
    );
    assert!(
        !Insets {
            left: 4,
            top: 0,
            right: 3,
            bottom: 0
        }
        .fits(6, 1)
    );
}

#[test]
fn panel_errors() {
    let errors = |text: &str| -> Vec<(usize, usize, ErrorCode)> {
        check(text, &Limits::default())
            .iter()
            .map(|e| (e.position.unwrap().line, e.position.unwrap().column, e.code))
            .collect()
    };
    let p = "palette p\n  k #000000\n";
    let one = |header: &str| errors(&format!("{p}{header}\n  kk\n  kk\n"));
    assert_eq!(one("sprite s 2x2 slice=1"), []);
    assert_eq!(
        one("sprite s 2x2 slice=2"),
        [(3, 20, ErrorCode::InvalidArgument)]
    );
    assert_eq!(one("sprite s 2x2 slice=1,1"), [(3, 20, ErrorCode::Syntax)]);
    assert_eq!(one("sprite s 2x2 slice=a"), [(3, 20, ErrorCode::Syntax)]);
    assert_eq!(
        one("sprite s 2x2 slice=1,0,1,-1"),
        [(3, 20, ErrorCode::Syntax)]
    );
    let derived = |line: &str| errors(&format!("{p}sprite s 2x2\n  kk\n  kk\n{line}\n"));
    assert_eq!(derived("sprite d = s slice=1"), []);
    assert_eq!(
        derived("sprite d = s slice=3"),
        [(6, 20, ErrorCode::InvalidArgument)]
    );
    assert_eq!(
        derived("sprite d = s slice=1 slice=1"),
        [(6, 22, ErrorCode::Syntax)]
    );
    assert_eq!(derived("sprite d = s wide"), [(6, 14, ErrorCode::Syntax)]);
}
