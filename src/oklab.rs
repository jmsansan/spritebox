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
    fn roots() {
        for n in [0u64, 1, 7, 8, 26, 27, 1 << 60, u64::MAX] {
            let want = (n as f64).cbrt().round() as u64;
            assert!(cube_root(n).abs_diff(want) <= 1, "{n}");
        }
    }
}
