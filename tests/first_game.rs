//! The features that the first game's concept art asked for (0.6.0): layers in their
//! own palettes, `scale(n)`, stroked shapes, backgrounds inside backgrounds, tiles as
//! sprites, colors inside a text, and enlarged background exports.

use spritebox::{BackgroundRenderOptions, Color, Document, ErrorCode, Limits, check};

/// The errors of `text` as `(line, column, code, message)`.
fn errors(text: &str) -> Vec<(usize, usize, ErrorCode, String)> {
    check(text, &Limits::default())
        .into_iter()
        .map(|e| {
            let p = e.position.expect("text errors have positions");
            (p.line, p.column, e.code, e.message)
        })
        .collect()
}

fn parse(text: &str) -> Document {
    text.parse()
        .unwrap_or_else(|e| panic!("{e}\n{:?}", errors(text)))
}

// --- Layers with their own palettes (#56) ---------------------------------------

/// A stone in the colors of its rank, and a glyph in the color of its element.
const RUNE: &str = "\
palette rank
  .  transparent
  s  #606060
  d  #303030
  k  #000000
palette frost
  .  transparent
  g  #40c0ff
  h  #ffffff80
sprite stone 4x3 palette=rank
  dssd
  ssss
  dssd
sprite glyph 4x3 palette=frost
  .g..
  .gh.
  ....
sprite rune 4x3 palette=rank
  layer stone = stone
  layer glyph = glyph
";

#[test]
fn layers_keep_their_own_palettes() {
    let doc = parse(RUNE);
    let rune = doc.sprite("rune").unwrap();
    assert_eq!(rune.palette().name(), "rank");
    assert!(rune.mixes_palettes());
    let palettes: Vec<&str> = rune.layer_palettes().map(|p| p.name()).collect();
    assert_eq!(palettes, ["rank", "frost"]);
    let image = rune.render();
    let stone = Color::rgb(0x60, 0x60, 0x60);
    let dark = Color::rgb(0x30, 0x30, 0x30);
    let frost = Color::rgb(0x40, 0xc0, 0xff);
    let half_white = Color::rgba(255, 255, 255, 0x80);
    assert_eq!(image.pixel(0, 0), Some(dark));
    assert_eq!(image.pixel(1, 0), Some(frost));
    assert_eq!(image.pixel(1, 1), Some(frost));
    // Source-over, as for layers of one palette.
    assert_eq!(image.pixel(2, 1), Some(half_white.over(stone)));
    assert_eq!(image.pixel(3, 2), Some(dark));
    // The key seen is the glyph's, from the glyph's palette.
    assert_eq!(rune.key(1, 1), Some('g'));
    assert!(!doc.sprite("stone").unwrap().mixes_palettes());
}

#[test]
fn palette_free_modifiers_work_on_mixed_sprites() {
    let doc = parse(&format!(
        "{RUNE}sprite flipped = rune:flip_x:rot180:offset(1,0):pad(1)\n\
         sprite lined = rune:pad(1):outline(k)\n\
         sprite shaded = rune:shadow(k,1,1)\n"
    ));
    let rune = doc.sprite("rune").unwrap().render();
    let flipped = doc.sprite("flipped").unwrap();
    assert!(flipped.mixes_palettes());
    // flip_x then rot180 is flip_y; offset moves right by one; pad adds a border.
    let image = flipped.render();
    for y in 0..3 {
        for x in 1..4 {
            assert_eq!(image.pixel(x + 1, y + 1), rune.pixel(x - 1, 2 - y));
        }
    }
    let lined = doc.sprite("lined").unwrap();
    // The outline's key comes from the sprite's declared palette.
    assert_eq!(lined.render().pixel(0, 1), Some(Color::BLACK));
    assert_eq!(lined.render().pixel(2, 1), rune.pixel(1, 0));
    let shaded = doc.sprite("shaded").unwrap();
    assert_eq!(shaded.render().pixel(4, 3), Some(Color::BLACK));
}

#[test]
fn swapping_a_mixed_sprite_is_a_positioned_error() {
    let text = format!("{RUNE}sprite night = rune:swap(frost)\n");
    let found = errors(&text);
    assert_eq!(found.len(), 1, "{found:?}");
    let (line, column, code, message) = &found[0];
    // At the palette that `swap` names.
    assert_eq!((*line, *column, *code), (21, 26, ErrorCode::InvalidArgument));
    assert!(
        message.contains("`stone` uses `rank`") && message.contains("`glyph` uses `frost`"),
        "{message}"
    );
}

#[test]
fn mixed_sprites_in_themes_backgrounds_and_lint() {
    let text = format!(
        "{RUNE}theme ice of frost\n  g #0000ff\n\
         background shrine 6x5\n  layer runes 6x5\n    place sprite rune at=1,1\n"
    );
    let doc = parse(&text);
    let iced = doc.with_theme("ice").unwrap();
    // A theme recolors the layers drawn with its palette.
    assert_eq!(
        iced.sprite("rune").unwrap().render().pixel(1, 0),
        Some(Color::rgb(0, 0, 255))
    );
    let flat = |doc: &Document| {
        doc.background("shrine")
            .unwrap()
            .render(doc, &BackgroundRenderOptions::default())
            .unwrap()
    };
    assert_eq!(flat(&doc).pixel(2, 1), Some(Color::rgb(0x40, 0xc0, 0xff)));
    assert_eq!(flat(&doc).pixel(1, 1), Some(Color::rgb(0x30, 0x30, 0x30)));
    assert_eq!(flat(&iced).pixel(2, 1), Some(Color::rgb(0, 0, 255)));
    // Keys used through another palette's layer count as used in that palette.
    let lint = doc.lint(&spritebox::LintOptions::default());
    assert!(
        lint.warnings
            .iter()
            .all(|w| !w.message.contains("palette `frost`")),
        "{:?}",
        lint.warnings
    );
}

#[test]
fn mixed_sprites_are_not_tiles() {
    let text = format!("{RUNE}tileset t 4x3 palette=rank\n  tile r = rune\n");
    let found = errors(&text);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!((found[0].0, found[0].1), (22, 12));
    assert!(found[0].3.contains("mixes palettes"), "{}", found[0].3);
}

// --- scale(n) (#57) -----------------------------------------------------------

const ICONS: &str = "\
palette ui
  .  transparent
  k  #000000
  w  #ffffff
  r  #ff000080
sprite gem 3x2
  kw.
  .rk
sprite panel 4x4 slice=1,1,1,2
  kkkk
  kwwk
  kwwk
  kkkk
sprite big_gem = gem:scale(3)
sprite big_panel = panel:scale(2)
sprite wide_panel = panel:scale(2):size(12x10)
";

#[test]
fn scale_enlarges_every_pixel_to_a_block() {
    let doc = parse(ICONS);
    let gem = doc.sprite("gem").unwrap();
    let big = doc.sprite("big_gem").unwrap();
    assert_eq!((big.width(), big.height()), (9, 6));
    let (small, image) = (gem.render(), big.render());
    for y in 0..6 {
        for x in 0..9 {
            assert_eq!(image.pixel(x, y), small.pixel(x / 3, y / 3), "{x},{y}");
            assert_eq!(big.key(x, y), gem.key(x / 3, y / 3));
        }
    }
    assert_eq!(gem.scale(2).unwrap().name(), "gem:scale(2)");
    // The metadata sizes follow the sprite.
    let export = doc.export(&spritebox::ExportOptions::default()).unwrap();
    assert_eq!(export.atlas.region("big_gem").unwrap().width, 9);
    assert_eq!(export.atlas.region("big_gem").unwrap().height, 6);
}

#[test]
fn scale_multiplies_panel_insets() {
    let doc = parse(ICONS);
    let panel = doc.sprite("big_panel").unwrap();
    assert_eq!((panel.width(), panel.height()), (8, 8));
    let insets = panel.slice().unwrap();
    assert_eq!(
        (insets.left, insets.top, insets.right, insets.bottom),
        (2, 2, 2, 4)
    );
    let wide = doc.sprite("wide_panel").unwrap();
    assert_eq!((wide.width(), wide.height()), (12, 10));
    assert_eq!(wide.slice(), Some(insets));
    let json = doc
        .export(&spritebox::ExportOptions::default())
        .unwrap()
        .to_json();
    assert!(
        json.contains("\"big_panel\": {\"left\": 2, \"top\": 2, \"right\": 2, \"bottom\": 4}"),
        "{json}"
    );
}

#[test]
fn scale_rejects_other_factors_and_checks_limits() {
    for (n, column) in [("1", 24), ("9", 24), ("0", 24), ("x", 24)] {
        let text = format!("{ICONS}sprite bad = gem:scale({n})\n");
        let found = errors(&text);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].0, found[0].1), (17, column), "{n}: {found:?}");
    }
    let text = format!("{ICONS}sprite bad = gem:scale(2,2)\n");
    assert_eq!(errors(&text)[0].2, ErrorCode::Syntax);
    // 3 × 8 = 24 fits 24 pixels a side; 25 does not, and nothing is allocated.
    let limits = Limits {
        max_sprite_side: 24,
        ..Limits::default()
    };
    let at_limit = format!("{ICONS}sprite ok = gem:scale(8)\n");
    assert!(check(&at_limit, &limits).is_empty());
    let over = format!("{ICONS}sprite bad = gem:scale(8):pad(1)\n");
    let found = check(&over, &limits);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code, ErrorCode::LimitExceeded);
    let tall = format!("{ICONS}sprite bad = panel:scale(8)\n");
    let found = check(&tall, &limits);
    assert_eq!(found[0].code, ErrorCode::LimitExceeded, "{found:?}");
    let gem = parse(ICONS).sprite("gem").unwrap().clone();
    assert_eq!(gem.scale(1).unwrap_err().code, ErrorCode::InvalidArgument);
    assert_eq!(gem.scale(9).unwrap_err().code, ErrorCode::InvalidArgument);
    assert_eq!(
        gem.scale_with(8, &limits).unwrap().width(),
        24,
        "at the limit"
    );
    let narrow = Limits {
        max_sprite_side: 23,
        ..Limits::default()
    };
    assert_eq!(
        gem.scale_with(8, &narrow).unwrap_err().code,
        ErrorCode::LimitExceeded
    );
}

// --- Stroked rectangles and ellipses (#58) ------------------------------------

/// The keys of surface `name`, row by row, with `-` for EMPTY.
fn surface_rows(doc: &Document, name: &str) -> Vec<String> {
    let s = doc.surface(name).unwrap();
    let (w, h) = s.size();
    (0..h)
        .map(|y| (0..w).map(|x| s.key(x, y).unwrap_or('-')).collect())
        .collect()
}

#[test]
fn strokes_are_the_shape_minus_the_shape_shrunk() {
    for shape in ["rect", "ellipse"] {
        for (w, h) in [(7, 5), (8, 6), (9, 9), (10, 4), (12, 12), (3, 8)] {
            for n in [1u32, 2] {
                let (x, y) = (1, 2);
                let mut text = format!(
                    "palette p\n  a #102030\n  b #ffffff\n\
                     surface ring 14x14\n  rect base at=0,0 size=14x14 key=a\n  \
                     {shape} edge at={x},{y} size={w}x{h} key=b stroke={n}\n\
                     surface filled 14x14\n  rect base at=0,0 size=14x14 key=a\n  \
                     {shape} outer at={x},{y} size={w}x{h} key=b\n"
                );
                if w > 2 * n && h > 2 * n {
                    text += &format!(
                        "  {shape} inner at={},{} size={}x{} key=a\n",
                        x + n,
                        y + n,
                        w - 2 * n,
                        h - 2 * n
                    );
                }
                let doc = parse(&text);
                assert_eq!(
                    surface_rows(&doc, "ring"),
                    surface_rows(&doc, "filled"),
                    "{shape} {w}x{h} stroke={n}"
                );
            }
        }
    }
}

#[test]
fn strokes_keep_what_is_inside_the_ring() {
    let doc = parse(
        "palette p\n  a #102030\n  b #ffffff\n  c #ff0000\n\
         surface s 9x7\n  rect base at=0,0 size=9x7 key=a\n  rect mid at=3,2 size=3x3 key=c\n  \
         rect frame at=0,0 size=9x7 key=b stroke=2\n",
    );
    assert_eq!(
        surface_rows(&doc, "s"),
        [
            "bbbbbbbbb",
            "bbbbbbbbb",
            "bbacccabb",
            "bbacccabb",
            "bbacccabb",
            "bbbbbbbbb",
            "bbbbbbbbb",
        ]
    );
    // A stroke at least half the shape's size fills it.
    let full = parse(
        "palette p\n  b #ffffff\nsurface s 4x3\n  rect r at=0,0 size=4x3 key=b stroke=2\n",
    );
    assert_eq!(surface_rows(&full, "s"), ["bbbb", "bbbb", "bbbb"]);
}

#[test]
fn stroke_errors_and_the_builder() {
    let base = "palette p\n  b #ffffff\nsurface s 8x8\n";
    let found = errors(&format!("{base}  line l from=0,0 to=7,7 key=b stroke=1\n"));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!((found[0].0, found[0].1, found[0].2), (4, 39, ErrorCode::Syntax));
    let found = errors(&format!("{base}  rect r at=0,0 size=4x4 key=b stroke=0\n"));
    assert_eq!((found[0].0, found[0].1, found[0].2), (4, 39, ErrorCode::InvalidArgument));
    let found = errors(&format!("{base}  ellipse e at=0,0 size=4x4 key=b stroke=x\n"));
    assert_eq!(found[0].2, ErrorCode::InvalidArgument);

    let doc = parse(&format!("{base}  ellipse e at=1,1 size=6x6 key=b stroke=1\n"));
    let palette = doc.palette("p").unwrap();
    let mut builder =
        spritebox::Surface::builder("s", 8, 8, palette, 0, &Limits::default()).unwrap();
    let ellipse = spritebox::Primitive::Ellipse {
        at: (1, 1),
        size: (6, 6),
    };
    builder.stroke("e", &ellipse, 'b', 1).unwrap();
    let built = builder.finish();
    let parsed = doc.surface("s").unwrap();
    for y in 0..8 {
        for x in 0..8 {
            assert_eq!(built.key(x, y), parsed.key(x, y));
        }
    }
    let mut builder =
        spritebox::Surface::builder("s", 8, 8, palette, 0, &Limits::default()).unwrap();
    let line = spritebox::Primitive::Line {
        from: (0, 0),
        to: (3, 3),
    };
    assert!(builder.stroke("l", &line, 'b', 1).is_err());
    assert!(builder.stroke("e", &ellipse, 'b', 0).is_err());
}

// --- Tiles wherever a chain starts (#60) ----------------------------------------

const GRAVEYARD: &str = "\
palette night
  .  transparent
  g  #808080
  k  #000000
  m  #304030
sprite cross 4x4 palette=night
  .g..
  gggg
  .g..
  .g..
tileset graveyard 4x4 palette=night
  tile tomb1
    .gg.
    gggg
    gkkg
    gggg
  tile tomb2 = cross:flip_x
  terrain moss 16
    island = cross
    h_strip = cross
    v_strip = cross
    fill = cross
";

#[test]
fn tiles_start_chains_in_sprites_layers_and_frames() {
    let doc = parse(&format!(
        "{GRAVEYARD}\
sprite lone = graveyard.tomb2:flip_x
sprite stack 4x4 palette=night
  layer under = graveyard.tomb1
  layer over = graveyard.tomb2:offset(1,0)
animation blink loop
  graveyard.tomb1 4
  graveyard.tomb1:flip_x 4
screen menu 8x4 palette=night
  graveyard.tomb2 at 4,0
"
    ));
    let tileset = doc.tileset("graveyard").unwrap();
    let tomb1 = tileset.tile(tileset.index("tomb1").unwrap()).unwrap();
    let tomb2 = tileset.tile(tileset.index("tomb2").unwrap()).unwrap();
    // flip_x of a flip_x is the cross again.
    assert_eq!(
        doc.sprite("lone").unwrap().render(),
        doc.sprite("cross").unwrap().render()
    );
    let stack = doc.sprite("stack").unwrap();
    assert_eq!(stack.palette().name(), "night");
    let image = stack.render();
    assert_eq!(image.pixel(1, 2), tomb1.render().pixel(1, 2));
    assert_eq!(image.pixel(3, 0), tomb2.render().pixel(2, 0));
    let atlas = doc.export(&Default::default()).unwrap().atlas;
    let frames: Vec<&str> = atlas.animations[0]
        .frames
        .iter()
        .map(|f| f.sprite.as_str())
        .collect();
    assert_eq!(frames, ["graveyard.tomb1", "graveyard.tomb1"]);
    assert!(atlas.animations[0].frames[1].flip_x);
    assert!(atlas.region("graveyard.tomb1").is_some());
    let screen = doc.screen("menu").unwrap();
    assert_eq!(screen.pixel(5, 1), tomb2.render().pixel(1, 1));
}

#[test]
fn tiles_are_placed_in_backgrounds() {
    let doc = parse(&format!(
        "{GRAVEYARD}\
background yard 12x6
  layer stones 12x6
    place sprite graveyard.tomb2 at=1,1
    place sprite graveyard.tomb1:flip_y at=6,1
"
    ));
    let tileset = doc.tileset("graveyard").unwrap();
    let tomb1 = tileset.tile(tileset.index("tomb1").unwrap()).unwrap().render();
    let tomb2 = tileset.tile(tileset.index("tomb2").unwrap()).unwrap().render();
    let flat = doc
        .background("yard")
        .unwrap()
        .render(&doc, &BackgroundRenderOptions::default())
        .unwrap();
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(flat.pixel(x + 1, y + 1), tomb2.pixel(x, y));
            assert_eq!(flat.pixel(x + 6, y + 1), tomb1.pixel(x, 3 - y));
        }
    }
    let placed = &doc.background("yard").unwrap().layers()[0].placements()[0];
    assert_eq!(
        placed.source(),
        &spritebox::SourceRef::Sprite("graveyard.tomb2".into())
    );
    // The typed builder takes the same reference.
    let mut builder =
        spritebox::Background::builder(&doc, "copy", (12, 6), 0, &Limits::default()).unwrap();
    builder
        .layer("stones", (12, 6), Default::default())
        .unwrap()
        .place(
            spritebox::SourceRef::Sprite("graveyard.tomb2".into()),
            spritebox::PlacementOptions {
                at: (1, 1),
                ..Default::default()
            },
        )
        .unwrap();
    let copy = builder.finish().render(&doc, &Default::default()).unwrap();
    assert_eq!(copy.pixel(2, 2), flat.pixel(2, 2));
}

#[test]
fn unknown_tiles_name_what_exists() {
    let cases = [
        (
            "sprite a = graveyard.tomb9\n",
            "tileset `graveyard` has no tile `tomb9`; its tiles are `tomb1`, `tomb2`",
        ),
        (
            "sprite a = graveyard.moss\n",
            "tileset `graveyard` has no tile `moss`; its tiles are `tomb1`, `tomb2`",
        ),
        (
            "sprite a = crypt.tomb1\n",
            "no tileset `crypt`; the tilesets are `graveyard`",
        ),
        (
            "animation a once\n  crypt.tomb1 2\n",
            "no tileset `crypt`; the tilesets are `graveyard`",
        ),
        (
            "screen s 8x8 palette=night\n  graveyard.nope at 0,0\n",
            "tileset `graveyard` has no tile `nope`",
        ),
        (
            "tileset other 4x4 palette=night\n  tile t = graveyard.nope\n",
            "tileset `graveyard` has no tile `nope`",
        ),
        (
            "background b 4x4\n  layer l 4x4\n    place sprite graveyard.nope at=0,0\n",
            "tileset `graveyard` has no tile `nope`; its tiles are `tomb1`, `tomb2`",
        ),
    ];
    for (body, expected) in cases {
        let found = errors(&format!("{GRAVEYARD}{body}"));
        assert_eq!(found.len(), 1, "{body}{found:?}");
        assert_eq!(found[0].2, ErrorCode::UnknownName, "{body}");
        assert!(found[0].3.contains(expected), "{body}{}", found[0].3);
        assert!(found[0].0 >= 23, "{body}{found:?}");
    }
    let found = errors("palette p\n  k #000000\nsprite a = no.such\n");
    assert!(
        found[0].3.contains("no tileset `no`") && !found[0].3.contains("the tilesets are"),
        "{}",
        found[0].3
    );
}

#[test]
fn tile_references_follow_sprite_rules() {
    // A tile made from another tileset's tile, in any order.
    let doc = parse(&format!(
        "tileset copies 4x4 palette=night\n  tile again = graveyard.tomb2:flip_x\n{GRAVEYARD}"
    ));
    let copies = doc.tileset("copies").unwrap();
    assert_eq!(
        copies.tile(1).unwrap().render(),
        doc.sprite("cross").unwrap().render()
    );
    // A sprite of the same dotted name wins over the tile.
    let doc = parse(&format!(
        "{GRAVEYARD}sprite graveyard.tomb1 = cross:rot90\nsprite b = graveyard.tomb1\n"
    ));
    assert_eq!(
        doc.sprite("b").unwrap().render(),
        doc.sprite("cross").unwrap().rotate(1).render()
    );
    // A circle through a tile is an error.
    let found = errors(&format!(
        "{GRAVEYARD}tileset loop 4x4 palette=night\n  tile t = ring\nsprite ring = loop.t\n"
    ));
    assert!(
        found.iter().any(|e| e.2 == ErrorCode::Cycle),
        "{found:?}"
    );
    // A tile with errors reports them once, at the tile.
    let found = errors(&format!(
        "{GRAVEYARD}tileset bad 4x4 palette=night\n  tile t = missing\nsprite s = bad.t\n"
    ));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!((found[0].0, found[0].1), (24, 12));
}
