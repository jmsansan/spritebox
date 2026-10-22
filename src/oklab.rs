//! OKLab, in fixed point: the same results on every platform, for lint and ramps.

use crate::Color;

/// A color in OKLab, in ten-thousandths: `l` from 0 (black) to 10000 (white), and
/// `a` and `b` about -4000 to 4000.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Lab {
    pub l: i32,
    pub a: i32,
    pub b: i32,
}

/// One in the fixed point of the steps between sRGB and OKLab.
const ONE: i64 = 1 << 20;

/// The linear value of each sRGB channel value, times [`ONE`].
#[rustfmt::skip]
pub(crate) const SRGB_TO_LINEAR: [u32; 256] = [
    0, 318, 637, 955, 1273, 1591, 1910, 2228,
    2546, 2864, 3183, 3509, 3855, 4220, 4605, 5009,
    5433, 5878, 6343, 6828, 7335, 7863, 8413, 8984,
    9578, 10193, 10832, 11492, 12176, 12883, 13614, 14368,
    15145, 15947, 16773, 17624, 18499, 19399, 20324, 21274,
    22250, 23251, 24278, 25331, 26410, 27516, 28648, 29807,
    30993, 32205, 33445, 34713, 36008, 37331, 38681, 40060,
    41467, 42903, 44367, 45860, 47381, 48932, 50512, 52121,
    53760, 55428, 57127, 58855, 60613, 62402, 64221, 66071,
    67951, 69862, 71805, 73778, 75783, 77819, 79886, 81985,
    84117, 86280, 88475, 90702, 92962, 95254, 97579, 99937,
    102328, 104751, 107208, 109698, 112222, 114779, 117370, 119994,
    122653, 125345, 128072, 130833, 133628, 136458, 139323, 142222,
    145156, 148125, 151130, 154169, 157244, 160355, 163501, 166683,
    169900, 173154, 176443, 179769, 183131, 186530, 189964, 193436,
    196944, 200489, 204072, 207691, 211347, 215041, 218772, 222540,
    226346, 230190, 234071, 237991, 241948, 245944, 249978, 254050,
    258161, 262310, 266498, 270724, 274990, 279294, 283637, 288020,
    292442, 296903, 301404, 305944, 310523, 315143, 319802, 324502,
    329241, 334021, 338840, 343700, 348601, 353542, 358523, 363546,
    368609, 373713, 378858, 384044, 389271, 394539, 399849, 405201,
    410594, 416028, 421504, 427022, 432582, 438184, 443828, 449515,
    455243, 461014, 466827, 472683, 478582, 484523, 490507, 496534,
    502604, 508717, 514873, 521072, 527315, 533601, 539930, 546303,
    552720, 559181, 565685, 572234, 578826, 585462, 592143, 598868,
    605637, 612451, 619309, 626211, 633159, 640151, 647188, 654270,
    661397, 668569, 675786, 683048, 690356, 697709, 705108, 712552,
    720042, 727577, 735159, 742786, 750459, 758178, 765944, 773755,
    781613, 789517, 797468, 805465, 813509, 821599, 829736, 837920,
    846151, 854429, 862753, 871125, 879545, 888011, 896525, 905086,
    913695, 922351, 931055, 939807, 948606, 957453, 966349, 975292,
    984283, 993323, 1002411, 1011547, 1020731, 1029964, 1039246, 1048576,
];

/// Linear sRGB to LMS, times [`ONE`]; each row adds up to one.
const TO_LMS: [[i64; 3]; 3] = [
    [432246, 562385, 53945],
    [222197, 713765, 112614],
    [92592, 295404, 660581],
];

/// The cube roots of LMS to OKLab, times [`ONE`].
const TO_LAB: [[i64; 3]; 3] = [
    [220677, 832169, -4270],
    [2074082, -2546563, 472482],
    [27162, 820796, -847958],
];

impl Lab {
    /// The OKLab color of an sRGB color; alpha is ignored.
    pub fn from_color(c: Color) -> Lab {
        let linear = [c.r, c.g, c.b].map(|v| SRGB_TO_LINEAR[v as usize] as i64);
        let lms = TO_LMS.map(|row| {
            let sum: i64 = row.iter().zip(&linear).map(|(m, v)| m * v).sum();
            // The cube root of sum / ONE², times ONE.
            cube_root(((sum + ONE / 2) >> 20).max(0) as u64 * (1 << 40)) as i64
        });
        let [l, a, b] = TO_LAB.map(|row| {
            let sum: i64 = row.iter().zip(&lms).map(|(m, v)| m * v).sum();
            // From ONE² to ten-thousandths, rounded to nearest.
            let scaled = sum as i128 * 10000;
            let half = (ONE * ONE / 2) as i128;
            let rounded = if scaled >= 0 {
                (scaled + half) / (ONE * ONE) as i128
            } else {
                (scaled - half) / (ONE * ONE) as i128
            };
            rounded as i32
        });
        Lab { l, a, b }
    }
}

/// OKLab to the cube roots of LMS, times [`ONE`].
const FROM_LAB: [[i64; 3]; 3] = [
    [1048576, 415590, 226287],
    [1048576, -110689, -66956],
    [1048576, -93831, -1354221],
];

/// LMS to linear sRGB, times [`ONE`].
const FROM_LMS: [[i64; 3]; 3] = [
    [4274773, -3468387, 242190],
    [-1330054, 2736529, -357899],
    [-4400, -737588, 1790564],
];

/// The sine of each whole degree from 0 to 90, times [`ONE`].
#[rustfmt::skip]
const SIN: [i64; 91] = [
    0, 18300, 36595, 54878, 73145, 91389, 109606, 127789,
    145934, 164033, 182083, 200078, 218011, 235878, 253673, 271391,
    289027, 306574, 324028, 341383, 358634, 375776, 392803, 409711,
    426494, 443147, 459665, 476044, 492277, 508360, 524288, 540057,
    555661, 571095, 586356, 601438, 616338, 631049, 645568, 659890,
    674012, 687928, 701634, 715127, 728402, 741455, 754282, 766880,
    779244, 791370, 803256, 814897, 826289, 837430, 848316, 858943,
    869309, 879410, 889243, 898805, 908093, 917105, 925838, 934288,
    942454, 950333, 957922, 965219, 972223, 978930, 985339, 991448,
    997255, 1002758, 1007956, 1012847, 1017429, 1021701, 1025662, 1029311,
    1032646, 1035666, 1038371, 1040760, 1042832, 1044586, 1046022, 1047139,
    1047937, 1048416, 1048576,
];
/// The sine and cosine of `degrees`, times [`ONE`].
fn sin_cos(degrees: i64) -> (i64, i64) {
    let d = degrees.rem_euclid(360);
    let sin = |d: i64| match d {
        0..=90 => SIN[d as usize],
        91..=180 => SIN[(180 - d) as usize],
        181..=270 => -SIN[(d - 180) as usize],
        _ => -SIN[(360 - d) as usize],
    };
    (sin(d), sin((d + 90) % 360))
}

impl Lab {
    /// The sRGB color of an OKLab color, opaque; `None` when it is outside sRGB by
    /// more than rounding.
    pub fn to_color(self) -> Option<Color> {
        let lab = [self.l, self.a, self.b].map(|v| v as i64 * ONE / 10000);
        let lms = FROM_LAB.map(|row| {
            let root: i64 = row.iter().zip(&lab).map(|(m, v)| m * v).sum::<i64>() / ONE;
            // root³ / ONE², times ONE.
            (root as i128).pow(3) / (ONE as i128 * ONE as i128)
        });
        let linear = FROM_LMS.map(|row| {
            row.iter()
                .zip(&lms)
                .map(|(&m, &v)| m as i128 * v)
                .sum::<i128>()
                / ONE as i128
        });
        let slack = (ONE / 500) as i128;
        if linear
            .iter()
            .any(|&v| v < -slack || v > ONE as i128 + slack)
        {
            return None;
        }
        let [r, g, b] = linear.map(|v| to_srgb(v.clamp(0, ONE as i128) as i64));
        Some(Color::rgb(r, g, b))
    }

    /// The distance between two colors, in ten-thousandths: OKLab's ΔE.
    pub fn distance(self, other: Lab) -> i64 {
        let d = [self.l - other.l, self.a - other.a, self.b - other.b].map(|v| v as i64);
        isqrt((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]) as u64) as i64
    }

    /// The chroma, in ten-thousandths: how far the color is from grey.
    pub fn chroma(self) -> i64 {
        let (a, b) = (self.a as i64, self.b as i64);
        isqrt((a * a + b * b) as u64) as i64
    }
}

/// The sRGB channel value whose linear value is nearest `linear`, times [`ONE`].
fn to_srgb(linear: i64) -> u8 {
    let above = SRGB_TO_LINEAR.partition_point(|&v| (v as i64) < linear);
    if above == 0 {
        return 0;
    }
    if above == 256 {
        return 255;
    }
    let (low, high) = (
        SRGB_TO_LINEAR[above - 1] as i64,
        SRGB_TO_LINEAR[above] as i64,
    );
    if linear - low <= high - linear {
        (above - 1) as u8
    } else {
        above as u8
    }
}

/// The hue of yellow, toward which highlights turn, and of blue, toward which
/// shadows turn, as OKLab `(a, b)` directions.
const WARM: (i64, i64) = (130, 1000);
const COOL: (i64, i64) = (-190, -1000);

/// `count` colors from dark to light around `base`: lightness in even steps of OKLab
/// L, centred on the base's where it fits; the hue turned toward yellow in the
/// highlights and blue in the shadows, up to `shift` degrees at the ends; and chroma
/// easing toward both ends. Colors that sRGB cannot show lose chroma until it can.
pub(crate) fn ramp(base: Color, count: usize, shift: u32) -> Vec<Color> {
    let base = Lab::from_color(base);
    let n = count.max(2) as i64;
    let step = 1200.min(7600 / (n - 1));
    let span = step * (n - 1);
    let low = (base.l as i64 - span / 2).min(9700 - span).max(600);
    (0..n)
        .map(|i| {
            // Where the color sits, from -(n-1) at the dark end to n-1 at the light.
            let t = 2 * i - (n - 1);
            let reach = n - 1;
            let l = low + step * i;
            // Chroma: 100% in the middle, 70% at the ends.
            let ease = 100 - 30 * t.abs() / reach;
            let (mut a, mut b) = (base.a as i64 * ease / 100, base.b as i64 * ease / 100);
            let angle = (shift as i64 * t.abs() + reach / 2) / reach;
            if base.chroma() >= 100 && angle > 0 && t != 0 {
                (a, b) = turn((a, b), if t > 0 { WARM } else { COOL }, angle);
            }
            let mut lab = Lab {
                l: l as i32,
                a: a as i32,
                b: b as i32,
            };
            for _ in 0..24 {
                if let Some(c) = lab.to_color() {
                    return c;
                }
                lab.a = lab.a * 9 / 10;
                lab.b = lab.b * 9 / 10;
            }
            Lab { a: 0, b: 0, ..lab }.to_color().unwrap_or(Color::BLACK)
        })
        .collect()
}

/// `(a, b)` turned toward the direction `target` by `degrees`, stopping at it.
fn turn((a, b): (i64, i64), target: (i64, i64), degrees: i64) -> (i64, i64) {
    let cross = |(a, b): (i64, i64)| a * target.1 - b * target.0;
    let side = cross((a, b)).signum();
    if side == 0 {
        return (a, b);
    }
    let (sin, cos) = sin_cos(side * degrees);
    let turned = ((a * cos - b * sin) / ONE, (a * sin + b * cos) / ONE);
    if cross(turned).signum() == side {
        return turned;
    }
    // Past the target: stop on it, at the same chroma.
    let length = isqrt((a * a + b * b) as u64) as i64;
    let unit = isqrt((target.0 * target.0 + target.1 * target.1) as u64) as i64;
    (target.0 * length / unit, target.1 * length / unit)
}

/// The square root of `n`, rounded down.
fn isqrt(n: u64) -> u64 {
    let (mut low, mut high) = (0u64, 1 << 32);
    while high - low > 1 {
        let mid = (low + high) / 2;
        if (mid as u128) * (mid as u128) <= n as u128 {
            low = mid;
        } else {
            high = mid;
        }
    }
    low
}

/// The cube root of `n`, rounded to nearest.
fn cube_root(n: u64) -> u64 {
    // The largest y with y³ ≤ n; n < 2^64, so y < 2^22.
    let (mut low, mut high) = (0u64, 1 << 22);
    while high - low > 1 {
        let mid = (low + high) / 2;
        if (mid as u128).pow(3) <= n as u128 {
            low = mid;
        } else {
            high = mid;
        }
    }
    // Round up when n is past (low + 1/2)³, that is when 8n > (2·low + 1)³.
    if 8 * n as u128 > (2 * low as u128 + 1).pow(3) {
        low + 1
    } else {
        low
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OKLab by floating point, as Björn Ottosson defines it.
    fn reference(c: Color) -> [f64; 3] {
        let lin = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let (r, g, b) = (lin(c.r), lin(c.g), lin(c.b));
        let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
        let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
        let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
        [
            0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
            1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
            0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
        ]
    }

    #[test]
    fn matches_the_reference() {
        // Every grey, and a lattice of colors.
        let mut colors: Vec<Color> = (0..=255).map(|v| Color::rgb(v, v, v)).collect();
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    colors.push(Color::rgb(r, g, b));
                }
            }
        }
        for c in colors {
            let lab = Lab::from_color(c);
            let want = reference(c).map(|v| v * 10000.0);
            for (got, want) in [lab.l, lab.a, lab.b].into_iter().zip(want) {
                assert!(
                    (got as f64 - want).abs() <= 1.0,
                    "{c:?}: {lab:?} vs {want:?}"
                );
            }
        }
    }

    #[test]
    fn pinned_values() {
        let lab = |r, g, b| {
            let lab = Lab::from_color(Color::rgb(r, g, b));
            (lab.l, lab.a, lab.b)
        };
        assert_eq!(lab(0, 0, 0), (0, 0, 0));
        assert_eq!(lab(255, 255, 255), (10000, 0, 0));
        assert_eq!(lab(255, 0, 0), (6280, 2249, 1258));
        assert_eq!(lab(0x1a, 0x1c, 0x2c), (2324, 44, -307));
        assert_eq!(lab(0xf4, 0xb4, 0x1b), (8082, 230, 1601));
    }

    #[test]
    fn back_to_srgb() {
        for r in (0..=255).step_by(15) {
            for g in (0..=255).step_by(15) {
                for b in (0..=255).step_by(15) {
                    let c = Color::rgb(r, g, b);
                    let back = Lab::from_color(c).to_color().unwrap();
                    let off = [back.r.abs_diff(r), back.g.abs_diff(g), back.b.abs_diff(b)];
                    assert!(off.iter().all(|&d| d <= 1), "{c:?} came back {back:?}");
                }
            }
        }
        // Too much chroma for sRGB.
        let vivid = Lab {
            l: 5000,
            a: 4000,
            b: 4000,
        };
        assert_eq!(vivid.to_color(), None);
        assert_eq!(
            Lab::from_color(Color::BLACK).distance(Lab::from_color(Color::WHITE)),
            10000
        );
    }

    #[test]
    fn ramps() {
        let base = Color::rgb(0xd0, 0x81, 0x59);
        for count in 2..=9 {
            let colors = ramp(base, count, 20);
            assert_eq!(colors.len(), count);
            let labs: Vec<Lab> = colors.iter().map(|&c| Lab::from_color(c)).collect();
            // Dark to light, in clear steps.
            for pair in labs.windows(2) {
                assert!(pair[1].l - pair[0].l >= 600, "{count}: {labs:?}");
            }
        }
        // An odd ramp has the base in its middle.
        let five = ramp(base, 5, 20);
        let middle = Lab::from_color(five[2]);
        assert!(middle.distance(Lab::from_color(base)) <= 20, "{five:?}");
        // The hue turns toward yellow in the highlight and blue in the shadow: the
        // highlight's b (yellow) rises against the base's, the shadow's falls.
        let (dark, light) = (Lab::from_color(five[0]), Lab::from_color(five[4]));
        let hue_b = |lab: Lab| lab.b as i64 * 1000 / lab.chroma().max(1);
        assert!(hue_b(light) > hue_b(middle), "{light:?} {middle:?}");
        assert!(hue_b(dark) < hue_b(middle), "{dark:?} {middle:?}");
        // No shift, no turn; grey stays grey.
        let flat = ramp(base, 5, 0);
        let hue = |c: Color| {
            let lab = Lab::from_color(c);
            (
                lab.a as i64 * 1000 / lab.chroma(),
                lab.b as i64 * 1000 / lab.chroma(),
            )
        };
        assert!(hue(flat[0]).0.abs_diff(hue(flat[4]).0) <= 20, "{flat:?}");
        for c in ramp(Color::rgb(128, 128, 128), 4, 30) {
            assert!(c.r.abs_diff(c.g) <= 1 && c.g.abs_diff(c.b) <= 1, "{c:?}");
        }
        // Pinned: the same colors on every platform.
        assert_eq!(
            five.iter().map(|c| c.to_array()).collect::<Vec<_>>(),
            [
                [119, 64, 58, 255],
                [163, 95, 75, 255],
                [208, 129, 89, 255],
                [236, 174, 128, 255],
                [254, 221, 188, 255]
            ]
        );
    }

    #[test]
    fn roots() {
        for n in [0u64, 1, 7, 8, 26, 27, 1 << 60, u64::MAX] {
            let want = (n as f64).cbrt().round() as u64;
            assert!(cube_root(n).abs_diff(want) <= 1, "{n}");
        }
    }
}
