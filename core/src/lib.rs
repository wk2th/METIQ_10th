//! ```text
//!  __  __ _____ _____ ___ ___
//! |  \/  | ____|_   _|_ _/ _ \     HOME PAGE RENDERING KERNEL
//! | |\/| |  _|   | |  | | | | |    ---------------------------
//! | |  | | |___  | |  | | |_| |    no_std / no alloc / no bindgen
//! |_|  |_|_____| |_| |___\__\_\    wasm32 + simd128
//! ```
//!
//! Every pixel on the page that *looks* like a 1996 animated GIF is actually
//! rasterised here, every frame, into a static buffer that JavaScript maps
//! straight into an `ImageData`. Nothing is allocated after instantiation:
//! all state lives in `.bss` behind [`Slot`] cells, so `memory.buffer` never
//! grows and the typed-array views on the JS side never detach.
//!
//! Subsystems
//! ----------
//! * [`field`]    – per-glyph springs and the mojibake decode ripples
//! * [`spread`]   – corruption spread as first-passage percolation
//! * [`life`]     – objects breathe, fibrillate and drip like cells
//! * [`destroy`]  – electric / rust / lightning / meteor / hack destruction
//! * [`crt`]      – SIMD signal noise, scanlines, tear bands
//! * [`odometer`] – the access counter (true carry-propagating drum)
//! * [`globe`]    – ray-cast, ordered-dithered spinning globe w/ satellite
//! * [`sign`]     – "UNDER CONSTRUCTION" barrier tape
//! * [`logo`]     – SDF-rendered METIQ mark that turns on its apex axis
//! * [`badge`]    – gaudy gold 10th ANNIVERSARY medal
//! * [`banner`]   – 88x31 link banner with specular sweep
//! * [`rainbow`]  – the obligatory rainbow `<HR>`
//! * [`texture`]  – seamless BACKGROUND= tile
//! * [`spark`]    – cursor trail particles
//! * [`fx`]       – global energy envelope, glitch scheduler, calm switch

#![no_std]
#![allow(clippy::missing_safety_doc, clippy::too_many_arguments)]

mod math;
mod rng;
mod noise;
mod palette;
mod font;

mod fx;
mod field;
mod spread;
mod destroy;
mod life;
mod crt;
mod odometer;
mod globe;
mod sign;
mod banner;
mod badge;
mod logo;
mod logo_data;
mod rainbow;
mod texture;
mod spark;

use core::cell::UnsafeCell;

/// A single-threaded global. wasm32 without the `atomics` feature has exactly
/// one thread per instance, so handing out `&mut` from a shared static is
/// sound as long as no two borrows overlap – which every export guarantees by
/// taking at most one borrow per `Slot` at a time.
pub(crate) struct Slot<T>(UnsafeCell<T>);

unsafe impl<T> Sync for Slot<T> {}

impl<T> Slot<T> {
    pub const fn new(v: T) -> Self {
        Self(UnsafeCell::new(v))
    }

    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub fn get(&self) -> &mut T {
        unsafe { &mut *self.0.get() }
    }
}

/// 1996-10-12, BCD. Lets the loader verify it got the right binary.
#[no_mangle]
pub extern "C" fn metiq_build() -> u32 {
    0x1996_1012
}

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}
