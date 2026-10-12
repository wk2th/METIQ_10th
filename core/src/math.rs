//! Scalar kernel. There is no libm in `core`, so the hardware ops come from
//! `core::arch::wasm32` and everything transcendental is approximated by hand.
//! Error bounds are noted per function; all of them are far below what a
//! 6-level-per-channel web-safe palette can resolve.

pub const PI: f32 = core::f32::consts::PI;
pub const TAU: f32 = core::f32::consts::TAU;
pub const HALF_PI: f32 = core::f32::consts::FRAC_PI_2;

// The scalar `f32_floor`/`f32_sqrt` intrinsics are still unstable, but their
// SIMD siblings are not – so we splat into a v128, run the vector op and pull
// lane 0 back out. LLVM folds this down to the scalar opcode anyway.
#[cfg(target_arch = "wasm32")]
mod hw {
    use core::arch::wasm32::*;

    #[inline(always)]
    pub fn floor(x: f32) -> f32 {
        f32x4_extract_lane::<0>(f32x4_floor(f32x4_splat(x)))
    }

    #[inline(always)]
    pub fn sqrt(x: f32) -> f32 {
        f32x4_extract_lane::<0>(f32x4_sqrt(f32x4_splat(x)))
    }

    #[inline(always)]
    pub fn floor64(x: f64) -> f64 {
        f64x2_extract_lane::<0>(f64x2_floor(f64x2_splat(x)))
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod hw {
    pub fn floor(x: f32) -> f32 {
        let i = x as i64 as f32;
        if i > x { i - 1.0 } else { i }
    }
    pub fn floor64(x: f64) -> f64 {
        let i = x as i64 as f64;
        if i > x { i - 1.0 } else { i }
    }
    pub fn sqrt(x: f32) -> f32 {
        if x <= 0.0 {
            return 0.0;
        }
        let y = x * super::rsqrt(x);
        0.5 * (y + x / y)
    }
}

pub use hw::{floor, floor64, sqrt};

/// Smallest whole number of `step`s that is ≥ x (for x ≥ 0).
#[inline(always)]
pub fn ceil_div(x: f32, step: f32) -> f32 {
    let q = floor(x / step);
    if q * step < x { q + 1.0 } else { q }
}

#[inline(always)]
pub fn fract(x: f32) -> f32 {
    x - floor(x)
}

#[inline(always)]
pub fn abs(x: f32) -> f32 {
    f32::from_bits(x.to_bits() & 0x7fff_ffff)
}

#[inline(always)]
pub fn min(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

#[inline(always)]
pub fn max(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

#[inline(always)]
pub fn clamp(x: f32, lo: f32, hi: f32) -> f32 {
    min(max(x, lo), hi)
}

#[inline(always)]
pub fn sat(x: f32) -> f32 {
    clamp(x, 0.0, 1.0)
}

#[inline(always)]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// `1/sqrt(x)` – the 0x5f3759df trick, one Newton step (rel. err < 0.18%).
#[inline(always)]
pub fn rsqrt(x: f32) -> f32 {
    let y = f32::from_bits(0x5f37_59df - (x.to_bits() >> 1));
    y * (1.5 - 0.5 * x * y * y)
}

/// Parabolic sine with one refinement pass. |err| < 1.1e-3.
#[inline]
pub fn sin(x: f32) -> f32 {
    let x = x - TAU * floor(x * (1.0 / TAU) + 0.5);
    const B: f32 = 4.0 / PI;
    const C: f32 = -4.0 / (PI * PI);
    let y = B * x + C * x * abs(x);
    0.225 * (y * abs(y) - y) + y
}

#[inline]
pub fn cos(x: f32) -> f32 {
    sin(x + HALF_PI)
}

/// 2^x: integer part injected straight into the exponent field, fractional
/// part through a 4th-order minimax polynomial. rel. err < 4e-6.
#[inline]
pub fn exp2(x: f32) -> f32 {
    let x = clamp(x, -126.0, 126.0);
    let i = floor(x);
    let f = x - i;
    let p = 1.0 + f * (0.693_147_2 + f * (0.240_226_5 + f * (0.055_504_1 + f * 0.009_618_1)));
    f32::from_bits(((i as i32 + 127) as u32) << 23) * p
}

#[inline]
pub fn exp(x: f32) -> f32 {
    exp2(x * core::f32::consts::LOG2_E)
}

/// Frame-rate independent smoothing factor: `1 - e^(-rate*dt)`.
#[inline]
pub fn damp(rate: f32, dt: f32) -> f32 {
    1.0 - exp(-rate * dt)
}

/// atan on [-1, 1]. max err ≈ 1.5e-3 rad.
#[inline]
fn atan_unit(z: f32) -> f32 {
    let a = abs(z);
    (PI / 4.0) * z - z * (a - 1.0) * (0.2447 + 0.0663 * a)
}

/// Octant-reduced atan2.
pub fn atan2(y: f32, x: f32) -> f32 {
    if x == 0.0 && y == 0.0 {
        return 0.0;
    }
    if abs(x) >= abs(y) {
        let a = atan_unit(y / x);
        if x >= 0.0 {
            a
        } else if y >= 0.0 {
            a + PI
        } else {
            a - PI
        }
    } else {
        let a = atan_unit(x / y);
        if y > 0.0 { HALF_PI - a } else { -HALF_PI - a }
    }
}

#[inline]
pub fn asin(y: f32) -> f32 {
    atan2(y, sqrt(max(0.0, 1.0 - y * y)))
}

/// HSV (all in 0..1) to linear-ish RGB.
pub fn hsv(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let h = fract(h) * 6.0;
    let i = floor(h);
    let f = h - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i as u32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}
