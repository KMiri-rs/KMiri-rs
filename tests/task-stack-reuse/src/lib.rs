#![no_std]
#![deny(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

use alloc::boxed::Box;
use core::cell::Cell;
use core::sync::atomic::{AtomicBool, Ordering::SeqCst};

use ostd::task::{Task, TaskOptions};

struct Payload([u64; 4]);

static TASK1_DONE: AtomicBool = AtomicBool::new(false);
static TASK2_DONE: AtomicBool = AtomicBool::new(false);

fn run_loop(label: &str) -> (usize, usize) {
    let cell: Cell<Option<Box<Payload>>> = Cell::new(None);
    let mut first = 0usize;
    let mut last = 0usize;
    for i in 0..300u64 {
        let _old = cell.replace(Some(Box::new(Payload([i; 4]))));
        if i == 1 || i == 299 {
            let probe = 0u8;
            let addr = core::ptr::addr_of!(probe) as usize;
            if i == 1 {
                first = addr;
            } else {
                last = addr;
            }
        }
    }
    ostd::miri_println!("{label}: i=1 addr={first:#x}, i=299 addr={last:#x}");
    (first, last)
}

#[ostd::ktest::miri_main]
fn kernel_main() {
    let (main_first, main_last) = run_loop("kernel_main");
    assert_eq!(
        main_first, main_last,
        "kernel_main stack address drifted: {main_first:#x} -> {main_last:#x}"
    );

    let _task1 = TaskOptions::new(|| {
        let (task_first, task_last) = run_loop("spawned_task_1");
        TASK1_DONE.store(true, SeqCst);
        assert_eq!(
            task_first, task_last,
            "spawned_task_1 stack address drifted: {task_first:#x} -> {task_last:#x}"
        );
    })
    .spawn()
    .unwrap();

    let _task2 = TaskOptions::new(|| {
        let (task_first, task_last) = run_loop("spawned_task_2");
        TASK2_DONE.store(true, SeqCst);
        assert_eq!(
            task_first, task_last,
            "spawned_task_2 stack address drifted: {task_first:#x} -> {task_last:#x}"
        );
    })
    .spawn()
    .unwrap();

    while !TASK1_DONE.load(SeqCst) || !TASK2_DONE.load(SeqCst) {
        Task::yield_now();
    }

    ostd::miri_println!("task-stack-reuse: ok");
}
