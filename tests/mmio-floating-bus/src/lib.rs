//! An MMIO range with no device behind it must read as all ones under KMiri.
//!
//! The full Asterinas kernel probes QEMU microvm's virtio-mmio slots, starting
//! at 0xfeb00000, and checks each slot's magic value. Natively, with no device
//! there, the load returns all ones and the driver skips the slot. KMiri models
//! no device outside RAM, so its MMIO bus floats the same way.
//!
//! Expected: reads return all ones, a write is dropped, and the kernel exits
//! normally.
#![no_std]
#![deny(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

use ostd::{io::IoMem, mm::VmIoOnce};

#[ostd::ktest::miri_main]
fn kernel_main() {
    let io_mem = IoMem::acquire(0xfeb0_0000..0xfeb0_0200).unwrap();
    assert_eq!(io_mem.read_once::<u32>(0).unwrap(), 0xffff_ffff);
    assert_eq!(io_mem.read_once::<u8>(4).unwrap(), 0xff);
    io_mem.write_once::<u32>(0, &0x1234_5678).unwrap();
    assert_eq!(io_mem.read_once::<u32>(0).unwrap(), 0xffff_ffff);
    ostd::miri_println!("mmio-floating-bus: ok");
}
