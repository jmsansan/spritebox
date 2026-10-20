//! Procedural textures: seeded value noise, mapped onto a ramp of palette keys with
//! ordered dithering. Integer arithmetic only, so a seed gives the same pixels on
//! every platform.

use crate::{Error, Result};

/// How noise values between two ramp keys are drawn.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Dither {
    /// Each pixel takes the nearest key.
    #[default]
    None,
    /// Ordered dithering with a Bayer matrix of 2×2, 4×4 or 8×8 thresholds: a value
    /// between two keys mixes them in a fixed pattern, in proportion.
    Bayer(u32),
}

impl Dither {
    /// The ramp index, of `steps` keys, for `value` at pixel `(x, y)`.
    pub fn pick(self, value: u16, x: u32, y: u32, steps: usize) -> usize {
        if steps <= 1 {
            return 0;
        }
        // The value's place along the ramp, in 16.16 fixed point.
        let place = value as u64 * (steps as u64 - 1) * 65536 / 65535;
        let (index, fraction) = ((place >> 16) as usize, place & 0xffff);
        match self {
            Dither::None => (index + (fraction >= 32768) as usize).min(steps - 1),
            Dither::Bayer(size) => {
                let cells = (size * size) as u64;
                let threshold = bayer(size, x % size, y % size) as u64;
                // Take the next key when the fraction passes the cell's threshold,
                // the centre of its share of the matrix.
                let up = fraction * cells > (2 * threshold + 1) * 32768;
                (index + up as usize).min(steps - 1)
            }
        }
    }
}

/// The threshold of cell `(x, y)` of the Bayer matrix of side `size`, a power of two:
/// 0 to `size * size - 1`, built as `M(2n) = [[4M, 4M+2], [4M+3, 4M+1]]`.
pub fn bayer(size: u32, x: u32, y: u32) -> u32 {
    // The finest level of the recursion is the most significant.
    let mut value = 0;
    let mut step = 1;
    while step < size {
        let (bx, by) = ((x / step) % 2, (y / step) % 2);
        value = value * 4 + [[0, 2], [3, 1]][by as usize][bx as usize];
        step *= 2;
    }
    value
}

/// Seeded value noise that wraps around a `width` × `height` area, so a texture tiles
/// with itself.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Noise {
    pub width: u32,
    pub height: u32,
    /// Pixels between lattice points of the first octave; each further octave halves
    /// it. It divides the width and the height.
    pub cell: u32,
    /// Layers of noise, 1..=8, each with half the cell of the one before.
    pub octaves: u32,
    /// How much each octave weighs against the one before, in percent, 0..=100.
    pub roughness: u32,
    pub seed: u64,
}

impl Noise {
    pub(crate) fn check(&self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            return Err(Error::invalid("A texture needs at least one pixel a side"));
        }
        if self.cell == 0 || self.width % self.cell != 0 || self.height % self.cell != 0 {
            return Err(Error::invalid(format!(
                "The cell, {}, must divide the size {}x{}, so that the noise wraps",
                self.cell, self.width, self.height
            )));
        }
        if !(1..=8).contains(&self.octaves) || self.cell >> (self.octaves - 1) == 0 {
            return Err(Error::invalid(format!(
                "Octaves must be 1..=8, and the cell of {} must halve that often",
                self.cell
            )));
        }
        if self.roughness > 100 {
            return Err(Error::invalid("Roughness must be 0..=100"));
        }
        Ok(())
    }

    /// The noise at `(x, y)`, 0..=65535, for any coordinates: it repeats every
    /// `width` and `height` pixels.
    pub fn value(&self, x: i64, y: i64) -> u16 {
        let x = x.rem_euclid(self.width as i64) as u32;
        let y = y.rem_euclid(self.height as i64) as u32;
        let mut sum = 0u64;
        let mut total = 0u64;
        let mut amplitude = 1u64 << 20;
        for octave in 0..self.octaves {
            let cell = (self.cell >> octave).max(1);
            sum += amplitude * self.octave(octave, cell, x, y) as u64;
            total += amplitude;
            amplitude = amplitude * self.roughness as u64 / 100;
        }
        (sum / total) as u16
    }

    /// The noise of every pixel, row by row, stretched so that its lowest value is 0
    /// and its highest 65535: value noise gathers around its middle, and a texture
    /// should reach both ends of its ramp.
    pub fn stretched(&self) -> Vec<u16> {
        let mut values = Vec::with_capacity(self.width as usize * self.height as usize);
        for y in 0..self.height {
            for x in 0..self.width {
                values.push(self.value(x as i64, y as i64));
            }
        }
        let low = values.iter().copied().min().unwrap_or(0) as u64;
        let high = values.iter().copied().max().unwrap_or(0) as u64;
        if high > low {
            for v in &mut values {
                *v = ((*v as u64 - low) * 65535 / (high - low)) as u16;
            }
        }
        values
    }

    /// One octave: lattice values, smoothly interpolated.
    fn octave(&self, octave: u32, cell: u32, x: u32, y: u32) -> u32 {
        let (columns, rows) = (self.width / cell, self.height / cell);
        let (cx, cy) = (x / cell, y / cell);
        let corner = |dx: u32, dy: u32| {
            lattice(self.seed, octave, (cx + dx) % columns, (cy + dy) % rows) as u64
        };
        // Position within the cell in 0..65536, eased with 3t² - 2t³.
        let ease = |offset: u32| {
            let t = (offset as u64 * 65536) / cell as u64;
            t * t * (3 * 65536 - 2 * t) / (65536 * 65536)
        };
        let (tx, ty) = (ease(x % cell), ease(y % cell));
        let mix = |a: u64, b: u64, t: u64| (a * (65536 - t) + b * t) / 65536;
        let top = mix(corner(0, 0), corner(1, 0), tx);
        let bottom = mix(corner(0, 1), corner(1, 1), tx);
        mix(top, bottom, ty) as u32
    }
}

/// The value of a lattice point, 0..=65535: a hash of the seed, the octave and the
/// point, by the SplitMix64 finalizer.
fn lattice(seed: u64, octave: u32, x: u32, y: u32) -> u16 {
    let mut z = seed
        ^ (octave as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (x as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
        ^ (y as u64).wrapping_mul(0x1656_67b1_9e37_79f9);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    (z >> 48) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bayer_matrices() {
        let m = |size| {
            (0..size)
                .map(|y| (0..size).map(|x| bayer(size, x, y)).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        };
        assert_eq!(m(2), [[0, 2], [3, 1]]);
        assert_eq!(
            m(4),
            [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]]
        );
        let mut all: Vec<u32> = m(8).into_iter().flatten().collect();
        all.sort();
        assert_eq!(all, (0..64).collect::<Vec<_>>());
    }

    #[test]
    fn dithering_mixes_in_proportion() {
        // A value a quarter of the way from key 0 to key 1 picks key 1 in a quarter of
        // the cells of each matrix.
        for size in [2, 4, 8] {
            let up = (0..size)
                .flat_map(|y| (0..size).map(move |x| (x, y)))
                .filter(|&(x, y)| Dither::Bayer(size).pick(16384, x, y, 2) == 1)
                .count() as u32;
            assert_eq!(up, size * size / 4, "bayer{size}");
        }
        assert_eq!(Dither::None.pick(16384, 0, 0, 2), 0);
        assert_eq!(Dither::None.pick(40000, 0, 0, 2), 1);
        assert_eq!(Dither::None.pick(65535, 0, 0, 3), 2);
        assert_eq!(Dither::Bayer(4).pick(0, 3, 3, 3), 0);
        assert_eq!(Dither::Bayer(4).pick(65535, 0, 0, 3), 2);
        assert_eq!(Dither::None.pick(1234, 0, 0, 1), 0);
    }

    #[test]
    fn noise_wraps_and_depends_on_the_seed() {
        let noise = Noise {
            width: 16,
            height: 8,
            cell: 4,
            octaves: 3,
            roughness: 50,
            seed: 7,
        };
        noise.check().unwrap();
        for y in -8..16 {
            for x in -16..32 {
                assert_eq!(noise.value(x, y), noise.value(x + 16, y + 8));
            }
        }
        let other = Noise {
            seed: 8,
            ..noise.clone()
        };
        let differs = (0..16).any(|x| noise.value(x, 0) != other.value(x, 0));
        assert!(differs);
        let values: Vec<u16> = (0..128).map(|i| noise.value(i % 16, i / 16)).collect();
        let (min, max) = (values.iter().min().unwrap(), values.iter().max().unwrap());
        assert!(max - min > 8000, "{min}..{max}");
        let stretched = noise.stretched();
        assert_eq!(stretched.len(), 128);
        assert_eq!(
            (
                *stretched.iter().min().unwrap(),
                *stretched.iter().max().unwrap()
            ),
            (0, 65535)
        );
        // Lattice points of one octave are its hash, whatever the cell.
        let flat = Noise {
            octaves: 1,
            ..noise.clone()
        };
        assert_eq!(flat.value(4, 4), lattice(7, 0, 1, 1));
    }

    #[test]
    fn invalid_noise() {
        let ok = Noise {
            width: 16,
            height: 16,
            cell: 8,
            octaves: 4,
            roughness: 50,
            seed: 0,
        };
        assert!(ok.check().is_ok());
        assert!(
            Noise {
                cell: 5,
                ..ok.clone()
            }
            .check()
            .is_err()
        );
        assert!(
            Noise {
                octaves: 5,
                ..ok.clone()
            }
            .check()
            .is_err()
        );
        assert!(
            Noise {
                octaves: 0,
                ..ok.clone()
            }
            .check()
            .is_err()
        );
        assert!(
            Noise {
                roughness: 101,
                ..ok
            }
            .check()
            .is_err()
        );
    }
}
