//! Regression test for F8: calling `Task::run` twice on the same `Arc<Task>`
//! (1-CPU and 2-CPU concurrent) must not cause a KMiri-only data race on
//! `TaskContext::kmiri_thread`.
#![no_std]
#![forbid(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

use alloc::{boxed::Box, collections::VecDeque, sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use ostd::{
    cpu::{CpuId, PinCurrentCpu, num_cpus},
    sync::SpinLock,
    task::{
        Task, TaskOptions, disable_preempt,
        scheduler::{EnqueueFlags, LocalRunQueue, Scheduler, UpdateFlags, info::CommonSchedInfo},
    },
};

struct Pinned(usize);

fn current_cpu() -> usize {
    u32::from(disable_preempt().current_cpu()) as usize
}

struct PinScheduler {
    rq: Vec<SpinLock<RunQueue>>,
}

struct RunQueue {
    current: Option<Arc<Task>>,
    queue: VecDeque<Arc<Task>>,
}

impl Scheduler for PinScheduler {
    fn enqueue(&self, runnable: Arc<Task>, _flags: EnqueueFlags) -> Option<CpuId> {
        let place = runnable.data().downcast_ref::<Pinned>().map_or(0, |p| p.0);
        let place = CpuId::try_from(place).unwrap();
        let (still_in_rq, target) = match runnable.cpu().set_if_is_none(place) {
            Err(cpu) => (true, cpu),
            Ok(()) => (false, place),
        };
        let mut rq = self.rq[u32::from(target) as usize].disable_irq().lock();
        if still_in_rq && runnable.cpu().set_if_is_none(target).is_err() {
            return None;
        }
        rq.queue.push_back(runnable);
        None
    }

    fn local_rq_with(&self, f: &mut dyn FnMut(&dyn LocalRunQueue)) {
        let guard = disable_preempt();
        let rq: &RunQueue = &self.rq[u32::from(guard.current_cpu()) as usize].disable_irq().lock();
        f(rq);
    }

    fn mut_local_rq_with(&self, f: &mut dyn FnMut(&mut dyn LocalRunQueue)) {
        let guard = disable_preempt();
        let rq: &mut RunQueue =
            &mut self.rq[u32::from(guard.current_cpu()) as usize].disable_irq().lock();
        f(rq);
    }
}

impl LocalRunQueue for RunQueue {
    fn current(&self) -> Option<&Arc<Task>> {
        self.current.as_ref()
    }

    fn update_current(&mut self, flags: UpdateFlags) -> bool {
        match flags {
            UpdateFlags::Tick => false,
            UpdateFlags::Wait | UpdateFlags::Yield | UpdateFlags::Exit => !self.queue.is_empty(),
        }
    }

    fn try_pick_next(&mut self) -> Option<&Arc<Task>> {
        let next = self.queue.pop_front()?;
        if let Some(prev) = self.current.replace(next) {
            self.queue.push_back(prev);
        }
        self.current.as_ref()
    }

    fn dequeue_current(&mut self) -> Option<Arc<Task>> {
        self.current.take().inspect(|task| task.cpu().set_to_none())
    }
}

static AP_READY: AtomicBool = AtomicBool::new(false);
static GO: AtomicBool = AtomicBool::new(false);
static AP_DONE: AtomicBool = AtomicBool::new(false);
static TARGET: SpinLock<Option<Arc<Task>>> = SpinLock::new(None);
static SINGLE_RAN: AtomicUsize = AtomicUsize::new(0);
static SMP_RAN: AtomicUsize = AtomicUsize::new(0);
static DRIVER_DONE: AtomicBool = AtomicBool::new(false);

fn ap_entry() {
    let cpu = current_cpu();
    TaskOptions::new(move || {
        AP_READY.store(true, SeqCst);
        while !GO.load(SeqCst) {
            core::hint::spin_loop();
        }
        let target = TARGET.lock().as_ref().unwrap().clone();
        for _ in 0..4 {
            target.run();
            core::hint::spin_loop();
        }
        AP_DONE.store(true, SeqCst);
        loop {
            Task::yield_now();
        }
    })
    .data(Pinned(cpu))
    .spawn()
    .expect("spawn ap worker");
}

#[ostd::ktest::miri_main]
fn kernel_main() {
    assert_eq!(num_cpus(), 2);
    let rq = (0..num_cpus())
        .map(|_| SpinLock::new(RunQueue { current: None, queue: VecDeque::new() }))
        .collect();
    ostd::task::scheduler::inject_scheduler(Box::leak(Box::new(PinScheduler { rq })));
    ostd::boot::smp::register_ap_entry(ap_entry);

    TaskOptions::new(|| {
        // Phase 1: 1-CPU double-run on BSP while task is in runqueue.
        let t1 = Arc::new(
            TaskOptions::new(|| {
                SINGLE_RAN.fetch_add(1, SeqCst);
                Task::yield_now();
            })
            .data(Pinned(0))
            .build()
            .expect("build t1"),
        );
        t1.run();
        t1.run();
        while SINGLE_RAN.load(SeqCst) < 1 {
            Task::yield_now();
        }
        assert_eq!(SINGLE_RAN.load(SeqCst), 1);

        // Phase 2: 2-CPU concurrent double-run across BSP and AP.
        while !AP_READY.load(SeqCst) {
            core::hint::spin_loop();
        }
        let t2 = Arc::new(
            TaskOptions::new(|| {
                SMP_RAN.fetch_add(1, SeqCst);
                Task::yield_now();
            })
            .data(Pinned(0))
            .build()
            .expect("build t2"),
        );
        *TARGET.lock() = Some(t2.clone());
        GO.store(true, SeqCst);
        for _ in 0..4 {
            t2.run();
            core::hint::spin_loop();
        }
        while !AP_DONE.load(SeqCst) {
            core::hint::spin_loop();
        }
        while SMP_RAN.load(SeqCst) < 1 {
            Task::yield_now();
        }
        assert_eq!(SMP_RAN.load(SeqCst), 1);
        DRIVER_DONE.store(true, SeqCst);
    })
    .data(Pinned(0))
    .spawn()
    .expect("spawn driver");

    while !DRIVER_DONE.load(SeqCst) {
        Task::yield_now();
    }
}
