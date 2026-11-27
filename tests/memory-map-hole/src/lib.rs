//! Verifies that KMiri's default firmware memory map contains a non-RAM hole
//! (typed `Unknown` after `into_non_overlapping`) and a `Reserved` region, and
//! that safe `Frame::from_unused` on pages in both returns `Err(InUse)`, as natively.
#![no_std]
#![deny(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

use ostd::{
    boot::{boot_info, memory_region::MemoryRegionType},
    mm::{Frame, frame::meta::GetFrameError},
};

#[ostd::ktest::miri_main]
fn kernel_main() {
    let regions = &boot_info().memory_regions;
    let mut unknown_paddr = None;
    let mut reserved_paddr = None;

    for r in regions.iter() {
        match r.typ() {
            MemoryRegionType::Unknown if unknown_paddr.is_none() => {
                unknown_paddr = Some(r.base());
            }
            MemoryRegionType::Reserved if reserved_paddr.is_none() => {
                reserved_paddr = Some(r.base());
            }
            _ => {}
        }
    }

    let u = unknown_paddr.expect("expected an Unknown hole region in boot memory map");
    let res = reserved_paddr.expect("expected a Reserved region in boot memory map");

    let err_u = Frame::<()>::from_unused(u, ()).unwrap_err();
    assert!(matches!(err_u, GetFrameError::InUse));
    let err_r = Frame::<()>::from_unused(res, ()).unwrap_err();
    assert!(matches!(err_r, GetFrameError::InUse));

    ostd::miri_println!(
        "memory-map-hole: unknown={:#x} reserved={:#x} from_unused=Err(InUse)",
        u,
        res
    );
}
