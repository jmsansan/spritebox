//! Imports through a caller's loader, with no file system.

use std::collections::HashMap;

use spritebox::{AtlasOptions, Color, Document, Error, ErrorCode, Limits, check_with_loader};

/// A loader over files held in memory, recording what it was asked for.
fn files(list: &[(&str, &str)]) -> (HashMap<String, String>, Vec<String>) {
    (
        list.iter()
            .map(|(n, t)| (n.to_string(), t.to_string()))
            .collect(),
        Vec::new(),
    )
}

fn load<'f>(
    files: &'f HashMap<String, String>,
    asked: &'f mut Vec<String>,
) -> impl FnMut(&str) -> Result<String, Error> + 'f {
    move |name: &str| {
        asked.push(name.to_owned());
        files
            .get(name)
            .cloned()
            .ok_or_else(|| Error::new(ErrorCode::Io, format!("no file {name}")))
    }
}

#[test]
fn renders_with_a_palette_from_another_file() {
    let (files, mut asked) = files(&[(
        "shared/palettes.spritebox",
        "palette dawn\n  .  transparent\n  k  #1a1c2c\n  o  #f4b41b\n",
    )]);
    let text = "import shared/palettes.spritebox\n\nsprite dot 3x1 palette=dawn\n  ko.\n";
    let doc = Document::parse_with_loader(text, &Limits::default(), &mut load(&files, &mut asked))
        .unwrap();
    let image = doc.sprite("dot").unwrap().render();
    assert_eq!(image.pixel(1, 0), Some(Color::rgb(0xf4, 0xb4, 0x1b)));
    let atlas = doc.atlas(&AtlasOptions::default()).unwrap();
    assert!(atlas.region("dot").is_some());
    assert_eq!(asked, ["shared/palettes.spritebox"]);
}

#[test]
fn imports_nest_and_load_once() {
    let (files, mut asked) = files(&[
        (
            "art/hero.spritebox",
            "import ../palettes.spritebox\nimport ./parts/eyes.spritebox\nsprite hero = eyes:pad(1)\n",
        ),
        (
            "art/parts/eyes.spritebox",
            "import ../../palettes.spritebox\nsprite eyes 2x1 palette=p\n  kk\n",
        ),
        ("palettes.spritebox", "palette p\n  k #000000\n"),
    ]);
    let text = "import art/hero.spritebox\nimport palettes.spritebox\nanimation idle\n  hero 6\n";
    let doc = Document::parse_with_loader(text, &Limits::default(), &mut load(&files, &mut asked))
        .unwrap();
    // Imported definitions take the place of their `import`, each file once.
    let names: Vec<&str> = doc.sprites().iter().map(|s| s.name()).collect();
    assert_eq!(names, ["eyes", "hero"]);
    assert_eq!(doc.palettes().len(), 1);
    assert_eq!(
        asked,
        [
            "art/hero.spritebox",
            "palettes.spritebox",
            "art/parts/eyes.spritebox"
        ]
    );
    assert_eq!(doc.sprite("hero").unwrap().width(), 4);
}

fn diagnostics(errors: &[Error]) -> Vec<(Option<&str>, usize, usize, ErrorCode)> {
    errors
        .iter()
        .map(|e| {
            let p = e.position.unwrap();
            (e.file.as_deref(), p.line, p.column, e.code)
        })
        .collect()
}

#[test]
fn errors_name_their_file() {
    let (files, mut asked) = files(&[
        ("a.spritebox", "palette p\n  k #00\n"),
        ("loop1.spritebox", "import loop2.spritebox\n"),
        ("loop2.spritebox", "import loop1.spritebox\n"),
    ]);
    let mut loader = load(&files, &mut asked);
    let text = "import a.spritebox\nimport missing.spritebox\nimport loop1.spritebox\nimport\nimport a b\nsprite s 1x1 palette=p\n  x\n";
    let errors = check_with_loader(text, &Limits::default(), &mut loader);
    assert_eq!(
        diagnostics(&errors),
        [
            (None, 2, 8, ErrorCode::Io),
            (None, 4, 7, ErrorCode::Syntax),
            (None, 5, 10, ErrorCode::Syntax),
            (None, 7, 3, ErrorCode::UnknownName),
            (Some("a.spritebox"), 2, 5, ErrorCode::InvalidArgument),
            (Some("loop2.spritebox"), 1, 8, ErrorCode::Cycle),
        ]
    );
    assert!(
        errors[0].message.contains("no file missing.spritebox"),
        "{}",
        errors[0].message
    );
    assert_eq!(
        errors[4].to_string(),
        "a.spritebox, line 2, column 5: Color `#00` must be `#rrggbb`, `#rrggbbaa` or `transparent`"
    );
}

#[test]
fn imports_need_a_loader() {
    let errors = spritebox::check("import a.spritebox\n", &Limits::default());
    assert_eq!(
        diagnostics(&errors),
        [(None, 1, 8, ErrorCode::InvalidArgument)]
    );
    assert!(errors[0].message.contains("loader"));
}

#[test]
fn imports_are_bounded() {
    let (files, mut asked) = files(&[
        ("1", "import 2\n"),
        ("2", "import 3\n"),
        ("3", "palette p\n"),
        ("big", "palette big\n"),
    ]);
    let limits = Limits {
        max_nesting: 2,
        ..Limits::default()
    };
    let errors = check_with_loader("import 1\n", &limits, &mut load(&files, &mut asked));
    assert_eq!(
        diagnostics(&errors),
        [(Some("2"), 1, 8, ErrorCode::LimitExceeded)]
    );
    let limits = Limits {
        max_text_bytes: 15,
        ..Limits::default()
    };
    let errors = check_with_loader("import big\n", &limits, &mut load(&files, &mut asked));
    assert_eq!(
        diagnostics(&errors),
        [(None, 1, 8, ErrorCode::LimitExceeded)]
    );
    let limits = Limits {
        max_definitions: 1,
        ..Limits::default()
    };
    let errors = check_with_loader(
        "import 3\npalette q\n",
        &limits,
        &mut load(&files, &mut asked),
    );
    assert_eq!(
        diagnostics(&errors),
        [(None, 2, 1, ErrorCode::LimitExceeded)]
    );
}
