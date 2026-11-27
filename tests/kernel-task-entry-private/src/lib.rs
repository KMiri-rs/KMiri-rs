#![no_std]
#![forbid(unsafe_code)]
#![feature(format_args_nl, stmt_expr_attributes)]

extern crate alloc;

#[ostd::ktest::miri_main]
fn kernel_main() {
    #[cfg(miri)]
    ostd::task::scheduler::kernel_task_entry(0);
}
