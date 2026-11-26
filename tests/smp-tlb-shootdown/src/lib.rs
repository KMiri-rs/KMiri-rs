// SPDX-License-Identifier: MPL-2.0

//! Two (or more) CPUs. CPU 1 activates a shared `VmSpace` and yields with
//! interrupts enabled; CPU 0 maps and unmaps a page 4 times, dispatching and
//! synchronizing the remote TLB flush on each round.

#![no_std]
#![deny(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

#[path = "../../smp-ap-tasks/src/pin_sched.rs"]
mod pin_sched;

use alloc::sync::Arc;
use core::sync::atomic::{AtomicBool, Ordering::SeqCst};

use ostd::{
    cpu::num_cpus,
    mm::{CachePolicy, FrameAllocOptions, PageFlags, PageProperty, VmSpace},
    sync::SpinLock,
    task::{Task, TaskOptions},
};
use pin_sched::Pinned;

const ROUNDS: usize = 4;

static SHARED_VM: SpinLock<Option<Arc<VmSpace>>> = SpinLock::new(None);
static CPU1_ACTIVATED: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static DRIVER_DONE: AtomicBool = AtomicBool::new(false);

fn ap_entry() {
    let cpu = pin_sched::cpu();
    TaskOptions::new(move || {
        if cpu == 1 {
            let vm = loop {
                if let Some(vm) = SHARED_VM.lock().clone() {
                    break vm;
                }
                Task::yield_now();
            };
            vm.activate();
            CPU1_ACTIVATED.store(true, SeqCst);
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
        let vm = Arc::new(VmSpace::new());
        *SHARED_VM.lock() = Some(Arc::clone(&vm));

        wait_until("CPU 1 activated VmSpace", || CPU1_ACTIVATED.load(SeqCst));

        for _ in 0..ROUNDS {
            let frame = FrameAllocOptions::new().zeroed(true).alloc_frame().unwrap();
            let prop = PageProperty::new_user(PageFlags::RW, CachePolicy::Writeback);

            let preempt = ostd::task::disable_preempt();
            let mut cursor = vm.cursor_mut(&preempt, &(0x1000..0x2000)).unwrap();
            cursor.map(frame.into(), prop);
            cursor.jump(0x1000).unwrap();
            cursor.unmap(0x1000);
            cursor.flusher().dispatch_tlb_flush();
            cursor.flusher().sync_tlb_flush();
        }

        STOP.store(true, SeqCst);
        ostd::miri_println!("smp-tlb-shootdown: rounds={ROUNDS}");
        ostd::miri_println!("smp-tlb-shootdown: ok");
        DRIVER_DONE.store(true, SeqCst);
    })
    .spawn()
    .unwrap();

    while !DRIVER_DONE.load(SeqCst) {
        Task::yield_now();
    }
}
