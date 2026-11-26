// SPDX-License-Identifier: MPL-2.0

//! Two (or more) CPUs. CPU 0 issues inter-processor calls to CPU 1 and to both
//! CPUs and waits for each call to complete.

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
    task::{Task, TaskOptions},
};
use pin_sched::Pinned;

static CPU1_READY: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static DRIVER_DONE: AtomicBool = AtomicBool::new(false);

static LAST_CPU: AtomicUsize = AtomicUsize::new(usize::MAX);
static CALLS_CPU0: AtomicUsize = AtomicUsize::new(0);
static CALLS_CPU1: AtomicUsize = AtomicUsize::new(0);

fn record_cpu() {
    let cpu = pin_sched::cpu();
    LAST_CPU.store(cpu, SeqCst);
    match cpu {
        0 => {
            CALLS_CPU0.fetch_add(1, SeqCst);
        }
        1 => {
            CALLS_CPU1.fetch_add(1, SeqCst);
        }
        other => panic!("unexpected CPU {other} in record_cpu"),
    }
}

fn ap_entry() {
    let cpu = pin_sched::cpu();
    TaskOptions::new(move || {
        if cpu == 1 {
            CPU1_READY.store(true, SeqCst);
        }
        while !STOP.load(SeqCst) {
            Task::yield_now();
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

        let cpu0 = CpuId::try_from(0usize).unwrap();
        let cpu1 = CpuId::try_from(1usize).unwrap();

        // (a) Single remote call targeting CPU 1.
        inter_processor_call(&CpuSet::from(cpu1), record_cpu).wait();
        assert_eq!(LAST_CPU.load(SeqCst), 1);
        assert_eq!(CALLS_CPU0.load(SeqCst), 0);
        assert_eq!(CALLS_CPU1.load(SeqCst), 1);

        // (b) Call targeting both CPU 0 (synchronous) and CPU 1 (remote IPI).
        let mut both = CpuSet::new_empty();
        both.add(cpu0);
        both.add(cpu1);
        inter_processor_call(&both, record_cpu).wait();
        assert_eq!(CALLS_CPU0.load(SeqCst), 1);
        assert_eq!(CALLS_CPU1.load(SeqCst), 2);

        // (c) Repeat (a) 3 times.
        for round in 0..3 {
            LAST_CPU.store(usize::MAX, SeqCst);
            inter_processor_call(&CpuSet::from(cpu1), record_cpu).wait();
            assert_eq!(LAST_CPU.load(SeqCst), 1);
            assert_eq!(CALLS_CPU0.load(SeqCst), 1);
            assert_eq!(CALLS_CPU1.load(SeqCst), 3 + round);
        }

        STOP.store(true, SeqCst);
        ostd::miri_println!("smp-ipi-call: ok");
        DRIVER_DONE.store(true, SeqCst);
    })
    .spawn()
    .unwrap();

    while !DRIVER_DONE.load(SeqCst) {
        Task::yield_now();
    }
}
