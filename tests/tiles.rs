//! Tilesets, autotiling and tilemaps.

use spritebox::neighbours::*;
use spritebox::{
    BLOB_MASKS, Color, Document, ErrorCode, ExportOptions, Limits, Piece, Scheme, check,
};

/// The 47 blob masks, computed independently (see the README's table).
const DOCUMENTED_BLOB: [u8; 47] = [
    0, 1, 4, 5, 7, 16, 17, 20, 21, 23, 28, 29, 31, 64, 65, 68, 69, 71, 80, 81, 84, 85, 87, 92, 93,
    95, 112, 113, 116, 117, 119, 124, 125, 127, 193, 197, 199, 209, 213, 215, 221, 223, 241, 245,
    247, 253, 255,
];

/// A tileset of 4x4 tiles whose pieces are each one key: `i`sland, `h`_strip,
/// `v`_strip, i`n`ner and `f`ill.
fn pieces(scheme: &str) -> String {
    let piece = |name: &str, key: char| {
        let row: String = std::iter::repeat_n(key, 4).collect();
        format!("    {name}\n      {row}\n      {row}\n      {row}\n      {row}\n")
    };
    let mut text = String::from(
        "palette p\n  . transparent\n  i #ff0000\n  h #00ff00\n  v #0000ff\n  n #ffff00\n  f #ffffff\n  b #000000\n",
    );
    text.push_str(&format!("tileset t 4x4\n  terrain land {scheme}\n"));
    text.push_str(&piece("island", 'i'));
    text.push_str(&piece("h_strip", 'h'));
    text.push_str(&piece("v_strip", 'v'));
    if scheme == "47" {
        text.push_str(&piece("inner", 'n'));
    }
    text.push_str(&piece("fill", 'f'));
    text.push_str("  tile block\n    bbbb\n    bbbb\n    bbbb\n    bbbb\n");
    text
}

fn key_of(piece: Piece) -> char {
    match piece {
        Piece::Island => 'i',
        Piece::HStrip => 'h',
        Piece::VStrip => 'v',
        Piece::Inner => 'n',
        Piece::Fill => 'f',
    }
}

#[test]
fn blob_masks_are_documented() {
    assert_eq!(BLOB_MASKS, DOCUMENTED_BLOB);
    assert_eq!((N, NE, E, SE, S, SW, W, NW), (1, 2, 4, 8, 16, 32, 64, 128));
}

#[test]
fn quarters_come_from_the_documented_pieces() {
    for (scheme, text) in [(Scheme::Blob, "47"), (Scheme::Edges, "16")] {
        let doc: Document = pieces(text).parse().unwrap();
        let tileset = doc.tileset("t").unwrap();
        let terrain = tileset.terrain("land").unwrap();
        assert_eq!(terrain.scheme(), scheme);
        assert_eq!(terrain.first_index(), 1);
        assert_eq!(tileset.tile_count(), scheme.variants() + 1);
        assert_eq!(tileset.index("block"), Some(scheme.variants() + 1));
        for variant in 0..scheme.variants() {
            let mask = scheme.mask(variant);
            let tile = tileset.tile(terrain.first_index() + variant).unwrap();
            assert_eq!(tile.name(), format!("land/{variant}"));
            // (quarter's top-left pixel, vertical, horizontal and corner bits)
            for (x, y, v, h, c) in [
                (0, 0, N, W, NW),
                (2, 0, N, E, NE),
                (0, 2, S, W, SW),
                (2, 2, S, E, SE),
            ] {
                let corner = scheme == Scheme::Edges || mask & c != 0;
                let expected = key_of(Piece::for_quarter(mask & v != 0, mask & h != 0, corner));
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    assert_eq!(
                        tile.key(x + dx, y + dy),
                        Some(expected),
                        "{scheme:?} variant {variant} mask {mask}"
                    );
                }
            }
        }
    }
}

/// A 3x3 map whose centre is land, with land around it where `mask` says.
fn neighbourhood(mask: u8) -> String {
    let at = |bit: u8| if mask & bit != 0 { '#' } else { '.' };
    format!(
        "tilemap m outside=empty\n  legend\n    . empty\n    # land\n  map\n    {}{}{}\n    {}#{}\n    {}{}{}\n",
        at(NW),
        at(N),
        at(NE),
        at(W),
        at(E),
        at(SW),
        at(S),
        at(SE)
    )
}

#[test]
fn every_neighbour_configuration_maps_to_the_documented_variant() {
    for (scheme, text) in [(Scheme::Blob, "47"), (Scheme::Edges, "16")] {
        let base = pieces(text);
        for mask in 0..=255u8 {
            let doc: Document = format!("{base}{}", neighbourhood(mask)).parse().unwrap();
            let map = doc.tilemap("m").unwrap();
            let documented = match scheme {
                Scheme::Edges => {
                    (mask & N != 0) as u32
                        + 2 * (mask & E != 0) as u32
                        + 4 * (mask & S != 0) as u32
                        + 8 * (mask & W != 0) as u32
                }
                Scheme::Blob => {
                    let mut reduced = mask & (N | E | S | W);
                    for (corner, a, b) in [(NE, N, E), (SE, S, E), (SW, S, W), (NW, N, W)] {
                        if mask & corner != 0 && mask & a != 0 && mask & b != 0 {
                            reduced |= corner;
                        }
                    }
                    DOCUMENTED_BLOB.iter().position(|&m| m == reduced).unwrap() as u32
                }
            };
            assert_eq!(scheme.variant(mask), documented);
            assert_eq!(
                map.tile(1, 1),
                Some(1 + documented),
                "{scheme:?} mask {mask:#010b}"
            );
        }
    }
}

#[test]
fn map_edges_connect_or_not() {
    let base = pieces("47");
    let one = |outside: &str| {
        let doc: Document =
            format!("{base}tilemap m {outside}\n  legend\n    # land\n  map\n    #\n")
                .parse()
                .unwrap();
        doc.tilemap("m").unwrap().tile(0, 0).unwrap()
    };
    assert_eq!(
        one(""),
        47,
        "outside=same is the default: fill, the last variant"
    );
    assert_eq!(one("outside=same"), 47);
    assert_eq!(one("outside=empty"), 1, "island, variant 0");
}

#[test]
fn legends() {
    let base = pieces("47");
    let doc: Document = format!(
        "{base}tilemap m outside=empty
  legend
    .  empty
    #  land   solid
    5  land/5  solid   ; a fixed variant, still land to its neighbours
    B  block  solid
  map
    #5..
    ..B.
"
    )
    .parse()
    .unwrap();
    let map = doc.tilemap("m").unwrap();
    assert_eq!((map.width(), map.height(), map.tileset()), (4, 2, "t"));
    // `#` has land east: mask E, variant 2.
    assert_eq!(map.tiles(), [1 + 2, 1 + 5, 0, 0, 0, 0, 48, 0]);
    // Solid: every land variant (the fixed one included) and the block.
    assert_eq!(map.solid(), (1..=48).collect::<Vec<u32>>());
    assert!(map.is_solid(6) && !map.is_solid(0));
    // Without `solid`, nothing is.
    let doc: Document = format!("{base}tilemap m\n  legend\n    # land\n  map\n    #\n")
        .parse()
        .unwrap();
    assert!(doc.tilemap("m").unwrap().solid().is_empty());
}

#[test]
fn terrains_connect_only_to_themselves() {
    let doc: Document = format!(
        "{}  terrain rock 16\n{}tilemap m outside=empty\n  legend\n    # land\n    r rock\n  map\n    #r\n",
        pieces("47"),
        ["island", "h_strip", "v_strip", "fill"]
            .iter()
            .map(|p| format!("    {p}\n      bbbb\n      bbbb\n      bbbb\n      bbbb\n"))
            .collect::<String>()
    )
    .parse()
    .unwrap();
    let tileset = doc.tileset("t").unwrap();
    let rock = tileset.terrain("rock").unwrap();
    assert_eq!(rock.first_index(), 49);
    assert_eq!(doc.tilemap("m").unwrap().tiles(), [1, rock.index(0)]);
}

#[test]
fn tileset_textures() {
    let doc: Document = pieces("16")
        .replace("tileset t 4x4", "tileset t 4x4 columns=5")
        .parse()
        .unwrap();
    let t = doc.tileset("t").unwrap();
    assert_eq!(t.columns(), 5);
    assert_eq!(t.image_size(), (20, 16), "17 tiles in rows of 5");
    let image = t.render();
    assert_eq!((image.width(), image.height()), (20, 16));
    for index in 1..=t.tile_count() {
        let r = t.tile_rect(index).unwrap();
        assert_eq!((r.x, r.y), (((index - 1) % 5) * 4, ((index - 1) / 5) * 4));
        let tile = t.tile(index).unwrap().render();
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(image.pixel(r.x + x, r.y + y), tile.pixel(x, y));
            }
        }
    }
    assert_eq!(t.tile_rect(0), None);
    assert_eq!(t.tile_rect(18), None);
    assert_eq!(t.tile(0), None);
    // The default is 8 columns, or fewer for fewer tiles.
    let doc: Document = "palette p\n  a #ff0000\ntileset t 1x1\n  tile a\n    a\n  tile b = a:flip_x\nsprite a 1x1\n  a\n"
        .parse()
        .unwrap();
    assert_eq!(doc.tileset("t").unwrap().columns(), 2);
}

#[test]
fn tilemaps_render() {
    let base = pieces("47");
    let doc: Document =
        format!("{base}tilemap m\n  legend\n    . empty\n    B block\n  map\n    B.\n")
            .parse()
            .unwrap();
    let map = doc.tilemap("m").unwrap();
    let image = map.render(doc.tileset("t").unwrap()).unwrap();
    assert_eq!((image.width(), image.height()), (8, 4));
    assert_eq!(image.pixel(0, 0), Some(Color::BLACK));
    assert_eq!(image.pixel(4, 0), Some(Color::TRANSPARENT));
}

#[test]
fn export_metadata() {
    let doc: Document = "
palette p
  .  transparent
  a  #ff0000
tileset t 2x2
  tile a
    aa
    a.
  tile b = s
sprite s 2x2
  .a
  aa
tilemap m
  legend
    .  empty
    a  a  solid
    b  b
  map
    ab.
    .ba
"
    .parse()
    .unwrap();
    let export = doc.export(&ExportOptions::default()).unwrap();
    let json = export.to_json();
    let tail = &json[json.find("  \"tilesets\"").unwrap()..];
    assert_eq!(
        tail,
        r#"  "tilesets": {
    "t": {
      "image": "tileset-t.png",
      "size": [8, 4],
      "tile_size": [2, 2],
      "columns": 2,
      "padding": 1,
      "tiles": [
        "a",
        "b"
      ],
      "terrains": {}
    }
  },
  "tilemaps": {
    "m": {
      "tileset": "t",
      "size": [3, 2],
      "tiles": [
        [1, 2, 0],
        [0, 2, 1]
      ],
      "solid": [1]
    }
  },
  "fonts": {}
}
"#
    );
    assert!(json.starts_with("{\n  \"format\": \"spritebox\",\n  \"version\": 1,\n  \"atlas\": {"));
    let names: Vec<String> = export.files().into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, ["atlas.png", "tileset-t.png", "spritebox.json"]);
    let files = export.files();
    assert_eq!(
        spritebox::png::decode(&files[1].1).unwrap(),
        export.tilesets[0].image
    );
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

const P: &str = "palette p\n  . transparent\n  a #ff0000\nsprite s 2x2\n  .a\n  aa\n";

#[test]
fn tileset_errors() {
    // `P` takes lines 1..=6.
    let e = |body: &str| error(&format!("{P}{body}"));
    assert_eq!(e("tileset t\n"), (7, 10, Syntax));
    assert_eq!(e("tileset t 2x2 3x3\n"), (7, 15, Syntax));
    assert_eq!(e("tileset t 2x2 columns=0\n"), (7, 23, InvalidArgument));
    assert_eq!(e("tileset t 2x2 columns=x\n"), (7, 23, Syntax));
    assert_eq!(e("tileset t 2x2 spacing=1\n"), (7, 15, Syntax));
    assert_eq!(e("tileset t 2x2\n  tiles a\n"), (8, 3, Syntax));
    assert_eq!(e("tileset t 2x2\n  tile\n"), (8, 7, Syntax));
    assert_eq!(e("tileset t 2x2\n  tile a\n    aa\n"), (8, 9, SizeMismatch));
    assert_eq!(
        e("tileset t 2x2\n  tile a\n    aa\n    a\n"),
        (10, 6, SizeMismatch)
    );
    assert_eq!(
        e("tileset t 2x2\n  tile a\n    aa\n    ax\n"),
        (10, 6, UnknownName)
    );
    assert_eq!(
        e("tileset t 2x2\n  tile a = s\n  tile a = s\n"),
        (9, 8, DuplicateName)
    );
    assert_eq!(
        e("tileset t 2x2\n  tile a = s:pad(1)\n"),
        (8, 12, SizeMismatch)
    );
    assert_eq!(e("tileset t 2x2\n  tile a = z\n"), (8, 12, UnknownName));
    assert_eq!(e("tileset t 2x2\n  tile a = s\n    aa\n"), (9, 5, Syntax));
    assert_eq!(e("tileset t 2x2\n  tile a =\n"), (8, 11, Syntax));
    assert_eq!(e("tileset t 2x2\n  tile a b\n"), (8, 10, Syntax));
    let two_palettes = "palette p\n  . transparent\n  a #ff0000\nsprite s 2x2 palette=p\n  .a\n  aa\n\
        palette q\n  . transparent\n  a #000000\ntileset t 2x2 palette=q\n  tile a = s\n";
    assert_eq!(error(two_palettes), (11, 12, InvalidArgument));
    assert_eq!(
        e("tileset t 3x3\n  terrain d 47\n"),
        (8, 13, InvalidArgument)
    );
    assert_eq!(e("tileset t 2x2\n  terrain d 8\n"), (8, 13, Syntax));
    assert_eq!(e("tileset t 2x2\n  terrain d\n"), (8, 12, Syntax));
    assert_eq!(e("tileset t 2x2\n  terrain d 16 x\n"), (8, 16, Syntax));
    let piece = |name: &str| format!("    {name} = s\n");
    let all16: String = ["island", "h_strip", "v_strip", "fill"]
        .iter()
        .map(|p| piece(p))
        .collect();
    assert!(errors(&format!("{P}tileset t 2x2\n  terrain d 16\n{all16}")).is_empty());
    assert_eq!(
        e(&format!(
            "tileset t 2x2\n  terrain d 16\n{all16}{}",
            piece("inner")
        )),
        (13, 5, Syntax)
    );
    assert_eq!(
        e(&format!(
            "tileset t 2x2\n  terrain d 16\n{all16}{}",
            piece("fill")
        )),
        (13, 5, DuplicateName)
    );
    assert_eq!(
        e(&format!(
            "tileset t 2x2\n  terrain d 16\n{all16}{}",
            piece("corner")
        )),
        (13, 5, Syntax)
    );
    assert_eq!(
        e(&format!("tileset t 2x2\n  terrain d 47\n{all16}")),
        (8, 11, Syntax),
        "no `inner`"
    );
    assert_eq!(
        e("tileset t 2x2\n  terrain d 16\n    island\n      aa\n      aa\n"),
        (8, 11, Syntax)
    );
    assert_eq!(
        e("tileset t 2x2\n  terrain d 16\n    island\n      aa\n"),
        (9, 11, SizeMismatch)
    );
    let limits = Limits {
        max_tiles: 20,
        ..Limits::default()
    };
    let text = format!(
        "{P}tileset t 2x2\n  terrain d 16\n{all16}  tile x = s\n  tile y = s\n  tile z = s\n  tile w = s\n  tile v = s\n"
    );
    assert_eq!(
        check(&text, &limits)
            .iter()
            .map(|e| (e.position.unwrap().line, e.code))
            .collect::<Vec<_>>(),
        [(7, LimitExceeded)]
    );
    assert_eq!(e("tileset t 2x2\ntileset t 2x2\n"), (8, 9, DuplicateName));
}

#[test]
fn tilemap_errors() {
    // `P` and a tileset take lines 1..=8.
    let e = |body: &str| error(&format!("{P}tileset t 2x2\n  tile a = s\n{body}"));
    assert_eq!(
        e("tilemap m\n  legend\n    . empty\n"),
        (9, 9, Syntax),
        "no map"
    );
    assert_eq!(e("tilemap m\n  map\n    ..\n"), (9, 9, Syntax), "no legend");
    assert_eq!(
        e("tilemap m extra\n  legend\n    . empty\n  map\n    .\n"),
        (9, 11, Syntax)
    );
    assert_eq!(
        e("tilemap m tileset=u\n  legend\n    . empty\n  map\n    .\n"),
        (9, 19, UnknownName)
    );
    assert_eq!(
        e("tilemap m outside=wall\n  legend\n    . empty\n  map\n    .\n"),
        (9, 19, Syntax)
    );
    let m = |legend: &str, map: &str| e(&format!("tilemap m\n  legend\n{legend}  map\n{map}"));
    assert_eq!(m("    . empty\n  key\n", "    .\n"), (12, 3, Syntax));
    assert_eq!(m("    .. empty\n", "    .\n"), (11, 5, Syntax));
    assert_eq!(m("    .\n", "    .\n"), (11, 6, Syntax));
    assert_eq!(m("    . empty hard\n", "    .\n"), (11, 13, Syntax));
    assert_eq!(
        m("    . empty solid\n", "    .\n"),
        (11, 7, InvalidArgument)
    );
    assert_eq!(m("    . wall\n", "    .\n"), (11, 7, UnknownName));
    assert_eq!(m("    . a/x\n", "    .\n"), (11, 7, Syntax));
    assert_eq!(
        m("    . empty\n    . a\n", "    .\n"),
        (12, 5, DuplicateName)
    );
    assert_eq!(
        m("    a a solid\n    b a\n", "    ab\n"),
        (12, 7, InvalidArgument)
    );
    assert_eq!(m("    . empty\n", "    ..\n    .\n"), (14, 6, SizeMismatch));
    assert_eq!(m("    . empty\n", "    . .\n"), (13, 5, Syntax));
    let unknown = format!(
        "{P}tileset t 2x2\n  tile a = s\ntilemap m\n  legend\n    . empty\n  map\n    .x.x\n    .xx.\n"
    );
    assert_eq!(
        errors(&unknown),
        [(13, 6, UnknownName), (14, 6, UnknownName)],
        "the first of each row"
    );
    assert_eq!(
        e("tilemap m\n  legend\n    . empty\n  legend\n  map\n    .\n"),
        (12, 3, DuplicateName)
    );
    assert_eq!(
        e("tilemap m\n  legend\n    . empty\n  map\n"),
        (12, 6, Syntax),
        "a map without rows"
    );
    let limits = Limits {
        max_map_side: 3,
        ..Limits::default()
    };
    let text = format!(
        "{P}tileset t 2x2\n  tile a = s\ntilemap m\n  legend\n    . empty\n  map\n    ....\n"
    );
    assert_eq!(
        check(&text, &limits)
            .iter()
            .map(|e| (e.position.unwrap().line, e.position.unwrap().column, e.code))
            .collect::<Vec<_>>(),
        [(13, 8, LimitExceeded)]
    );
    let two = format!(
        "{P}tileset t 2x2\n  tile a = s\ntileset u 2x2\n  tile a = s\ntilemap m\n  legend\n    . empty\n  map\n    .\n"
    );
    assert_eq!(error(&two), (11, 9, Syntax), "two tilesets need `tileset=`");
    assert_eq!(
        e(
            "tilemap m\n  legend\n    . empty\n  map\n    .\ntilemap m\n  legend\n    . empty\n  map\n    .\n"
        ),
        (14, 9, DuplicateName)
    );
}
