#![no_std]
#![forbid(unsafe_code)]
#![feature(format_args_nl, stmt_expr_attributes)]

extern crate alloc;

use ostd::{cpu::CpuId, cpu_local};

#[repr(align(65536))]
struct OverAligned(u64);

cpu_local! {
    static OVER_ALIGNED: OverAligned = OverAligned(0x1234_5678);
}

#[ostd::ktest::miri_main]
fn kernel_main() {
    let val = OVER_ALIGNED.get_on_cpu(CpuId::bsp()).0;
    assert_eq!(val, 0x1234_5678);
    ostd::miri_println!("cpu-local-overaligned: ok");
}
