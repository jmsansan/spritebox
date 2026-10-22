//! Rendering checked by checksums of the RGBA output, and determinism.

use spritebox::{Color, Document, Image, PreviewOptions, png};

const COIN: &str = include_str!("../examples/coin.spritebox");
const SHOWCASE: &str = include_str!("../examples/showcase.spritebox");
const TILES: &str = include_str!("../examples/tiles.spritebox");

/// FNV-1a of the image's size and RGBA bytes.
fn checksum(image: &Image) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let size = [image.width().to_le_bytes(), image.height().to_le_bytes()].concat();
    for &byte in size.iter().chain(image.pixels()) {
        hash = (hash ^ byte as u64).wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

#[test]
fn golden_sprites() {
    let doc: Document = COIN.parse().unwrap();
    let sums: Vec<(String, u64)> = doc
        .sprites()
        .iter()
        .map(|s| (s.name().to_owned(), checksum(&s.render())))
        .collect();
    let expected = [
        ("coin", 0x181e_a313_09ba_cf27),
        ("coin_dusk", 0x1182_807f_6b0c_21a1),
        ("gem", 0x4952_9233_43b9_0211),
    ];
    for ((name, sum), (want_name, want)) in sums.iter().zip(expected) {
        assert_eq!(name, want_name);
        assert_eq!(*sum, want, "{name}: {sum:#018x}");
    }
}

#[test]
fn golden_preview_and_png() {
    let doc: Document = COIN.parse().unwrap();
    let sheet = doc.preview(&PreviewOptions::default()).unwrap();
    assert_eq!(
        checksum(&sheet),
        0x59f5_0531_473c_77fd,
        "{:#018x}",
        checksum(&sheet)
    );
    let small = doc
        .preview(&PreviewOptions {
            scale: 1,
            grid: false,
            max_width: 64,
            ..PreviewOptions::default()
        })
        .unwrap();
    assert_eq!(
        checksum(&small),
        0xd48d_637b_4465_3df7,
        "{:#018x}",
        checksum(&small)
    );
    let coin_png = doc.sprite("coin").unwrap().render().to_png();
    assert_eq!(
        fnv(&coin_png),
        0x6cb0_778c_1c87_24d3,
        "{:#018x}",
        fnv(&coin_png)
    );
}

#[test]
fn coin_pixels() {
    let doc: Document = COIN.parse().unwrap();
    let coin = doc.sprite("coin").unwrap().render();
    let dusk = doc.sprite("coin_dusk").unwrap().render();
    assert_eq!(coin.pixel(0, 0), Some(Color::TRANSPARENT));
    assert_eq!(coin.pixel(2, 0), Some(Color::rgb(0x1a, 0x1c, 0x2c)));
    assert_eq!(coin.pixel(2, 1), Some(Color::rgb(0xf4, 0xb4, 0x1b)));
    assert_eq!(dusk.pixel(2, 1), Some(Color::rgb(0xb8, 0x6f, 0x50)));
    assert_eq!(coin.pixel(5, 2), Some(Color::WHITE));
    assert_eq!(dusk.pixel(5, 2), Some(Color::WHITE));
}

#[test]
fn deterministic() {
    let render = || {
        let doc: Document = COIN.parse().unwrap();
        let sheet = doc.preview(&PreviewOptions::default()).unwrap();
        png::encode(&sheet)
    };
    let first = render();
    assert_eq!(first, render());
    assert_eq!(
        png::decode(&first).unwrap(),
        Document::parse(COIN)
            .unwrap()
            .preview(&PreviewOptions::default())
            .unwrap()
    );
}

#[test]
fn preview_options() {
    let doc: Document = COIN.parse().unwrap();
    for scale in [0, 65] {
        let options = PreviewOptions {
            scale,
            ..PreviewOptions::default()
        };
        assert!(doc.preview(&options).is_err());
    }
    // Narrow sheets wrap their rows; wide ones do not.
    let narrow = doc
        .preview(&PreviewOptions {
            max_width: 50,
            ..Default::default()
        })
        .unwrap();
    let wide = doc.preview(&PreviewOptions::default()).unwrap();
    assert!(narrow.height() > wide.height());
    assert!(narrow.width() < wide.width());
    let empty = Document::parse("")
        .unwrap()
        .preview(&PreviewOptions::default())
        .unwrap();
    assert!(empty.width() > 0 && empty.height() > 0);
    let limits = spritebox::Limits {
        max_image_side: 50,
        ..Default::default()
    };
    assert_eq!(
        doc.preview_with(&PreviewOptions::default(), &limits)
            .unwrap_err()
            .code,
        spritebox::ErrorCode::LimitExceeded
    );
}

#[test]
fn golden_showcase() {
    let doc: Document = SHOWCASE.parse().unwrap();
    let sums: Vec<String> = doc
        .sprites()
        .iter()
        .chain(doc.frame_sprites())
        .map(|s| format!("{} {:#018x}", s.name(), checksum(&s.render())))
        .collect();
    let expected = [
        "sprout 0xc66c7d367373108d",
        "sprout_left 0x124913331e827c65",
        "sprout_night 0x526bcf94e6169a7f",
        "sprout_outlined 0x98ada1746f675815",
        "coin 0xdefcae9f2446c526",
        "coin_edge 0x0dbde90e2bc13136",
        "coin_thin 0x39648771957a95bd",
        "arrow 0x4764a09a1ea97c85",
        "arrow_down 0x21e1efaa8353da05",
        "badge 0xd90dbc557dffcd3d",
        "sprout:offset(0,1) 0x9cc406057b316335",
        "sprout:swap(dusk) 0x526bcf94e6169a7f",
    ];
    assert_eq!(sums, expected);
    let sheet = doc.preview(&PreviewOptions::default()).unwrap();
    assert_eq!(
        checksum(&sheet),
        0xe378_7ca4_e44a_770c,
        "{:#018x}",
        checksum(&sheet)
    );
}

#[test]
fn golden_tiles() {
    let doc: Document = TILES.parse().unwrap();
    let tileset = doc.tileset("ground").unwrap();
    let map = doc.tilemap("level").unwrap();
    let sums = [
        checksum(&tileset.render()),
        checksum(&map.render(tileset).unwrap()),
        checksum(&doc.preview(&PreviewOptions::default()).unwrap()),
    ];
    assert_eq!(
        sums,
        [
            0xab9c_431d_e5ce_847b,
            0xe2cf_f3a4_a5b4_7251,
            0x80d6_236f_6e83_83fd
        ],
        "{sums:#018x?}"
    );
}

#[test]
fn golden_textures() {
    let doc: Document = include_str!("../examples/textures.spritebox")
        .parse()
        .unwrap();
    let sheet = doc.preview(&PreviewOptions::default()).unwrap();
    assert_eq!(
        checksum(&sheet),
        0xee68_0f6f_e391_11a8,
        "{:#018x}",
        checksum(&sheet)
    );
}
