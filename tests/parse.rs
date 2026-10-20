//! Every construct of the text format, and every error with its position and code.

use spritebox::{Color, Document, ErrorCode, Limits, check};

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
    let base = format!("{DAWN}\nsprite coin 4x2 palette=dawn\n  .ko.\n  kook\n");
    let fragments = [
        "", " ", "\t", "\n", ";", "(", ")", "=", "x", "#", "é", "palette", "sprite", "from=",
        "palette=", "0x0", "4x2", "\u{0}",
    ];
    for i in 0..=base.len() {
        if !base.is_char_boundary(i) {
            continue;
        }
        for fragment in fragments {
            let text = format!("{}{fragment}{}", &base[..i], &base[i..]);
            let _ = check(&text, &Limits::default());
            let _ = Document::parse(&text);
        }
        let _ = check(&base[..i], &Limits::default());
    }
}
