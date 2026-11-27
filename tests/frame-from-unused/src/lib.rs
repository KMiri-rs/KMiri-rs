#![no_std]
#![forbid(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

use ostd::mm::io::util::HasVmReaderWriter;
use ostd::mm::{Frame, FrameAllocOptions, HasPaddr};
use ostd::power::{ExitCode, poweroff};

// Regression test: reclaiming an unused RAM frame via `Frame::from_unused`
// must give the page KMiri backing memory at the OSTD ownership point
// (`MetaSlot::get_from_unused`), not only on allocator paths.
//
// Expected: no UB, prints `frame-from-unused: val=0xa5`, then powers off
// deliberately before post-`kernel_main` allocations can reuse the frame
// still present in the global allocator free pool (asterinas#4295).
#[ostd::ktest::miri_main]
fn kernel_main() {
    let options = FrameAllocOptions::new();
    let frame = options.alloc_frame().unwrap();
    let paddr = frame.paddr();
    drop(frame);

    let reclaimed = Frame::from_unused(paddr, ()).unwrap();
    reclaimed.writer().write_val(&0xa5u8).unwrap();
    let val: u8 = reclaimed.reader().read_val().unwrap();
    assert_eq!(val, 0xa5u8);
    ostd::miri_println!("frame-from-unused: val={:#x}", val);
    core::mem::forget(reclaimed);
    poweroff(ExitCode::Success);
}
