//! The command-line tool: exit statuses, diagnostics and written files.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn spritebox(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_spritebox"))
        .args(args)
        .output()
        .expect("the binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A fresh directory for one test.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("spritebox-cli-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, name: &str, text: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path.display().to_string()
}

const COIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/coin.spritebox");
const BAD: &str = "palette p\n  k #000\nsprite s 2x1 palette=p\n  kx\n";

#[test]
fn check_valid_file() {
    let out = spritebox(&["check", COIN]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).ends_with(": ok\n"), "{}", stdout(&out));
    let out = spritebox(&["check", COIN, "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let json = stdout(&out);
    assert!(json.starts_with("{\"version\":1,\"file\":"), "{json}");
    assert!(
        json.contains("\"ok\":true,\"diagnostics\":[],\"written\":[]}"),
        "{json}"
    );
}

#[test]
fn check_invalid_file() {
    let dir = temp_dir("invalid");
    let file = write(&dir, "bad.spritebox", BAD);
    let out = spritebox(&["check", &file]);
    assert_eq!(out.status.code(), Some(1));
    let text = stderr(&out);
    assert!(
        text.contains(&format!(
            "{file}:2:5: error[invalid_argument]: Color `#000`"
        )),
        "{text}"
    );
    assert!(
        text.contains(&format!("{file}:4:4: error[unknown_name]: Key `x`")),
        "{text}"
    );
    assert!(text.ends_with(": 2 errors\n"), "{text}");

    let out = spritebox(&["check", "--json", &file]);
    assert_eq!(out.status.code(), Some(1));
    let json = stdout(&out);
    assert!(json.contains("\"ok\":false"), "{json}");
    assert!(json.contains(
        "{\"code\":\"invalid_argument\",\"severity\":\"error\",\"line\":2,\"column\":5,\"message\":\"Color `#000` must be `#rrggbb`, `#rrggbbaa` or `transparent`\"}"
    ), "{json}");
    assert!(
        json.contains("{\"code\":\"unknown_name\",\"severity\":\"error\",\"line\":4,\"column\":4,"),
        "{json}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn check_reads_standard_input_and_rejects_bad_utf8() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_spritebox"))
        .args(["check", "-", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"palette p\n  k #000000\n  \xff #000000\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let json = stdout(&out);
    assert!(json.contains("\"file\":\"<stdin>\""), "{json}");
    assert!(json.contains("{\"code\":\"syntax\",\"severity\":\"error\",\"line\":3,\"column\":3,\"message\":\"The text is not valid UTF-8\"}"), "{json}");
}

#[test]
fn render_writes_files() {
    let dir = temp_dir("render");
    let out = spritebox(&[
        "render",
        COIN,
        "--out",
        dir.to_str().unwrap(),
        "--scale",
        "2",
        "--padding",
        "2",
        "--no-extrude",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let json = stdout(&out);
    for name in ["atlas.png", "spritebox.json", "preview.png"] {
        assert!(
            json.contains(&format!("{:?}", dir.join(name).display().to_string())),
            "{json}"
        );
    }
    let doc = spritebox::Document::parse(&std::fs::read_to_string(COIN).unwrap()).unwrap();
    let options = spritebox::ExportOptions {
        atlas: spritebox::AtlasOptions {
            padding: 2,
            extrude: false,
            ..Default::default()
        },
    };
    let export = doc.export(&options).unwrap();
    assert_eq!(
        spritebox::png::read_file(dir.join("atlas.png")).unwrap(),
        export.atlas.image
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("spritebox.json")).unwrap(),
        export.to_json()
    );
    let preview = doc
        .preview(&spritebox::PreviewOptions {
            scale: 2,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        spritebox::png::read_file(dir.join("preview.png")).unwrap(),
        preview
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn render_writes_tilesets() {
    let dir = temp_dir("render-tiles");
    let tiles = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/tiles.spritebox");
    let out = spritebox(&["render", tiles, "--out", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let doc = spritebox::Document::parse(&std::fs::read_to_string(tiles).unwrap()).unwrap();
    let export = doc.export(&spritebox::ExportOptions::default()).unwrap();
    assert_eq!(
        spritebox::png::read_file(dir.join("tileset-ground.png")).unwrap(),
        export.tilesets[0].image
    );
    let text = stdout(&out);
    let written: Vec<&str> = text
        .lines()
        .map(|l| l.rsplit(['/', '\\']).next().unwrap())
        .collect();
    assert_eq!(
        written,
        [
            "atlas.png",
            "tileset-ground.png",
            "spritebox.json",
            "preview.png"
        ]
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn font_writes_truetype() {
    let dir = temp_dir("font");
    let font = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/font.spritebox");
    let path = dir.join("nested/hud.ttf");
    let out = spritebox(&[
        "font",
        font,
        "--out",
        path.to_str().unwrap(),
        "--family",
        "HUD Pixel",
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let doc = spritebox::Document::parse(&std::fs::read_to_string(font).unwrap()).unwrap();
    let expected = doc
        .font("hud")
        .unwrap()
        .to_ttf(&spritebox::TtfOptions {
            family: Some("HUD Pixel".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), expected);
    assert!(stdout(&out).contains("hud.ttf"));

    // Several fonts need --font; an unknown font is bad usage too.
    let two = write(&dir, "two.spritebox", "font a height=1\nfont b height=1\n");
    let ttf = dir.join("x.ttf");
    let out = spritebox(&["font", &two, "--out", ttf.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stdout(&out).contains("choose one with --font"),
        "{}",
        stdout(&out)
    );
    let out = spritebox(&["font", &two, "--out", ttf.to_str().unwrap(), "--font", "c"]);
    assert_eq!(out.status.code(), Some(2));
    let out = spritebox(&["font", &two, "--out", ttf.to_str().unwrap(), "--font", "b"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(ttf.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn imports_are_read_from_disk_relative_to_the_file() {
    let dir = temp_dir("imports");
    std::fs::create_dir_all(dir.join("art/shared")).unwrap();
    write(
        &dir,
        "art/shared/palettes.spritebox",
        "palette p\n  k #000000\n  x #00\n",
    );
    let main = write(
        &dir,
        "art/main.spritebox",
        "import shared/palettes.spritebox\nsprite s 1x1\n  k\n",
    );
    let out = spritebox(&["check", &main]);
    assert_eq!(out.status.code(), Some(1));
    let expected = dir.join("art").join("shared").join("palettes.spritebox");
    let text = stderr(&out);
    assert!(
        text.contains(&format!(
            "{}:3:5: error[invalid_argument]",
            expected.display()
        )),
        "{text}"
    );
    let out = spritebox(&["check", &main, "--json"]);
    assert!(
        stdout(&out).contains(&format!("\"file\":{:?}", expected.display().to_string())),
        "{}",
        stdout(&out)
    );
    write(
        &dir,
        "art/shared/palettes.spritebox",
        "palette p\n  k #000000\n",
    );
    let out = spritebox(&["render", &main, "--out", dir.join("out").to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn render_does_not_write_invalid_files() {
    let dir = temp_dir("render-invalid");
    let file = write(&dir, "bad.spritebox", BAD);
    let out_dir = dir.join("out");
    let out = spritebox(&["render", &file, "--out", out_dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(!out_dir.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn usage_and_io_errors() {
    for args in [
        &[][..],
        &["draw", COIN],
        &["check"],
        &["check", COIN, "--frobnicate"],
        &["check", COIN, COIN],
        &["render", COIN],
        &["render", COIN, "--out"],
        &["render", COIN, "--out", "x", "--scale", "0"],
        &["check", COIN, "--out", "x"],
        &["check", COIN, "--no-extrude"],
        &["render", COIN, "--out", "x", "--max-side", "0"],
        &["font", COIN],
        &["render", COIN, "--out", "x", "--font", "hud"],
        &["render", COIN, "--out", "x", "--padding", "65"],
        &["lint", COIN, "--allow", "nope"],
        &["lint", COIN, "--deny"],
        &["lint", COIN, "--out", "x"],
        &["check", COIN, "--strict"],
        &["render", COIN, "--out", "x", "--allow", "unused_key"],
    ] {
        let out = spritebox(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(stderr(&out).contains("Usage:"), "{args:?}");
    }
    let out = spritebox(&["check", "/no/such/file.spritebox", "--json"]);
    assert_eq!(out.status.code(), Some(3));
    assert!(stdout(&out).contains("\"code\":\"io\""), "{}", stdout(&out));

    let out = spritebox(&["--help"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).contains("spritebox check <file>"));
    let out = spritebox(&["--version"]);
    assert_eq!(
        stdout(&out),
        format!("spritebox {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn lint_reports_warnings() {
    let dir = temp_dir("lint");
    let file = dir.join("noisy.spritebox");
    std::fs::write(
        &file,
        "palette p\n  g #3a7d44\n  h #468a50\n  k #000000\n\
         sprite leaf 3x3 palette=p\n  ggg\n  ghg\n  ggg\n",
    )
    .unwrap();
    let file = file.to_str().unwrap();

    // Warnings alone succeed.
    let out = spritebox(&["lint", file]);
    assert_eq!(out.status.code(), Some(0));
    let text = stderr(&out);
    assert!(
        text.contains(&format!(
            "{file}:4:3: warning[unused_key]: Nothing uses key `k` of palette `p`"
        )),
        "{text}"
    );
    assert!(
        text.contains(&format!("{file}:7:4: warning[orphan_pixel]: A lone pixel")),
        "{text}"
    );
    assert!(text.ends_with(": 2 warnings\n"), "{text}");

    // --strict fails on any warning, --deny on its codes; --allow drops a code.
    assert_eq!(
        spritebox(&["lint", file, "--strict"]).status.code(),
        Some(1)
    );
    assert_eq!(
        spritebox(&["lint", file, "--deny", "orphan_pixel"])
            .status
            .code(),
        Some(1)
    );
    let out = spritebox(&[
        "lint",
        file,
        "--allow",
        "orphan_pixel",
        "--deny",
        "orphan_pixel",
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stderr(&out).ends_with(": 1 warning\n"), "{}", stderr(&out));

    let out = spritebox(&["lint", file, "--json", "--strict"]);
    assert_eq!(out.status.code(), Some(1));
    let json = stdout(&out);
    assert!(json.contains("\"ok\":false"), "{json}");
    assert!(
        json.contains("{\"code\":\"unused_key\",\"severity\":\"warning\",\"line\":4,\"column\":3,"),
        "{json}"
    );

    // A clean file, and a file with errors, which are not linted.
    let out = spritebox(&["lint", COIN, "--strict"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).ends_with(": ok\n"), "{}", stdout(&out));
    let bad = dir.join("bad.spritebox");
    std::fs::write(&bad, "palette p\n  g #3a7d4\n").unwrap();
    let out = spritebox(&["lint", bad.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("error[invalid_argument]"),
        "{}",
        stderr(&out)
    );
    std::fs::remove_dir_all(dir).unwrap();
}
