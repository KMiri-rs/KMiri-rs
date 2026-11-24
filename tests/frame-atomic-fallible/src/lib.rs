//! The fallible atomics of `VmReader`/`VmWriter` on an untyped frame behave as
//! natively: `atomic_load` reads the word, and `atomic_compare_exchange`
//! replaces it only when it holds the expected value, returning the old value.
//!
//! Expected: completes without Undefined Behavior.
#![no_std]
#![deny(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

use ostd::mm::{Fallible, FrameAllocOptions, UFrame, VmReader, VmWriter, io::util::HasVmReaderWriter};

#[ostd::ktest::miri_main]
fn kernel_main() {
    let frame: UFrame = FrameAllocOptions::new().alloc_frame().unwrap().into();
    let at = |offset: usize| -> (VmReader<'_, Fallible>, VmWriter<'_, Fallible>) {
        let (mut reader, mut writer) = (frame.reader(), frame.writer());
        reader.skip(offset);
        writer.skip(offset);
        (reader.to_fallible(), writer.to_fallible())
    };
    let reader = || at(8).0;
    let writer = || at(8).1;
    {
        let mut w = frame.writer();
        w.skip(8);
        w.write_val::<u32>(&5).unwrap();
    }

    assert_eq!(reader().atomic_load::<u32>().unwrap(), 5);
    // A matching exchange stores the new value.
    assert_eq!(writer().atomic_compare_exchange::<u32>(&reader(), 5, 6).unwrap(), (5, true));
    // A mismatching one leaves the word alone.
    assert_eq!(writer().atomic_compare_exchange::<u32>(&reader(), 5, 7).unwrap(), (6, false));

    let word = |offset: usize| {
        let mut r = frame.reader();
        r.skip(offset);
        r.read_val::<u32>().unwrap()
    };
    assert_eq!(word(8), 6);
    // The neighbouring words are untouched.
    assert_eq!(word(4), 0);
    assert_eq!(word(12), 0);
    ostd::miri_println!("frame-atomic-fallible: done");
}
