//! Every construct of the text format, and every error with its position and code.

use spritebox::{Color, Document, ErrorCode, Limits, LintOptions, check};

/// The errors of `text` as `(line, column, code)`.
fn errors_with(text: &str, limits: &Limits) -> Vec<(usize, usize, ErrorCode)> {
    check(text, limits)
        .iter()
        .map(|e| {
            let p = e.position.expect("text errors have positions");
            (p.line, p.column, e.code)
        })
        .collect()
}

fn errors(text: &str) -> Vec<(usize, usize, ErrorCode)> {
    errors_with(text, &Limits::default())
}

/// The only error of `text`.
fn error(text: &str) -> (usize, usize, ErrorCode) {
    let found = errors(text);
    assert_eq!(
        found.len(),
        1,
        "{text}\n{:?}",
        check(text, &Limits::default())
    );
    found[0]
}

use ErrorCode::*;

const DAWN: &str = "\
palette dawn
  .  transparent
  k  #1a1c2c
  o  #f4b41b80
";

#[test]
fn palettes() {
    let doc: Document = "
; A comment line.
palette dawn   ; a comment after a header
  .  transparent
  k  #1a1c2c
  o  #F4B41B80

palette dusk from=dawn
  o  #b86f50
  é  #010203
"
    .parse()
    .unwrap();
    let dawn = doc.palette("dawn").unwrap();
    assert_eq!(dawn.name(), "dawn");
    assert_eq!(dawn.color('.'), Some(Color::TRANSPARENT));
    assert_eq!(dawn.color('o'), Some(Color::rgba(0xf4, 0xb4, 0x1b, 0x80)));
    assert_eq!(dawn.color('x'), None);
    let dusk = doc.palette("dusk").unwrap();
    let keys: Vec<char> = dusk.colors().iter().map(|&(k, _)| k).collect();
    assert_eq!(keys, vec!['.', 'k', 'o', 'é']);
    assert_eq!(dusk.color('o'), Some(Color::rgb(0xb8, 0x6f, 0x50)));
    assert_eq!(dusk.color('k'), Some(Color::rgb(0x1a, 0x1c, 0x2c)));
    let names: Vec<&str> = doc.palettes().iter().map(|p| p.name()).collect();
    assert_eq!(names, ["dawn", "dusk"]);
}

#[test]
fn palettes_copy_palettes_defined_later_and_in_chains() {
    let doc: Document = "
palette c from=b
  z #000003
palette a
  x #000001
palette b from=a
  y #000002
"
    .parse()
    .unwrap();
    let keys: Vec<char> = doc
        .palette("c")
        .unwrap()
        .colors()
        .iter()
        .map(|&(k, _)| k)
        .collect();
    assert_eq!(keys, vec!['x', 'y', 'z']);
}

#[test]
fn sprites() {
    let text = format!(
        "{DAWN}
sprite dot 3x2 palette=dawn
  .k.
  kok
"
    );
    let doc: Document = text.parse().unwrap();
    let dot = doc.sprite("dot").unwrap();
    assert_eq!((dot.name(), dot.width(), dot.height()), ("dot", 3, 2));
    assert_eq!(dot.palette().name(), "dawn");
    assert_eq!(dot.key(1, 1), Some('o'));
    assert_eq!(dot.key(3, 0), None);
    let image = dot.render();
    assert_eq!(image.pixel(0, 0), Some(Color::TRANSPARENT));
    assert_eq!(image.pixel(1, 0), Some(Color::rgb(0x1a, 0x1c, 0x2c)));
    assert_eq!(image.pixel(1, 1), Some(Color::rgba(0xf4, 0xb4, 0x1b, 0x80)));
}

#[test]
fn a_single_palette_needs_no_name() {
    let doc: Document = format!("sprite a 1x1\n  k\n{DAWN}").parse().unwrap();
    assert_eq!(doc.sprite("a").unwrap().palette().name(), "dawn");
    let two = format!("{DAWN}palette b\n  k #000000\nsprite a 1x1\n  k\n");
    assert_eq!(error(&two), (7, 8, Syntax));
}

#[test]
fn layout_is_free_within_the_rules() {
    // Any indentation, the same within a block; comments and blank lines anywhere;
    // Windows line endings.
    let text = "palette p\r\n      x #ff0000\r\n\r\n      ; comment\r\n      y #00ff00\r\nsprite s 2x1\r\n x y\r\n";
    assert_eq!(error(text), (7, 2, Syntax));
    let text = "palette p\r\n      x #ff0000\r\n\r\n      ; comment\r\n      y #00ff00\r\nsprite s 2x1\r\n xy ; trailing\r\n";
    assert!(errors(text).is_empty());
}

#[test]
fn structural_errors() {
    assert_eq!(error("  palette p\n"), (1, 3, Syntax));
    assert_eq!(error("palette p\n\tx #000000\n"), (2, 1, Syntax));
    assert_eq!(
        error("palette p\n    x #000000\n  y #000000\n"),
        (3, 3, Syntax)
    );
    assert_eq!(error("sprites s 1x1\n"), (1, 1, Syntax));
    assert_eq!(error("palette\n"), (1, 8, Syntax));
    assert_eq!(error("palette a.b\n"), (1, 9, Syntax));
    assert_eq!(error("palette p extra\n"), (1, 11, Syntax));
    assert_eq!(error("palette p size=2\n"), (1, 11, Syntax));
    assert_eq!(error("palette p from=a from=b\n"), (1, 18, Syntax));
    assert_eq!(error("palette p from=a b\n"), (1, 18, Syntax));
    assert_eq!(error("palette p from=a!\n"), (1, 16, Syntax));
}

#[test]
fn ramps() {
    let doc: Document = "\
palette p
  a  #000000
  b  #ffffff
  ramp warm cde base=#d08159
  ramp grey ab
palette q from=p
  ramp warm edc
  ramp more fg base=#3b5dc9 shift=0
"
    .parse()
    .unwrap();
    let p = doc.palette("p").unwrap();
    let names = |palette: &spritebox::Palette| -> Vec<(String, String)> {
        palette
            .ramps()
            .iter()
            .map(|r| (r.name().to_owned(), r.keys().iter().collect()))
            .collect()
    };
    let pairs = |list: &[(&str, &str)]| -> Vec<(String, String)> {
        list.iter()
            .map(|&(a, b)| (a.to_owned(), b.to_owned()))
            .collect()
    };
    assert_eq!(names(p), pairs(&[("warm", "cde"), ("grey", "ab")]));
    // A made ramp defines its keys, dark to light, with the base in the middle.
    assert_eq!(p.color('d'), Some(Color::rgb(0xd0, 0x81, 0x59)));
    let keys: Vec<char> = p.colors().iter().map(|&(k, _)| k).collect();
    assert_eq!(keys, ['a', 'b', 'c', 'd', 'e']);
    // A copy keeps the ramps it does not replace.
    let q = doc.palette("q").unwrap();
    assert_eq!(
        names(q),
        pairs(&[("warm", "edc"), ("grey", "ab"), ("more", "fg")])
    );
    assert_eq!(q.ramp("more").unwrap().keys(), ['f', 'g']);
    assert!(q.ramp("none").is_none());
}

#[test]
fn ramp_errors() {
    use ErrorCode::*;
    // The palette takes lines 1 to 3; the ramp is on line 4.
    let e = |body: &str| errors(&format!("palette p\n  a #000000\n  b #ffffff\n{body}"));
    assert_eq!(e("  ramp\n"), [(4, 7, Syntax)]);
    assert_eq!(e("  ramp r\n"), [(4, 9, Syntax)]);
    assert_eq!(e("  ramp r base=#808080\n"), [(4, 10, Syntax)]);
    assert_eq!(e("  ramp r ab x\n"), [(4, 13, Syntax)]);
    assert_eq!(e("  ramp r a\n"), [(4, 10, InvalidArgument)]);
    assert_eq!(e("  ramp r aa\n"), [(4, 11, DuplicateName)]);
    assert_eq!(e("  ramp r ab\n  ramp s ba\n"), [(5, 10, DuplicateName)]);
    assert_eq!(
        e("  ramp r ab\n  ramp r cd base=#808080\n"),
        [(5, 8, DuplicateName)]
    );
    assert_eq!(e("  ramp r az\n"), [(4, 11, UnknownName)]);
    assert_eq!(e("  ramp r cd base=#80808\n"), [(4, 18, InvalidArgument)]);
    assert_eq!(
        e("  ramp r cd base=#808080ff\n"),
        [(4, 18, InvalidArgument)]
    );
    assert_eq!(
        e("  ramp r cd base=#808080 shift=61\n"),
        [(4, 32, InvalidArgument)]
    );
    assert_eq!(e("  ramp r cd shift=10\n"), [(4, 19, Syntax)]);
    assert_eq!(e("  ramp r ab base=#808080\n"), [(4, 10, DuplicateName)]);
    assert_eq!(e("  ramp r cd\n    x\n"), [(5, 5, Syntax)]);
    // A copy's ramps may not share keys with the ramps it copies, unless they replace
    // them by name.
    let copied = "palette p\n  ramp original ab base=#808080\npalette q from=p\n";
    assert_eq!(
        errors(&format!("{copied}  ramp other ba\n")),
        [(4, 14, DuplicateName)]
    );
    assert_eq!(errors(&format!("{copied}  ramp original ba\n")), []);
    // A ramp longer than a palette can be is refused at once.
    let limits = Limits {
        max_palette_colors: 4,
        ..Limits::default()
    };
    let text = "palette p\n  ramp r abcde base=#808080\n";
    assert_eq!(errors_with(text, &limits), [(2, 10, LimitExceeded)]);
    // A key a ramp made is defined, so a line defining it again is an error.
    assert_eq!(
        e("  ramp r cd base=#808080\n  c #000000\n"),
        [(5, 3, DuplicateName)]
    );
}

#[test]
fn palette_errors() {
    assert_eq!(error("palette p\n  xy #000000\n"), (2, 3, Syntax));
    // `;` starts a comment, so it cannot be a key.
    assert!(errors("palette p\n  ; #000000\n").is_empty());
    assert_eq!(error("palette p\n  = #000000\n"), (2, 3, InvalidArgument));
    assert_eq!(error("palette p\n  x\n"), (2, 4, Syntax));
    assert_eq!(error("palette p\n  x #000000 y\n"), (2, 13, Syntax));
    assert_eq!(error("palette p\n  x #00000\n"), (2, 5, InvalidArgument));
    assert_eq!(error("palette p\n  x red\n"), (2, 5, InvalidArgument));
    assert_eq!(
        error("palette p\n  x #000000\n  x #ffffff\n"),
        (3, 3, DuplicateName)
    );
    assert_eq!(
        error("palette p\n  x #000000\n    y #ffffff\n"),
        (3, 5, Syntax)
    );
    assert_eq!(error("palette p\npalette p\n"), (2, 9, DuplicateName));
    assert_eq!(error("palette p from=q\n"), (1, 16, UnknownName));
    assert_eq!(
        errors("palette a from=b\npalette b from=a\n"),
        vec![(2, 16, Cycle)],
        "the error is at the `from=` that closes the circle"
    );
    assert_eq!(error("palette a from=a\n"), (1, 16, Cycle));
}

#[test]
fn sprite_errors() {
    let e = |body: &str| error(&format!("{DAWN}{body}"));
    // The palette takes lines 1..=4.
    assert_eq!(e("sprite s\n"), (5, 9, Syntax));
    assert_eq!(e("sprite s 2by2\n"), (5, 10, Syntax));
    assert_eq!(e("sprite s 2x\n"), (5, 10, Syntax));
    assert_eq!(e("sprite s 0x1\n  .\n"), (5, 10, InvalidArgument));
    assert_eq!(e("sprite s 1x0\n"), (5, 12, InvalidArgument));
    assert_eq!(e("sprite s 1x1 2x2\n  .\n"), (5, 14, Syntax));
    assert_eq!(e("sprite s 1x1 pallete=dawn\n  .\n"), (5, 14, Syntax));
    assert_eq!(e("sprite s 3x1\n  . k\n"), (6, 3, Syntax));
    assert_eq!(e("sprite s 3x1\n  .k\n"), (6, 5, SizeMismatch));
    assert_eq!(e("sprite s 3x1\n  .k.k\n"), (6, 6, SizeMismatch));
    assert_eq!(e("sprite s 1x2\n  .\n"), (5, 10, SizeMismatch));
    assert_eq!(e("sprite s 1x1\n  .\n  .\n"), (7, 3, SizeMismatch));
    assert_eq!(e("sprite s 3x1\n  .kx\n"), (6, 5, UnknownName));
    assert_eq!(e("sprite s 1x1 palette=night\n  .\n"), (5, 22, UnknownName));
    assert_eq!(
        e("sprite s 1x1\n  .\nsprite s 1x1\n  .\n"),
        (7, 8, DuplicateName)
    );
    assert_eq!(e("sprite s 1x1\n  .\n    .\n"), (6, 3, Syntax));
}

#[test]
fn all_errors_are_reported_in_order() {
    let text = "\
palette p
  . transparent
  k #000
  kk #ffffff

sprite a 4x3 palette=p
  ....
  .kx.
  ...

sprite b 2x2 palette=q
  ..
  ..
sprites c
";
    assert_eq!(
        errors(text),
        vec![
            (3, 5, InvalidArgument),
            (4, 3, Syntax),
            (8, 5, UnknownName),
            (9, 6, SizeMismatch),
            (11, 22, UnknownName),
            (14, 1, Syntax),
        ]
    );
    // `parse` returns the first.
    let first = Document::parse(text).unwrap_err();
    assert_eq!(first.position.map(|p| (p.line, p.column)), Some((3, 5)));
    assert_eq!(
        first.to_string(),
        "line 3, column 5: Color `#000` must be `#rrggbb`, `#rrggbbaa` or `transparent`"
    );
}

#[test]
fn limits() {
    let limits = Limits {
        max_text_bytes: 10,
        ..Limits::default()
    };
    assert_eq!(
        errors_with("palette pp\n", &limits),
        vec![(1, 1, LimitExceeded)]
    );

    let limits = Limits {
        max_definitions: 2,
        ..Limits::default()
    };
    assert_eq!(
        errors_with("palette a\npalette b\npalette c\n", &limits),
        vec![(3, 1, LimitExceeded)]
    );

    let limits = Limits {
        max_palette_colors: 2,
        ..Limits::default()
    };
    let text = "palette a\n  x #000000\n  y #000000\npalette b from=a\n  z #000000\n";
    assert_eq!(errors_with(text, &limits), vec![(4, 9, LimitExceeded)]);

    let limits = Limits {
        max_nesting: 2,
        ..Limits::default()
    };
    let text = "palette a\npalette b from=a\npalette c from=b\npalette d from=c\n";
    assert_eq!(errors_with(text, &limits), vec![(4, 16, LimitExceeded)]);

    let limits = Limits {
        max_sprite_side: 4,
        ..Limits::default()
    };
    let text = "palette p\n  x #000000\nsprite s 5x1\n  xxxxx\n";
    assert_eq!(errors_with(text, &limits), vec![(3, 10, LimitExceeded)]);
    assert_eq!(
        errors("palette p\nsprite s 99999999999999999999x1\n"),
        vec![(2, 10, LimitExceeded)]
    );

    let limits = Limits {
        max_image_pixels: 5,
        ..Limits::default()
    };
    let text = "palette p\n  x #000000\nsprite s 2x2\n  xx\n  xx\nsprite t 1x2\n  x\n  x\n";
    assert_eq!(errors_with(text, &limits), vec![(6, 8, LimitExceeded)]);
}

#[test]
fn never_panics_on_malformed_text() {
    let base = format!(
        "{DAWN}  ramp warm ko\n  ramp made xy base=#d08159 shift=10\nsprite dot 2x2 palette=dawn\n  ko\n  ok\nsprite wide 4x2 palette=dawn\n  kkoo\n  ookk\n\
         sprite coin 4x2 palette=dawn\n  layer a\n    .ko.\n    kook\n  layer b = wide:flip_y\n\
         sprite c = coin:rot90:offset(1,-1):swap(dawn):outline(k)\nanimation x once\n  c:flip_x 6\n\
         tileset t 2x2 palette=dawn columns=3\n  tile a\n    ko\n    ok\n  tile b = dot:flip_x\n\
         \x20 terrain d 16\n    island = dot\n    h_strip\n      kk\n      oo\n    v_strip = dot:rot90\n\
         \x20   fill = dot\ntilemap m outside=empty\n  legend\n    . empty\n    # d solid\n    a a\n\
         \x20 map\n    .#a\n    ##.\nfont f height=2 baseline=1 spacing=2\n  glyph A advance=3\n    #.\n    ##\n\
         \x20 glyph U+003B\n    #\n    #\ntexture n 4x4 palette=dawn seed=9\n  noise cell=2 octaves=2\n\
         \x20 ramp k o .\n  dither bayer2\nsprite panel = n:pad(1) slice=2 lint=allow(orphan_pixel)\n\
         lint allow unused_key\n"
    );
    let base_errors = check(&base, &Limits::default());
    assert!(base_errors.is_empty(), "{base_errors:?}");
    let fragments = [
        "",
        " ",
        "\t",
        "\n",
        ";",
        "(",
        ")",
        "=",
        ":",
        ",",
        "x",
        "#",
        "é",
        "palette",
        "sprite",
        "layer",
        "animation",
        "from=",
        "palette=",
        "0x0",
        "4x2",
        "\u{0}",
        "pad(9)",
        "-1",
        "tileset",
        "tilemap",
        "terrain",
        "legend",
        "map",
        "solid",
        "47",
        "d/3",
        "island",
        "font",
        "glyph",
        "U+",
        "advance=",
        "height=",
        "texture",
        "noise",
        "ramp",
        "dither",
        "cell=",
        "seed=",
        "import",
        "lint",
        "lint=",
        "ramp",
        "base=",
        "shift=",
        "allow(",
        "orphan_pixel",
    ];
    for i in 0..=base.len() {
        if !base.is_char_boundary(i) {
            continue;
        }
        for fragment in fragments {
            let text = format!("{}{fragment}{}", &base[..i], &base[i..]);
            let _ = check(&text, &Limits::default());
            if let Ok(doc) = Document::parse(&text) {
                let _ = doc.lint(&LintOptions::default());
            }
        }
        let _ = check(&base[..i], &Limits::default());
    }
}

const FIELD: &str = "\
palette p
  .  transparent
  k  #000000
  a  #ff0000
  b  #0000ff80
palette q
  .  transparent
  k  #ffffff
  a  #00ff00
  b  #ffff00
sprite s 3x2 palette=p
  ka.
  ..b
";

/// `FIELD` and `body`, whose first line is line 14.
fn field(body: &str) -> String {
    format!("{FIELD}{body}")
}

fn grid(doc: &Document, name: &str) -> Vec<String> {
    let s = doc
        .sprite(name)
        .unwrap_or_else(|| panic!("no sprite {name}"));
    (0..s.height())
        .map(|y| (0..s.width()).map(|x| s.key(x, y).unwrap_or(' ')).collect())
        .collect()
}

#[test]
fn layers() {
    let doc: Document = field(
        "sprite l 3x2 palette=p
  layer under
    aaa
    aaa
  layer over   ; transparent keys let the layer under show
    .k.
    ..b
  layer copy = s:flip_y
",
    )
    .parse()
    .unwrap();
    let l = doc.sprite("l").unwrap();
    let names: Vec<Option<&str>> = l.layer_names().collect();
    assert_eq!(names, [Some("under"), Some("over"), Some("copy")]);
    assert_eq!(grid(&doc, "l"), ["akb", "kab"]);
    let image = l.render();
    assert_eq!(image.pixel(0, 0), Some(Color::rgb(255, 0, 0)));
    // Translucent blue over red.
    assert_eq!(image.pixel(2, 0), Some(Color::rgba(127, 0, 128, 255)));
}

#[test]
fn derived_sprites() {
    let doc: Document = field(
        "sprite fx = s:flip_x
sprite fy = s:flip_y
sprite r = s:rot90:rot180   ; any number of modifiers
sprite o = s:offset(1,-1)
sprite padded = s:pad( 1 )
sprite night = s:swap(q)
sprite line = s:pad(1):outline(k)
sprite chain = fx:flip_x    ; from a derived sprite
",
    )
    .parse()
    .unwrap();
    assert_eq!(grid(&doc, "fx"), [".ak", "b.."]);
    assert_eq!(grid(&doc, "fy"), ["..b", "ka."]);
    assert_eq!(grid(&doc, "r"), [".b", "a.", "k."], "270 degrees clockwise");
    assert_eq!(grid(&doc, "o"), [" ..", "   "]);
    assert_eq!(grid(&doc, "padded"), ["     ", " ka. ", " ..b ", "     "]);
    assert_eq!(grid(&doc, "chain"), grid(&doc, "s"));
    assert_eq!(grid(&doc, "line"), [" kk  ", "kkak ", " kkbk", "   k "]);
    let night = doc.sprite("night").unwrap();
    assert_eq!(night.palette().name(), "q");
    assert_eq!(night.render().pixel(1, 0), Some(Color::rgb(0, 255, 0)));
    assert_eq!(doc.sprite("o").unwrap().name(), "o");
    let names: Vec<&str> = doc.sprites().iter().map(|s| s.name()).collect();
    assert_eq!(
        names,
        [
            "s", "fx", "fy", "r", "o", "padded", "night", "line", "chain"
        ]
    );
}

#[test]
fn derived_sprites_may_come_first() {
    let doc: Document = field("sprite a = b:flip_x\nsprite b = s:rot90\n")
        .parse()
        .unwrap();
    assert_eq!(grid(&doc, "a"), ["k.", "a.", ".b"]);
}

#[test]
fn animations() {
    let doc: Document = field(
        "animation spin
  s                 6
  s:flip_x          3   ; a trailing flip is a flag of the frame
  s:rot90:flip_y    3   ; a chain builds a frame sprite; the flip stays a flag
  s:rot180          1   ; both flips
  s:flip_x:flip_x   1   ; no flip at all
animation hit once
  s:swap(q)  2
",
    )
    .parse()
    .unwrap();
    let spin = doc.animation("spin").unwrap();
    assert!(spin.looping());
    assert_eq!(spin.total_ticks(), 14);
    let frames: Vec<(&str, u32, bool, bool)> = spin
        .frames()
        .iter()
        .map(|f| (f.sprite(), f.ticks(), f.flip_x(), f.flip_y()))
        .collect();
    assert_eq!(
        frames,
        [
            ("s", 6, false, false),
            ("s", 3, true, false),
            ("s:rot90", 3, false, true),
            ("s", 1, true, true),
            ("s", 1, false, false)
        ]
    );
    let hit = doc.animation("hit").unwrap();
    assert!(!hit.looping());
    let built: Vec<&str> = doc.frame_sprites().iter().map(|s| s.name()).collect();
    assert_eq!(built, ["s:rot90", "s:swap(q)"]);
    assert_eq!(grid(&doc, "s:rot90"), [".k", ".a", "b."]);
    assert!(doc.sprites().iter().all(|s| !s.name().contains(':')));
    let explicit: Document = field("animation a loop\n  s 1\n").parse().unwrap();
    assert!(explicit.animation("a").unwrap().looping());
}

#[test]
fn modifier_errors() {
    // `field` bodies start on line 14; `sprite x = ` is 11 characters.
    let e = |chain: &str| error(&field(&format!("sprite x = {chain}\n")));
    assert_eq!(e("s:spin"), (14, 14, Syntax));
    assert_eq!(e("s:"), (14, 14, Syntax));
    assert_eq!(e("s::flip_x"), (14, 14, Syntax));
    assert_eq!(e(":flip_x"), (14, 12, Syntax));
    assert_eq!(e("s:flip_x()"), (14, 14, Syntax));
    assert_eq!(e("s:offset(1)"), (14, 14, Syntax));
    assert_eq!(e("s:offset(1,x)"), (14, 23, Syntax));
    assert_eq!(e("s:offset(1,99999)"), (14, 23, InvalidArgument));
    assert_eq!(e("s:pad(0)"), (14, 18, InvalidArgument));
    assert_eq!(e("s:pad(1"), (14, 19, Syntax));
    assert_eq!(e("s:pad(600)"), (14, 14, LimitExceeded));
    assert_eq!(e("s:pad(500):pad(100)"), (14, 23, LimitExceeded));
    assert_eq!(e("s:outline(kk)"), (14, 22, Syntax));
    assert_eq!(e("s:outline(z)"), (14, 14, UnknownName));
    assert_eq!(e("s:swap(night)"), (14, 19, UnknownName));
    assert_eq!(e("t:flip_x"), (14, 12, UnknownName));
    assert_eq!(
        error(&field("palette r\n  k #000000\nsprite x = s:swap(r)\n")),
        (16, 19, UnknownName)
    );
    assert_eq!(
        error(&field("sprite x = s:flip_x extra\n")),
        (14, 21, Syntax)
    );
    assert_eq!(error(&field("sprite x =\n")), (14, 11, Syntax));
    assert_eq!(error(&field("sprite x = s\n  ka.\n")), (15, 3, Syntax));
}

#[test]
fn derivation_errors() {
    assert_eq!(
        errors(&field("sprite a = b:flip_x\nsprite b = a:flip_y\n")),
        vec![(15, 12, Cycle)]
    );
    assert_eq!(error(&field("sprite a = a\n")), (14, 12, Cycle));
    assert_eq!(
        error(&field("sprite s = s:flip_x\n")),
        (14, 8, DuplicateName)
    );
    // Sprites built from a sprite with errors fail without errors of their own.
    assert_eq!(
        errors(&field(
            "sprite bad 2x1 palette=p\n  kz\nsprite a = bad:flip_x\nanimation x\n  bad 1\n  a:flip_y 1\n"
        )),
        vec![(15, 4, UnknownName)]
    );
    let limits = Limits {
        max_nesting: 2,
        ..Limits::default()
    };
    let text = field("sprite a = s:flip_x\nsprite b = a:flip_x\nsprite c = b:flip_x\n");
    assert_eq!(errors_with(&text, &limits), vec![(16, 12, LimitExceeded)]);
}

#[test]
fn layer_errors() {
    let e = |body: &str| error(&field(body));
    assert_eq!(
        e("sprite l 3x2 palette=p\n  layer a\n    ka.\n    ..b\n  ka.\n"),
        (18, 3, Syntax)
    );
    assert_eq!(
        e("sprite l 3x2 palette=p\n  layer a\n    ka.\n"),
        (15, 10, SizeMismatch)
    );
    assert_eq!(
        e("sprite l 3x2 palette=p\n  layer a\n    ka.\n    ..\n"),
        (17, 7, SizeMismatch)
    );
    assert_eq!(
        e("sprite l 3x2 palette=p\n  layer a\n    ka.\n    ..b\n  layer a\n    ka.\n    ..b\n"),
        (18, 9, DuplicateName)
    );
    assert_eq!(
        e("sprite l 2x2 palette=p\n  layer a = s\n"),
        (15, 13, SizeMismatch)
    );
    assert_eq!(
        e("sprite l 3x2 palette=q\n  layer a = s\n"),
        (15, 13, InvalidArgument)
    );
    assert_eq!(
        e("sprite l 3x2 palette=p\n  layer a = s x\n"),
        (15, 15, Syntax)
    );
    assert_eq!(
        e("sprite l 3x2 palette=p\n  layer a extra\n"),
        (15, 11, Syntax)
    );
    assert_eq!(
        e("sprite l 3x2 palette=p\n  layer a = s\n    ka.\n"),
        (16, 5, Syntax)
    );
    let limits = Limits {
        max_layers: 2,
        ..Limits::default()
    };
    let three = field("sprite l 3x2 palette=p\n  layer a = s\n  layer b = s\n  layer c = s\n");
    assert_eq!(errors_with(&three, &limits), vec![(17, 3, LimitExceeded)]);
    let nested = field("sprite l 3x2 palette=p\n  layer a = s:outline(k)\n  layer b = s\n");
    assert_eq!(errors_with(&nested, &limits), vec![(14, 8, LimitExceeded)]);
}

#[test]
fn animation_errors() {
    let e = |body: &str| error(&field(body));
    assert_eq!(e("animation\n"), (14, 10, Syntax));
    assert_eq!(e("animation a\n"), (14, 11, Syntax));
    assert_eq!(e("animation a forever\n  s 1\n"), (14, 13, Syntax));
    assert_eq!(e("animation a loop once\n  s 1\n"), (14, 18, Syntax));
    assert_eq!(e("animation a\n  s\n"), (15, 4, Syntax));
    assert_eq!(e("animation a\n  s 0\n"), (15, 5, InvalidArgument));
    assert_eq!(e("animation a\n  s 65536\n"), (15, 5, InvalidArgument));
    assert_eq!(e("animation a\n  s six\n"), (15, 5, Syntax));
    assert_eq!(e("animation a\n  s 6 6\n"), (15, 7, Syntax));
    assert_eq!(e("animation a\n  t 6\n"), (15, 3, UnknownName));
    assert_eq!(e("animation a\n  s:spin 6\n"), (15, 5, Syntax));
    assert_eq!(e("animation a\n  s 6\n    s 6\n"), (16, 5, Syntax));
    assert_eq!(
        e("animation a\n  s 6\nanimation a\n  s 6\n"),
        (16, 11, DuplicateName)
    );
    let limits = Limits {
        max_frames: 2,
        ..Limits::default()
    };
    let three = field("animation a\n  s 1\n  s 1\n  s 1\n");
    assert_eq!(errors_with(&three, &limits), vec![(17, 3, LimitExceeded)]);
}

#[test]
fn random_edits_never_panic() {
    let examples = [
        include_str!("../examples/coin.spritebox"),
        include_str!("../examples/showcase.spritebox"),
        include_str!("../examples/tiles.spritebox"),
        include_str!("../examples/font.spritebox"),
        include_str!("../examples/textures.spritebox"),
    ];
    let alphabet: Vec<char> = " \n\t.#kKo=:;(),-_x0123456789abcdefgélsupwy"
        .chars()
        .collect();
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut next = |n: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % n as u64) as usize
    };
    for example in examples {
        let original: Vec<char> = example.chars().collect();
        for _ in 0..400 {
            let mut text = original.clone();
            for _ in 0..1 + next(4) {
                let at = next(text.len());
                match next(4) {
                    0 => {
                        text.remove(at);
                    }
                    1 => text.insert(at, alphabet[next(alphabet.len())]),
                    2 => text[at] = alphabet[next(alphabet.len())],
                    _ => {
                        // Move a slice of text elsewhere.
                        let len = 1 + next(40).min(text.len() - at - 1);
                        let piece: Vec<char> = text.drain(at..at + len).collect();
                        let to = next(text.len() + 1);
                        text.splice(to..to, piece);
                    }
                }
            }
            let text: String = text.into_iter().collect();
            if let Ok(doc) = Document::parse(&text) {
                let _ = doc.preview(&spritebox::PreviewOptions::default());
                let _ = doc.export(&spritebox::ExportOptions::default());
            }
        }
    }
}
