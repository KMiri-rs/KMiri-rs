//! The kernel learns the machine's RAM size (here 64 MiB, set in `kmiri.toml`)
//! from its boot memory map, as natively from the firmware. The frame allocator
//! must only hand out frames inside that RAM.
//!
//! Expected: a 64 MiB segment cannot be allocated (the kernel image takes part of
//! the RAM), a 4 MiB one can be written to its last byte and freed, and there is
//! no Undefined Behavior.
#![no_std]
#![deny(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

use ostd::mm::{FrameAllocOptions, HasPaddr, PAGE_SIZE, VmIo};

#[ostd::ktest::miri_main]
fn kernel_main() {
    let mut options = FrameAllocOptions::new();
    options.zeroed(false);
    assert!(options.alloc_segment((64 << 20) / PAGE_SIZE).is_err());
    ostd::miri_println!("ram-size-small: 64 MiB segment refused");

    let segment = options.alloc_segment(1024).unwrap();
    segment.write_val(1024 * PAGE_SIZE - 8, &0x1234_5678_u64).unwrap();
    assert_eq!(segment.read_val::<u64>(1024 * PAGE_SIZE - 8).unwrap(), 0x1234_5678);
    ostd::miri_println!("ram-size-small: 4 MiB segment ok, paddr {:#x}", segment.paddr());
    drop(segment);
    ostd::miri_println!("ram-size-small: freed");
}
