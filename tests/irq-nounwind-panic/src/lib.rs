// SPDX-License-Identifier: MPL-2.0
//! Regression test for F9: panicking inside a timer IRQ callback (which runs
//! under , a nounwind ABI boundary) must
//! terminate with  instead of looping
//! forever in \x27s .
#![no_std]
#![forbid(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

use core::sync::atomic::{AtomicBool, Ordering};

static ARMED: AtomicBool = AtomicBool::new(false);

fn timer_cb() {
    if ARMED.swap(false, Ordering::AcqRel) {
        ostd::miri_println!("IRQ_NOUNWIND_PANIC_TRIGGER");
        panic!("timer callback panic");
    }
}

#[ostd::ktest::miri_main]
fn kernel_main() {
    ostd::timer::register_callback_on_cpu(timer_cb);
    ARMED.store(true, Ordering::Release);
    for _ in 0..10_000_000usize {
        core::hint::spin_loop();
    }
    panic!("timer callback never fired");
}
