// SPDX-License-Identifier: MPL-2.0

//! Three CPUs and a 96 KiB CPU-local variable, which does not fit in KMiri's
//! default 64 KiB section. `kmiri.toml` sets `cpu_local_size` to 128 KiB.
//!
//! Each CPU's copy must be separate, and OSTD must not hand out the kernel stack
//! section, which moves up with the larger section, as free frames.

#![no_std]
#![feature(format_args_nl)]

extern crate alloc;

use core::sync::atomic::{AtomicU64, Ordering::Relaxed};

use ostd::{
    cpu::all_cpus,
    mm::{FrameAllocOptions, HasPaddr},
};

const SIZE: usize = 0x1_8000;

#[repr(C)]
struct Large {
    first: AtomicU64,
    padding: [u8; SIZE - 16],
    last: AtomicU64,
}

ostd::cpu_local! {
    static LARGE: Large = Large {
        first: AtomicU64::new(7),
        padding: [0; SIZE - 16],
        last: AtomicU64::new(7),
    };
}

unsafe extern "C" {
    fn __kernel_start();
    fn __kernel_end();
}

#[ostd::ktest::miri_main]
fn kernel_main() {
    for cpu in all_cpus() {
        let large = LARGE.get_on_cpu(cpu);
        assert_eq!(large.first.load(Relaxed), 7);
        assert_eq!(large.last.load(Relaxed), 7);
        assert_eq!(large.padding[SIZE / 2], 0);
        large.last.store(100 + u32::from(cpu) as u64, Relaxed);
    }
    for cpu in all_cpus() {
        let large = LARGE.get_on_cpu(cpu);
        assert_eq!(large.last.load(Relaxed), 100 + u32::from(cpu) as u64);
    }

    let kernel_size = __kernel_end as *const () as usize - __kernel_start as *const () as usize;
    assert_eq!(kernel_size, 0x101_0000);
    for _ in 0..64 {
        let frame = FrameAllocOptions::new().alloc_frame().unwrap();
        assert!(frame.paddr() >= kernel_size);
        core::mem::forget(frame);
    }
    ostd::miri_println!("cpu-local-large-section: done");
}
