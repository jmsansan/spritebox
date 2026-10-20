//! Pixel fonts, text, and TrueType export checked by reading the file back.

use std::collections::HashMap;

use spritebox::{Color, Document, ErrorCode, Font, Limits, TtfOptions, check};

const HUD: &str = include_str!("../examples/font.spritebox");

fn hud() -> Font {
    let doc: Document = HUD.parse().unwrap();
    doc.font("hud").unwrap().clone()
}

#[test]
fn fonts() {
    let doc: Document = "
font tiny height=3 baseline=2 line_height=5 spacing=2
  glyph A
    .#.
    ###
    #.#
  glyph U+003B advance=4   ; `;`, which starts a comment, by its code
    #
    .
    #
  glyph U+1F600
    #
    #
    #
  glyph j
    .#
    .#
    #.
"
    .parse()
    .unwrap();
    let font = doc.font("tiny").unwrap();
    assert_eq!(
        (
            font.name(),
            font.height(),
            font.baseline(),
            font.line_height()
        ),
        ("tiny", 3, 2, 5)
    );
    let a = font.glyph('A').unwrap();
    assert_eq!((a.width(), a.advance()), (3, 5), "width + spacing");
    assert!(a.pixel(1, 0) && !a.pixel(0, 0) && a.pixel(2, 2));
    assert_eq!(font.glyph(';').unwrap().advance(), 4);
    assert!(font.glyph('😀').is_some());
    // A space is added: half the height, rounded up.
    let space = font.glyph(' ').unwrap();
    assert_eq!((space.width(), space.advance()), (0, 2));
    let chars: String = font.glyphs().iter().map(|g| g.char()).collect();
    assert_eq!(chars, "A;😀j ");

    let blank: Document =
        "font f height=2 space=5\n  glyph x\n    #.\n    .#\n  glyph U+00A0 advance=3\n"
            .parse()
            .unwrap();
    let f = blank.font("f").unwrap();
    assert_eq!(f.baseline(), 2, "no descent by default");
    assert_eq!(f.line_height(), 3, "height + 1 by default");
    assert_eq!(f.glyph(' ').unwrap().advance(), 5);
    assert_eq!(f.glyph('\u{a0}').unwrap().width(), 0);
}

#[test]
fn text() {
    let font = hud();
    assert_eq!(font.measure("AB"), (12, 11));
    assert_eq!(font.measure("A\nBB"), (12, 24));
    let image = font.render("A1", Color::WHITE).unwrap();
    assert_eq!((image.width(), image.height()), (10, 11));
    assert_eq!(
        image.pixel(1, 2),
        Some(Color::WHITE),
        "capitals start below the accent rows"
    );
    assert_eq!(image.pixel(0, 0), Some(Color::TRANSPARENT));
    // `1` starts after `A`'s advance of 6.
    assert_eq!(image.pixel(7, 2), Some(Color::WHITE));
    // Characters without a glyph are drawn as `?`.
    assert_eq!(
        font.render("€", Color::WHITE).unwrap(),
        font.render("?", Color::WHITE).unwrap()
    );
}

/// Whether `(x, y)` is inside contours, by the non-zero winding rule.
fn inside(contours: &[Vec<(i32, i32)>], x: f64, y: f64) -> bool {
    let mut winding = 0;
    for c in contours {
        for i in 0..c.len() {
            let (a, b) = (c[i], c[(i + 1) % c.len()]);
            let (ax, ay, bx, by) = (a.0 as f64, a.1 as f64, b.0 as f64, b.1 as f64);
            if (ay <= y) != (by <= y) {
                let t = (y - ay) / (by - ay);
                if ax + t * (bx - ax) > x {
                    winding += if by > ay { 1 } else { -1 };
                }
            }
        }
    }
    winding != 0
}

fn signed_area(contour: &[(i32, i32)]) -> i64 {
    (0..contour.len())
        .map(|i| {
            let (a, b) = (contour[i], contour[(i + 1) % contour.len()]);
            a.0 as i64 * b.1 as i64 - b.0 as i64 * a.1 as i64
        })
        .sum::<i64>()
}

#[test]
fn outlines_cover_exactly_the_pixels() {
    let font = hud();
    for glyph in font.glyphs() {
        let contours = font.outline(glyph.char()).unwrap();
        let mut ink = 0i64;
        for row in 0..font.height() {
            for x in 0..glyph.width() + 1 {
                // The centre of the pixel, y up from the baseline.
                let y = font.baseline() as f64 - row as f64 - 0.5;
                assert_eq!(
                    inside(&contours, x as f64 + 0.5, y),
                    glyph.pixel(x, row),
                    "{:?} at ({x}, {row})",
                    glyph.char()
                );
                ink += glyph.pixel(x, row) as i64;
            }
        }
        // Clockwise outer contours have negative area, holes positive; together they
        // cover one unit for each inked pixel.
        let total: i64 = contours.iter().map(|c| signed_area(c)).sum();
        assert_eq!(total, -2 * ink, "{:?}", glyph.char());
        // No point lies in the middle of a straight run.
        for c in &contours {
            for i in 0..c.len() {
                let (a, b, d) = (c[(i + c.len() - 1) % c.len()], c[i], c[(i + 1) % c.len()]);
                assert_ne!(
                    (b.0 - a.0) * (d.1 - b.1),
                    (b.1 - a.1) * (d.0 - b.0),
                    "{:?}",
                    glyph.char()
                );
            }
        }
    }
    // A ring has an outer contour and a hole; pixels touching only at a corner make
    // separate contours.
    let doc: Document = "font f height=3\n  glyph o\n    ###\n    #.#\n    ###\n  glyph x\n    #.\n    .#\n    ..\n"
        .parse()
        .unwrap();
    let f = doc.font("f").unwrap();
    let ring = f.outline('o').unwrap();
    assert_eq!(
        ring.iter()
            .map(|c| signed_area(c).signum())
            .collect::<Vec<_>>(),
        [-1, 1]
    );
    assert_eq!(f.outline('x').unwrap().len(), 2);
}

/// A minimal TrueType reader for the tests.
struct Ttf<'a> {
    data: &'a [u8],
    tables: HashMap<[u8; 4], &'a [u8]>,
}

fn u16_at(d: &[u8], i: usize) -> u16 {
    u16::from_be_bytes([d[i], d[i + 1]])
}
fn i16_at(d: &[u8], i: usize) -> i32 {
    u16_at(d, i) as i16 as i32
}
fn u32_at(d: &[u8], i: usize) -> u32 {
    u32::from_be_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]])
}
fn checksum(d: &[u8]) -> u32 {
    d.chunks(4).fold(0u32, |s, c| {
        let mut w = [0; 4];
        w[..c.len()].copy_from_slice(c);
        s.wrapping_add(u32::from_be_bytes(w))
    })
}

impl<'a> Ttf<'a> {
    fn new(data: &'a [u8]) -> Self {
        assert_eq!(u32_at(data, 0), 0x0001_0000, "TrueType outlines");
        let n = u16_at(data, 4) as usize;
        let power = 1usize << (usize::BITS - 1 - n.leading_zeros());
        assert_eq!(u16_at(data, 6) as usize, power * 16);
        assert_eq!(u16_at(data, 8) as usize, power.trailing_zeros() as usize);
        assert_eq!(u16_at(data, 10) as usize, n * 16 - power * 16);
        let mut tables = HashMap::new();
        let mut previous = [0u8; 4];
        for i in 0..n {
            let r = 12 + 16 * i;
            let tag: [u8; 4] = data[r..r + 4].try_into().unwrap();
            assert!(tag > previous, "tables sorted by tag");
            previous = tag;
            let (sum, offset, len) = (
                u32_at(data, r + 4),
                u32_at(data, r + 8) as usize,
                u32_at(data, r + 12) as usize,
            );
            assert_eq!(offset % 4, 0, "{tag:?} aligned");
            let table = &data[offset..offset + len];
            if &tag == b"head" {
                let mut zeroed = table.to_vec();
                zeroed[8..12].fill(0);
                assert_eq!(checksum(&zeroed), sum, "head checksum");
            } else {
                assert_eq!(
                    checksum(table),
                    sum,
                    "{} checksum",
                    String::from_utf8_lossy(&tag)
                );
            }
            tables.insert(tag, table);
        }
        let ttf = Self { data, tables };
        // The whole file sums to the magic number.
        assert_eq!(checksum(ttf.data), 0xb1b0_afba);
        ttf
    }
    fn table(&self, tag: &[u8; 4]) -> &'a [u8] {
        self.tables
            .get(tag)
            .copied()
            .unwrap_or_else(|| panic!("no {}", String::from_utf8_lossy(tag)))
    }
    /// The glyph for `c` through the format 4 or 12 subtable of platform 3.
    fn glyph_id(&self, c: char) -> u32 {
        let cmap = self.table(b"cmap");
        let records = u16_at(cmap, 2) as usize;
        for r in 0..records {
            let (platform, encoding, offset) = (
                u16_at(cmap, 4 + 8 * r),
                u16_at(cmap, 6 + 8 * r),
                u32_at(cmap, 8 + 8 * r) as usize,
            );
            let sub = &cmap[offset..];
            let code = c as u32;
            if (platform, encoding) == (3, 10) && u16_at(sub, 0) == 12 {
                for g in 0..u32_at(sub, 12) as usize {
                    let (start, end, glyph) = (
                        u32_at(sub, 16 + 12 * g),
                        u32_at(sub, 20 + 12 * g),
                        u32_at(sub, 24 + 12 * g),
                    );
                    if (start..=end).contains(&code) {
                        return glyph + code - start;
                    }
                }
                return 0;
            }
            if (platform, encoding) == (3, 1)
                && u16_at(sub, 0) == 4
                && code <= 0xffff
                && !self.has_format_12()
            {
                let segs = u16_at(sub, 6) as usize / 2;
                for s in 0..segs {
                    let end = u16_at(sub, 14 + 2 * s) as u32;
                    let start = u16_at(sub, 16 + 2 * segs + 2 * s) as u32;
                    let delta = u16_at(sub, 16 + 4 * segs + 2 * s) as u32;
                    assert_eq!(u16_at(sub, 16 + 6 * segs + 2 * s), 0, "no glyph id arrays");
                    if (start..=end).contains(&code) {
                        return (code + delta) & 0xffff;
                    }
                }
                return 0;
            }
        }
        0
    }
    fn has_format_12(&self) -> bool {
        let cmap = self.table(b"cmap");
        (0..u16_at(cmap, 2) as usize)
            .any(|r| (u16_at(cmap, 4 + 8 * r), u16_at(cmap, 6 + 8 * r)) == (3, 10))
    }
    /// Contours of glyph `id`.
    fn contours(&self, id: u32) -> Vec<Vec<(i32, i32)>> {
        let loca = self.table(b"loca");
        let glyf = self.table(b"glyf");
        let (start, end) = (
            u32_at(loca, 4 * id as usize) as usize,
            u32_at(loca, 4 * id as usize + 4) as usize,
        );
        if start == end {
            return Vec::new();
        }
        let g = &glyf[start..end];
        let n = i16_at(g, 0);
        assert!(n >= 0, "simple glyphs only");
        let ends: Vec<usize> = (0..n as usize)
            .map(|i| u16_at(g, 10 + 2 * i) as usize)
            .collect();
        let points = ends.last().map_or(0, |e| e + 1);
        let mut at = 10 + 2 * n as usize;
        let instructions = u16_at(g, at) as usize;
        at += 2 + instructions;
        let flags = &g[at..at + points];
        assert!(
            flags.iter().all(|&f| f == 1),
            "on-curve points with 16-bit deltas"
        );
        at += points;
        let mut xs = Vec::new();
        let mut x = 0;
        for _ in 0..points {
            x += i16_at(g, at);
            xs.push(x);
            at += 2;
        }
        let mut y = 0;
        let mut all = Vec::new();
        for &x in &xs {
            y += i16_at(g, at);
            all.push((x, y));
            at += 2;
        }
        // The glyph's bounding box matches its points.
        let (x0, x1) = (
            xs.iter().min().copied().unwrap(),
            xs.iter().max().copied().unwrap(),
        );
        assert_eq!((i16_at(g, 2), i16_at(g, 6)), (x0, x1));
        let mut contours = Vec::new();
        let mut first = 0;
        for e in ends {
            contours.push(all[first..=e].to_vec());
            first = e + 1;
        }
        contours
    }
    fn advance(&self, id: u32) -> u32 {
        u16_at(self.table(b"hmtx"), 4 * id as usize) as u32
    }
    fn name(&self, id: u16) -> String {
        let name = self.table(b"name");
        let (count, storage) = (u16_at(name, 2) as usize, u16_at(name, 4) as usize);
        for r in 0..count {
            let rec = 6 + 12 * r;
            if u16_at(name, rec + 6) == id && u16_at(name, rec) == 3 {
                let (len, offset) = (
                    u16_at(name, rec + 8) as usize,
                    u16_at(name, rec + 10) as usize,
                );
                let units: Vec<u16> = name[storage + offset..storage + offset + len]
                    .chunks(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                return String::from_utf16(&units).unwrap();
            }
        }
        panic!("no name {id}");
    }
}

#[test]
fn ttf_tables_outlines_and_metrics() {
    let font = hud();
    let bytes = font.to_ttf(&TtfOptions::default()).unwrap();
    let ttf = Ttf::new(&bytes);
    let tags: Vec<String> = {
        let mut t: Vec<String> = ttf
            .tables
            .keys()
            .map(|k| String::from_utf8_lossy(k).into_owned())
            .collect();
        t.sort();
        t
    };
    assert_eq!(
        tags,
        [
            "OS/2", "cmap", "glyf", "head", "hhea", "hmtx", "loca", "maxp", "name", "post"
        ]
    );
    let u = font.units_per_pixel() as i32;
    assert_eq!(u, 128);
    let head = ttf.table(b"head");
    assert_eq!(u32_at(head, 12), 0x5f0f_3cf5);
    assert_eq!(
        u16_at(head, 18) as i32,
        font.height() as i32 * u,
        "the em is the glyph height"
    );
    assert_eq!(i16_at(head, 50), 1, "long loca offsets");
    let hhea = ttf.table(b"hhea");
    assert_eq!(i16_at(hhea, 4), font.baseline() as i32 * u, "ascender");
    assert_eq!(
        i16_at(hhea, 6),
        -((font.height() - font.baseline()) as i32) * u,
        "descender"
    );
    assert_eq!(
        i16_at(hhea, 8),
        (font.line_height() - font.height()) as i32 * u,
        "line gap"
    );
    let glyph_count = u16_at(ttf.table(b"maxp"), 4) as u32;
    assert_eq!(
        glyph_count,
        font.glyphs().len() as u32 + 1,
        ".notdef and every glyph"
    );
    assert_eq!(u16_at(hhea, 34) as u32, glyph_count);
    let os2 = ttf.table(b"OS/2");
    assert_eq!((u16_at(os2, 0), os2.len()), (4, 96));
    assert_eq!(
        i16_at(os2, 68),
        font.baseline() as i32 * u,
        "typographic ascender"
    );
    assert_eq!(i16_at(os2, 88), 7 * u, "cap height: the top of `H`");
    assert_eq!(i16_at(os2, 86), 5 * u, "x-height: the top of `x`");
    assert_eq!(u32_at(ttf.table(b"post"), 0), 0x0003_0000);
    assert_eq!(ttf.name(1), "hud");
    assert_eq!(ttf.name(2), "Regular");
    assert_eq!(ttf.name(5), "Version 1.000");
    assert_eq!(ttf.name(6), "hud");
    // Each character maps to its glyph, whose outline and advance are the pixels'.
    for (i, glyph) in font.glyphs().iter().enumerate() {
        let id = ttf.glyph_id(glyph.char());
        assert_eq!(id, i as u32 + 1, "{:?}", glyph.char());
        let expected: Vec<Vec<(i32, i32)>> = font
            .outline(glyph.char())
            .unwrap()
            .into_iter()
            .map(|c| c.into_iter().map(|(x, y)| (x * u, y * u)).collect())
            .collect();
        assert_eq!(ttf.contours(id), expected, "{:?}", glyph.char());
        assert_eq!(ttf.advance(id), glyph.advance() * u as u32);
    }
    assert_eq!(ttf.glyph_id('€'), 0, "no glyph: .notdef");
    assert!(!ttf.contours(0).is_empty(), ".notdef is drawn");
    // The same font gives the same bytes.
    assert_eq!(font.to_ttf(&TtfOptions::default()).unwrap(), bytes);
}

#[test]
fn ttf_beyond_the_basic_plane_and_options() {
    let doc: Document = "font f height=2\n  glyph A\n    #\n    #\n  glyph U+1F600\n    ##\n    ##\n  glyph B\n    #\n    .\n".parse().unwrap();
    let font = doc.font("f").unwrap();
    let options = TtfOptions {
        family: Some("Pixel Party".into()),
        version: (2, 5),
    };
    let bytes = font.to_ttf(&options).unwrap();
    let ttf = Ttf::new(&bytes);
    assert!(ttf.has_format_12());
    assert_eq!(ttf.glyph_id('A'), 1);
    assert_eq!(ttf.glyph_id('😀'), 2);
    assert_eq!(ttf.glyph_id('B'), 3);
    assert_eq!(ttf.name(1), "Pixel Party");
    assert_eq!(ttf.name(5), "Version 2.005");
    assert_eq!(ttf.name(6), "PixelParty");
    // A tall font uses fewer units per pixel, so the em stays within 16384.
    let tall: String = format!("font t height=300\n  glyph I\n{}", "    #\n".repeat(300));
    let tall: Document = tall.parse().unwrap();
    let t = tall.font("t").unwrap();
    assert_eq!(t.units_per_pixel(), 54);
    let ttf_bytes = t.to_ttf(&TtfOptions::default()).unwrap();
    let ttf = Ttf::new(&ttf_bytes);
    assert_eq!(u16_at(ttf.table(b"head"), 18), 300 * 54);
}

fn errors(text: &str) -> Vec<(usize, usize, ErrorCode)> {
    check(text, &Limits::default())
        .iter()
        .map(|e| (e.position.unwrap().line, e.position.unwrap().column, e.code))
        .collect()
}

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

#[test]
fn font_errors() {
    assert_eq!(error("font f\n"), (1, 7, Syntax));
    assert_eq!(error("font f 7\n"), (1, 8, Syntax));
    assert_eq!(error("font f height=0\n"), (1, 15, InvalidArgument));
    assert_eq!(error("font f height=x\n"), (1, 15, Syntax));
    assert_eq!(
        error("font f height=2 baseline=3\n"),
        (1, 26, InvalidArgument)
    );
    assert_eq!(
        error("font f height=2 line_height=0\n"),
        (1, 29, InvalidArgument)
    );
    assert_eq!(error("font f height=2 kerning=1\n"), (1, 17, Syntax));
    assert_eq!(error("font f height=2\n  glyph\n"), (2, 8, Syntax));
    assert_eq!(error("font f height=2\n  char A\n"), (2, 3, Syntax));
    assert_eq!(error("font f height=2\n  glyph AB\n"), (2, 9, Syntax));
    assert_eq!(error("font f height=2\n  glyph U+12\n"), (2, 9, Syntax));
    assert_eq!(
        error("font f height=2\n  glyph U+D800\n"),
        (2, 9, InvalidArgument)
    );
    assert_eq!(
        error("font f height=2\n  glyph A advance=x\n"),
        (2, 19, Syntax)
    );
    assert_eq!(error("font f height=2\n  glyph A wide\n"), (2, 11, Syntax));
    assert_eq!(
        error("font f height=2\n  glyph A\n    #\n"),
        (2, 10, SizeMismatch)
    );
    assert_eq!(
        error("font f height=2\n  glyph A\n    #\n    #\n    #\n"),
        (5, 5, SizeMismatch)
    );
    assert_eq!(
        error("font f height=2\n  glyph A\n    ##\n    #\n"),
        (4, 6, SizeMismatch)
    );
    assert_eq!(
        error("font f height=2\n  glyph A\n    #x\n    ##\n"),
        (3, 6, Syntax)
    );
    assert_eq!(
        error("font f height=2\n  glyph A\n    # #\n    ##\n"),
        (3, 5, Syntax)
    );
    assert_eq!(
        error("font f height=1\n  glyph A\n    #\n  glyph A\n    #\n"),
        (4, 9, DuplicateName)
    );
    assert_eq!(
        error("font f height=1 space=2\n  glyph U+0020\n"),
        (1, 6, DuplicateName)
    );
    assert_eq!(
        error("font f height=1\nfont f height=1\n"),
        (2, 6, DuplicateName)
    );
    // A font with errors still counts as defined.
    assert_eq!(
        errors("font f height=1\n  glyph AB\nfont f height=1\n"),
        [(2, 9, Syntax), (3, 6, DuplicateName)]
    );
    let limits = Limits {
        max_glyphs: 1,
        ..Limits::default()
    };
    let found: Vec<(usize, ErrorCode)> = check("font f height=1\n  glyph a\n  glyph b\n", &limits)
        .iter()
        .map(|e| (e.position.unwrap().line, e.code))
        .collect();
    assert_eq!(found, [(3, LimitExceeded)]);
}

/// FreeType and macOS Core Text read the exported font. Opt in with
/// SPRITEBOX_FONT_TESTS=1; needs `fc-scan`, and on macOS `swift`.
#[test]
fn other_font_engines_read_the_font() {
    if std::env::var_os("SPRITEBOX_FONT_TESTS").is_none() {
        return;
    }
    let font = hud();
    let dir = std::env::temp_dir().join(format!("spritebox-font-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hud.ttf");
    std::fs::write(&path, font.to_ttf(&TtfOptions::default()).unwrap()).unwrap();

    let scan = std::process::Command::new("fc-scan")
        .arg(&path)
        .output()
        .expect("fc-scan runs");
    let text = String::from_utf8_lossy(&scan.stdout);
    assert!(text.contains("family: \"hud\""), "{text}");
    assert!(text.contains("fontformat: \"TrueType\""), "{text}");
    // Printable ASCII, and ¡ ¿ Á É Í Ñ Ó Ú Ü á é í ñ ó ú ü.
    assert!(
        text.contains(
            "0000: 00000000 ffffffff ffffffff 7fffffff 00000000 80000002 140a2202 140a2202"
        ),
        "{text}"
    );
    assert!(
        text.contains("lang: es|"),
        "FreeType finds Spanish covered: {text}"
    );

    if cfg!(target_os = "macos") {
        let chars: String = font.glyphs().iter().map(|g| g.char()).collect();
        let script = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/check-ttf.swift");
        let out = std::process::Command::new("swift")
            .args([script, path.to_str().unwrap(), "11", "9", "4", &chars])
            .output()
            .expect("swift runs");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{text}{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let mut lines = text.lines();
        assert_eq!(lines.next(), Some("family hud"));
        assert_eq!(
            lines.next(),
            Some(format!("glyphs {}", font.glyphs().len() + 1).as_str())
        );
        for glyph in font.glyphs() {
            assert_eq!(
                lines.next().unwrap(),
                format!(
                    "glyph {} advance {:.1}",
                    glyph.char() as u32,
                    glyph.advance() as f64
                )
            );
            for row in 0..font.height() {
                let line = lines.next().unwrap();
                let expected: String = (0..line.len() as u32)
                    .map(|x| if glyph.pixel(x, row) { '#' } else { '.' })
                    .collect();
                assert_eq!(line, expected, "{:?} row {row}", glyph.char());
            }
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn export_and_preview() {
    let doc: Document = HUD.parse().unwrap();
    let export = doc.export(&spritebox::ExportOptions::default()).unwrap();
    let names: Vec<String> = export.files().into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["atlas.png", "font-hud.ttf", "spritebox.json"]);
    assert_eq!(
        export.fonts[0].ttf,
        hud().to_ttf(&TtfOptions::default()).unwrap()
    );
    let json = export.to_json();
    assert!(json.ends_with(
        "  \"fonts\": {\n    \"hud\": {\n      \"file\": \"font-hud.ttf\",\n      \"height\": 11,\n      \"baseline\": 9,\n      \"line_height\": 13,\n      \"units_per_pixel\": 128\n    }\n  }\n}\n"
    ), "{json}");
    let sheet = doc.preview(&spritebox::PreviewOptions::default()).unwrap();
    let sum = sheet
        .pixels()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
            (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
        });
    assert_eq!(
        (sheet.width(), sheet.height(), sum),
        (554, 340, 0xb1df_998b_71de_5c21),
        "{sum:#018x}"
    );
}

#[test]
fn coverage_is_checked() {
    // The example font covers printable ASCII and Spanish.
    let font = hud();
    for c in (' '..='~').chain("áéíóúüñÁÉÍÓÚÜÑ¿¡".chars()) {
        assert!(font.glyph(c).is_some(), "{c:?}");
    }
    let found = check(
        "font f height=1 covers=spanish\n  glyph ñ\n    #\n",
        &Limits::default(),
    );
    assert_eq!(found.len(), 1);
    let e = &found[0];
    assert_eq!(e.code, UnknownName);
    assert_eq!(
        (e.position.unwrap().line, e.position.unwrap().column),
        (1, 24)
    );
    assert!(e.message.contains("15 of the characters"), "{}", e.message);
    assert!(e.message.contains("`á` (U+00E1)"), "{}", e.message);
    assert!(!e.message.contains("`ñ`"), "{}", e.message);
    assert_eq!(error("font f height=1 covers=greek\n"), (1, 24, Syntax));
    // The space that a font gets without writing one counts.
    let text: String = std::iter::once("font f height=1 covers=ascii".to_owned())
        .chain(('!'..='~').map(|c| {
            let name = if c == ';' {
                "U+003B".to_owned()
            } else {
                c.to_string()
            };
            format!("  glyph {name}\n    #")
        }))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(check(&text, &Limits::default()).is_empty());
}
