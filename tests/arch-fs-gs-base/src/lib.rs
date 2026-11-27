#![no_std]
#![forbid(unsafe_code)]
#![feature(format_args_nl, stmt_expr_attributes)]

extern crate alloc;

use ostd::arch::cpu::context::{FsBase, GsBase, USER_MODIFIABLE_RFLAGS};

#[ostd::ktest::miri_main]
fn kernel_main() {
    assert_ne!(USER_MODIFIABLE_RFLAGS, 0);

    let mut saved_fs = FsBase::default();
    saved_fs.save();
    let fs = FsBase::new(0x1234_5678);
    assert_eq!(fs.addr(), 0x1234_5678);
    fs.load();
    let mut read_fs = FsBase::default();
    read_fs.save();
    assert_eq!(read_fs.addr(), 0x1234_5678);
    saved_fs.load();

    {
        let irq_guard = ostd::irq::disable_local();
        let mut saved_gs = GsBase::default();
        saved_gs.save(&irq_guard);
        let gs = GsBase::new(0x8765_4321);
        assert_eq!(gs.addr(), 0x8765_4321);
        gs.load(&irq_guard);
        let mut read_gs = GsBase::default();
        read_gs.save(&irq_guard);
        assert_eq!(read_gs.addr(), 0x8765_4321);
        saved_gs.load(&irq_guard);
    }

    ostd::miri_println!("arch-fs-gs-base: ok");
}
