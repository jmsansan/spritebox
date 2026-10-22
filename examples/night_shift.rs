//! Original pixel art, authored as palette-key grids and rendered with spritebox.
//! cargo run --release --example night_shift
use spritebox::{Document, Image, png};
use std::{fmt::Write, path::PathBuf};

const W: i32 = 480;
const H: i32 = 300;
const FRAMES: i32 = 96;
const PALETTE: &[(char, &str)] = &[
    ('.', "transparent"),
    ('0', "#080d1c"),
    ('1', "#10162b"),
    ('2', "#181d36"),
    ('3', "#262640"),
    ('4', "#37304b"),
    ('5', "#49344f"),
    ('6', "#65425a"),
    ('7', "#8c5262"),
    ('8', "#ba666e"),
    ('9', "#ed8e7b"),
    ('a', "#ffb08a"),
    ('b', "#ffe3b2"),
    ('c', "#fc725a"),
    ('d', "#c94756"),
    ('e', "#7c304a"),
    ('f', "#153442"),
    ('g', "#20505b"),
    ('h', "#2a7277"),
    ('i', "#3e9e9c"),
    ('j', "#78d9c4"),
    ('k', "#b9f4d5"),
    ('l', "#f7f2d0"),
    ('m', "#273a50"),
    ('n', "#42516b"),
    ('o', "#6a7793"),
    ('p', "#9cacc0"),
    ('q', "#dd995b"),
    ('r', "#b46b47"),
    ('s', "#203c5570"),
    ('t', "#7cbacb58"),
    ('u', "#b9f4d530"),
    ('v', "#ff9b6b38"),
    ('w', "#78d9c418"),
    ('x', "#3e9e9c50"),
    ('y', "#3e9e9c34"),
    ('z', "#3e9e9c20"),
];

#[derive(Clone)]
struct Canvas {
    w: i32,
    h: i32,
    pixels: Vec<char>,
}
impl Canvas {
    fn new(w: i32, h: i32, key: char) -> Self {
        Self {
            w,
            h,
            pixels: vec![key; (w * h) as usize],
        }
    }
    fn put(&mut self, x: i32, y: i32, c: char) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.pixels[(y * self.w + x) as usize] = c;
        }
    }
    fn get(&self, x: i32, y: i32) -> char {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.pixels[(y * self.w + x) as usize]
        } else {
            '.'
        }
    }
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: char) {
        for yy in y.max(0)..(y + h).min(self.h) {
            for xx in x.max(0)..(x + w).min(self.w) {
                self.put(xx, yy, c);
            }
        }
    }
    fn line(&mut self, mut x: i32, mut y: i32, x1: i32, y1: i32, c: char) {
        let dx = (x1 - x).abs();
        let sx = if x < x1 { 1 } else { -1 };
        let dy = -(y1 - y).abs();
        let sy = if y < y1 { 1 } else { -1 };
        let mut e = dx + dy;
        loop {
            self.put(x, y, c);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = e * 2;
            if e2 >= dy {
                e += dy;
                x += sx;
            }
            if e2 <= dx {
                e += dx;
                y += sy;
            }
        }
    }
    fn poly(&mut self, points: &[(i32, i32)], c: char) {
        let lo = points.iter().map(|p| p.1).min().unwrap();
        let hi = points.iter().map(|p| p.1).max().unwrap();
        for y in lo.max(0)..hi.min(self.h) {
            let mut cuts = Vec::new();
            for i in 0..points.len() {
                let (x0, y0) = points[i];
                let (x1, y1) = points[(i + 1) % points.len()];
                if (y0 <= y && y1 > y) || (y1 <= y && y0 > y) {
                    cuts.push(x0 + (y - y0) * (x1 - x0) / (y1 - y0));
                }
            }
            cuts.sort_unstable();
            for pair in cuts.chunks_exact(2) {
                self.rect(pair[0], y, pair[1] - pair[0] + 1, 1, c);
            }
        }
    }
    fn ellipse(&mut self, cx: i32, cy: i32, rx: i32, ry: i32, c: char) {
        for y in -ry..=ry {
            for x in -rx..=rx {
                if x * x * ry * ry + y * y * rx * rx <= rx * rx * ry * ry {
                    self.put(cx + x, cy + y, c);
                }
            }
        }
    }
    fn ring(&mut self, cx: i32, cy: i32, r: i32, c: char) {
        let mut x = r;
        let mut y = 0;
        let mut e = 1 - r;
        while x >= y {
            for (a, b) in [
                (x, y),
                (y, x),
                (-y, x),
                (-x, y),
                (-x, -y),
                (-y, -x),
                (y, -x),
                (x, -y),
            ] {
                self.put(cx + a, cy + b, c);
            }
            y += 1;
            if e < 0 {
                e += 2 * y + 1;
            } else {
                x -= 1;
                e += 2 * (y - x) + 1;
            }
        }
    }
    fn text(&mut self, x: i32, y: i32, text: &str, c: char, scale: i32) {
        let mut at = x;
        for ch in text.chars() {
            for (yy, row) in glyph(ch).iter().enumerate() {
                for xx in 0..5 {
                    if row & (1 << (4 - xx)) != 0 {
                        self.rect(at + xx * scale, y + yy as i32 * scale, scale, scale, c);
                    }
                }
            }
            at += 6 * scale;
        }
    }
    fn source(&self, name: &str, out: &mut String) {
        writeln!(out, "\nsprite {name} {}x{} palette=night", self.w, self.h).unwrap();
        for row in self.pixels.chunks_exact(self.w as usize) {
            out.push_str("  ");
            out.extend(row);
            out.push('\n');
        }
    }
}
fn glyph(c: char) -> [u8; 7] {
    match c {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [31, 4, 4, 4, 4, 4, 31],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [6, 8, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 2, 12],
        ':' => [0, 4, 4, 0, 4, 4, 0],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        _ => [0; 7],
    }
}
fn hash(mut n: u32) -> u32 {
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb352d);
    n ^= n >> 15;
    n = n.wrapping_mul(0x846ca68b);
    n ^ (n >> 16)
}
fn pick(seed: u32, n: i32) -> i32 {
    (hash(seed) % n as u32) as i32
}

fn background() -> Canvas {
    let mut c = Canvas::new(W, H, '1');
    // A restrained, stepped atmosphere; the moon is the single warm focal mass.
    for (y, h, k) in [
        (0, 35, '1'),
        (35, 32, '2'),
        (67, 28, '3'),
        (95, 31, '4'),
        (126, 39, '5'),
    ] {
        c.rect(0, y, W, h, k);
    }
    for i in 0..165u32 {
        let x = pick(i * 41 + 1, W);
        let y = pick(i * 41 + 2, 151);
        if pick(i + 3, 5) == 0 {
            c.put(x, y, '6');
        } else {
            c.put(x, y, '4');
        }
    }
    c.ellipse(215, 78, 57, 57, '7');
    c.ellipse(215, 77, 54, 54, 'c');
    for y in 25..132 {
        for x in 160..271 {
            let old = c.get(x, y);
            if old == 'c' {
                let k = if y < 51 {
                    'a'
                } else if y < 82 {
                    '9'
                } else if y < 103 {
                    'c'
                } else {
                    'd'
                };
                c.put(x, y, k);
            }
        }
    }
    // Horizontal interruptions preserve the pixel texture of the moon.
    for (y, w) in [(102, 103), (109, 98), (117, 86), (124, 67)] {
        c.rect(215 - w / 2, y, w, 2, '5');
    }
    c.rect(141, 43, 31, 2, '3');
    c.rect(124, 46, 61, 1, '3');
    c.rect(251, 64, 63, 2, '4');
    c.rect(268, 67, 63, 1, '4');
    // Far skyline: deliberately varied roofs, setbacks and window rhythms.
    for i in 0..30u32 {
        let x = i as i32 * 18 - 15;
        let w = 13 + pick(i + 80, 13);
        let y = 78 + pick(i + 90, 65);
        c.rect(x, y, w, 172 - y, '3');
        c.rect(x + 2, y - 3, w - 5, 3, '3');
        if i % 3 == 0 {
            c.line(x + w / 2, y - 12, x + w / 2, y, '3');
        }
        for yy in (y + 7..166).step_by(7) {
            for xx in (x + 3..x + w - 2).step_by(5) {
                if pick((xx * 31 + yy * 7).unsigned_abs(), 4) == 0 {
                    c.rect(xx, yy, 2, 2, if i % 4 == 0 { '8' } else { '6' });
                }
            }
        }
    }
    // Central blade tower, silhouetted against the moon.
    c.poly(
        &[
            (199, 137),
            (201, 59),
            (207, 49),
            (208, 20),
            (211, 13),
            (214, 51),
            (221, 59),
            (225, 137),
        ],
        '2',
    );
    c.rect(206, 58, 10, 82, '1');
    c.line(210, 29, 210, 115, '5');
    c.line(211, 30, 211, 52, '8');
    c.rect(209, 15, 3, 2, 'c');
    c.rect(199, 65, 23, 3, '4');
    c.rect(199, 96, 25, 3, '4');
    for y in (62..135).step_by(7) {
        c.rect(203, y, 2, 2, '7');
        c.rect(217, y + 2, 2, 3, '6');
    }
    c.rect(241, 87, 25, 89, '2');
    c.rect(245, 80, 17, 7, '2');
    c.line(254, 64, 254, 80, '3');
    for y in (94..169).step_by(8) {
        c.rect(245, y, 4, 2, 'g');
        c.rect(254, y, 7, 2, '5');
    }
    // Mid-ground buildings frame a long, narrow street.
    for (x, y, w, h) in [
        (91, 102, 39, 86),
        (134, 116, 38, 74),
        (279, 95, 39, 97),
        (322, 119, 39, 73),
        (368, 87, 48, 108),
    ] {
        c.rect(x, y, w, h, '2');
        c.rect(x, y, w, 3, '4');
        c.rect(x + w - 5, y + 3, 5, h - 3, '1');
        for yy in (y + 9..y + h - 4).step_by(9) {
            for xx in (x + 5..x + w - 7).step_by(8) {
                let r = pick((xx * 73 + yy * 43) as u32, 10);
                c.rect(
                    xx,
                    yy,
                    4,
                    3,
                    if r < 2 {
                        'q'
                    } else if r < 5 {
                        'g'
                    } else {
                        '3'
                    },
                );
            }
        }
    }
    // Low cables and a distant neon torii.
    c.line(91, 128, 173, 142, '0');
    c.line(173, 142, 321, 119, '0');
    c.rect(283, 126, 27, 2, 'h');
    c.rect(287, 129, 2, 18, 'g');
    c.rect(304, 129, 2, 18, 'g');
    c.rect(0, 187, W, 113, '0');
    c.poly(&[(188, 188), (283, 188), (416, 300), (60, 300)], '1');
    c.poly(&[(207, 190), (260, 190), (322, 300), (116, 300)], '2');
    c.poly(&[(252, 191), (257, 191), (289, 300), (266, 300)], 'f');
    // The wet street is painted with broken, widening horizontal reflections.
    for i in 0..230u32 {
        let y = 195 + pick(i * 17 + 2, 91);
        let d = y - 184;
        let cx = if i % 3 == 0 {
            348
        } else if i % 3 == 1 {
            150
        } else {
            250
        };
        let spread = d / 2 + 4;
        let x = cx + pick(i * 17 + 3, spread * 2) - spread;
        let len = 2 + pick(i * 17 + 4, (d / 5).max(3));
        let col = if i % 3 == 0 {
            ['r', 'e', 'q', '5'][(i / 3 % 4) as usize]
        } else {
            ['f', 'g', 'h', 'm'][(i / 3 % 4) as usize]
        };
        c.rect(x, y, len, 1, col);
    }
    for (x, y, w, k) in [
        (207, 223, 12, 'h'),
        (216, 242, 20, 'g'),
        (225, 258, 30, 'h'),
        (201, 278, 53, 'g'),
        (335, 272, 29, 'q'),
        (325, 280, 47, 'r'),
        (146, 275, 23, 'i'),
    ] {
        c.rect(x, y, w, 1, k);
    }
    // Pavement perspective, drain and scattered leaves.
    for x in [-50, 49, 137, 325, 414, 533] {
        c.line(235 + (x - 235) / 8, 190, x, 299, '3');
    }
    for y in [206, 224, 252, 285] {
        c.line(0, y, W - 1, y, '1');
    }
    c.poly(&[(65, 273), (107, 273), (100, 278), (57, 278)], '0');
    for x in (65..105).step_by(5) {
        c.line(x, 274, x - 4, 277, '3');
    }
    for i in 0..30u32 {
        let x = pick(i * 13, W);
        let y = 245 + pick(i * 13 + 1, 37);
        c.rect(x, y, 2, 1, if i % 2 == 0 { '5' } else { 'g' });
    }
    c
}

fn train() -> Canvas {
    let mut c = Canvas::new(280, 31, '.');
    c.poly(
        &[
            (0, 9),
            (6, 3),
            (260, 3),
            (271, 7),
            (279, 19),
            (274, 27),
            (0, 27),
        ],
        '1',
    );
    c.poly(
        &[(3, 8), (8, 4), (259, 4), (269, 8), (274, 15), (3, 15)],
        'o',
    );
    c.rect(3, 8, 258, 15, 'p');
    c.rect(3, 10, 258, 10, 'f');
    for i in 0..16 {
        let x = 7 + i * 16;
        c.rect(x, 11, 12, 7, 'j');
        c.rect(x + 1, 11, 11, 2, 'k');
        c.rect(x + 1, 16, 11, 2, 'h');
        if i % 3 != 0 {
            c.rect(x + 4, 13, 3, 5, 'g');
            c.put(x + 5, 12, 'f');
        }
        c.rect(x + 12, 10, 2, 11, 'm');
    }
    for x in [69, 137, 205] {
        c.rect(x, 5, 3, 22, '0');
        c.line(x + 1, 8, x + 1, 25, 'n');
    }
    c.rect(1, 22, 270, 3, 'g');
    c.line(2, 22, 270, 22, 'j');
    c.line(3, 26, 271, 26, 'm');
    c.poly(&[(263, 10), (270, 10), (275, 18), (263, 18)], '0');
    c.line(265, 11, 270, 11, 'j');
    c.rect(274, 20, 4, 2, 'l');
    c.rect(2, 20, 2, 2, 'c');
    for x in [17, 50, 85, 118, 154, 187, 221, 253] {
        c.rect(x, 27, 9, 3, '0');
        c.rect(x + 1, 27, 7, 1, 'n');
    }
    c.rect(12, 5, 30, 3, '3');
    c.rect(15, 6, 23, 1, 'j');
    c.rect(220, 5, 25, 3, '3');
    c.rect(223, 6, 19, 1, 'a');
    c
}

fn frontage() -> Canvas {
    let mut c = Canvas::new(W, H, '.');
    // Track, steel ribs and columns. The moving train passes behind this layer.
    c.rect(0, 176, W, 4, '0');
    c.rect(0, 177, W, 1, 'n');
    c.rect(0, 181, W, 9, '2');
    c.line(0, 181, W - 1, 181, 'g');
    c.line(0, 189, W - 1, 189, '0');
    for x in (0..W).step_by(24) {
        c.line(x, 184, x + 12, 188, '4');
        c.line(x + 12, 188, x + 24, 184, '4');
    }
    for x in [111, 306, 397] {
        c.rect(x, 190, 8, 37, '1');
        c.rect(x + 1, 190, 2, 36, '3');
        c.rect(x - 2, 221, 12, 4, '0');
    }
    for x in (93..405).step_by(14) {
        c.rect(x, 169, 1, 7, '3');
    }
    c.line(92, 169, 411, 169, 'n');
    // Left brick apartment, pipes, warm windows and fire escape.
    c.rect(0, 51, 77, 218, '1');
    c.rect(0, 51, 73, 3, '4');
    c.rect(72, 54, 9, 215, '0');
    c.rect(2, 57, 5, 200, '3');
    for y in (60..260).step_by(8) {
        c.line(9, y, 68, y, '2');
        for x in (12..68).step_by(17) {
            let shift = if y % 16 == 12 { 8 } else { 0 };
            c.line(x + shift, y, x + shift, y + 6, '2');
        }
    }
    for y in [64, 99, 136, 174] {
        for x in [15, 44] {
            c.rect(x, y, 20, 25, '0');
            c.rect(x + 2, y + 2, 15, 19, 'r');
            c.rect(x + 3, y + 2, 13, 5, 'q');
            c.rect(
                x + 3,
                y + 8,
                13,
                12,
                if (x + y) % 3 == 0 { '5' } else { '8' },
            );
            c.rect(x + 9, y + 2, 2, 20, '2');
            c.rect(x + 2, y + 12, 15, 2, '2');
            c.rect(x - 1, y + 24, 22, 2, '4');
        }
    }
    c.rect(63, 62, 3, 180, '3');
    c.line(64, 63, 64, 238, 'n');
    c.rect(59, 234, 9, 5, '2');
    c.rect(8, 193, 48, 5, '0');
    c.line(8, 193, 55, 193, 'n');
    for x in [10, 21, 32, 43, 54] {
        c.line(x, 187, x, 193, 'm');
    }
    c.line(10, 187, 54, 187, 'm');
    c.line(20, 199, 51, 229, '0');
    c.line(16, 199, 47, 229, '0');
    for i in 0..10 {
        c.line(16 + i * 3, 199 + i * 3, 22 + i * 3, 199 + i * 3, 'm');
    }
    // Large cyan sign is a second focal point, balancing the warm restaurant.
    c.rect(75, 86, 23, 86, '0');
    c.rect(77, 87, 21, 83, 'g');
    c.rect(79, 89, 17, 79, '1');
    c.line(78, 89, 78, 167, 'j');
    c.line(96, 89, 96, 167, 'i');
    c.text(82, 94, "N", 'k', 2);
    c.text(82, 112, "0", 'k', 2);
    c.text(82, 130, "9", 'k', 2);
    c.rect(82, 151, 10, 2, 'j');
    c.rect(85, 156, 4, 3, 'j');
    c.line(72, 83, 90, 83, '3');
    c.line(90, 83, 90, 86, '3');
    c.rect(14, 219, 46, 31, '0');
    c.rect(16, 221, 42, 27, '2');
    c.text(20, 226, "OPEN", 'j', 1);
    c.text(22, 238, "24 H", 'o', 1);
    c.rect(0, 260, 83, 8, '0');
    c.line(0, 260, 82, 260, '4');
    c.rect(0, 267, 88, 4, '2');
    // Right apartment tower: a cutaway of dense, lived-in details.
    c.rect(421, 27, 59, 251, '0');
    c.rect(424, 30, 56, 244, '2');
    c.rect(424, 30, 56, 3, '5');
    c.rect(424, 33, 4, 242, '3');
    for y in (38..176).step_by(25) {
        for x in [432, 457] {
            c.rect(x, y, 17, 17, '0');
            c.rect(x + 2, y + 2, 12, 12, if y % 3 == 0 { '8' } else { 'g' });
            c.rect(x + 7, y + 2, 1, 12, '1');
            c.rect(x + 2, y + 8, 12, 1, '1');
            c.rect(x - 2, y + 17, 21, 2, '4');
        }
    }
    c.rect(433, 179, 46, 10, '3');
    for x in (436..477).step_by(6) {
        c.line(x, 181, x, 186, '0');
    }
    c.rect(462, 16, 3, 14, '3');
    c.line(454, 16, 476, 16, '4');
    c.rect(407, 56, 16, 104, 'e');
    c.rect(409, 58, 12, 100, '1');
    for (i, ch) in "HOTEL".chars().enumerate() {
        c.text(412, 65 + i as i32 * 18, &ch.to_string(), 'c', 1);
    }
    c.rect(411, 153, 8, 2, 'a');
    // Ramen stall. A broad warm pool of light sits low on the right.
    c.rect(326, 195, 99, 69, '0');
    c.rect(329, 202, 91, 53, 'r');
    c.rect(332, 205, 86, 46, '8');
    c.rect(337, 207, 77, 36, 'q');
    c.rect(340, 209, 70, 31, 'a');
    c.rect(343, 211, 64, 27, '9');
    c.rect(345, 212, 3, 25, 'r');
    c.rect(371, 210, 3, 29, 'r');
    c.rect(400, 210, 3, 29, 'r');
    // Shelves, bowls, hanging utensils, seated customers, and the cook.
    c.line(344, 218, 404, 218, 'q');
    for x in (350..399).step_by(9) {
        c.rect(x, 214, 4, 3, 'b');
        c.line(x, 213, x + 3, 213, 'r');
    }
    for x in [352, 360, 385, 393] {
        c.line(x, 220, x, 225, 'r');
        c.rect(x - 1, 225, 3, 2, 'r');
    }
    c.rect(382, 225, 8, 14, 'l');
    c.rect(381, 222, 9, 3, 'b');
    c.rect(384, 225, 5, 5, '9');
    c.rect(381, 232, 10, 7, 'b');
    c.rect(389, 231, 5, 2, 'l');
    c.ellipse(352, 234, 4, 4, '2');
    c.rect(347, 238, 10, 9, '2');
    c.rect(346, 245, 13, 2, '0');
    c.ellipse(402, 234, 4, 4, '4');
    c.rect(398, 238, 9, 9, '3');
    c.rect(396, 245, 13, 2, '0');
    c.rect(334, 240, 81, 4, 'b');
    c.rect(332, 244, 85, 3, 'r');
    c.rect(335, 247, 78, 11, '5');
    c.line(336, 247, 411, 247, '8');
    for x in [362, 392] {
        c.rect(x, 238, 7, 2, 'l');
        c.rect(x + 1, 240, 5, 1, 'c');
    }
    for x in [347, 399] {
        c.rect(x, 251, 12, 3, '0');
        c.rect(x + 3, 254, 2, 10, '3');
        c.rect(x + 8, 254, 2, 10, '3');
    }
    c.poly(&[(319, 201), (327, 186), (420, 186), (432, 201)], '0');
    c.poly(&[(322, 198), (329, 188), (418, 188), (428, 198)], 'd');
    for x in (328..420).step_by(14) {
        c.poly(&[(x, 188), (x + 6, 188), (x + 10, 198), (x + 2, 198)], '9');
    }
    c.line(323, 199, 429, 199, 'r');
    c.rect(325, 200, 102, 3, 'e');
    for x in (325..426).step_by(7) {
        c.rect(x, 202, 5, 3, 'e');
    }
    // The sign has its own metal frame and brackets.
    c.rect(337, 170, 75, 15, '0');
    c.rect(339, 171, 71, 12, 'e');
    c.rect(340, 172, 69, 10, 'd');
    c.text(345, 174, "RAMEN 09", 'b', 1);
    c.rect(344, 185, 2, 3, '0');
    c.rect(403, 185, 2, 3, '0');
    // Lantern, curbside vending machine and little planters.
    c.line(321, 204, 321, 213, '3');
    c.ellipse(321, 219, 6, 8, 'd');
    c.ellipse(320, 218, 4, 6, 'c');
    c.line(321, 213, 321, 225, '9');
    c.rect(318, 211, 6, 2, '0');
    c.rect(318, 226, 6, 2, '0');
    c.rect(434, 210, 26, 50, '0');
    c.rect(436, 212, 22, 45, '4');
    c.rect(438, 214, 18, 27, 'g');
    c.rect(439, 215, 16, 3, 'k');
    for y in [220, 227, 234] {
        for x in [440, 446, 452] {
            c.rect(x, y, 3, 4, if (x + y) % 3 == 0 { 'c' } else { 'j' });
            c.rect(x, y, 3, 1, 'l');
        }
    }
    c.rect(439, 244, 11, 7, '1');
    c.rect(452, 244, 3, 2, 'c');
    c.rect(439, 253, 16, 2, '0');
    c.rect(416, 252, 12, 11, 'r');
    c.rect(415, 251, 14, 3, 'q');
    for (x, y) in [(419, 247), (425, 245), (416, 244), (424, 240)] {
        c.line(422, 253, x, y, 'g');
        c.ellipse(x, y, 3, 2, 'h');
    }
    // A bicycle and a human-scale detail in the quieter side of the composition.
    c.ring(127, 263, 9, '0');
    c.ring(127, 263, 7, 'n');
    c.ring(153, 263, 9, '0');
    c.ring(153, 263, 7, 'n');
    for (x, y, x1, y1) in [
        (127, 263, 137, 249),
        (137, 249, 145, 263),
        (145, 263, 127, 263),
        (137, 249, 149, 249),
        (149, 249, 153, 263),
        (149, 249, 145, 263),
    ] {
        c.line(x, y, x1, y1, 'd');
    }
    c.line(137, 249, 136, 245, 'n');
    c.rect(132, 244, 9, 2, '0');
    c.line(149, 249, 150, 242, 'n');
    c.line(150, 242, 155, 242, 'n');
    // Courier with a coral umbrella: the eye's resting point in the street.
    c.ellipse(236, 270, 12, 2, '0');
    c.line(235, 244, 232, 268, '0');
    c.line(238, 246, 242, 268, '0');
    c.rect(230, 266, 5, 3, '0');
    c.rect(241, 267, 6, 2, '0');
    c.poly(&[(231, 233), (240, 233), (242, 249), (230, 249)], '2');
    c.rect(231, 236, 3, 11, 'm');
    c.rect(233, 230, 5, 5, 'q');
    c.rect(232, 228, 7, 4, '0');
    c.line(241, 236, 245, 239, '3');
    c.line(245, 222, 245, 242, 'n');
    c.line(245, 242, 242, 244, 'n');
    c.poly(
        &[(213, 228), (219, 220), (232, 213), (244, 216), (258, 228)],
        '0',
    );
    c.poly(
        &[(215, 226), (221, 220), (232, 215), (242, 218), (255, 226)],
        'd',
    );
    c.poly(&[(215, 226), (231, 216), (229, 226)], 'c');
    c.poly(&[(230, 226), (233, 216), (242, 218), (246, 226)], '9');
    c.line(215, 227, 228, 227, 'c');
    c.line(231, 227, 244, 227, 'd');
    c.line(247, 227, 255, 227, 'c');
    c.put(232, 213, 'a');
    // A small cat waits in the restaurant's reflected light.
    c.ellipse(368, 269, 6, 3, '0');
    c.ellipse(368, 265, 4, 4, '2');
    c.poly(
        &[(365, 263), (364, 258), (368, 261), (372, 258), (372, 265)],
        '2',
    );
    c.put(366, 263, 'q');
    c.put(370, 263, 'q');
    c.line(372, 267, 377, 267, '2');
    c.line(377, 267, 379, 264, '2');
    // Fine aerial wires cross open sky, staying out of the moon's centre.
    for (x, y, x1, y1) in [
        (0, 30, 94, 61),
        (94, 61, 170, 49),
        (307, 41, 420, 19),
        (317, 44, 479, 34),
        (0, 78, 69, 84),
    ] {
        c.line(x, y, x1, y1, '0');
    }
    for x in [28, 51, 367, 391] {
        c.line(x, 0, x, if x < 100 { 39 } else { 30 }, '0');
    }
    c
}

fn overlay(image: &mut Image, source: &Image, x: i32, y: i32) {
    for sy in 0..source.height() as i32 {
        let yy = sy + y;
        if yy < 0 || yy >= image.height() as i32 {
            continue;
        }
        for sx in 0..source.width() as i32 {
            let xx = sx + x;
            if xx < 0 || xx >= image.width() as i32 {
                continue;
            }
            let col = source.pixel(sx as u32, sy as u32).unwrap();
            if col.a > 0 {
                let under = image.pixel(xx as u32, yy as u32).unwrap();
                image.set_pixel(xx as u32, yy as u32, col.over(under));
            }
        }
    }
}
fn scale(image: &Image, n: u32) -> spritebox::Result<Image> {
    let mut out = Image::new(image.width() * n, image.height() * n)?;
    for y in 0..out.height() {
        for x in 0..out.width() {
            out.set_pixel(x, y, image.pixel(x / n, y / n).unwrap());
        }
    }
    Ok(out)
}
fn effects(frame: i32) -> Canvas {
    let mut c = Canvas::new(W, H, '.');
    // Periodic rain; its entire state returns after exactly 96 frames.
    for i in 0..225u32 {
        let y = (pick(i * 29 + 2, 384) + frame * 4) % 384 - 42;
        let x = (pick(i * 29 + 1, W) - y / 3).rem_euclid(W);
        let len = 3 + pick(i * 29 + 3, 6);
        c.line(
            x,
            y,
            x - len / 3,
            y + len,
            if i % 7 == 0 { 't' } else { 's' },
        );
    }
    // Broken puddle rings rather than a full, distracting reflection mirror.
    for i in 0..27u32 {
        let phase = (frame + pick(i + 401, 32)) % 32;
        let x = 96 + pick(i + 500, 326);
        let y = 225 + pick(i + 600, 56);
        if phase < 16 {
            let r = phase / 4 + 1;
            c.line(x - r, y, x + r, y, if phase < 8 { 'g' } else { 'f' });
            c.put(x - r - 1, y - 1, 'f');
            c.put(x + r + 1, y - 1, 'f');
        }
    }
    // Slow wisps escaping the restaurant pot; offsets wrap out of sight.
    for i in 0..5 {
        let t = (frame + i * 19) % FRAMES;
        let y = 234 - t / 5;
        let x = 389 + ((t as f64 * 0.15).sin() * 3.) as i32;
        c.ellipse(x, y, 1 + t / 32, 2, 'u');
    }
    // Lantern light and a subtle heartbeat in the cyan sign.
    c.ellipse(321, 219, 10, 13, 'v');
    if frame % 48 < 43 {
        c.rect(76, 91, 1, 75, 'w');
        c.rect(98, 91, 2, 75, 'w');
    }
    c.rect(12, 284, 112, 1, 'g');
    c.text(13, 289, "NIGHT SHIFT", 'p', 1);
    c.text(407, 289, "02:17", 'h', 1);
    c
}

fn reflection(frame: i32) -> Canvas {
    let mut c = Canvas::new(W, H, '.');
    let x = (frame * 8 + 396) % 768 - 280;
    for row in 0..7 {
        let yy = 197 + row * 7;
        for xx in (x.max(98)..(x + 260).min(417)).step_by(11) {
            c.rect(
                xx,
                yy,
                3 + row,
                1,
                if row < 2 {
                    'x'
                } else if row < 4 {
                    'y'
                } else {
                    'z'
                },
            );
        }
    }
    c
}

fn render_frame(
    frame: i32,
    city: &Image,
    train: &Image,
    street: &Image,
    palette: &str,
) -> Result<Image, Box<dyn std::error::Error>> {
    let mut image = city.clone();
    overlay(
        &mut image,
        &render_canvas(&reflection(frame), palette)?,
        0,
        0,
    );
    overlay(&mut image, train, (frame * 8 + 396) % 768 - 280, 145);
    overlay(&mut image, street, 0, 0);
    overlay(&mut image, &render_canvas(&effects(frame), palette)?, 0, 0);
    Ok(image)
}
fn source_header() -> String {
    let mut s = String::from(
        "; NIGHT SHIFT / an original spritebox scene.\n; All shapes are authored procedurally, without a reference bitmap.\npalette night\n",
    );
    for (key, value) in PALETTE {
        writeln!(s, "  {key} {value}").unwrap();
    }
    s
}
fn render_canvas(canvas: &Canvas, palette: &str) -> Result<Image, Box<dyn std::error::Error>> {
    let mut s = palette.to_string();
    canvas.source("effect", &mut s);
    let d = Document::parse(&s)?;
    Ok(d.sprite("effect").unwrap().render())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = root.join("out/night-shift");
    let frames = out.join("frames");
    std::fs::create_dir_all(&frames)?;
    std::fs::create_dir_all(root.join("examples/night_shift"))?;
    let palette = source_header();
    let mut source = palette.clone();
    background().source("city", &mut source);
    train().source("last_train", &mut source);
    frontage().source("street", &mut source);
    // A clean, editable hero frame uses spritebox's native layer composition.
    let hero_x = 116;
    let mut train_hero = Canvas::new(W, H, '.');
    let train_pixels = train();
    for y in 0..train_pixels.h {
        for x in 0..train_pixels.w {
            train_hero.put(x + hero_x, y + 145, train_pixels.get(x, y));
        }
    }
    train_hero.source("train_at_station", &mut source);
    reflection(0).source("reflections", &mut source);
    effects(0).source("rain", &mut source);
    source.push_str("\nsprite night_shift 480x300 palette=night\n  layer city = city\n  layer reflections = reflections\n  layer train = train_at_station\n  layer street = street\n  layer atmosphere = rain\n");
    let art_path = root.join("examples/night_shift/night_shift.spritebox");
    std::fs::write(&art_path, &source)?;
    let doc = Document::parse(&source)?;
    let city = doc.sprite("city").unwrap().render();
    let train = doc.sprite("last_train").unwrap().render();
    let street = doc.sprite("street").unwrap().render();
    let hero = doc.sprite("night_shift").unwrap().render();
    let first = render_frame(0, &city, &train, &street, &palette)?;
    if hero != first {
        return Err("The editable layered sprite differs from animation frame zero".into());
    }
    if first != render_frame(FRAMES, &city, &train, &street, &palette)? {
        return Err("The animation does not close seamlessly".into());
    }
    png::write_file(out.join("night-shift-native.png"), &hero)?;
    png::write_file(out.join("night-shift.png"), &scale(&hero, 3)?)?;
    let mut sheet = Image::new(W as u32 * 2, H as u32 * 2)?;
    for f in 0..FRAMES {
        let image = render_frame(f, &city, &train, &street, &palette)?;
        png::write_file(frames.join(format!("{f:03}.png")), &scale(&image, 3)?)?;
        if f == 0 {
            png::write_file(out.join("night-shift.png"), &scale(&image, 3)?)?;
            png::write_file(out.join("night-shift-native.png"), &image)?;
        }
        if f % 24 == 0 {
            sheet.copy_from(&image, i64::from((f / 24) % 2 * W), i64::from((f / 48) * H));
            println!("Rendered {f}/{FRAMES} frames");
        }
    }
    png::write_file(out.join("contact-sheet.png"), &sheet)?;
    if png::read_file(out.join("night-shift-native.png"))? != hero {
        return Err("PNG round-trip mismatch".into());
    }
    // Keep a transparent sheet of useful original assets as well as the scene.
    png::write_file(out.join("train.png"), &scale(&train, 4)?)?;
    println!("Art: {}\nFrames: {}", art_path.display(), frames.display());
    Ok(())
}
