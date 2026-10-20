//! Procedural textures: fixed for a seed, wrapping, and usable as any sprite.

use spritebox::{Document, ErrorCode, Limits, Noise, check};

const EXAMPLE: &str = include_str!("../examples/textures.spritebox");

fn rows(doc: &Document, name: &str) -> Vec<String> {
    let s = doc.sprite(name).unwrap();
    (0..s.height())
        .map(|y| (0..s.width()).map(|x| s.key(x, y).unwrap()).collect())
        .collect()
}

const SMALL: &str = "palette p
  a  #000000
  b  #808080
  c  #ffffff
texture t 8x4 seed=42
  noise cell=4 octaves=2
  ramp a b c
  dither bayer2
";

#[test]
fn a_seed_gives_fixed_pixels() {
    let doc: Document = SMALL.parse().unwrap();
    // Fixed for this seed on every platform; any change to the noise, the hash or
    // the dithering shows here.
    assert_eq!(
        rows(&doc, "t"),
        ["cbbbcbcc", "bbababbc", "bbbaabcb", "bbababbc"]
    );
}

#[test]
fn seeds_and_options_change_the_pixels() {
    let base: Document = SMALL.parse().unwrap();
    assert_eq!(
        rows(&base, "t"),
        rows(&SMALL.parse::<Document>().unwrap(), "t")
    );
    for variant in [
        SMALL.replace("seed=42", "seed=43"),
        SMALL.replace("octaves=2", "octaves=1"),
        SMALL.replace("bayer2", "none"),
        SMALL.replace("ramp a b c", "ramp c b a"),
    ] {
        let doc: Document = variant.parse().unwrap();
        assert_ne!(rows(&doc, "t"), rows(&base, "t"), "{variant}");
    }
    // Without `seed=`, the seed is 0; the texture still uses every ramp end.
    let zero: Document = SMALL.replace(" seed=42", "").parse().unwrap();
    let all: String = rows(&zero, "t").concat();
    assert!(all.contains('a') && all.contains('c'), "{all}");
}

#[test]
fn textures_wrap() {
    // The noise repeats every width and height, so the texture tiles seamlessly: its
    // first column continues its last as smoothly as any two neighbors.
    let noise = Noise {
        width: 16,
        height: 16,
        cell: 8,
        octaves: 3,
        roughness: 60,
        seed: 7,
    };
    let step = |x0: i64, x1: i64| {
        (0..16)
            .map(|y| (noise.value(x0, y) as i64 - noise.value(x1, y) as i64).abs())
            .max()
            .unwrap()
    };
    let seam = step(15, 16);
    let inner = (0..15).map(|x| step(x, x + 1)).max().unwrap();
    assert!(seam <= inner, "seam {seam}, largest inner step {inner}");
    assert_eq!(noise.value(16, 3), noise.value(0, 3));
}

#[test]
fn textures_are_sprites() {
    let doc: Document = EXAMPLE.parse().unwrap();
    for name in ["grass", "stone", "sky", "dirt", "block"] {
        assert!(doc.sprite(name).is_some(), "{name}");
    }
    // As a layer: the block's face is the stone texture, under its rim.
    let (stone, block) = (rows(&doc, "stone"), rows(&doc, "block"));
    assert_eq!(&block[5][1..15], &stone[5][1..15]);
    assert!(block[0].chars().all(|c| c == 'k'));
    // As a tile and a terrain's fill.
    let tileset = doc.tileset("ground").unwrap();
    let dirt = doc.sprite("dirt").unwrap().render();
    assert_eq!(
        tileset
            .tile(tileset.index("dirt").unwrap())
            .unwrap()
            .render(),
        dirt
    );
    let soil = tileset.terrain("soil").unwrap();
    assert_eq!(
        tileset.tile(soil.index(255)).unwrap().render(),
        dirt,
        "surrounded: all fill"
    );
    // Through chains, and as frames.
    let doc: Document = format!(
        "{EXAMPLE}sprite grass_left = grass:flip_x\nanimation wave\n  grass 4\n  grass:rot90 4\n"
    )
    .parse()
    .unwrap();
    let (grass, left) = (rows(&doc, "grass"), rows(&doc, "grass_left"));
    assert_eq!(left[3], grass[3].chars().rev().collect::<String>());
    assert!(doc.sprite("grass:rot90").is_some());
}

fn errors(text: &str) -> Vec<(usize, usize, ErrorCode)> {
    check(text, &Limits::default())
        .iter()
        .map(|e| (e.position.unwrap().line, e.position.unwrap().column, e.code))
        .collect()
}

#[test]
fn texture_errors() {
    use ErrorCode::*;
    let p = "palette p\n  a #000000\n  b #ffffff\n";
    // `p` takes lines 1..=3; the texture's header is line 4.
    let e = |body: &str| errors(&format!("{p}{body}"));
    let t = |lines: &str| e(&format!("texture t 8x8\n{lines}"));
    assert_eq!(e("texture t\n"), [(4, 10, Syntax)]);
    assert_eq!(
        e("texture t 8x8 seed=x\n  noise cell=4\n  ramp a b\n"),
        [(4, 20, InvalidArgument)]
    );
    assert_eq!(
        e("texture t 8x8 seed=-1\n  noise cell=4\n  ramp a b\n"),
        [(4, 20, InvalidArgument)]
    );
    assert_eq!(e("texture t 8x8 smooth=1\n"), [(4, 15, Syntax)]);
    assert_eq!(t("  ramp a b\n"), [(4, 9, Syntax)], "no noise");
    assert_eq!(t("  noise cell=4\n"), [(4, 9, Syntax)], "no ramp");
    assert_eq!(t("  noise\n  ramp a b\n"), [(5, 8, Syntax)], "no cell");
    assert_eq!(
        t("  noise cell=3\n  ramp a b\n"),
        [(5, 14, InvalidArgument)]
    );
    assert_eq!(
        t("  noise cell=4 octaves=4\n  ramp a b\n"),
        [(5, 14, InvalidArgument)]
    );
    assert_eq!(
        t("  noise cell=4 roughness=101\n  ramp a b\n"),
        [(5, 14, InvalidArgument)]
    );
    assert_eq!(t("  noise cell=x\n  ramp a b\n"), [(5, 14, Syntax)]);
    assert_eq!(t("  noise cell=4 size=2\n  ramp a b\n"), [(5, 16, Syntax)]);
    assert_eq!(t("  noise cell=4 cell\n  ramp a b\n"), [(5, 16, Syntax)]);
    assert_eq!(
        t("  noise cell=4\n  noise cell=2\n  ramp a b\n"),
        [(6, 3, DuplicateName)]
    );
    assert_eq!(t("  noise cell=4\n  ramp\n"), [(6, 7, Syntax)]);
    assert_eq!(t("  noise cell=4\n  ramp ab\n"), [(6, 8, Syntax)]);
    assert_eq!(t("  noise cell=4\n  ramp a z\n"), [(6, 10, UnknownName)]);
    assert_eq!(
        t("  noise cell=4\n  ramp a b\n  dither halftone\n"),
        [(7, 10, Syntax)]
    );
    assert_eq!(
        t("  noise cell=4\n  ramp a b\n  dither\n"),
        [(7, 9, Syntax)]
    );
    assert_eq!(
        t("  noise cell=4\n  ramp a b\n  blur 2\n"),
        [(7, 3, Syntax)]
    );
    assert_eq!(t("  noise cell=4\n    ramp a b\n"), [(6, 5, Syntax)]);
    assert_eq!(
        e("texture t 8x8\n  noise cell=4\n  ramp a b\nsprite t 1x1\n  a\n"),
        [(7, 8, DuplicateName)]
    );
}
