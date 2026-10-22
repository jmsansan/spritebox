//! Lint: warnings with their codes and positions, and the ways to allow them.

use std::collections::HashMap;

use spritebox::{
    Document, Error, ErrorCode, Limits, LintCode, LintOptions, Warning, check, check_with_loader,
};

const EXAMPLES: [&str; 5] = [
    include_str!("../examples/coin.spritebox"),
    include_str!("../examples/showcase.spritebox"),
    include_str!("../examples/tiles.spritebox"),
    include_str!("../examples/font.spritebox"),
    include_str!("../examples/textures.spritebox"),
];

/// Lines 1 to 7; `h` is a shade of `g` too close to tell apart, and `w` a glint.
const P: &str = "\
palette p
  .  transparent
  k  #1a1c2c
  g  #3a7d44
  h  #468a50
  w  #ffffff
sprite uses_all 3x2 palette=p
  .kg
  hww
";

fn lint(text: &str) -> Vec<Warning> {
    let doc: Document = text.parse().unwrap_or_else(|e| panic!("{e}"));
    doc.lint(&LintOptions::default()).warnings
}

/// The `(line, column)` of each warning with `code`.
fn found(text: &str, code: LintCode) -> Vec<(usize, usize)> {
    lint(text)
        .iter()
        .filter(|w| w.code == code)
        .map(|w| {
            let p = w.position.expect("these warnings have positions");
            (p.line, p.column)
        })
        .collect()
}

fn orphans(body: &str) -> Vec<(usize, usize)> {
    found(&format!("{P}{body}"), LintCode::OrphanPixel)
}

#[test]
fn the_examples_are_clean() {
    for (i, text) in EXAMPLES.iter().enumerate() {
        let warnings = lint(text);
        assert!(warnings.is_empty(), "example {i}: {warnings:?}");
    }
}

#[test]
fn orphan_pixels() {
    // Lines 10 to 15: noise at (1, 1) and a glint at (3, 3).
    let leaf = "sprite leaf 5x5 palette=p\n  ggggg\n  ghggg\n  ggggg\n  gggwg\n  ggggg\n";
    assert_eq!(orphans(leaf), [(12, 4)]);
    let warnings = lint(&format!("{P}{leaf}"));
    let w = &warnings[0];
    assert_eq!(w.code, LintCode::OrphanPixel);
    assert_eq!(w.sprite.as_deref(), Some("leaf"));
    assert_eq!(w.pixels, [(1, 1)]);
    assert!(w.message.contains("`leaf` at (1, 1)"), "{}", w.message);

    // A shade with its like touching diagonally is a line, not noise.
    assert_eq!(
        orphans("sprite s 4x4 palette=p\n  gggg\n  ghgg\n  gghg\n  gggg\n"),
        []
    );
    // A speck with nothing around it; one with a neighbour on a corner is a shape.
    assert_eq!(
        orphans("sprite s 3x3 palette=p\n  ...\n  .k.\n  ...\n"),
        [(12, 4)]
    );
    assert_eq!(orphans("sprite s 3x3 palette=p\n  ...\n  .k.\n  ..k\n"), []);
    // Beyond the edge is transparent.
    assert_eq!(
        orphans("sprite s 3x3 palette=p\n  ...\n  ...\n  ..k\n"),
        [(13, 5)]
    );
    // Sprites under 3x3 are too small to judge.
    assert_eq!(orphans("sprite s 2x3 palette=p\n  ..\n  k.\n  ..\n"), []);
    // A threshold that takes the glint for noise too.
    let doc: Document = format!("{P}{leaf}").parse().unwrap();
    let strict = LintOptions {
        orphan_contrast: 255,
        ..LintOptions::default()
    };
    let pixels: Vec<_> = doc
        .lint(&strict)
        .warnings
        .into_iter()
        .flat_map(|w| w.pixels)
        .collect();
    assert_eq!(pixels, [(1, 1), (3, 3)]);
}

#[test]
fn orphan_pixels_in_layers_and_derived_sprites() {
    // The warning points at the layer that draws the pixel.
    let layered = "sprite s 3x3 palette=p\n  layer base\n    ggg\n    ggg\n    ggg\n  \
                   layer top\n    ...\n    .h.\n    ...\n";
    assert_eq!(orphans(layered), [(17, 6)]);
    // Derived sprites repeat their base's pixels and are not checked again; a sprite
    // made of other sprites' layers is, at its name.
    let leaf = "sprite leaf 3x3 palette=p\n  ggg\n  ghg\n  ggg\n";
    let more = "sprite turned = leaf:rot90\nsprite stacked 3x3 palette=p\n  layer a = leaf\n";
    assert_eq!(orphans(&format!("{leaf}{more}")), [(12, 4), (15, 8)]);
    // Allowing it for the base allows it for what is built from the base.
    let allowed = leaf.replace("palette=p", "palette=p lint=allow(orphan_pixel)");
    assert_eq!(orphans(&format!("{allowed}{more}")), []);
    // Each lone pixel of a sprite made of other sprites is its own warning, though
    // all are at its name.
    let twice = "sprite pair 7x3 palette=p\n  ggggggg\n  ghgggHg\n  ggggggg\n\
                 sprite both 7x3 palette=p\n  layer a = pair\n";
    let doc: Document = format!("{P}{}", twice.replace('H', "h")).parse().unwrap();
    let both: Vec<_> = doc
        .lint(&LintOptions::default())
        .warnings
        .into_iter()
        .filter(|w| w.sprite.as_deref() == Some("both"))
        .map(|w| (w.position.unwrap().line, w.pixels))
        .collect();
    assert_eq!(both, [(14, vec![(1, 1)]), (14, vec![(5, 1)])]);
    // A pixel seen through a layer made of another sprite is not blamed on the rows
    // it covers.
    let over = "sprite over 3x3 palette=p\n  layer base\n    ggg\n    ggg\n    ggg\n  \
                layer top = leaf\n";
    assert_eq!(orphans(&format!("{leaf}{over}")), [(12, 4), (14, 8)]);
}

const SWAPS: &str = "\
palette base
  .  transparent
  k  #000000
  o  #ff8800
  y  #ffff00
palette night from=base
  o  #884400
  y  #888800
sprite s 2x1 palette=base
  ko
sprite t = s:swap(night)
";

#[test]
fn unused_keys() {
    // `o` of `night` colors the swapped sprite; neither palette's `y` colors anything.
    assert_eq!(found(SWAPS, LintCode::UnusedKey), [(2, 3), (5, 3), (8, 3)]);
    let messages: Vec<String> = lint(SWAPS).into_iter().map(|w| w.message).collect();
    assert_eq!(messages[1], "Nothing uses key `y` of palette `base`");
    // Frames, tiles and the keys of a texture's ramp count as uses.
    let text = "palette p\n  a #000000\n  b #808080\n  c #ffffff\n  d #ff0000\n  e #00ff00\n\
                sprite s 1x1 palette=p\n  a\nanimation x\n  s:swap(q) 4\n\
                palette q from=p\n  a #111111\n\
                tileset t 1x1 palette=p\n  tile one\n    b\n\
                texture n 4x4 palette=p\n  noise cell=4\n  ramp a c d\n";
    assert_eq!(found(text, LintCode::UnusedKey), [(6, 3)]);
}

#[test]
fn allowing_warnings() {
    let all = |text: &str| lint(text).len();
    assert_eq!(all(SWAPS), 3);
    assert_eq!(
        all(&SWAPS.replace("palette base", "palette base lint=allow(unused_key)")),
        1
    );
    assert_eq!(all(&format!("lint allow unused_key\n{SWAPS}")), 0);
    assert_eq!(
        all(&format!("{SWAPS}lint allow orphan_pixel unused_key\n")),
        0
    );
    let doc: Document = SWAPS.parse().unwrap();
    let options = LintOptions {
        allow: vec![LintCode::UnusedKey],
        ..LintOptions::default()
    };
    assert!(doc.lint(&options).is_clean());
    // Every definition takes `lint=`.
    let text = format!(
        "{P}animation a once lint=allow(orphan_pixel)\n  uses_all 1\n\
         animation b lint=allow(orphan_pixel)\n  uses_all 1\n\
         tileset t 1x1 palette=p lint=allow(orphan_pixel)\n  tile g\n    g\n\
         tilemap m tileset=t lint=allow(orphan_pixel)\n  legend\n    a g\n  map\n    a\n\
         font f height=1 lint=allow(orphan_pixel)\n  glyph A\n    #\n\
         texture n 4x4 palette=p lint=allow(orphan_pixel)\n  noise cell=4\n  ramp g h\n\
         sprite d = uses_all:flip_x slice=0 lint=allow(orphan_pixel)\n"
    );
    assert_eq!(lint(&text), []);
}

fn errors(text: &str) -> Vec<(usize, usize, ErrorCode)> {
    check(text, &Limits::default())
        .iter()
        .map(|e| (e.position.unwrap().line, e.position.unwrap().column, e.code))
        .collect()
}

#[test]
fn lint_errors() {
    use ErrorCode::*;
    let s = |option: &str| {
        errors(&format!(
            "palette p\n  k #000000\nsprite s 1x1 palette=p {option}\n  k\n"
        ))
    };
    assert_eq!(s("lint=allow(jaggy)"), [(3, 35, Syntax)]);
    assert_eq!(s("lint=allow(orphan_pixel, nope)"), [(3, 49, Syntax)]);
    assert_eq!(s("lint=allow()"), [(3, 35, Syntax)]);
    assert_eq!(s("lint=orphan_pixel"), [(3, 29, Syntax)]);
    assert_eq!(
        s("lint=allow(unused_key) lint=allow(unused_key)"),
        [(3, 47, Syntax)]
    );
    let message = &check("lint allow nope\n", &Limits::default())[0].message;
    assert!(
        message.starts_with("Unknown lint `nope`; expected one of orphan_pixel, unused_key"),
        "{message}"
    );
    assert_eq!(errors("lint\n"), [(1, 5, Syntax)]);
    assert_eq!(errors("lint deny unused_key\n"), [(1, 6, Syntax)]);
    assert_eq!(errors("lint allow\n"), [(1, 11, Syntax)]);
    assert_eq!(errors("lint allow unused_key nope\n"), [(1, 23, Syntax)]);
    assert_eq!(errors("lint allow unused_key\n  x\n"), [(2, 3, Syntax)]);
    assert_eq!(
        errors("animation a once lint=allow(nope)\n  s 1\n"),
        [(1, 29, Syntax)]
    );
}

#[test]
fn warnings_in_imports_name_their_file() {
    let files: HashMap<&str, &str> = HashMap::from([(
        "shared.spritebox",
        "lint allow orphan_pixel\npalette shared\n  . transparent\n  k #000000\n  z #ffffff\n",
    )]);
    let mut loader = |name: &str| {
        files
            .get(name)
            .map(|t| t.to_string())
            .ok_or_else(|| Error::new(ErrorCode::Io, name))
    };
    let text = "import shared.spritebox\nsprite s 3x3 palette=shared\n  ...\n  .k.\n  ...\n";
    assert!(check_with_loader(text, &Limits::default(), &mut loader).is_empty());
    let doc = Document::parse_with_loader(text, &Limits::default(), &mut loader).unwrap();
    let warnings = doc.lint(&LintOptions::default()).warnings;
    let summary: Vec<_> = warnings
        .iter()
        .map(|w| {
            let p = w.position.unwrap();
            (w.code, w.file.as_deref(), p.line, p.column)
        })
        .collect();
    // The file's own warnings come first. `lint allow` in the imported file does not
    // reach the file that imports it.
    assert_eq!(
        summary,
        [
            (LintCode::OrphanPixel, None, 4, 4),
            (LintCode::UnusedKey, Some("shared.spritebox"), 5, 3)
        ]
    );
    assert_eq!(
        warnings[1].to_string(),
        "shared.spritebox, line 5, column 3: Nothing uses key `z` of palette `shared`"
    );
    // Nor to a sprite here built from one of its sprites, while `lint=` on that
    // sprite does reach it.
    let files: HashMap<&str, &str> = HashMap::from([
        (
            "base.spritebox",
            "lint allow orphan_pixel\npalette q\n  . transparent\n  g #3a7d44\n  h #468a50\n\
             sprite base 3x3 palette=q\n  ggg\n  ggg\n  ggg\n",
        ),
        (
            "allowed.spritebox",
            "palette q\n  . transparent\n  g #3a7d44\n  h #468a50\n\
             sprite base 3x3 palette=q lint=allow(orphan_pixel)\n  ggg\n  ggg\n  ggg\n",
        ),
    ]);
    let mut loader = |name: &str| {
        files
            .get(name)
            .map(|t| t.to_string())
            .ok_or_else(|| Error::new(ErrorCode::Io, name))
    };
    let local =
        "sprite noisy 3x3 palette=q\n  layer a = base\n  layer b\n    ...\n    .h.\n    ...\n";
    let orphans_with = |import: &str, loader: &mut dyn FnMut(&str) -> Result<String, Error>| {
        let text = format!("import {import}\n{local}");
        let doc = Document::parse_with_loader(&text, &Limits::default(), loader).unwrap();
        doc.lint(&LintOptions::default())
            .count(LintCode::OrphanPixel)
    };
    assert_eq!(orphans_with("base.spritebox", &mut loader), 1);
    assert_eq!(orphans_with("allowed.spritebox", &mut loader), 0);
}

#[test]
fn warnings_are_bounded() {
    let doc: Document = SWAPS.parse().unwrap();
    let limits = |max_warnings| Limits {
        max_warnings,
        ..Limits::default()
    };
    // The first warnings are kept; the counts include all of them.
    let lint = doc.lint_with(&LintOptions::default(), &limits(2));
    let lines: Vec<usize> = lint
        .warnings
        .iter()
        .map(|w| w.position.unwrap().line)
        .collect();
    assert_eq!(lines, [2, 5]);
    assert_eq!(lint.counts, [(LintCode::UnusedKey, 3)]);
    assert_eq!((lint.total(), lint.omitted()), (3, 1));
    assert_eq!(lint.count(LintCode::OrphanPixel), 0);
    let none = doc.lint_with(&LintOptions::default(), &limits(0));
    assert!(none.warnings.is_empty());
    assert_eq!(
        (none.total(), none.omitted(), none.is_clean()),
        (3, 3, false)
    );
    // Kept in order however they were found: noise found first, last in the text.
    let text = format!("{P}sprite leaf 3x3 palette=p\n  ggg\n  ghg\n  ggg\n{SWAPS}");
    let doc: Document = text.parse().unwrap();
    let lint = doc.lint_with(&LintOptions::default(), &limits(1));
    assert_eq!(lint.warnings[0].code, LintCode::OrphanPixel);
    assert_eq!(
        lint.counts,
        [(LintCode::OrphanPixel, 1), (LintCode::UnusedKey, 3)]
    );
}

#[test]
fn codes_have_stable_names() {
    for code in LintCode::ALL {
        assert_eq!(code.as_str().parse::<LintCode>().unwrap(), code);
        assert_eq!(code.to_string(), code.as_str());
    }
    assert_eq!(LintCode::OrphanPixel.as_str(), "orphan_pixel");
    assert_eq!(LintCode::UnusedKey.as_str(), "unused_key");
}
