//! Global "signal energy". Every non-period-correct effect on the page is
//! scaled by this one scalar. It rises with pointer velocity and hover kicks,
//! and bleeds off within ~2s of stillness – so the moment you stop to look
//! (or to take a screenshot) the page is a perfectly ordinary 1996 page again.

use crate::math::{damp, sat};
use crate::rng::Pcg;
use crate::Slot;

struct Fx {
    energy: f32,
    glitch: f32,
    hold: f32,
    rng: Pcg,
}

static FX: Slot<Fx> = Slot::new(Fx { energy: 0.0, glitch: 0.0, hold: 0.0, rng: Pcg::fixed() });

#[no_mangle]
pub extern "C" fn init(seed: u32) {
    FX.get().rng.seed(seed as u64, 0x1996);
    crate::crt::reseed(seed);
    crate::odometer::reseed(seed ^ 0x0D0);
    crate::spark::reseed(seed ^ 0x5AA);
}

/// Advance the envelope. `speed` is pointer speed in CSS px/s, `kick` an
/// instantaneous bump (hover enter, click). Attack is fast, release slow.
#[no_mangle]
pub extern "C" fn fx_energy(dt: f32, speed: f32, kick: f32) -> f32 {
    let fx = FX.get();
    if fx.hold > 0.0 {
        fx.hold -= dt;
        fx.energy = 0.0;
        return 0.0;
    }
    let target = sat(speed * (1.0 / 1400.0));
    let rate = if target > fx.energy { 7.0 } else { 0.95 };
    fx.energy = sat(fx.energy + (target - fx.energy) * damp(rate, dt) + kick);
    if fx.energy < 0.004 {
        fx.energy = 0.0;
    }
    fx.energy
}

/// Poisson-scheduled signal glitches. Only fire while energy is high, last
/// 60–180ms. Returns current glitch strength in [0, 1].
#[no_mangle]
pub extern "C" fn fx_glitch(dt: f32) -> f32 {
    let fx = FX.get();
    if fx.glitch > 0.0 {
        fx.glitch -= dt;
        return if fx.glitch > 0.0 { 1.0 } else { 0.0 };
    }
    if fx.hold <= 0.0 && fx.energy > 0.55 && fx.rng.f01() < dt * 0.6 * fx.energy {
        fx.glitch = 0.06 + fx.rng.f01() * 0.12;
    }
    0.0
}

/// Hard stop. Zero every envelope and spring in the module and refuse to
/// re-arm for `hold` seconds. Called synchronously from the screenshot guard.
#[no_mangle]
pub extern "C" fn fx_calm(hold: f32) {
    let fx = FX.get();
    fx.energy = 0.0;
    fx.glitch = 0.0;
    fx.hold = hold;
    crate::field::field_reset();
    crate::odometer::settle();
    crate::globe::settle();
    crate::logo::settle();
    crate::spark::clear();
}
