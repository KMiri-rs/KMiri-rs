//! KMiri model test: even if buggy kernel code calls `kern_miri_alloc_pages`
//! on a non-RAM hole page and accesses it through the linear mapping, KMiri
//! leaves the hole unbacked and reports a dangling/no-memory access UB.
#![no_std]
#![feature(format_args_nl)]

extern crate alloc;

use ostd::{
    boot::{boot_info, memory_region::MemoryRegionType},

};

unsafe extern "Rust" {
    fn kern_miri_alloc_pages(paddr: usize, count: usize);
}

#[ostd::ktest::miri_main]
fn kernel_main() {
    let hole = boot_info()
        .memory_regions
        .iter()
        .find(|r| r.typ() == MemoryRegionType::Unknown)
        .expect("expected an Unknown hole region in boot memory map");
    let paddr = hole.base();
    ostd::miri_println!("memory-map-hole-unbacked: probing hole paddr={:#x}", paddr);

    // Even if a buggy kernel tries to back the hole page, KMiri must leave it unbacked.
    unsafe {
        kern_miri_alloc_pages(paddr, 1);
        let ptr = (0xffff_8000_0000_0000usize + paddr) as *const u8;
        let val = core::ptr::read_volatile(ptr);
        ostd::miri_println!("memory-map-hole-unbacked: unexpectedly read {:#x}", val);
    }
}
