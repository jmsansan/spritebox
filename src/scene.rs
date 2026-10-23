//! Shared scene accounting and deterministic integer helpers.
use crate::{Error, Limits, Result};

#[derive(Clone, Debug)]
pub(crate) struct Budget {
    pub pixels: u64,
    pub work: u64,
    commands: usize,
    attempts: u64,
}
impl Budget {
    pub fn new(limits: &Limits) -> Self {
        Self {
            pixels: limits.max_scene_pixels,
            work: limits.max_scene_work,
            commands: limits.max_scene_commands,
            attempts: limits.max_scatter_attempts,
        }
    }
    pub fn pixels(&mut self, n: u64) -> Result<()> {
        spend(&mut self.pixels, n, "Scene storage")
    }
    pub fn ensure_pixels(&self, n: u64) -> Result<()> {
        if n > self.pixels {
            Err(Error::limit("Scene temporary storage limit exceeded"))
        } else {
            Ok(())
        }
    }
    pub fn work(&mut self, n: u64) -> Result<()> {
        spend(&mut self.work, n, "Scene work")
    }
    pub fn attempts(&mut self, n: u64) -> Result<()> {
        spend(&mut self.attempts, n, "Scatter attempts")
    }
    pub fn commands(&mut self, n: usize) -> Result<()> {
        self.commands = self
            .commands
            .checked_sub(n)
            .ok_or_else(|| Error::limit("Scene command limit exceeded"))?;
        Ok(())
    }
}
fn spend(left: &mut u64, n: u64, what: &str) -> Result<()> {
    *left = left
        .checked_sub(n)
        .ok_or_else(|| Error::limit(format!("{what} limit exceeded")))?;
    Ok(())
}

pub(crate) fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}
/// Stable FNV-1a-64 over length-prefixed UTF-8 components (u64 LE lengths), then
/// u64 LE seeds, followed by the SplitMix64 finalizer. No path or order enters it.
pub fn scene_seed(identity: &[&str], seeds: &[u64]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut bytes = |bytes: &[u8]| {
        for &b in bytes {
            hash = (hash ^ b as u64).wrapping_mul(0x100_0000_01b3);
        }
    };
    for part in identity {
        bytes(&(part.len() as u64).to_le_bytes());
        bytes(part.as_bytes());
    }
    for seed in seeds {
        bytes(&seed.to_le_bytes());
    }
    mix(hash)
}
pub(crate) fn random(seed: u64, attempt: u64, lane: u64, range: u64) -> u64 {
    let z = seed
        .wrapping_add(attempt.wrapping_mul(0x9e37_79b9_7f4a_7c15))
        .wrapping_add(lane.wrapping_mul(0xd1b5_4a32_d192_ed03));
    ((mix(z) as u128 * range as u128) >> 64) as u64
}
