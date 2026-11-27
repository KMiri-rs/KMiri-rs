#![no_std]
#![deny(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

use core::sync::atomic::{AtomicUsize, Ordering};

static TIMER_CALLBACKS: AtomicUsize = AtomicUsize::new(0);
static CALLBACK_INTERRUPT_LEVEL: AtomicUsize = AtomicUsize::new(0);

#[ostd::ktest::miri_main]
fn kernel_main() {
    ostd::timer::register_callback_on_cpu(|| {
        CALLBACK_INTERRUPT_LEVEL.store(
            if ostd::irq::InterruptLevel::current()
                == ostd::irq::InterruptLevel::L1(ostd::cpu::PrivilegeLevel::Kernel)
            {
                1
            } else {
                0
            },
            Ordering::SeqCst,
        );
        TIMER_CALLBACKS.fetch_add(1, Ordering::SeqCst);
    });

    // The nested guard must preserve the outer guard's disabled state. While
    // local IRQs are disabled, config-injected timer IRQs (`tick_interval_basic_blocks = 2000`)
    // are queued and must not run until the outer guard is dropped.
    let outer_irq = ostd::irq::disable_local();
    let inner_irq = ostd::irq::disable_local();
    let before = TIMER_CALLBACKS.load(Ordering::SeqCst);

    for _ in 0..4_000 {
        core::hint::spin_loop();
    }
    assert_eq!(TIMER_CALLBACKS.load(Ordering::SeqCst), before);

    drop(inner_irq);
    for _ in 0..4_000 {
        core::hint::spin_loop();
    }
    assert_eq!(TIMER_CALLBACKS.load(Ordering::SeqCst), before);

    drop(outer_irq);
    for _ in 0..10_000 {
        if TIMER_CALLBACKS.load(Ordering::SeqCst) > before {
            break;
        }
        core::hint::spin_loop();
    }
    assert!(TIMER_CALLBACKS.load(Ordering::SeqCst) > before);
    assert_eq!(CALLBACK_INTERRUPT_LEVEL.load(Ordering::SeqCst), 1);

    ostd::miri_println!("irq-simulation: pending timer delivered at L1 kernel context");
}
