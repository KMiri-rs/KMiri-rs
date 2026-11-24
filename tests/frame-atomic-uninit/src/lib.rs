//! A fallible atomic load of a word on an untyped frame that was never written
//! reads uninitialized memory into a `u32`.
//!
//! Expected: Undefined Behavior (uninitialized memory) at the load.
#![no_std]
#![deny(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

use ostd::mm::{FrameAllocOptions, UFrame, io::util::HasVmReaderWriter};

#[ostd::ktest::miri_main]
fn kernel_main() {
    let mut options = FrameAllocOptions::new();
    options.zeroed(false);
    let frame: UFrame = options.alloc_frame().unwrap().into();
    ostd::miri_println!("frame-atomic-uninit: before load");
    let value = frame.reader().to_fallible().atomic_load::<u32>();
    ostd::miri_println!("frame-atomic-uninit: loaded {:?}", value.is_ok());
}
