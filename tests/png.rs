//! PNG round trips, and files written by other tools (see tests/fixtures/README.md).

use spritebox::{Color, ErrorCode, Image, Limits, png};

const W: u32 = 13;
const H: u32 = 7;
const ALPHAS: [u8; 5] = [255, 0, 128, 64, 200];
const PALETTE: [[u8; 3]; 12] = [
    [0x1a, 0x1c, 0x2c],
    [0x5d, 0x27, 0x5d],
    [0xb1, 0x3e, 0x53],
    [0xef, 0x7d, 0x57],
    [0xff, 0xcd, 0x75],
    [0xa7, 0xf0, 0x70],
    [0x38, 0xb7, 0x64],
    [0x25, 0x71, 0x79],
    [0x29, 0x36, 0x6f],
    [0x3b, 0x5d, 0xc9],
    [0x41, 0xa6, 0xf6],
    [0x73, 0xef, 0xf7],
];

// The patterns of scripts/make-png-fixtures.py.
fn rgba(x: u32, y: u32) -> [u8; 4] {
    [
        (x * 19 % 256) as u8,
        (y * 36 % 256) as u8,
        (x * y * 7 % 256) as u8,
        ALPHAS[((x + 2 * y) % 5) as usize],
    ]
}
fn gray(x: u32, y: u32) -> u8 {
    ((x * 17 + y * 29) % 256) as u8
}
fn wide(x: u32, y: u32, k: usize) -> u32 {
    [
        x * 4099 + y * 9001,
        x * 1237 + y * 30011,
        x * y * 613,
        x * 7919 + y * 104729,
    ][k] % 65536
}
fn narrow(v: u32) -> u8 {
    ((v * 255 + 32767) / 65535) as u8
}
fn indexed(x: u32, y: u32) -> [u8; 3] {
    PALETTE[((x * 3 + y) % 12) as usize]
}

fn fixture(name: &str) -> Image {
    let path = format!("{}/tests/fixtures/png/{name}", env!("CARGO_MANIFEST_DIR"));
    png::read_file(&path).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn assert_pattern(name: &str, expected: impl Fn(u32, u32) -> [u8; 4]) {
    let image = fixture(name);
    assert_eq!((image.width(), image.height()), (W, H), "{name}");
    for y in 0..H {
        for x in 0..W {
            assert_eq!(
                image.pixel(x, y).unwrap().to_array(),
                expected(x, y),
                "{name} at ({x}, {y})"
            );
        }
    }
}

#[test]
fn reads_files_written_by_ffmpeg() {
    assert_pattern("ffmpeg-rgba.png", rgba);
    assert_pattern("ffmpeg-rgba-pred-none.png", rgba);
    assert_pattern("ffmpeg-rgb24.png", |x, y| {
        let [r, g, b, _] = rgba(x, y);
        [r, g, b, 255]
    });
    assert_pattern("ffmpeg-gray.png", |x, y| {
        let v = gray(x, y);
        [v, v, v, 255]
    });
    assert_pattern("ffmpeg-ya8.png", |x, y| {
        let v = gray(x, y);
        [v, v, v, rgba(x, y)[3]]
    });
    assert_pattern("ffmpeg-gray16be.png", |x, y| {
        let v = narrow(wide(x, y, 0));
        [v, v, v, 255]
    });
    assert_pattern("ffmpeg-ya16be.png", |x, y| {
        let v = narrow(wide(x, y, 0));
        [v, v, v, narrow(wide(x, y, 3))]
    });
    assert_pattern("ffmpeg-rgb48be.png", |x, y| {
        [
            narrow(wide(x, y, 0)),
            narrow(wide(x, y, 1)),
            narrow(wide(x, y, 2)),
            255,
        ]
    });
    assert_pattern("ffmpeg-rgba64be.png", |x, y| {
        [
            narrow(wide(x, y, 0)),
            narrow(wide(x, y, 1)),
            narrow(wide(x, y, 2)),
            narrow(wide(x, y, 3)),
        ]
    });
    assert_pattern("ffmpeg-monob.png", |x, y| {
        let v = if (x + y) % 3 == 0 { 255 } else { 0 };
        [v, v, v, 255]
    });
    assert_pattern("ffmpeg-pal8.png", |x, y| {
        let [r, g, b] = indexed(x, y);
        [r, g, b, 255]
    });
}

#[test]
fn reads_files_written_by_macos_image_io() {
    assert_pattern("sips-rgb24.png", |x, y| {
        let [r, g, b, _] = rgba(x, y);
        [r, g, b, 255]
    });
    // Image I/O stores premultiplied colors, so translucent colors may move by a step
    // and transparent pixels lose their color.
    let image = fixture("sips-rgba.png");
    for y in 0..H {
        for x in 0..W {
            let got = image.pixel(x, y).unwrap().to_array();
            let want = rgba(x, y);
            assert_eq!(got[3], want[3], "alpha at ({x}, {y})");
            if want[3] == 255 {
                assert_eq!(got, want, "({x}, {y})");
            } else if want[3] > 0 {
                for k in 0..3 {
                    let d = (got[k] as i32 - want[k] as i32).abs();
                    assert!(
                        d <= 255 / want[3] as i32 + 1,
                        "({x}, {y}): {got:?} {want:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn reads_interlaced_and_keyed_files() {
    assert_pattern("zlib-rgba-adam7.png", rgba);
    assert_pattern("zlib-gray2-trns-adam7.png", |x, y| {
        let level = (x + 2 * y) % 4;
        let v = (level * 85) as u8;
        [v, v, v, if level == 1 { 0 } else { 255 }]
    });
    let key = rgba(4, 3);
    assert_pattern("zlib-rgb-trns.png", |x, y| {
        let [r, g, b, _] = rgba(x, y);
        let keyed = [r, g, b] == [key[0], key[1], key[2]];
        [r, g, b, if keyed { 0 } else { 255 }]
    });
    assert_pattern("zlib-indexed4-trns.png", |x, y| {
        let i = ((x * 3 + y) % 12) as usize;
        let [r, g, b] = PALETTE[i];
        [r, g, b, [0, 128, 255, 7].get(i).copied().unwrap_or(255)]
    });
}

fn sample_image(width: u32, height: u32, colors: u32) -> Image {
    let mut image = Image::new(width, height).unwrap();
    for y in 0..height {
        for x in 0..width {
            let k = (x * 7 + y * 13) % colors;
            let alpha = if k % 5 == 0 {
                0
            } else if k % 3 == 0 {
                100
            } else {
                255
            };
            image.set_pixel(
                x,
                y,
                Color::rgba((k * 37) as u8, ((k >> 8) * 90) as u8, (k * 11) as u8, alpha),
            );
        }
    }
    image
}

#[test]
fn round_trips() {
    for (w, h) in [(1, 1), (2, 3), (9, 1), (1, 9), (64, 40), (301, 17)] {
        for colors in [1, 2, 4, 7, 16, 255, 256, 3000] {
            let image = sample_image(w, h, colors);
            let bytes = png::encode(&image);
            assert_eq!(
                png::decode(&bytes).unwrap(),
                image,
                "{w}x{h}, {colors} colors"
            );
            assert_eq!(png::encode(&image), bytes, "deterministic");
        }
    }
}

#[test]
fn encodes_pixel_art_compactly() {
    let image = sample_image(256, 256, 12);
    let bytes = png::encode(&image);
    // Indexed at 4 bits a pixel, a repeating pattern compresses to almost nothing.
    assert!(bytes.len() < 2_000, "{} bytes", bytes.len());
    let rgba = sample_image(256, 256, 3000);
    assert!(png::encode(&rgba).len() < 256 * 256 * 4 / 4);
}

#[test]
fn rejects_malformed_files() {
    let good = png::encode(&sample_image(20, 10, 40));
    assert_eq!(
        png::decode(b"GIF89a").unwrap_err().code,
        ErrorCode::UnsupportedFormat
    );
    for cut in [0, 8, 20, 33, good.len() / 2, good.len() - 1] {
        assert!(png::decode(&good[..cut]).is_err(), "cut at {cut}");
    }
    let mut bad_crc = good.clone();
    bad_crc[30] ^= 0x40;
    assert_eq!(
        png::decode(&bad_crc).unwrap_err().code,
        ErrorCode::DecodeError
    );
    // Every single-bit corruption is an error or a different image, never a panic.
    for i in 8..good.len() {
        for bit in [0, 3, 7] {
            let mut c = good.clone();
            c[i] ^= 1 << bit;
            let _ = png::decode(&c);
        }
    }
    let big = png::encode(&Image::new(300, 2).unwrap());
    let limits = Limits {
        max_image_side: 256,
        ..Limits::default()
    };
    assert_eq!(
        png::decode_with(&big, &limits).unwrap_err().code,
        ErrorCode::LimitExceeded
    );
}

/// Our files decoded by FFmpeg give our pixels. Opt in with SPRITEBOX_FFMPEG_TESTS=1.
#[test]
fn ffmpeg_reads_our_files() {
    if std::env::var_os("SPRITEBOX_FFMPEG_TESTS").is_none() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("spritebox-ffmpeg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for colors in [2, 12, 256, 3000] {
        let image = sample_image(61, 23, colors);
        let path = dir.join(format!("{colors}.png"));
        png::write_file(&path, &image).unwrap();
        let output = std::process::Command::new("ffmpeg")
            .args(["-loglevel", "error", "-i"])
            .arg(&path)
            .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
            .output()
            .expect("ffmpeg runs");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, image.pixels(), "{colors} colors");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
