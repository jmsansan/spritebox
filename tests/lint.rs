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

/// Lines 1 to 5; `a` is translucent.
const LINES: &str = "\
palette p
  .  transparent
  k  #1a1c2c
  o  #f4b41b
  a  #f4b41b80
";

/// A warning's line, column and pixels.
type Spot = (usize, usize, Vec<(u32, u32)>);

/// Each warning with `code` in `body` after `LINES`.
fn line_work(body: &str, code: LintCode) -> Vec<Spot> {
    lint(&format!("{LINES}{body}"))
        .into_iter()
        .filter(|w| w.code == code)
        .map(|w| {
            let p = w.position.unwrap();
            (p.line, p.column, w.pixels)
        })
        .collect()
}

/// A sprite whose top edge climbs from the bottom left in steps of `steps` pixels.
fn slope(steps: &[usize]) -> String {
    let width: usize = steps.iter().sum();
    let mut text = format!("sprite s {width}x{} palette=p\n", steps.len());
    for y in 0..steps.len() {
        let start: usize = steps[..steps.len() - 1 - y].iter().sum();
        text += &format!("  {}{}\n", ".".repeat(start), "o".repeat(width - start));
    }
    text
}

/// A stroke one pixel thin that climbs from the bottom left in steps of `steps`
/// pixels, with `extra` pixels added.
fn stroke(steps: &[usize], extra: &[(usize, usize)]) -> String {
    let width: usize = steps.iter().sum();
    let mut rows: Vec<Vec<char>> = (0..steps.len())
        .map(|y| {
            let run = steps.len() - 1 - y;
            let start: usize = steps[..run].iter().sum();
            (0..width)
                .map(|x| {
                    if x >= start && x < start + steps[run] {
                        'o'
                    } else {
                        '.'
                    }
                })
                .collect()
        })
        .collect();
    for &(x, y) in extra {
        rows[y][x] = 'o';
    }
    let mut text = format!("sprite s {width}x{} palette=p\n", steps.len());
    for row in &rows {
        text += &format!("  {}\n", row.iter().collect::<String>());
    }
    text
}

#[test]
fn jaggies() {
    // A step of 3 among steps of 1, reported at its pixels; the sprite starts on line
    // 6, so row 2 is line 9.
    assert_eq!(
        line_work(&slope(&[1, 1, 1, 3, 1, 5]), LintCode::Jaggy),
        [(9, 6, vec![(3, 2), (4, 2), (5, 2)])]
    );
    let warnings = lint(&format!("{LINES}{}", slope(&[1, 1, 1, 3, 1, 5])));
    let jaggy = warnings.iter().find(|w| w.code == LintCode::Jaggy).unwrap();
    let message = &jaggy.message;
    assert!(
        message.contains("1, 1, 3, 1 pixels long; the step of 3 at (3, 2)"),
        "{message}"
    );
    // A short step among long ones too.
    assert_eq!(
        line_work(&slope(&[3, 3, 3, 1, 3, 3, 3]), LintCode::Jaggy).len(),
        1
    );
    // Steady lines, alternating lines and curves are clean; so are the first and last
    // steps, which corners cut short.
    for steps in [
        &[2, 2, 2, 2, 2, 2][..],
        &[2, 1, 2, 1, 2, 1, 2],
        &[1, 1, 2, 3, 4, 6],
        &[6, 4, 3, 2, 1, 1],
        &[1, 4, 1, 1, 4],
    ] {
        assert_eq!(line_work(&slope(steps), LintCode::Jaggy), [], "{steps:?}");
    }
    // Where the edge turns, the step at the turn counts on both sides of it: a valley
    // whose far side climbs 1, 1, 3, 1, 1 from its lowest step, mirrored or not.
    let tops = [3, 3, 4, 4, 5, 4, 3, 3, 3, 2, 1];
    let valley = |tops: &[usize]| {
        let mut text = format!("sprite s {}x6 palette=p\n", tops.len());
        for y in 0..6 {
            let row: String = tops
                .iter()
                .map(|&t| if y >= t { 'o' } else { '.' })
                .collect();
            text += &format!("  {row}\n");
        }
        text
    };
    let mirrored: Vec<usize> = tops.iter().rev().copied().collect();
    for tops in [&tops[..], &mirrored] {
        let found = line_work(&valley(tops), LintCode::Jaggy);
        assert_eq!(found.len(), 1, "{tops:?}");
        assert_eq!(found[0].2.len(), 3, "{tops:?}");
    }
    // A stroke one pixel thin is reported once, not from above and below.
    assert_eq!(
        line_work(&stroke(&[1, 1, 1, 3, 1, 5], &[]), LintCode::Jaggy).len(),
        1
    );
    // One warning for each uneven step, however the sides see it: a stroke with two,
    // thickened where one side's stretch ends and the other's goes on.
    let found = line_work(
        &stroke(&[1, 1, 1, 3, 1, 1, 1, 3, 1, 1, 1], &[(10, 4)]),
        LintCode::Jaggy,
    );
    let mut pixels: Vec<(u32, u32)> = found.iter().flat_map(|(_, _, p)| p.clone()).collect();
    let all = pixels.len();
    pixels.sort();
    pixels.dedup();
    assert_eq!((found.len(), pixels.len()), (2, all), "{found:?}");
    // Also when the sides see overlapping parts of a step: a stroke thickened at one
    // end of its long step.
    assert_eq!(
        line_work(&stroke(&[1, 1, 1, 5, 1, 1, 1], &[(7, 2)]), LintCode::Jaggy).len(),
        1
    );
    // Steep sides are measured down the rows: the same slope turned on its side, and
    // drawn so.
    let flat: Document = format!(
        "{LINES}{}sprite turned = s:rot90\n",
        slope(&[1, 1, 1, 3, 1, 5])
    )
    .parse()
    .unwrap();
    let turned = flat.sprite("turned").unwrap();
    let mut text = format!("{LINES}sprite drawn 6x12 palette=p\n");
    for y in 0..turned.height() {
        let row: String = (0..turned.width())
            .map(|x| turned.key(x, y).unwrap())
            .collect();
        text += &format!("  {row}\n");
    }
    let doc: Document = text.parse().unwrap();
    let lint = doc.lint(&LintOptions::default());
    assert_eq!(lint.count(LintCode::Jaggy), 1);
    assert_eq!(lint.count(LintCode::DoubleOutline), 0);
}

/// An 8×8 triangle outlined in `k`, its long side doubled when `doubled`.
fn triangle(doubled: bool) -> String {
    let mut text = String::from("sprite t 8x8 palette=p\n  ");
    for y in 0..8 {
        for x in 0..8 {
            text.push(if x > y {
                '.'
            } else if x == y || x == 0 || y == 7 || (doubled && x + 1 == y) {
                'k'
            } else {
                'o'
            });
        }
        text.push_str("\n  ");
    }
    text.trim_end().to_owned() + "\n"
}

#[test]
fn double_outlines() {
    // Each L corner on the long side, taken out in turn, leaves one diagonal line.
    let found = line_work(&triangle(true), LintCode::DoubleOutline);
    let pixels: Vec<(u32, u32)> = found.iter().flat_map(|(_, _, p)| p.clone()).collect();
    assert_eq!(pixels, [(2, 2), (3, 3), (4, 4), (5, 5)]);
    assert_eq!((found[0].0, found[0].1), (9, 5));
    assert_eq!(line_work(&triangle(false), LintCode::DoubleOutline), []);
    // Square corners, lines meeting lines, and lines two pixels thick throughout are
    // drawn on purpose.
    for body in [
        "sprite box 6x6 palette=p\n  kkkkk.\n  kooko.\n  kooko.\n  kkkkk.\n  ......\n  ......\n",
        "sprite split 5x5 palette=p\n  kkkkk\n  kokok\n  kokok\n  kkkkk\n  .....\n",
        "sprite thick 8x8 palette=p\n  ........\n  .kkkkkk.\n  .kkkkkk.\n  .kkookk.\n  \
         .kkookk.\n  .kkkkkk.\n  .kkkkkk.\n  ........\n",
        // A stroke drawn on its own, evenly two pixels wide, outlines nothing.
        "sprite stroke 8x7 palette=p\n  kk......\n  .kk.....\n  ..kk....\n  ...kk...\n  \
         ....kk..\n  .....kk.\n  ......kk\n",
        // A tile, with no transparent pixel, has no silhouette to outline.
        "sprite tile 4x4 palette=p\n  kkko\n  kkoo\n  kooo\n  oooo\n",
    ] {
        assert_eq!(line_work(body, LintCode::DoubleOutline), [], "{body}");
    }
}

#[test]
fn stray_alpha() {
    let edge = "sprite s 4x4 palette=p\n  .ao.\n  oooo\n  oooo\n  oooo\n";
    assert_eq!(
        line_work(edge, LintCode::StrayAlpha),
        [(7, 4, vec![(1, 0)])]
    );
    // Glass: translucent pixels together are an area, not leftovers; and a sprite with
    // many translucent pixels uses them on purpose.
    let glass = "sprite s 6x4 palette=p\n  oooooo\n  oaaooo\n  oaaooo\n  oooooo\n";
    assert_eq!(line_work(glass, LintCode::StrayAlpha), []);
    let ghost = "sprite s 4x3 palette=p\n  aoao\n  oooo\n  oaoa\n";
    assert_eq!(line_work(ghost, LintCode::StrayAlpha), []);
}

const SHADING: &str = include_str!("fixtures/lint/shading.spritebox");

/// The code and sprite of each warning of `text` with `options`.
fn reported(text: &str, options: &LintOptions) -> Vec<(LintCode, String)> {
    let doc: Document = text.parse().unwrap();
    doc.lint(options)
        .warnings
        .into_iter()
        .map(|w| (w.code, w.sprite.unwrap_or_default()))
        .collect()
}

#[test]
fn shading_and_scale() {
    // Git may check the fixture out with CRLF line ends, which the parser reads but
    // the searches below do not.
    let shading = SHADING.replace("\r\n", "\n");
    let shading = shading.as_str();
    let found = reported(shading, &LintOptions::default());
    let expected = [
        (LintCode::PillowShading, "pillow"),
        (LintCode::Banding, "bands"),
        (LintCode::Mixel, "big"),
        (LintCode::TooManyColors, "rainbow"),
    ];
    let expected: Vec<(LintCode, String)> =
        expected.iter().map(|&(c, s)| (c, s.to_owned())).collect();
    assert_eq!(found, expected);
    // The thresholds are options.
    let lenient = LintOptions {
        pillow_offset: 0,
        banding_length: 9,
        max_colors_small: 9,
        ..LintOptions::default()
    };
    // An offset of zero still finds the pillow, whose light is exactly in the middle.
    let found = reported(shading, &lenient);
    assert_eq!(
        found,
        [
            (LintCode::PillowShading, "pillow".to_owned()),
            (LintCode::Mixel, "big".to_owned())
        ]
    );
    // Short bands crossing are not one long band.
    let cross = format!(
        "{}sprite cross 5x5 palette=p\n  ddddd\n  dmlmd\n  ddmld\n  dmlmd\n  ddddd\n",
        shading.split("sprite pillow").next().unwrap()
    );
    let banded = reported(&cross, &LintOptions::default())
        .into_iter()
        .filter(|(c, _)| *c == LintCode::Banding)
        .count();
    assert_eq!(banded, 0);
    // A panel's bevel is shaded so on purpose.
    let panel = shading.replace(
        "sprite pillow 14x14 palette=p",
        "sprite pillow 14x14 palette=p slice=3",
    );
    assert_eq!(reported(&panel, &LintOptions::default()).len(), 3);
    // A document drawn at 2x throughout is consistent.
    let alone = shading.split("sprite pillow").next().unwrap().to_owned()
        + "sprite big"
        + shading
            .split("sprite big")
            .nth(1)
            .unwrap()
            .split("sprite rainbow")
            .next()
            .unwrap();
    let keys = LintOptions {
        allow: vec![LintCode::UnusedKey],
        ..LintOptions::default()
    };
    assert_eq!(reported(&alone, &keys), []);
    // Rings around a square: where the lighter half ends within a shade, no part of
    // that shade is favoured, so the light stays in the middle.
    let mut square = String::from("sprite square 12x12 palette=p\n");
    for y in 0..12i32 {
        let row: String = (0..12i32)
            .map(|x| {
                let ring = x.min(y).min(11 - x).min(11 - y);
                match ring {
                    0 => '.',
                    1 => 'k',
                    2 | 3 => 'd',
                    4 => 'm',
                    _ => 'w',
                }
            })
            .collect();
        square += &format!("  {row}\n");
    }
    let palette = shading.split("sprite pillow").next().unwrap();
    let found = reported(&format!("{palette}{square}"), &keys);
    // (Its ring one pixel thick is banding too.)
    assert!(
        found.contains(&(LintCode::PillowShading, "square".to_owned())),
        "{found:?}"
    );
    // What counts is the color seen: a translucent glaze over the outline and the
    // fill does not hide the pillow under it.
    let rows: Vec<&str> = shading
        .split("sprite pillow 14x14 palette=p\n")
        .nth(1)
        .unwrap()
        .lines()
        .take(14)
        .map(str::trim)
        .collect();
    let mut glazed =
        format!("{palette}  g #ffffff30\nsprite glazed 14x14 palette=p\n  layer base\n");
    for row in &rows {
        glazed += &format!("    {row}\n");
    }
    glazed += "  layer glaze\n";
    for row in &rows {
        let glaze: String = row
            .chars()
            .map(|c| if c == '.' { '.' } else { 'g' })
            .collect();
        glazed += &format!("    {glaze}\n");
    }
    let found = reported(&glazed, &keys);
    assert!(
        found.contains(&(LintCode::PillowShading, "glazed".to_owned())),
        "{found:?}"
    );
    // The limit for any sprite holds for small ones too.
    let capped = LintOptions {
        max_colors_small: 16,
        max_colors: 8,
        ..keys.clone()
    };
    let rainbow = format!("{palette}sprite rainbow 3x3 palette=p\n  123\n  456\n  789\n");
    assert_eq!(
        reported(&rainbow, &capped),
        [(LintCode::TooManyColors, "rainbow".to_owned())]
    );
}

/// Lines 1 to 14: colors too close, a flat ramp, a made ramp, and a grey ramp in
/// steps too small.
const RAMPS: &str = "\
palette p
  .  transparent
  k  #1a1c2c
  g  #38b764
  G  #39b865
  1  #203020
  2  #406040
  3  #609060
  4  #80c080
  ramp grass 1234
  ramp skin abcde base=#d08159 shift=20
  x  #808080
  y  #848484
  ramp grey xy
sprite s 13x1 palette=p lint=allow(too_many_colors)
  .kgG1234abcde
sprite t 2x1 palette=p
  xy
";

#[test]
fn palettes_and_ramps() {
    let found = |options: &LintOptions| -> Vec<(LintCode, usize, usize)> {
        let doc: Document = RAMPS.parse().unwrap();
        doc.lint(options)
            .warnings
            .into_iter()
            .map(|w| {
                let p = w.position.unwrap();
                (w.code, p.line, p.column)
            })
            .collect()
    };
    use LintCode::*;
    // The made ramp, `skin`, turns its hue and steps clearly: nothing to report.
    assert_eq!(
        found(&LintOptions::default()),
        [
            (NearDuplicateColor, 5, 3),
            (FlatRamp, 10, 8),
            (NearDuplicateColor, 13, 3),
            (LowContrast, 14, 14)
        ]
    );
    let messages: Vec<String> = RAMPS
        .parse::<Document>()
        .unwrap()
        .lint(&LintOptions::default())
        .warnings
        .into_iter()
        .map(|w| w.message)
        .collect();
    assert_eq!(
        messages[0],
        "Keys `g` and `G` of palette `p` are almost the same color, 0.0030 apart in OKLab; \
         one would do"
    );
    assert_eq!(
        messages[3],
        "Keys `x` and `y` of ramp `grey` in palette `p` differ by 0.0134 in lightness, too \
         little to tell apart"
    );
    // The thresholds are options; grey ramps are never flat.
    let lenient = LintOptions {
        min_color_distance: 0,
        min_ramp_step: 0,
        ..LintOptions::default()
    };
    assert_eq!(found(&lenient), [(FlatRamp, 10, 8)]);
    // Unused keys are not judged as too close: drop `G` from the sprite.
    let text = RAMPS.replace(".kgG1234abcde", ".kgg1234abcde");
    let doc: Document = text.parse().unwrap();
    let lint = doc.lint(&LintOptions::default());
    assert_eq!(lint.count(NearDuplicateColor), 1);
    assert_eq!(lint.count(UnusedKey), 1);
    // A copy's colors count with those it inherits: `a` from `p`, `b` from `q`.
    let copied = "palette p\n  a #ff0000\npalette q from=p\n  b #fe0000\n\
                  sprite s 2x1 palette=q\n  ab\n";
    let doc: Document = copied.parse().unwrap();
    let w = &doc.lint(&LintOptions::default()).warnings;
    let summary: Vec<_> = w
        .iter()
        .map(|w| (w.code, w.position.unwrap().line))
        .collect();
    assert_eq!(summary, [(NearDuplicateColor, 4)]);
    // An inherited ramp is checked with the colors the copy gives it, and a problem
    // inherited unchanged is reported once.
    let ramps = "palette p\n  ramp r abc base=#d08159\npalette q from=p\n  a #d08159\n\
                 sprite s 3x1 palette=p\n  abc\nsprite t 3x1 palette=q\n  abc\n";
    let doc: Document = ramps.parse().unwrap();
    let lint = doc.lint(&LintOptions::default());
    let summary: Vec<_> = lint
        .warnings
        .iter()
        .map(|w| (w.code, w.position.unwrap().line, w.position.unwrap().column))
        .collect();
    // `a` and `b` are now one color in `q`: no step between them, at `b` in the ramp;
    // and too close, at `q`'s `a`, the later line.
    assert_eq!(summary, [(LowContrast, 2, 11), (NearDuplicateColor, 4, 3)]);
    assert!(lint.warnings[0].message.contains("palette `q`"));
    let same = "palette p\n  ramp r ab\n  a #808080\n  b #848484\npalette q from=p\n\
                sprite s 2x1 palette=p\n  ab\nsprite t 2x1 palette=q\n  ab\n";
    let doc: Document = same.parse().unwrap();
    let lint = doc.lint(&LintOptions::default());
    assert_eq!(
        (lint.count(LowContrast), lint.count(NearDuplicateColor)),
        (1, 1)
    );
    // Two copies that change an inherited ramp differently each have their problem,
    // and each copy's `lint=` allows its own.
    let two = "palette p\n  ramp r ab\n  a #000000\n  b #ffffff\n\
               palette q from=p\n  a #808080\n  b #848484\n\
               palette u from=p\n  a #202020\n  b #242424\n\
               sprite s 2x1 palette=q\n  ab\nsprite t 2x1 palette=u\n  ab\n";
    let low = |text: &str| {
        let doc: Document = text.parse().unwrap();
        doc.lint(&LintOptions::default()).count(LowContrast)
    };
    assert_eq!(low(two), 2);
    assert_eq!(
        low(&two.replace(
            "palette q from=p",
            "palette q from=p lint=allow(low_contrast)"
        )),
        1
    );
    // Also when the copies change it the same way: the copy that allows it does not
    // hide it for the one that does not.
    let alike = two
        .replace("#202020", "#808080")
        .replace("#242424", "#848484");
    assert_eq!(low(&alike), 1);
    let allowed = "palette q from=p lint=allow(low_contrast)";
    assert_eq!(low(&alike.replace("palette q from=p", allowed)), 1);
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
    // So do keys that a chain uses from a palette it swaps away from: `q` places the
    // outline here, though the result is drawn with `p`.
    let midway = "palette p\n  . transparent\n  a #ff0000\n  k #000000\n\
                  palette q\n  . transparent\n  a #00ff00\n  k #111111\n\
                  sprite s 3x1 palette=p\n  .a.\nsprite t = s:swap(q):outline(k):swap(p)\n";
    assert_eq!(found(midway, LintCode::UnusedKey), []);
    // Even when a later step moves those keys out of the result. (Nothing outlines
    // with `p`'s `k` any more.)
    let gone = midway.replace(":swap(p)", ":offset(2,0)");
    assert_eq!(found(&gone, LintCode::UnusedKey), [(4, 3)]);
    assert_eq!(
        found(&midway.replace(":swap(q)", ""), LintCode::UnusedKey),
        [(6, 3), (7, 3), (8, 3)]
    );
}

#[test]
fn allowing_warnings() {
    let all = |text: &str| lint(text).len();
    assert_eq!(all(SWAPS), 3);
    // `lint=` on a palette reaches the palettes copied from it, as on a sprite.
    assert_eq!(
        all(&SWAPS.replace("palette base", "palette base lint=allow(unused_key)")),
        0
    );
    assert_eq!(
        all(&SWAPS.replace("palette night", "palette night lint=allow(unused_key)")),
        2
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
    assert_eq!(s("lint=allow(wobbly)"), [(3, 35, Syntax)]);
    assert_eq!(s("lint=allow(orphan_pixel, nope)"), [(3, 49, Syntax)]);
    assert_eq!(s("lint=allow()"), [(3, 35, Syntax)]);
    assert_eq!(s("lint=orphan_pixel"), [(3, 29, Syntax)]);
    assert_eq!(
        s("lint=allow(unused_key) lint=allow(unused_key)"),
        [(3, 47, Syntax)]
    );
    let message = &check("lint allow nope\n", &Limits::default())[0].message;
    assert!(
        message.starts_with("Unknown lint `nope`; expected one of orphan_pixel, "),
        "{message}"
    );
    for code in LintCode::ALL {
        assert!(message.contains(code.as_str()), "{message}");
    }
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
