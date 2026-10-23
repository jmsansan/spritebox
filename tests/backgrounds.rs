use spritebox::*;

const PALETTE: &str = "palette p\n  a #ff0000\n  b #00ff0080\n  c #0000ff\n  . transparent\n";
fn parse(body: &str) -> Document {
    Document::parse(&format!("{PALETTE}{body}")).unwrap()
}
fn keys(s: &Surface) -> Vec<String> {
    (0..s.size().1)
        .map(|y| {
            (0..s.size().0)
                .map(|x| s.key(x, y).unwrap_or('_'))
                .collect()
        })
        .collect()
}

#[test]
fn primitives_are_keys_and_overwrite_transparency() {
    let d = parse(
        "surface s 5x4\n  rect base at=-1,-1 size=4x4 key=a\n  ellipse cut at=1,0 size=3x3 key=.\n  line slash from=-5,3 to=9,3 key=c\n",
    );
    assert_eq!(
        keys(d.surface("s").unwrap()),
        ["a..._", "a..._", "a..._", "ccccc"]
    );
    assert!(d.sprites().is_empty());
    assert!(d.atlas(&Default::default()).unwrap().regions.is_empty());
    let themed = parse(
        "surface s 2x1\n  rect base at=0,0 size=2x1 key=a\n  rect cut at=0,0 size=1x1 key=.\ntheme solid of p\n  . #ffffff\n",
    );
    assert_eq!(
        themed.surface("s").unwrap().render().unwrap().pixel(0, 0),
        Some(Color::TRANSPARENT)
    );
    let solid = themed.with_theme("solid").unwrap();
    assert_eq!(
        keys(themed.surface("s").unwrap()),
        keys(solid.surface("s").unwrap())
    );
    assert_eq!(
        solid.surface("s").unwrap().render().unwrap().pixel(0, 0),
        Some(Color::WHITE)
    );
}

#[test]
fn lines_preserve_phase_reversal_and_extreme_clipping() {
    for &(a, b) in &[
        ((-8, -2), (11, 4)),
        ((0, 6), (3, -3)),
        ((4, 0), (-3, 5)),
        ((i32::MIN, 2), (i32::MAX, 2)),
    ] {
        let make = |a: (i32, i32), b: (i32, i32)| {
            parse(&format!(
                "surface s 5x5\n  line l from={},{} to={},{} key=a\n",
                a.0, a.1, b.0, b.1
            ))
        };
        assert_eq!(
            keys(make(a, b).surface("s").unwrap()),
            keys(make(b, a).surface("s").unwrap())
        );
    }
    let big = parse("surface s 8x8\n  line l from=0,0 to=7,4 key=a\n");
    let clipped = parse("surface s 4x4\n  line l from=-2,-1 to=5,3 key=a\n");
    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(
                big.surface("s").unwrap().key(x + 2, y + 1),
                clipped.surface("s").unwrap().key(x, y)
            );
        }
    }
}

#[test]
fn polygons_winding_shared_edges_and_validation() {
    let a = parse("surface s 4x3\n  polygon p key=a points=0,0 4,0 4,3 0,3\n");
    let b = parse("surface s 4x3\n  polygon p key=a points=0,3 4,3 4,0 0,0\n");
    assert_eq!(keys(a.surface("s").unwrap()), ["aaaa", "aaaa", "aaaa"]);
    assert_eq!(keys(a.surface("s").unwrap()), keys(b.surface("s").unwrap()));
    for points in [
        "0,0 3,3 0,3 3,0",
        "0,0 3,0 1,0 1,2",
        "0,0 1,1 2,2",
        "0,0 3,0 3,3 0,0",
    ] {
        assert_eq!(
            Document::parse(&format!(
                "{PALETTE}surface s 4x4\n  polygon p key=a points={points}\n"
            ))
            .unwrap_err()
            .code,
            ErrorCode::InvalidArgument
        );
    }
}

#[test]
fn gradients_obey_stops_and_existing_dither() {
    for dither in [
        Dither::None,
        Dither::Bayer(2),
        Dither::Bayer(4),
        Dither::Bayer(8),
    ] {
        let doc = parse("");
        let mut b =
            Surface::builder("s", 7, 3, doc.palette("p").unwrap(), 0, &Limits::default()).unwrap();
        b.gradient(
            "g",
            &Gradient {
                axis: Axis::X,
                mode: GradientMode::Blend,
                dither,
                stops: vec![
                    GradientStop {
                        position: 1,
                        key: 'a',
                    },
                    GradientStop {
                        position: 5,
                        key: 'c',
                    },
                ],
            },
        )
        .unwrap();
        let s = b.finish();
        for y in 0..3 {
            for x in 0..7 {
                let k = if x <= 1 {
                    'a'
                } else if x >= 5 {
                    'c'
                } else {
                    ['a', 'c'][dither.pick(((x - 1) * 65535 / 4) as u16, x, y, 2)]
                };
                assert_eq!(s.key(x, y), Some(k));
            }
        }
    }
    let d = parse("surface s 1x4\n  gradient g axis=y mode=bands\n    stop 0 a\n    stop 2 c\n");
    assert_eq!(keys(d.surface("s").unwrap()), ["a", "a", "c", "c"]);
}

#[test]
fn profiles_seeds_and_themes_do_not_change_authored_geometry() {
    let body = "surface s 16x8 seed=9\n  profile ridge key=a baseline=8 amplitude=6 cell=8 octaves=3 roughness=60 seed=3\n  rect paint at=0,0 size=16x8 key=b\ntheme night of p\n  a transparent\n  b #ffffff\n";
    let d = parse(body);
    let n = d.with_theme("night").unwrap();
    let heights = d.surface("s").unwrap().profile("ridge").unwrap().heights();
    assert_eq!(
        heights,
        n.surface("s").unwrap().profile("ridge").unwrap().heights()
    );
    let reordered = parse(&format!("surface unrelated 1x1\n{body}"));
    assert_eq!(
        heights,
        reordered
            .surface("s")
            .unwrap()
            .profile("ridge")
            .unwrap()
            .heights()
    );
    let noise = Noise {
        width: 16,
        height: 8,
        cell: 8,
        octaves: 3,
        roughness: 60,
        seed: scene_seed(&["profile", "s", "ridge"], &[9, 3]),
    };
    for (x, &height) in heights.iter().enumerate() {
        assert_eq!(
            height,
            8 - (6 * noise.value(x as i64, 0) as u64 / 65535) as u32
        );
    }
    assert_ne!(
        scene_seed(&["profile", "s", "ridge"], &[9, 3]),
        scene_seed(&["profile", "s", "ridge2"], &[9, 3])
    );
}

#[test]
fn layers_crop_anchor_clip_mix_palettes_and_alpha_grouping() {
    let d = parse(
        "palette q\n  k #ffffff80\nsprite a 2x2 palette=p\n  aa\n  aa\nsprite b 2x2 palette=q\n  kk\n  kk\nbackground bg 4x3\n  layer back 4x3\n    place sprite a at=1,2 anchor=bottom-left crop=0,0,1,2\n  layer front 2x2 at=1,0 clip=0,0,1,2\n    place sprite b at=1,1 anchor=center\n    place sprite b at=0,0\n",
    );
    let bg = d.background("bg").unwrap();
    let flat = bg.render(&d, &Default::default()).unwrap();
    let white = Color::rgba(255, 255, 255, 128);
    let native = white.over(white.over(Color::TRANSPARENT));
    assert_eq!(flat.pixel(1, 0), Some(native.over(Color::rgb(255, 0, 0))));
    assert_eq!(flat.pixel(2, 0), Some(Color::TRANSPARENT));
    assert_eq!(flat.pixel(0, 2), Some(Color::TRANSPARENT));
}

#[test]
fn repeat_and_floor_parallax_cover_negative_seams() {
    let d = parse(
        "surface s 3x1\n  rect a at=0,0 size=1x1 key=a\n  rect b at=1,0 size=1x1 key=b\n  rect c at=2,0 size=1x1 key=c\nbackground bg 5x1\n  layer l 3x1 at=-1,0 repeat=x parallax=1/4,0/2\n    place surface s at=0,0\n",
    );
    let bg = d.background("bg").unwrap();
    assert_eq!(
        bg.layers()[0].options().parallax.1,
        Ratio::new(0, 1).unwrap()
    );
    for camera in [i32::MIN, -5, -4, -1, 0, 1, 4, 5, i32::MAX] {
        let image = bg
            .render(
                &d,
                &BackgroundRenderOptions {
                    camera: (camera, 0),
                },
            )
            .unwrap();
        for x in 0..5 {
            let u = (x as i64 + 1 + (camera as i64).div_euclid(4)).rem_euclid(3) as u32;
            assert_eq!(
                image.pixel(x, 0),
                d.surface("s").unwrap().render().unwrap().pixel(u, 0)
            );
        }
    }
}

#[test]
fn scatter_is_scoped_masked_and_wrap_spaced() {
    // baseline at the bottom is valid, but this profile paints no mask pixels.
    let invalid=Document::parse(&format!("{PALETTE}sprite tree 1x1\n  a\nsurface ground 8x8\n  profile ridge key=c baseline=8 amplitude=0 cell=8\nbackground bg 8x8\n  layer trees 8x8\n    scatter grove count=1 attempts=8 mask=ground(c) baseline=ground(ridge)\n      variant tree\n")).unwrap_err();
    assert_eq!(invalid.code, ErrorCode::InvalidArgument);
    assert!(invalid.message.contains("accepted 0"));
    // Paint a final raw mask while preserving the original bottom-boundary heights.
    let src = format!(
        "{PALETTE}sprite tree 1x1\n  a\nsurface ground 16x8\n  profile ridge key=c baseline=8 amplitude=0 cell=8\n  rect mask at=0,7 size=16x1 key=c\nbackground bg 16x8 seed=11\n  layer trees 16x8 repeat=x\n    scatter grove count=4 spacing=2 attempts=512 seed=2 baseline=ground(ridge) mask=ground(c)\n      variant tree weight=3\n      variant tree:flip_x weight=1\ntheme night of p\n  c transparent\n"
    );
    let d = Document::parse(&src).unwrap();
    let other = Document::parse(&format!("surface irrelevant 1x1 palette=p\n{src}")).unwrap();
    let n = d.with_theme("night").unwrap();
    let placements = d.background("bg").unwrap().layers()[0].placements();
    let geometry = |d: &Document| {
        d.background("bg").unwrap().layers()[0]
            .placements()
            .iter()
            .map(|p| (p.source().clone(), p.options()))
            .collect::<Vec<_>>()
    };
    assert_eq!(geometry(&d), geometry(&other));
    assert_eq!(
        placements,
        n.background("bg").unwrap().layers()[0].placements()
    );
    for (i, a) in placements.iter().enumerate() {
        assert_eq!(a.options().at.1, 8);
        for b in &placements[..i] {
            let dx = (a.options().at.0 - b.options().at.0).unsigned_abs();
            assert!(dx.min(16 - dx) >= 2);
        }
    }
}

#[test]
fn native_exports_reconstruct_flat_pixels_independently() {
    let d = parse(
        "surface s 3x2\n  rect fill at=0,0 size=3x2 key=b\nbackground A 5x4\n  layer far 3x2 at=-1,1 repeat=both parallax=-1/3,1/2\n    place surface s at=0,0\n  layer near 3x2 at=1,0 repeat=x parallax=1/1,0/1\n    place surface s at=0,0\nbackground a 1x1\n",
    );
    for camera in [(-9, -1), (0, 0), (7, 3)] {
        let options = ExportOptions {
            backgrounds: BackgroundExportOptions {
                camera,
                ..Default::default()
            },
            ..Default::default()
        };
        let export = d.export(&options).unwrap();
        let bg = &export.backgrounds[0];
        let mut restored = Image::new(5, 4).unwrap();
        for l in &bg.layers {
            for y in 0..4 {
                for x in 0..5 {
                    let (rx, ry) = l.options.parallax;
                    let u = x as i64 - l.options.origin.0 as i64
                        + (camera.0 as i64 * rx.numerator() as i64)
                            .div_euclid(rx.denominator() as i64);
                    let v = y as i64 - l.options.origin.1 as i64
                        + (camera.1 as i64 * ry.numerator() as i64)
                            .div_euclid(ry.denominator() as i64);
                    let wrap = |v: i64, n: u32, r: bool| {
                        if r {
                            Some(v.rem_euclid(n as i64) as u32)
                        } else {
                            (v >= 0 && v < n as i64).then_some(v as u32)
                        }
                    };
                    let u = wrap(
                        u,
                        l.image.width(),
                        matches!(l.options.repeat, Repeat::X | Repeat::Both),
                    );
                    let v = wrap(
                        v,
                        l.image.height(),
                        matches!(l.options.repeat, Repeat::Y | Repeat::Both),
                    );
                    if let (Some(u), Some(v)) = (u, v) {
                        restored.set_pixel(
                            x,
                            y,
                            l.image
                                .pixel(u, v)
                                .unwrap()
                                .over(restored.pixel(x, y).unwrap()),
                        );
                    }
                }
            }
        }
        assert_eq!(restored, bg.flat.as_ref().unwrap().image);
        let json = export.to_json();
        assert!(json.contains("\"schema\": 1"));
        assert!(json.contains("background-0000-layer-0000.png"));
        let selected = d
            .export(&ExportOptions {
                backgrounds: BackgroundExportOptions {
                    selection: Some("a".into()),
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            selected.backgrounds[0].flat.as_ref().unwrap().file,
            "background-0001.png"
        );
    }
}

#[test]
fn scene_limits_fail_before_growth_and_generation_work() {
    let source = format!(
        "{PALETTE}sprite p 3x3 slice=1\n  aaa\n  aaa\n  aaa\nbackground bg 1x1\n  layer l 1x1\n    place sprite p:size(1024x1024):flip_x:flip_y at=0,0\n"
    );
    for limits in [
        Limits {
            max_scene_work: 1,
            ..Default::default()
        },
        Limits {
            max_scene_pixels: 0,
            ..Default::default()
        },
        Limits {
            max_scene_commands: 1,
            ..Default::default()
        },
    ] {
        let e = Document::parse_with(&source, &limits).unwrap_err();
        assert_eq!(e.code, ErrorCode::LimitExceeded);
        assert!(e.position.is_some());
    }
    let d = parse("surface s 8x8\nbackground bg 8x8\n  layer l 8x8\n    place surface s at=0,0\n");
    assert_eq!(
        d.background("bg")
            .unwrap()
            .render_with(
                &d,
                &Default::default(),
                &Limits {
                    max_scene_pixels: 128,
                    ..Default::default()
                }
            )
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
}

#[test]
fn source_diagnostics_keep_imported_unicode_stop_positions() {
    let unknown = check("surface s 1x1 palette=missing\n", &Default::default());
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].code, ErrorCode::UnknownName);
    assert_eq!(unknown[0].position.unwrap().column, 23);
    let mut loader = |_: &str| {
        Ok("surface s 1x1 palette=p\n  gradient g axis=x mode=bands\n    stop 0 Ω\n".into())
    };
    let errors = check_with_loader(
        &format!("{PALETTE}import colors.spritebox\n"),
        &Limits::default(),
        &mut loader,
    );
    assert_eq!(errors.len(), 1);
    let error = &errors[0];
    assert_eq!(error.code, ErrorCode::UnknownName);
    assert_eq!(error.file.as_deref(), Some("colors.spritebox"));
    assert_eq!(
        (error.position.unwrap().line, error.position.unwrap().column),
        (3, 12)
    );
    for line in ["    stop 2 a", "    stop 0 a\n    stop 0 c"] {
        let e = Document::parse(&format!(
            "{PALETTE}surface s 2x1\n  gradient g axis=x mode=bands\n{line}\n"
        ))
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidArgument);
        assert_eq!(e.position.unwrap().column, 10);
    }
}

#[test]
fn scene_lint_tracks_generator_and_mask_keys_and_clipped_placements() {
    let d = parse(
        "surface s 2x2\n  rect r at=0,0 size=2x2 key=a\nbackground before 2x2\n  layer l 2x2\n    place surface s at=5,5\nbackground after 2x2 lint=allow(background_clipped)\n  layer l 2x2\n    place surface s at=5,5\n",
    );
    let lint = d.lint(&Default::default());
    assert_eq!(lint.count(LintCode::BackgroundClipped), 1);
    assert!(
        !lint
            .warnings
            .iter()
            .any(|w| w.code == LintCode::UnusedKey && w.message.contains("`a`"))
    );
    let limited = d.lint_with(
        &Default::default(),
        &Limits {
            max_warnings: 0,
            ..Default::default()
        },
    );
    assert!(limited.warnings.is_empty());
    assert_eq!(limited.count(LintCode::BackgroundClipped), 1);
}

#[test]
fn complete_background_examples_render_and_theme_geometry_stays_fixed() {
    for source in [
        include_str!("../examples/backgrounds.spritebox"),
        include_str!("../examples/night-shift-background.spritebox"),
    ] {
        let d = Document::parse(source).unwrap();
        for bg in d.backgrounds() {
            let image = bg.render(&d, &Default::default()).unwrap();
            assert_eq!((image.width(), image.height()), bg.size());
        }
        let export = d.export_all_themes(&Default::default()).unwrap();
        assert!(!export.backgrounds.is_empty());
        assert!(
            export
                .files()
                .iter()
                .any(|(n, _)| n == "background-0000.png")
        );
        let preview = d.preview(&Default::default()).unwrap();
        assert!(preview.width() > 0);
        for theme in d.themes() {
            let t = d.with_theme(theme.name()).unwrap();
            for (a, b) in d.surfaces().iter().zip(t.surfaces()) {
                assert_eq!(keys(a), keys(b));
            }
            assert_eq!(
                d.backgrounds()[0].layers()[0].placements(),
                t.backgrounds()[0].layers()[0].placements()
            );
        }
    }
}

#[test]
fn typed_background_builder_uses_the_same_compositor() {
    let d = parse(
        "sprite dot 1x1\n  a\nbackground text 3x2\n  layer l 2x2 at=1,0 repeat=x parallax=1/4,0/1\n    place sprite dot at=0,0\n",
    );
    let mut b = Background::builder(&d, "typed", (3, 2), 0, &Default::default()).unwrap();
    b.layer(
        "l",
        (2, 2),
        LayerOptions {
            origin: (1, 0),
            repeat: Repeat::X,
            parallax: (Ratio::new(1, 4).unwrap(), Ratio::new(0, 1).unwrap()),
            ..Default::default()
        },
    )
    .unwrap()
    .place(SourceRef::Sprite("dot".into()), PlacementOptions::default())
    .unwrap();
    let typed = b.finish();
    for camera in [(-1, 0), (0, 0), (7, 0)] {
        let o = BackgroundRenderOptions { camera };
        assert_eq!(
            typed.render(&d, &o).unwrap(),
            d.background("text").unwrap().render(&d, &o).unwrap()
        );
    }
    assert!(
        d.with_background(typed, &Default::default())
            .unwrap()
            .background("typed")
            .is_some()
    );
}

#[test]
fn reviewed_temporary_storage_insertion_and_clone_budget_regressions() {
    let src = format!(
        "{PALETTE}sprite dot 1x1\n  a\nbackground bg 1x1\n  layer l 1x1\n    place sprite dot:outline(c) at=0,0\n"
    );
    assert_eq!(
        Document::parse_with(
            &src,
            &Limits {
                max_scene_pixels: 3,
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
    assert!(
        Document::parse_with(
            &src,
            &Limits {
                max_scene_pixels: 4,
                ..Default::default()
            }
        )
        .is_ok()
    );
    let d = parse(
        "sprite dot 1x1\n  a\nbackground bg 1x1\n  layer l 1x1\n    place sprite dot at=0,0\n",
    );
    let cloned = d.background("bg").unwrap().clone();
    assert!(
        cloned
            .render_with(
                &d,
                &Default::default(),
                &Limits {
                    max_scene_pixels: 5,
                    ..Default::default()
                }
            )
            .is_ok()
    );
    let with_surface = parse(
        "surface s 1x1\nbackground bg 1x1\n  layer one 1x1\n    place surface s at=0,0\n  layer two 1x1\n",
    );
    let receiver = parse("");
    assert_eq!(
        receiver
            .with_background(
                with_surface.background("bg").unwrap().clone(),
                &Default::default()
            )
            .unwrap_err()
            .code,
        ErrorCode::UnknownName
    );
    assert_eq!(
        receiver
            .with_background(
                with_surface.background("bg").unwrap().clone(),
                &Limits {
                    max_layers: 1,
                    ..Default::default()
                }
            )
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
    assert_eq!(
        receiver
            .with_surface(
                with_surface.surface("s").unwrap().clone(),
                &Limits {
                    max_definitions: 1,
                    ..Default::default()
                }
            )
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
    let surface_only = parse("surface s 8x8\n");
    assert_eq!(
        surface_only
            .preview_with(
                &Default::default(),
                &Limits {
                    max_scene_pixels: 63,
                    ..Default::default()
                }
            )
            .unwrap_err()
            .code,
        ErrorCode::LimitExceeded
    );
}

#[test]
fn export_modes_themes_and_whole_manifest_preflight() {
    let d = parse(
        "surface s 1x1\n  rect r at=0,0 size=1x1 key=a\nbackground bg 1x1\n  layer l 1x1\n    place surface s at=0,0\ntheme night of p\n  a transparent\n",
    );
    for mode in [
        BackgroundMode::None,
        BackgroundMode::Flat,
        BackgroundMode::Layers,
        BackgroundMode::Both,
    ] {
        let export = d
            .export(&ExportOptions {
                backgrounds: BackgroundExportOptions {
                    mode,
                    ..Default::default()
                },
                ..Default::default()
            })
            .unwrap();
        let names = export.file_names();
        assert_eq!(
            names,
            export
                .files()
                .into_iter()
                .map(|(n, _)| n)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            names.iter().any(|n| n == "background-0000.png"),
            matches!(mode, BackgroundMode::Flat | BackgroundMode::Both)
        );
        assert_eq!(
            names.iter().any(|n| n == "background-0000-layer-0000.png"),
            matches!(mode, BackgroundMode::Layers | BackgroundMode::Both)
        );
        assert_eq!(
            export.to_json().contains("\"backgrounds\""),
            mode != BackgroundMode::None
        );
    }
    let e = d
        .export_all_themes_with(
            &Default::default(),
            &Limits {
                max_export_files: 5,
                max_scene_work: 0,
                ..Default::default()
            },
        )
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::LimitExceeded);
    assert!(e.message.contains("file limit"));
    let e = d.export_all_themes(&Default::default()).unwrap();
    assert_eq!(e.backgrounds[0].layers[0].image.width(), 1);
    assert_ne!(
        e.backgrounds[0].flat.as_ref().unwrap().image,
        e.themes[0].backgrounds[0].flat.as_ref().unwrap().image
    );
}

#[test]
fn pinned_background_contract_vectors_and_outputs() {
    let d = Document::parse(include_str!("fixtures/backgrounds/scene.spritebox")).unwrap();
    let export = d
        .export(&ExportOptions {
            backgrounds: BackgroundExportOptions {
                camera: (-3, 1),
                ..Default::default()
            },
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        export.to_json().replace("\r\n", "\n"),
        include_str!("fixtures/backgrounds/scene.json").replace("\r\n", "\n")
    );
    assert_eq!(
        png::encode(&export.backgrounds[0].flat.as_ref().unwrap().image),
        include_bytes!("fixtures/backgrounds/flat.png")
    );
    assert_eq!(
        keys(d.surface("sky").unwrap()).join("\n") + "\n",
        include_str!("fixtures/backgrounds/sky.keys").replace("\r\n", "\n")
    );
    assert_eq!(
        format!(
            "{:?}\n",
            d.surface("ground")
                .unwrap()
                .profile("ridge")
                .unwrap()
                .heights()
        ),
        include_str!("fixtures/backgrounds/profile.txt").replace("\r\n", "\n")
    );
    let placements = d.background("scene").unwrap().layers()[1]
        .placements()
        .iter()
        .map(|p| format!("{:?} {:?}", p.source(), p.options().at))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(
        placements,
        include_str!("fixtures/backgrounds/placements.txt").replace("\r\n", "\n")
    );
    assert_eq!(
        scene_seed(&["profile", "s", "ridge"], &[9, 3]),
        0x4b19d0a4f3e342ed
    );
    assert_eq!(
        scene_seed(&["scatter", "scene", "hills", "stars"], &[11, 4]),
        0xae69762aa276d3ae
    );
    assert_eq!(
        scene_seed(&["profile", "Ω", "ridge"], &[0, 0]),
        0x1b8b20325fd5a34c
    );
}

#[test]
fn typed_insertion_uses_receiver_colors_and_rejects_missing_keys() {
    let original = parse(
        "surface s 1x1\n  rect fill at=0,0 size=1x1 key=a\nsprite dot 1x1\n  a\nbackground bg 1x1\n  layer l 1x1\n    place sprite dot at=0,0\n",
    );
    let blue = Document::parse("palette p\n  a #0000ff\n").unwrap();
    let inserted = blue
        .with_surface(original.surface("s").unwrap().clone(), &Default::default())
        .unwrap();
    assert_eq!(
        inserted.surface("s").unwrap().render().unwrap().pixel(0, 0),
        Some(Color::rgb(0, 0, 255))
    );
    let missing = Document::parse("palette p\n  b #0000ff\n").unwrap();
    assert_eq!(
        missing
            .with_background(
                original.background("bg").unwrap().clone(),
                &Default::default()
            )
            .unwrap_err()
            .code,
        ErrorCode::UnknownName
    );
}

#[test]
fn density_zero_weights_attempt_caps_and_bounded_thumbnails() {
    let source = |density: &str| {
        format!(
            "{PALETTE}sprite a 1x1\n  a\nsprite b 1x1\n  c\nsurface mask 8x2\n  rect fill at=0,0 size=8x2 key=a\nbackground bg 8x2\n  layer l 8x2\n    scatter group density={density} spacing=0 attempts=128 mask=mask(a)\n      variant a weight=1\n      variant b weight=4294967295\n"
        )
    };
    let d = Document::parse(&source("1/2")).unwrap();
    let placements = d.background("bg").unwrap().layers()[0].placements();
    assert_eq!(placements.len(), 8);
    assert!(
        placements
            .iter()
            .all(|p| p.source() == &SourceRef::Sprite("b".into()))
    );
    let zero = Document::parse_with(
        &source("0/1"),
        &Limits {
            max_scatter_attempts: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        zero.background("bg").unwrap().layers()[0]
            .placements()
            .is_empty()
    );
    assert_eq!(
        Document::parse_with(
            &source("1/1"),
            &Limits {
                max_scatter_attempts: 1,
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        ErrorCode::LimitExceeded
    );
    for bad in ["1/0", "2/1"] {
        assert_eq!(
            Document::parse(&source(bad)).unwrap_err().code,
            ErrorCode::InvalidArgument
        );
    }
    let large = parse(
        "surface s 641x361\n  rect fill at=0,0 size=641x361 key=a\nbackground big 641x361\n  layer l 641x361\n    place surface s at=0,0\n",
    );
    let preview = |scale| {
        large
            .preview(&PreviewOptions {
                scale,
                ..Default::default()
            })
            .unwrap()
    };
    let small = preview(1);
    assert_eq!(small, preview(16));
    assert!(small.width() < 400 && small.height() < 250);
}

#[test]
fn pinned_mixed_screen_background_preview() {
    let doc = Document::parse(include_str!("fixtures/backgrounds/scene.spritebox")).unwrap();
    assert_eq!(
        png::encode(&doc.preview(&Default::default()).unwrap()),
        include_bytes!("fixtures/backgrounds/preview.png")
    );
}
