//! Randomness. PCG-XSH-RR for streams, lowbias32 for stateless lattice hashes.

/// PCG32 (O'Neill 2014), XSH-RR output.
pub struct Pcg {
    state: u64,
    inc: u64,
}

impl Pcg {
    pub const fn fixed() -> Self {
        Self { state: 0x853c_49e6_748f_ea9b, inc: 0xda3e_39cb_94b9_5bdb }
    }

    pub fn seed(&mut self, seed: u64, seq: u64) {
        self.state = 0;
        self.inc = (seq << 1) | 1;
        self.next_u32();
        self.state = self.state.wrapping_add(seed);
        self.next_u32();
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(self.inc);
        let xs = (((old >> 18) ^ old) >> 27) as u32;
        xs.rotate_right((old >> 59) as u32)
    }

    /// Uniform in [0, 1) with 24 bits of mantissa.
    #[inline]
    pub fn f01(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    /// Uniform in [-1, 1).
    #[inline]
    pub fn sym(&mut self) -> f32 {
        self.f01() * 2.0 - 1.0
    }
}

/// Chris Wellons' lowbias32 – bias 0.17, good enough for value-noise lattices.
#[inline(always)]
pub fn hash32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// Hash to [-1, 1).
#[inline(always)]
pub fn hsym(h: u32) -> f32 {
    (h >> 8) as f32 * (2.0 / 16_777_216.0) - 1.0
}

/// Hash to [0, 1).
#[inline(always)]
pub fn h01(h: u32) -> f32 {
    (h >> 8) as f32 * (1.0 / 16_777_216.0)
}
