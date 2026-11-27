// SPDX-License-Identifier: MPL-2.0

//! Two (or more) CPUs. CPU 1 runs a pinned task that halts in a loop until
//! `FLAG` is set. CPU 0 sets `FLAG` and wakes CPU 1 with an inter-processor
//! call; CPU 1 must observe only a small number of halt-loop iterations.

#![no_std]
#![deny(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

#[path = "../../smp-ap-tasks/src/pin_sched.rs"]
mod pin_sched;

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};

use ostd::{
    cpu::{CpuId, CpuSet, num_cpus},
    smp::inter_processor_call,
    task::{Task, TaskOptions, halt_cpu},
};
use pin_sched::Pinned;

static CPU1_READY: AtomicBool = AtomicBool::new(false);
static FLAG: AtomicBool = AtomicBool::new(false);
static CPU1_ITERS: AtomicUsize = AtomicUsize::new(0);
static CPU1_DONE: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static DRIVER_DONE: AtomicBool = AtomicBool::new(false);

fn ap_entry() {
    let cpu = pin_sched::cpu();
    TaskOptions::new(move || {
        if cpu == 1 {
            CPU1_READY.store(true, SeqCst);
            let mut n = 0usize;
            while !FLAG.load(SeqCst) {
                halt_cpu();
                n += 1;
            }
            ostd::miri_println!("smp-halt-idle: cpu1 n={n}");
            CPU1_ITERS.store(n, SeqCst);
            CPU1_DONE.store(true, SeqCst);
        } else {
            while !STOP.load(SeqCst) {
                Task::yield_now();
            }
        }
    })
    .data(Pinned(cpu))
    .spawn()
    .unwrap();
}

fn wait_until(what: &str, cond: impl Fn() -> bool) {
    for _ in 0..10_000 {
        if cond() {
            return;
        }
        Task::yield_now();
    }
    panic!("timed out waiting until {what}");
}

#[ostd::ktest::miri_main]
fn kernel_main() {
    assert!(num_cpus() >= 2);
    pin_sched::install(ap_entry);

    TaskOptions::new(|| {
        wait_until("CPU 1 task started", || CPU1_READY.load(SeqCst));
        for _ in 0..10 {
            Task::yield_now();
        }

        FLAG.store(true, SeqCst);
        let cpu1 = CpuId::try_from(1usize).unwrap();
        inter_processor_call(&CpuSet::from(cpu1), || {}).wait();

        wait_until("CPU 1 finished", || CPU1_DONE.load(SeqCst));
        let n = CPU1_ITERS.load(SeqCst);
        assert!(n <= 3, "expected CPU 1 halt loop iterations <= 3, got {n}");

        STOP.store(true, SeqCst);
        ostd::miri_println!("smp-halt-idle: ok");
        DRIVER_DONE.store(true, SeqCst);
    })
    .spawn()
    .unwrap();

    while !DRIVER_DONE.load(SeqCst) {
        Task::yield_now();
    }
}
