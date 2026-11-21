// SPDX-License-Identifier: MPL-2.0

//! Three CPUs. A task on each AP adds to the same counter without a lock.
//! KMiri must report a data race. `smp-ap-tasks` is the control.
//!
//! Both tasks first wait until both have arrived, so that neither can finish
//! before the other starts. Otherwise the later task could take the heap lock
//! that the earlier one released when it exited, which orders all of the
//! earlier writes before the later ones, and there would be no race to report.

#![no_std]
#![feature(format_args_nl)]

extern crate alloc;

#[path = "../../smp-ap-tasks/src/pin_sched.rs"]
mod pin_sched;

use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};

use ostd::{
    cpu::num_cpus,
    task::{Task, TaskOptions},
};
use pin_sched::Pinned;

struct RacyCounter(UnsafeCell<u64>);

// SAFETY: None. Sharing this counter between CPUs is the bug under test.
unsafe impl Sync for RacyCounter {}

static COUNTER: RacyCounter = RacyCounter(UnsafeCell::new(0));
static ARRIVED: AtomicUsize = AtomicUsize::new(0);
static FINISHED: AtomicUsize = AtomicUsize::new(0);

fn ap_entry() {
    let cpu = pin_sched::cpu();
    TaskOptions::new(|| {
        // After this, neither task's writes are ordered with the other's.
        ARRIVED.fetch_add(1, SeqCst);
        while ARRIVED.load(SeqCst) < 2 {
            core::hint::spin_loop();
        }
        for _ in 0..100 {
            // SAFETY: Not safe: the task on the other AP writes concurrently.
            unsafe { *COUNTER.0.get() += 1 };
            core::hint::spin_loop();
        }
        FINISHED.fetch_add(1, SeqCst);
    })
    .data(Pinned(cpu))
    .spawn()
    .unwrap();
}

#[ostd::ktest::miri_main]
fn kernel_main() {
    assert_eq!(num_cpus(), 3);
    pin_sched::install(ap_entry);

    while FINISHED.load(SeqCst) < 2 {
        Task::yield_now();
        core::hint::spin_loop();
    }
    ostd::miri_println!("smp-data-race: finished without a report");
}
