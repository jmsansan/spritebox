use spritebox::{Color, Document, ErrorCode, Fill, Insets, Limits, Shadow};

fn panel() -> Document {
    let mut text = String::from("palette p\n");
    for (i, k) in "abcdefghi".chars().enumerate() {
        text.push_str(&format!("  {k} #{:06x}\n", (i + 1) * 0x111111));
    }
    text.push_str("sprite panel 9x9 slice=3\n");
    for row in [
        "abcdefghi",
        "bcdefghia",
        "cdefghiab",
        "defghiabc",
        "efghiabcd",
        "fghiabcde",
        "ghiabcdef",
        "hiabcdefg",
        "iabcdefgh",
    ] {
        text.push_str(&format!("  {row}\n"));
    }
    Document::parse(&text).unwrap()
}

#[test]
fn panel_pixels_and_transform_insets() {
    let doc = panel();
    let source = doc.sprite("panel").unwrap();
    assert_eq!(
        source.resize(9, 9, Fill::Stretch).unwrap().render(),
        source.render()
    );
    let stretched = source.resize(15, 12, Fill::Stretch).unwrap();
    let tiled = source.resize(15, 12, Fill::Tile).unwrap();
    assert_eq!(
        (0..15)
            .map(|x| stretched.key(x, 3).unwrap())
            .collect::<String>(),
        "defggghhhii iabc".replace(' ', "")
    );
    assert_eq!(
        (0..15)
            .map(|x| tiled.key(x, 3).unwrap())
            .collect::<String>(),
        "defghighi ghiabc".replace(' ', "")
    );
    for w in 9..20 {
        for h in 9..20 {
            let resized = source.resize(w, h, Fill::Tile).unwrap();
            for (ox, oy, nx, ny) in [
                (0, 0, 0, 0),
                (6, 0, w - 3, 0),
                (0, 6, 0, h - 3),
                (6, 6, w - 3, h - 3),
            ] {
                for y in 0..3 {
                    for x in 0..3 {
                        assert_eq!(source.key(ox + x, oy + y), resized.key(nx + x, ny + y));
                    }
                }
            }
            for q in 0..4 {
                let before = source
                    .rotate(q)
                    .resize(
                        if q % 2 == 0 { w } else { h },
                        if q % 2 == 0 { h } else { w },
                        Fill::Stretch,
                    )
                    .unwrap();
                assert_eq!(before.slice(), resized.rotate(q).slice());
                assert_eq!(
                    resized.flip_x().slice(),
                    Some(Insets {
                        left: 3,
                        top: 3,
                        right: 3,
                        bottom: 3
                    })
                );
            }
        }
    }
}

#[test]
fn panel_and_shadow_errors_have_positions() {
    for (chain, code) in [
        ("p:size(1x3)", ErrorCode::InvalidArgument),
        ("p:shadow(k,9,0)", ErrorCode::InvalidArgument),
        ("p:shadow(z,1,0)", ErrorCode::UnknownName),
    ] {
        let text = format!(
            "palette p\n  k #000000\nsprite p 3x3 slice=1\n  kkk\n  kkk\n  kkk\nsprite result = {chain}\n"
        );
        let e = Document::parse(&text).unwrap_err();
        assert_eq!(e.code, code);
        assert_eq!(e.position.unwrap().line, 7);
    }
    let doc =
        Document::parse("palette p\n  k #000000\nsprite bar 3x1 slice=1,0,1,0\n  kkk\n").unwrap();
    let bar = doc.sprite("bar").unwrap();
    assert_eq!(bar.resize(8, 1, Fill::Tile).unwrap().width(), 8);
    assert!(
        bar.resize(8, 2, Fill::Tile)
            .unwrap_err()
            .message
            .contains("height")
    );
    assert_eq!(
        bar.resize_with(
            8,
            1,
            Fill::Tile,
            &Limits {
                max_sprite_side: 4,
                ..Limits::default()
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
}

#[test]
fn shadow_directions_long_and_alpha() {
    let doc = Document::parse(
        "palette p\n  . transparent\n  w #ffffff\n  k #00000080\nsprite dot 1x1 slice=0\n  w\n",
    )
    .unwrap();
    let dot = doc.sprite("dot").unwrap();
    for dx in -1i32..=1 {
        for dy in -1i32..=1 {
            let s = dot.shadow('k', dx, dy, Shadow::Hard).unwrap();
            let (x, y) = ((-dx).max(0) as u32, (-dy).max(0) as u32);
            assert_eq!(s.render().pixel(x, y), Some(Color::WHITE));
            assert_eq!(
                s.slice(),
                Some(Insets {
                    left: x,
                    top: y,
                    right: dx.max(0) as u32,
                    bottom: dy.max(0) as u32
                })
            );
            if dx != 0 || dy != 0 {
                assert_eq!(
                    s.render()
                        .pixel((x as i32 + dx) as u32, (y as i32 + dy) as u32),
                    Some(Color::rgba(0, 0, 0, 128))
                );
            }
        }
    }
    let long = dot.shadow('k', 3, 2, Shadow::Long).unwrap().render();
    for (x, y) in [(1, 1), (2, 1), (3, 2)] {
        assert_eq!(long.pixel(x, y), Some(Color::rgba(0, 0, 0, 128)));
    }
}

#[test]
fn asymmetric_panel_borders_survive_effect_chains() {
    let doc=Document::parse("palette p\n  a #ff0000\n  b #00ff00\nsprite p 4x5 slice=1,2,2,1\n  aabb\n  abba\n  baab\n  abba\n  bbaa\n").unwrap();
    let s = doc.sprite("p").unwrap();
    assert_eq!(
        s.rotate(1).slice(),
        Some(Insets {
            left: 1,
            top: 1,
            right: 2,
            bottom: 2
        })
    );
    assert_eq!(
        s.flip_x().slice(),
        Some(Insets {
            left: 2,
            top: 2,
            right: 1,
            bottom: 1
        })
    );
    for w in 5..18 {
        for h in 6..18 {
            let a = s.resize(w, h, Fill::Tile).unwrap();
            assert_eq!(
                a.rotate(1).slice(),
                s.rotate(1).resize(h, w, Fill::Tile).unwrap().slice()
            );
            let shadow = s.shadow('b', -2, 3, Shadow::Hard).unwrap();
            let resized = shadow.resize(w + 2, h + 3, Fill::Tile).unwrap();
            assert_eq!(resized.render().pixel(2, 0), s.render().pixel(0, 0));
            assert_eq!(resized.render().pixel(w + 1, h - 1), s.render().pixel(3, 4));
        }
    }
}

#[test]
fn text_sprite_layout_chains_and_kerning() {
    let doc = Document::parse(include_str!("fixtures/ui-text.spritebox")).unwrap();
    let label = doc.sprite("label").unwrap();
    assert_eq!((label.width(), label.height()), (5, 3));
    assert_eq!(
        (0..5)
            .map(|x| label.key(x, 0).unwrap_or('.'))
            .collect::<String>(),
        "www.."
    );
    let paragraph = doc.sprite("paragraph").unwrap();
    assert_eq!((paragraph.width(), paragraph.height()), (6, 7));
    assert_eq!(paragraph.render().pixel(0, 4), label.render().pixel(0, 0));
    assert_eq!(doc.sprite("layered").unwrap().render(), label.render());
    assert!(doc.sprite("outlined").unwrap().width() > label.width());
    assert_eq!(doc.font("hud").unwrap().kern('A', 'V'), -1);
    let no_kern = Document::parse(
        &include_str!("fixtures/ui-text.spritebox")
            .replace("  kern AV -1\n", "")
            .replace("sprite layered 5x3", "sprite layered 6x3"),
    )
    .unwrap();
    assert_eq!(no_kern.sprite("label").unwrap().width(), 6);
    let bytes = doc
        .font("hud")
        .unwrap()
        .to_ttf(&Default::default())
        .unwrap();
    let read16 = |i| u16::from_be_bytes([bytes[i], bytes[i + 1]]);
    let n = read16(4) as usize;
    let entry = (0..n)
        .map(|i| 12 + 16 * i)
        .find(|&i| &bytes[i..i + 4] == b"kern")
        .unwrap();
    let offset = u32::from_be_bytes(bytes[entry + 8..entry + 12].try_into().unwrap()) as usize;
    assert_eq!(read16(offset + 10), 1);
    assert_eq!(read16(offset + 22) as i16, -128);
}

#[test]
fn literal_text_spaces_escapes_and_errors() {
    let prefix = include_str!("fixtures/ui-text.spritebox")
        .split("text label")
        .next()
        .unwrap();
    let doc = Document::parse(&format!(
        "{prefix}text special font=hud color=w\n    A\n      V\n    U+003B\n    \n    A\n"
    ))
    .unwrap();
    assert_eq!(doc.sprite("special").unwrap().height(), 19);
    assert_eq!(
        doc.sprite("special").unwrap().render().pixel(2, 4),
        Some(Color::WHITE)
    );
    let unknown =
        Document::parse(&format!("{prefix}text bad font=hud color=w\n  AX\n")).unwrap_err();
    assert_eq!(unknown.code, ErrorCode::UnknownName);
    assert_eq!(unknown.position.unwrap().column, 4);
    let word = Document::parse(&format!(
        "{prefix}text bad font=hud color=w width=2\n  AA\n"
    ))
    .unwrap_err();
    assert_eq!(word.code, ErrorCode::InvalidArgument);
}

#[test]
fn kerning_limits_and_overlapping_ink_regressions() {
    let source =
        "font f height=1\n  glyph A advance=4\n    ###\n  kern AV -4\n  glyph V advance=2\n    #\n";
    let doc = Document::parse(source).unwrap();
    let font = doc.font("f").unwrap();
    assert_eq!(font.measure("AV"), (4, 1));
    assert_eq!(
        font.render("AV", Color::WHITE).unwrap().pixel(2, 0),
        Some(Color::WHITE)
    );
    let glyphs = Document::parse_with(
        source,
        &Limits {
            max_glyphs: 1,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(glyphs.code, ErrorCode::LimitExceeded);
    let pairs = Document::parse_with(
        source,
        &Limits {
            max_kerning_pairs: 0,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(pairs.code, ErrorCode::LimitExceeded);
}

#[test]
fn widgets_are_sprites_and_export_metadata() {
    let doc=Document::parse("palette p\n  . transparent\n  k #000000\n  w #ffffff\nsprite btn 3x3 slice=1\n  kkk\n  kwk\n  kkk\nstates button\n  normal btn\n  hover btn:flip_x\nicons items 3x3 outline=k\n  icon one\n    kkk\n    kwk\n    kkk\n  icon two\n    kkk\n    kkk\n    kkk\nsprite copy = button.hover\ngroup selected button.normal items.one\n").unwrap();
    assert_eq!(
        doc.state_set("button").unwrap().state("hover"),
        Some("button.hover")
    );
    assert_eq!(
        doc.icon_set("items").unwrap().members(),
        ["items.one", "items.two"]
    );
    assert!(doc.sprite("copy").is_some());
    let json = doc.atlas(&Default::default()).unwrap().to_json("atlas.png");
    assert!(json.contains("\"states\""));
    assert!(json.contains("\"icon_sets\""));
    assert!(doc.preview(&Default::default()).is_ok());
}

#[test]
fn widget_errors_and_generated_definition_bounds() {
    let prefix = "palette p\n  . transparent\n  k #000000\nsprite a 1x1\n  k\nsprite b 2x1\n  kk\n";
    for (body, code) in [
        ("states s\n  hover a\n", ErrorCode::InvalidArgument),
        (
            "states s\n  normal a\n  normal a\n",
            ErrorCode::DuplicateName,
        ),
        ("states s\n  normal a\n  hover b\n", ErrorCode::SizeMismatch),
        (
            "icons i 1x1 margin=0 outline=k\n  icon one\n    .\n",
            ErrorCode::InvalidArgument,
        ),
        (
            "icons i 3x3 margin=1\n  icon one\n    k..\n    ...\n    ...\n",
            ErrorCode::InvalidArgument,
        ),
    ] {
        let error = Document::parse(&format!("{prefix}{body}")).unwrap_err();
        assert_eq!(error.code, code, "{body}: {error}");
        assert!(error.position.is_some());
    }
    let limited = Document::parse_with(
        &format!("{prefix}states s\n  normal a\n  hover a\n"),
        &Limits {
            max_definitions: 5,
            ..Limits::default()
        },
    )
    .unwrap_err();
    assert_eq!(limited.code, ErrorCode::LimitExceeded);
}

#[test]
fn themes_recolor_without_repacking_or_accumulating() {
    let doc=Document::parse("palette ui\n  a #ffffff\n  b #ffffff\nsprite a 1x1\n  a\nsprite b 1x1\n  b\ntheme dark of ui\n  b #000000\ntheme light of ui\n  a #ff0000\n").unwrap();
    let options = Default::default();
    let base = doc.atlas(&options).unwrap();
    let dark = doc.with_theme("dark").unwrap();
    let atlas = dark.atlas(&options).unwrap();
    assert_eq!(base.regions, atlas.regions);
    assert_ne!(base.region("a"), base.region("b"));
    assert_ne!(base.image, atlas.image);
    assert_eq!(
        dark.with_theme("light").unwrap(),
        doc.with_theme("light").unwrap()
    );
    let all = doc.export_all_themes(&Default::default()).unwrap();
    let files = all.files();
    assert!(files.iter().any(|(n, _)| n == "atlas-dark.png"));
    assert!(all.to_json().contains("\"atlas\": \"atlas-dark.png\""));
    assert_eq!(
        doc.with_theme("missing").unwrap_err().code,
        ErrorCode::UnknownName
    );
    let invalid =
        Document::parse("palette p\n  a #ffffff\ntheme dark of p\n  z #000000\n").unwrap_err();
    assert_eq!(invalid.code, ErrorCode::UnknownName);
}

#[test]
fn screen_composition_clipping_and_integer_scale_pngs() {
    let source = "palette p\n  k #000000\n  w #ffffff\n  s #ffffff80\nsprite pair 2x1\n  ws\nscreen view 2x2 background=k\n  pair at -1,0\n  pair at 0,1\n";
    let doc = Document::parse(source).unwrap();
    let image = doc.screen("view").unwrap();
    let expected = spritebox::Image::from_pixels(
        2,
        2,
        vec![
            128, 128, 128, 255, 0, 0, 0, 255, 255, 255, 255, 255, 128, 128, 128, 255,
        ],
    )
    .unwrap();
    assert_eq!(image.to_png(), expected.to_png());
    let scaled = image.scaled_with(3, &Limits::default()).unwrap();
    let mut bytes = Vec::new();
    for row in [[128u8, 0], [255, 128]] {
        for _ in 0..3 {
            for value in row {
                for _ in 0..3 {
                    bytes.extend_from_slice(&[value, value, value, 255]);
                }
            }
        }
    }
    assert_eq!(
        scaled.to_png(),
        spritebox::Image::from_pixels(6, 6, bytes).unwrap().to_png()
    );
    assert!(doc.screen("absent").is_none());
    let atlas = doc.atlas(&Default::default()).unwrap();
    assert_eq!(atlas.regions.len(), 1);
}

#[test]
fn screen_lint_checks_and_themed_text_contrast() {
    use spritebox::{LintCode, LintOptions};
    let source = "palette p\n  k #000000\n  w #ffffff\nfont f height=1 spacing=0\n  glyph A\n    #\ntext label font=f color=w\n  A\ntheme dim of p\n  w #111111\nscreen clean 4x4 background=k grid=2\n  label at 0,0\nscreen dirty 4x4 background=k grid=2\n  label at -1,0\n  label at 1,1\n  label at 1,1\n";
    let doc = Document::parse(source).unwrap();
    let lint = doc.lint(&Default::default());
    assert_eq!(lint.count(LintCode::ScreenBounds), 1);
    assert_eq!(lint.count(LintCode::OffGrid), 3);
    assert_eq!(lint.count(LintCode::ScreenOverlap), 0);
    assert_eq!(
        doc.lint(&LintOptions {
            screen_overlap: true,
            ..Default::default()
        })
        .count(LintCode::ScreenOverlap),
        1
    );
    assert!(
        doc.with_theme("dim")
            .unwrap()
            .lint(&Default::default())
            .count(LintCode::LowContrast)
            > lint.count(LintCode::LowContrast)
    );
    let clean = Document::parse(source.split("screen dirty").next().unwrap()).unwrap();
    assert_eq!(
        clean
            .lint(&LintOptions {
                screen_overlap: true,
                ..Default::default()
            })
            .count(LintCode::ScreenBounds),
        0
    );
    assert_eq!(clean.lint(&Default::default()).count(LintCode::OffGrid), 0);
    assert_eq!(
        clean.lint(&Default::default()).count(LintCode::LowContrast),
        0
    );
    assert_eq!(
        clean
            .lint(&LintOptions {
                screen_overlap: true,
                ..Default::default()
            })
            .count(LintCode::ScreenOverlap),
        0
    );
}

#[test]
fn screens_and_themes_reject_bad_inputs_and_budgets() {
    let source = "palette p\n  k #000000\nsprite s 1x1\n  k\nscreen view 4x4\n  s at 0,0\n";
    assert_eq!(
        Document::parse_with(
            source,
            &Limits {
                max_screen_items: 0,
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
    assert_eq!(
        Document::parse(&source.replace("s at 0,0", "missing at 0,0"))
            .unwrap_err()
            .code,
        ErrorCode::UnknownName
    );
    assert_eq!(
        Document::parse("palette p\n  k #000000\ntheme t of p\n  k #ffffff\n  w #ffffff\n")
            .unwrap_err()
            .code,
        ErrorCode::UnknownName
    );
    assert_eq!(
        Document::parse_with(
            "palette p\n  k #000000\ntheme t of p\n  k #ffffff\n  w #ffffff\n",
            &Limits {
                max_palette_colors: 1,
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
    let doc = Document::parse(&format!("{source}theme t of p\n  k #ffffff\n")).unwrap();
    assert_eq!(
        doc.export_all_themes_with(
            &Default::default(),
            &Limits {
                max_image_pixels: 10,
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
    assert_eq!(
        doc.export_all_themes_with(
            &Default::default(),
            &Limits {
                max_export_files: 2,
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
}

#[test]
fn layered_text_uses_its_own_panel_for_contrast() {
    let source = "palette p\n  k #000000\n  w #ffffff\nfont f height=1 spacing=0\n  glyph A\n    #\ntext label font=f color=w\n  A\nsprite panel 1x1\n  k\nsprite combined 1x1\n  layer panel = panel\n  layer label = label\nscreen view 1x1 background=w\n  combined at 0,0\n";
    let clean = Document::parse(source).unwrap();
    assert_eq!(
        clean
            .lint(&Default::default())
            .count(spritebox::LintCode::LowContrast),
        0
    );
    let bad = Document::parse(&source.replace("font=f color=w", "font=f color=k")).unwrap();
    assert_eq!(
        bad.lint(&Default::default())
            .count(spritebox::LintCode::LowContrast),
        1
    );
}

#[test]
fn chains_enforce_limits_before_growing_layers() {
    let chain = format!("s{}", ":outline(k)".repeat(16));
    let source =
        format!("palette p\n  k #000000\nsprite s 1x1\n  k\nscreen view 2x2\n  {chain} at 0,0\n");
    assert_eq!(
        Document::parse(&source).unwrap_err().code,
        ErrorCode::LimitExceeded
    );
}

#[test]
fn complete_ui_example_is_clean_in_both_themes() {
    let mut loader = |name: &str| {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join(name),
        )
        .map_err(spritebox::Error::from)
    };
    let doc = Document::parse_with_loader(
        include_str!("../examples/ui.spritebox"),
        &Limits::default(),
        &mut loader,
    )
    .unwrap();
    assert_eq!(doc.screens().len(), 2);
    assert_eq!(doc.icon_set("items").unwrap().members().len(), 6);
    assert!(doc.lint(&Default::default()).is_clean());
    let dark = doc.with_theme("dark").unwrap();
    assert!(dark.lint(&Default::default()).is_clean());
    assert_eq!(
        doc.atlas(&Default::default()).unwrap().regions,
        dark.atlas(&Default::default()).unwrap().regions
    );
    assert_ne!(doc.screen("title").unwrap(), dark.screen("title").unwrap());
}
