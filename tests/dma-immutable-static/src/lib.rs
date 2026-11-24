//! Asterinas issue #3905: safe code that holds an `IoMem<Insensitive>` of a
//! DMA-capable device can make the device write any physical memory, including
//! an immutable static, when no IOMMU restricts its DMA.
//!
//! The kernel drives the virtio block device that `kmiri.toml` adds at
//! 0xfeb00000, as QEMU microvm has one there, with nothing but safe OSTD
//! APIs: `IoMem::acquire` for its registers and `DmaCoherent` for its rings.
//!
//! - Control: a read request into `DmaCoherent` memory completes, and the
//!   buffer holds the disk's bytes.
//! - The issue: the same request with the data buffer at the physical address
//!   of `static VICTIM` makes the device write the immutable static.
//!
//! Expected: "control ok" is printed, then KMiri reports a write to read-only
//! memory at the MMIO write that notifies the device.
#![no_std]
#![forbid(unsafe_code)]
#![feature(format_args_nl)]

extern crate alloc;

use ostd::{
    io::IoMem,
    mm::{HasPaddr, VmIo, VmIoOnce, dma::DmaCoherent},
};

/// QEMU microvm's first virtio-mmio slot.
const VIRTIO_MMIO: core::ops::Range<usize> = 0xfeb0_0000..0xfeb0_0200;
/// On x86-64 OSTD links the kernel image at this fixed offset from its
/// physical address (`KERNEL_CODE_BASE_VADDR` in `ostd/src/mm/kspace`).
const KERNEL_IMAGE_OFFSET: usize = 0xffff_ffff_8000_0000;

// Offsets in the first page of `Blk::dma`; the data buffer is the second page.
const DESC: usize = 0x000;
const AVAIL: usize = 0x100;
const USED: usize = 0x200;
const HEADER: usize = 0x300;
const STATUS: usize = 0x310;
const DATA: usize = 0x1000;
const QUEUE_SIZE: u16 = 4;
const SECTOR_SIZE: usize = 512;

static VICTIM: [u8; SECTOR_SIZE] = [0; SECTOR_SIZE];

/// A minimal virtio-mmio block driver: version 2 registers, one queue.
struct Blk {
    regs: IoMem,
    dma: DmaCoherent,
}

impl Blk {
    /// Initializes the device as in VIRTIO 1.3, 3.1.1.
    fn new() -> Self {
        let regs = IoMem::acquire(VIRTIO_MMIO).unwrap();
        let dma = DmaCoherent::alloc(2, true).unwrap();
        let blk = Self { regs, dma };
        assert_eq!(blk.reg(0x000), 0x7472_6976, "magic");
        assert_eq!(blk.reg(0x004), 2, "version");
        assert_eq!(blk.reg(0x008), 2, "block device");
        blk.set_reg(0x070, 0);
        blk.set_reg(0x070, 1 | 2);
        blk.set_reg(0x014, 1);
        assert_eq!(blk.reg(0x010) & 1, 1, "VIRTIO_F_VERSION_1");
        blk.set_reg(0x024, 1);
        blk.set_reg(0x020, 1);
        blk.set_reg(0x070, 1 | 2 | 8);
        assert_eq!(blk.reg(0x070) & 8, 8, "FEATURES_OK");
        blk.set_reg(0x030, 0);
        assert!(blk.reg(0x034) >= u32::from(QUEUE_SIZE));
        blk.set_reg(0x038, u32::from(QUEUE_SIZE));
        let base = blk.dma.paddr() as u64;
        for (reg, offset) in [(0x080, DESC), (0x090, AVAIL), (0x0a0, USED)] {
            let addr = base + offset as u64;
            blk.set_reg(reg, addr as u32);
            blk.set_reg(reg + 4, (addr >> 32) as u32);
        }
        blk.set_reg(0x044, 1);
        blk.set_reg(0x070, 1 | 2 | 8 | 4);
        blk
    }

    fn reg(&self, offset: usize) -> u32 {
        self.regs.read_once::<u32>(offset).unwrap()
    }

    fn set_reg(&self, offset: usize, value: u32) {
        self.regs.write_once::<u32>(offset, &value).unwrap();
    }

    fn set_desc(&self, index: usize, addr: usize, len: u32, flags: u16, next: u16) {
        let desc = DESC + 16 * index;
        self.dma.write_val(desc, &(addr as u64)).unwrap();
        self.dma.write_val(desc + 8, &len).unwrap();
        self.dma.write_val(desc + 12, &flags).unwrap();
        self.dma.write_val(desc + 14, &next).unwrap();
    }

    /// Reads `sector` into the 512 bytes at physical address `buf`, and
    /// returns the request's status byte.
    fn read_sector(&self, sector: u64, buf: usize) -> u8 {
        self.dma.write_val(HEADER, &0u32).unwrap(); // VIRTIO_BLK_T_IN
        self.dma.write_val(HEADER + 4, &0u32).unwrap();
        self.dma.write_val(HEADER + 8, &sector).unwrap();
        self.dma.write_val(STATUS, &0xffu8).unwrap();
        let base = self.dma.paddr();
        self.set_desc(0, base + HEADER, 16, 1, 1);
        self.set_desc(1, buf, SECTOR_SIZE as u32, 2 | 1, 2);
        self.set_desc(2, base + STATUS, 1, 2, 0);

        let idx: u16 = self.dma.read_val(AVAIL + 2).unwrap();
        let slot = AVAIL + 4 + 2 * usize::from(idx % QUEUE_SIZE);
        self.dma.write_val(slot, &0u16).unwrap();
        self.dma.write_val(AVAIL + 2, &idx.wrapping_add(1)).unwrap();
        self.set_reg(0x050, 0); // QueueNotify

        while self.dma.read_val::<u16>(USED + 2).unwrap() != idx.wrapping_add(1) {
            core::hint::spin_loop();
        }
        self.dma.read_val(STATUS).unwrap()
    }
}

#[ostd::ktest::miri_main]
fn kernel_main() {
    let blk = Blk::new();

    let status = blk.read_sector(1, blk.dma.paddr() + DATA);
    assert_eq!(status, 0);
    for i in 0..SECTOR_SIZE {
        let byte: u8 = blk.dma.read_val(DATA + i).unwrap();
        assert_eq!(byte, ((SECTOR_SIZE + i) % 251) as u8);
    }
    ostd::miri_println!("dma-immutable-static: control ok");

    let victim = core::ptr::addr_of!(VICTIM) as usize - KERNEL_IMAGE_OFFSET;
    blk.read_sector(1, victim);
    ostd::miri_println!("dma-immutable-static: VICTIM[1] = {}", VICTIM[1]);
}
