//! The "MIDI" player is not MIDI. It is a three-voice chip synth running in an
//! AudioWorklet on its own instance of this module:
//!
//!   lead  – 25% pulse, delayed vibrato, attack/decay/sustain/release
//!   bass  – triangle quantised to 16 steps (2A03 style)
//!   drums – pitch-swept sine kick, 15-bit LFSR noise snare & hat
//!
//! Progression: IV–V–iii–vi (王道進行), 138 BPM, 16th-note grid, 8 bars.

use crate::math::{abs, exp, exp2, floor, sin, TAU};
use crate::Slot;

const BPM: f32 = 138.0;
const BLOCK: usize = 512;

/// 0 = rest/note-off, 1 = tie, otherwise MIDI note number.
#[rustfmt::skip]
const LEAD: [u8; 128] = [
    69,1,72,1, 77,1,1,76, 1,72,1,1, 74,1,72,1,  // F
    71,1,74,1, 79,1,1,77, 1,76,1,74, 1,1,1,0,   // G
    76,1,74,1, 72,1,71,1, 72,1,1,74, 1,76,1,1,  // Em
    69,1,1,1,  72,1,71,1, 69,1,1,1,  0,0,64,1,  // Am
    69,1,72,1, 77,1,1,76, 1,72,1,1, 81,1,79,1,  // F
    77,1,76,1, 74,1,1,76, 1,77,1,79, 1,1,1,0,   // G
    80,1,1,1,  76,1,1,1,  74,1,1,72, 1,1,71,1,  // E7
    69,1,1,1,  1,1,1,1,   1,1,0,0,  72,1,71,1,  // Am
];

const ROOTS: [u8; 8] = [41, 43, 40, 45, 41, 43, 40, 45];

#[derive(Clone, Copy)]
struct Voice {
    phase: f32,
    freq: f32,
    env: f32,
    age: f32,
    gate: bool,
}

const V0: Voice = Voice { phase: 0.0, freq: 0.0, env: 0.0, age: 0.0, gate: false };

struct Synth {
    sr: f32,
    spb: f32,
    ctr: f32,
    step: u32,
    lead: Voice,
    bass: Voice,
    kick_t: f32,
    kick_ph: f32,
    snare_t: f32,
    hat_t: f32,
    lfsr: u16,
    prev_n: f32,
    out: [f32; BLOCK],
}

static SY: Slot<Synth> = Slot::new(Synth {
    sr: 48_000.0,
    spb: 0.0,
    ctr: 0.0,
    step: 0,
    lead: V0,
    bass: V0,
    kick_t: 9.0,
    kick_ph: 0.0,
    snare_t: 9.0,
    hat_t: 9.0,
    lfsr: 1,
    prev_n: 0.0,
    out: [0.0; BLOCK],
});

#[inline]
fn hz(n: u8) -> f32 {
    440.0 * exp2((n as f32 - 69.0) / 12.0)
}

#[no_mangle]
pub extern "C" fn synth_init(sample_rate: f32) {
    let s = SY.get();
    s.sr = sample_rate;
    s.spb = sample_rate * 60.0 / BPM / 4.0;
    s.ctr = 0.0;
    s.step = 0;
    s.lead = V0;
    s.bass = V0;
    s.kick_t = 9.0;
    s.snare_t = 9.0;
    s.hat_t = 9.0;
    s.lfsr = 1;
}

#[no_mangle]
pub extern "C" fn synth_step() -> u32 {
    SY.get().step
}

fn trigger(s: &mut Synth) {
    let i = (s.step % 128) as usize;
    let bar = i / 16;
    let within = i % 16;

    match LEAD[i] {
        0 => s.lead.gate = false,
        1 => {}
        n => {
            s.lead.freq = hz(n);
            s.lead.env = 1.0;
            s.lead.age = 0.0;
            s.lead.gate = true;
        }
    }

    if within % 2 == 0 {
        let oct = if (within / 2) % 2 == 1 { 12 } else { 0 };
        s.bass.freq = hz(ROOTS[bar] + oct);
        s.bass.env = 1.0;
        s.bass.gate = true;
    } else {
        s.bass.gate = false;
    }

    if within == 0 || within == 8 || (bar % 2 == 1 && within == 10) {
        s.kick_t = 0.0;
        s.kick_ph = 0.0;
    }
    if within == 4 || within == 12 || (bar == 7 && within >= 13) {
        s.snare_t = 0.0;
    }
    if within % 2 == 0 {
        s.hat_t = 0.0;
    }
    s.step = s.step.wrapping_add(1);
}

#[inline]
fn lfsr(s: &mut Synth) -> f32 {
    let bit = (s.lfsr ^ (s.lfsr >> 1)) & 1;
    s.lfsr = (s.lfsr >> 1) | (bit << 14);
    if s.lfsr & 1 == 1 { 1.0 } else { -1.0 }
}

/// Render `frames` mono samples (≤ 512) and return a pointer to them.
#[no_mangle]
pub extern "C" fn synth_render(frames: u32) -> *const f32 {
    let s = SY.get();
    let n = if (frames as usize) < BLOCK { frames as usize } else { BLOCK };
    let inv = 1.0 / s.sr;

    for k in 0..n {
        if s.ctr <= 0.0 {
            trigger(s);
            s.ctr += s.spb;
        }
        s.ctr -= 1.0;

        // lead
        let l = &mut s.lead;
        l.age += inv;
        let vib = sin(l.age * TAU * 5.5) * 0.005 * crate::math::sat((l.age - 0.15) * 4.0);
        l.phase += l.freq * (1.0 + vib) * inv;
        l.phase -= floor(l.phase);
        l.env = if l.gate { 0.55 + (l.env - 0.55) * 0.9996 } else { l.env * 0.9985 };
        let lead = (if l.phase < 0.25 { 1.0 } else { -1.0 }) * l.env;

        // bass
        let b = &mut s.bass;
        b.phase += b.freq * inv;
        b.phase -= floor(b.phase);
        b.env = if b.gate { 0.8 + (b.env - 0.8) * 0.9997 } else { b.env * 0.999 };
        let tri = 4.0 * abs(b.phase - 0.5) - 1.0;
        let tri = floor((tri + 1.0) * 7.5) / 7.5 - 1.0;
        let bass = tri * b.env;

        // drums
        s.kick_t += inv;
        let kf = 48.0 + 110.0 * exp(-s.kick_t * 35.0);
        s.kick_ph += kf * inv;
        let kick = sin(s.kick_ph * TAU) * exp(-s.kick_t * 10.0);

        let nz = lfsr(s);
        s.snare_t += inv;
        let snare = nz * exp(-s.snare_t * 16.0) * 0.8 + sin(s.snare_t * TAU * 190.0) * exp(-s.snare_t * 25.0) * 0.4;
        s.hat_t += inv;
        let hat = (nz - s.prev_n) * 0.5 * exp(-s.hat_t * 55.0);
        s.prev_n = nz;

        let mix = lead * 0.16 + bass * 0.24 + kick * 0.55 + snare * 0.18 + hat * 0.07;
        s.out[k] = mix / (1.0 + abs(mix)) * 0.8;
    }
    s.out.as_ptr()
}
