//! With overflow checks off, `alloc_segment((1 << 52) + 1)` computes its byte count
//! `nframes * PAGE_SIZE` with wrapping arithmetic, as in a release build. The
//! allocator, the segment and the zeroing all use that wrapped count (one page),
//! so the call returns a one-page segment. That is a logic error in the caller's
//! favour, not Undefined Behavior, and KMiri must not report the allocation.
//!
//! Expected: completes without Undefined Behavior; the segment has one page.
#![no_std]
#![deny(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

use ostd::mm::{FrameAllocOptions, HasSize, PAGE_SIZE, VmIo};

#[ostd::ktest::miri_main]
fn kernel_main() {
    let nframes = (1usize << 52) + 1;
    let segment = FrameAllocOptions::new().alloc_segment(nframes).unwrap();
    assert_eq!(segment.size(), PAGE_SIZE);
    // The page was zeroed and can be used.
    assert_eq!(segment.read_val::<u64>(PAGE_SIZE - 8).unwrap(), 0);
    segment.write_val::<u64>(0, &7).unwrap();
    drop(segment);
    ostd::miri_println!("segment-wrapped-size: one-page segment");
}
