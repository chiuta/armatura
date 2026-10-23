// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
//! Deterministic hashing, value noise and a small seeded RNG.
//! Used by procedural channels, particles and wobble effects. Pure functions of
//! their inputs, so any frame can be evaluated in any order.

/// Integer hash → [0, 1).
///
/// Provenance of the constants (third-party, all public domain / unrestricted): the xorshift-multiply finaliser
/// (shifts 16, 15, 16 with multipliers `0x7FEB352D`, `0x846CA68B`) is Chris Wellons' `lowbias32` (public domain,
/// Unlicense/CC0, "Hash Prospector", 2018); `0x9E3779B1` is 2^32 / golden ratio (Knuth, TAOCP vol. 3); `0x85EBCA6B`
/// is a MurmurHash3 finaliser constant (Austin Appleby, public domain). See CLEANROOM.md and docs/THIRD_PARTY.md.
pub fn hash01(i: i32, seed: u32) -> f32 {
    let mut x = (i as u32).wrapping_mul(0x9E37_79B1) ^ seed.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Smooth 1-D value noise in [-1, 1].
pub fn noise1(x: f32, seed: u32) -> f32 {
    let i = x.floor();
    let f = x - i;
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash01(i as i32, seed) * 2.0 - 1.0;
    let b = hash01(i as i32 + 1, seed) * 2.0 - 1.0;
    a + (b - a) * u
}

/// Fractal (multi-octave) noise, normalised to roughly [-1, 1].
pub fn fbm1(x: f32, seed: u32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for o in 0..octaves.max(1) {
        sum += noise1(x * freq, seed.wrapping_add(o * 101)) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

/// Tiny xorshift RNG (deterministic, seedable).
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 32) as u32
    }
    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_deterministic_bounded_and_smooth() {
        assert_eq!(noise1(3.7, 5), noise1(3.7, 5));
        assert_ne!(noise1(3.7, 5), noise1(3.7, 6));
        let mut prev = noise1(0.0, 1);
        for i in 1..500 {
            let v = noise1(i as f32 * 0.01, 1);
            assert!(v.abs() <= 1.0);
            assert!((v - prev).abs() < 0.1, "noise should be continuous");
            prev = v;
        }
    }

    #[test]
    fn rng_is_reproducible_and_in_range() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            let (x, y) = (a.f32(), b.f32());
            assert_eq!(x, y);
            assert!((0.0..1.0).contains(&x));
        }
    }
}
