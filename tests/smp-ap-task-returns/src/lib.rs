// SPDX-License-Identifier: MPL-2.0

//! Three CPUs. Each AP runs one task pinned to it, and the task returns. With no
//! other task to run, the AP stops; its boot context stays suspended in the
//! switch to that task until the machine powers off, which is not a leak.
//! KMiri must report nothing.

#![no_std]
#![deny(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

mod pin_sched;

use core::sync::atomic::{AtomicUsize, Ordering::SeqCst};

use ostd::{
    cpu::num_cpus,
    task::{Task, TaskOptions},
};
use pin_sched::Pinned;

const NUM_CPUS: usize = 3;

static DONE: AtomicUsize = AtomicUsize::new(0);

fn ap_entry() {
    let cpu = pin_sched::cpu();
    TaskOptions::new(move || {
        assert_eq!(pin_sched::cpu(), cpu, "a pinned task left its CPU");
        DONE.fetch_add(1, SeqCst);
    })
    .data(Pinned(cpu))
    .spawn()
    .unwrap();
}

#[ostd::ktest::miri_main]
fn kernel_main() {
    assert_eq!(num_cpus(), NUM_CPUS);
    pin_sched::install(ap_entry);

    for _ in 0..10_000 {
        if DONE.load(SeqCst) == NUM_CPUS - 1 {
            ostd::miri_println!("smp-ap-task-returns: ok");
            return;
        }
        Task::yield_now();
    }
    panic!("timed out waiting for the AP tasks");
}
