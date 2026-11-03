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

// --- A background inside another (#59) ------------------------------------------

const BOARD: &str = "\
palette world
  .  transparent
  d  #203020
  g  #40a040
  w  #ffffff80
  k  #000000
sprite tower 3x4 palette=world
  .k.
  kwk
  kgk
  kkk
surface ground 256x256 palette=world
  rect soil at=0,0 size=256x256 key=d
  ellipse lawn at=20,30 size=200x150 key=g
  rect fog at=100,0 size=50x256 key=w
";

/// The board's placements, at `X`,`Y`, for a layer.
fn board_items(x: i32, y: i32) -> String {
    format!(
        "    place surface ground at={x},{y}\n\
         \x20   place sprite tower at={},{}\n\
         \x20   place sprite tower:flip_x at={},{} anchor=center\n",
        x + 10,
        y + 20,
        x + 128,
        y + 128
    )
}

#[test]
fn a_placed_background_equals_a_copy_of_its_placements() {
    let text = format!(
        "{BOARD}\
background board 256x256
  layer map 256x256
{}background play 600x270
  layer board 600x270
    place background board at=172,7
  layer hud 600x270
    place sprite tower at=4,4
background shop 600x270
  layer board 600x270
    place background board at=-40,-20
background play_copy 600x270
  layer board 600x270
{}  layer hud 600x270
    place sprite tower at=4,4
background shop_copy 600x270
  layer board 600x270
{}",
        board_items(0, 0),
        board_items(172, 7),
        board_items(-40, -20)
    );
    let doc = parse(&text);
    let render = |name: &str| {
        doc.background(name)
            .unwrap()
            .render(&doc, &BackgroundRenderOptions::default())
            .unwrap()
    };
    assert_eq!(render("play"), render("play_copy"));
    assert_eq!(render("shop"), render("shop_copy"));
    let placed = &doc.background("play").unwrap().layers()[0].placements()[0];
    assert_eq!(
        placed.source(),
        &spritebox::SourceRef::Background {
            name: "board".into(),
            layer: None
        }
    );
    // Exports keep the written order of backgrounds.
    let export = doc.export(&Default::default()).unwrap();
    let names: Vec<&str> = export.backgrounds.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["board", "play", "shop", "play_copy", "shop_copy"]);
    assert_eq!(
        export.backgrounds[1].flat.as_ref().unwrap().image,
        export.backgrounds[3].flat.as_ref().unwrap().image
    );
}

#[test]
fn placed_backgrounds_crop_name_a_layer_and_follow_themes() {
    let text = format!(
        "{BOARD}\
theme dusk of world
  g #a04040
background mockup 64x48
  layer view 64x48
    place background board layer=map at=0,0 crop=60,40,40,30 anchor=bottom-left
    place background board layer=units at=60,40 anchor=center
background board 256x256
  layer map 256x256 at=2,0
    place surface ground at=0,0
  layer units 256x256
    place sprite tower at=0,0
"
    );
    let doc = parse(&text);
    let board = doc.background("board").unwrap();
    let only_layer = |doc: &Document, layer: usize| {
        let mut builder =
            spritebox::Background::builder(doc, "one", (256, 256), 0, &Limits::default()).unwrap();
        let source = &board.layers()[layer];
        builder
            .layer("l", source.size(), source.options().clone())
            .unwrap();
        for p in source.placements() {
            builder.place(p.source().clone(), p.options()).unwrap();
        }
        builder.finish().render(doc, &Default::default()).unwrap()
    };
    let check = |doc: &Document| {
        let map = only_layer(doc, 0);
        let units = only_layer(doc, 1);
        let mockup = doc
            .background("mockup")
            .unwrap()
            .render(doc, &Default::default())
            .unwrap();
        for y in 0..48u32 {
            for x in 0..64u32 {
                // The crop's bottom-left corner is at 0,0, so it is above the view.
                let mut want = spritebox::Color::TRANSPARENT;
                let (ux, uy) = (x as i64 - 60 + 128, y as i64 - 40 + 128);
                if (0..256).contains(&ux) && (0..256).contains(&uy) {
                    want = units.pixel(ux as u32, uy as u32).unwrap().over(want);
                }
                assert_eq!(mockup.pixel(x, y), Some(want), "{x},{y}");
            }
        }
    };
    check(&doc);
    // The cropped map, placed at the top-left instead.
    let top = parse(&text.replace("anchor=bottom-left", "anchor=top-left"));
    let map = only_layer(&top, 0);
    let mockup = top
        .background("mockup")
        .unwrap()
        .render(&top, &Default::default())
        .unwrap();
    assert_eq!(mockup.pixel(0, 0), map.pixel(60, 40));
    assert_eq!(mockup.pixel(39, 29), map.pixel(99, 69));
    assert_eq!(mockup.pixel(40, 0), Some(spritebox::Color::TRANSPARENT));
    // Themes recolor the placed background.
    let dusk = top.with_theme("dusk").unwrap();
    let themed = dusk
        .background("mockup")
        .unwrap()
        .render(&dusk, &Default::default())
        .unwrap();
    assert_eq!(themed.pixel(20, 10), Some(Color::rgb(0xa0, 0x40, 0x40)));
    assert_eq!(mockup.pixel(20, 10), Some(Color::rgb(0x40, 0xa0, 0x40)));
}

#[test]
fn placed_background_errors_are_positioned() {
    let cycle = format!(
        "{BOARD}\
background a 8x8
  layer l 8x8
    place background b at=0,0
background b 8x8
  layer l 8x8
    place background c at=0,0
background c 8x8
  layer l 8x8
    place background a at=0,0
background d 8x8
  layer l 8x8
    place background a at=0,0
"
    );
    let found = errors(&cycle);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].2, ErrorCode::Cycle);
    assert!(found[0].3.contains("a -> b -> c -> a"), "{}", found[0].3);
    assert_eq!((found[0].0, found[0].1), (24, 22));
    let me = format!("{BOARD}background a 8x8\n  layer l 8x8\n    place background a at=0,0\n");
    let found = errors(&me);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].3.contains("a -> a"), "{}", found[0].3);
    let cases = [
        ("place background nope at=0,0", ErrorCode::UnknownName, 22, "Unknown background `nope`"),
        ("place background a layer=sky at=0,0", ErrorCode::UnknownName, 30, "its layers are `l`"),
        ("place background a at=0,0 crop=0,0,9,9", ErrorCode::InvalidArgument, 36, "Crop"),
        ("place surface ground layer=l at=0,0", ErrorCode::Syntax, 32, "Unknown scene option"),
    ];
    for (line, code, column, message) in cases {
        let text = format!(
            "{BOARD}background a 8x8\n  layer l 8x8\n    place sprite tower at=0,0\n\
             background b 8x8\n  layer l 8x8\n    {line}\n"
        );
        let found = errors(&text);
        assert_eq!(found.len(), 1, "{line}: {found:?}");
        assert_eq!((found[0].0, found[0].1, found[0].2), (21, column, code), "{line}");
        assert!(found[0].3.contains(message), "{line}: {}", found[0].3);
    }
    // Nesting deeper than the limit.
    let limits = Limits {
        max_nesting: 2,
        ..Limits::default()
    };
    let mut deep = String::from(BOARD);
    deep += "background n0 8x8\n  layer l 8x8\n    place sprite tower at=0,0\n";
    for i in 1..=3 {
        deep += &format!(
            "background n{i} 8x8\n  layer l 8x8\n    place background n{} at=0,0\n",
            i - 1
        );
    }
    let found = check(&deep, &limits);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code, ErrorCode::LimitExceeded);
    assert_eq!(found[0].position.unwrap().line, 27);
}

#[test]
fn placed_backgrounds_share_the_scene_budget() {
    let text = format!(
        "{BOARD}\
background board 256x256
  layer map 256x256
    place surface ground at=0,0
background mockup 256x256
  layer view 256x256
    place background board at=0,0
"
    );
    let doc = parse(&text);
    let board = doc.background("board").unwrap();
    let mockup = doc.background("mockup").unwrap();
    // Rendering the board alone fits a budget that the mockup, which draws the board
    // as well as its own layer, exceeds.
    let mut limits = Limits::default();
    limits.max_scene_work = 1;
    while board.render_with(&doc, &Default::default(), &limits).is_err() {
        limits.max_scene_work *= 2;
    }
    let error = mockup
        .render_with(&doc, &Default::default(), &limits)
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::LimitExceeded);
    // The typed builder places a background as well.
    let mut builder =
        spritebox::Background::builder(&doc, "copy", (256, 256), 0, &Limits::default()).unwrap();
    builder
        .layer("view", (256, 256), Default::default())
        .unwrap()
        .place(
            spritebox::SourceRef::Background {
                name: "board".into(),
                layer: Some("map".into()),
            },
            Default::default(),
        )
        .unwrap();
    let copy = builder.finish();
    assert_eq!(
        copy.render(&doc, &Default::default()).unwrap(),
        mockup.render(&doc, &Default::default()).unwrap()
    );
    let mut builder =
        spritebox::Background::builder(&doc, "bad", (8, 8), 0, &Limits::default()).unwrap();
    builder.layer("view", (8, 8), Default::default()).unwrap();
    let missing = builder.place(
        spritebox::SourceRef::Background {
            name: "board".into(),
            layer: Some("nope".into()),
        },
        Default::default(),
    );
    assert_eq!(missing.err().unwrap().code, ErrorCode::UnknownName);
}
