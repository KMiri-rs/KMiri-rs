//! Asterinas issue #3894: safe `IoMem<Insensitive>` single-access with an
//! unsupported `PodOnce` size (not 1, 2, 4, or 8 bytes) reaches
//! `core::hint::unreachable_unchecked()`.
#![no_std]
#![forbid(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

use ostd::{
    io::IoMem,
    mm::{PodOnce, VmIoOnce},
};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

#[repr(C)]
#[derive(FromBytes, IntoBytes, Immutable, KnownLayout, Clone, Copy, Debug)]
struct Three {
    a: u8,
    b: u8,
    c: u8,
}

impl PodOnce for Three {}

#[ostd::ktest::miri_main]
fn kernel_main() {
    let io = IoMem::acquire(0xfeb0_0000..0xfeb0_1000).unwrap();

    // Control: supported 4-byte single accesses succeed.
    let val = io.read_once::<u32>(0).unwrap();
    assert_eq!(val, u32::MAX);
    io.write_once::<u32>(0, &0x1234_5678u32).unwrap();
    ostd::miri_println!("iomem-podonce-size: control ok");

    // Trigger: unsupported 3-byte PodOnce read_once.
    let _ = io.read_once::<Three>(0);
    ostd::miri_println!("iomem-podonce-size: after trigger");
}
