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
