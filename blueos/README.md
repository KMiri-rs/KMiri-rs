# BlueOS under KMiri

Runs the unit tests of the [vivo BlueOS](https://github.com/vivoblueos/kernel)
kernel (riscv64, `qemu_riscv64` debug configuration) in the KMiri interpreter,
so that undefined behavior in kernel code is reported where it happens.

- `kernel/`: submodule, the BlueOS kernel with the `kmiri` board
  ([KMiri-rs/vivoblueos-kernel](https://github.com/KMiri-rs/vivoblueos-kernel)).
  The kernel's own allocator runs unchanged; at its boundary the kernel tells
  KMiri which blocks it hands out (`kernel/kernel/src/allocator/notify.rs`).
- `kmiri.toml`: the KMiri machine: the `qemu_riscv64` memory map, the heap as a
  `[[pool]]`, the NS16550 console, the link-section tables.
- `std/blueos-std.patch`: BlueOS's patch to the Rust standard library for its
  target, against the library of nightly-2026-11-16. `@BLUEOS_LIBC@` stands for
  the kernel's `libs/libc`.
- `run.sh`: setup, build and run.

## Requirements

Linux (tested on Ubuntu 24.04) with `gn`, `ninja`, `clang-18`/`lld-18`,
`python3` with `kconfiglib` and `tomli`, `bindgen` 0.72.1 (`cargo install
bindgen-cli@0.72.1 --locked`, the version BlueOS's own Dockerfile installs) and
rustup with the `nightly-2026-11-16` toolchain and its `rust-src`, `rustc-dev`
and `llvm-tools` components. KMiri installed from this
repository: `cd kmiri && ./miri install` puts `miri` and `cargo-miri` into the
`nightly-2026-11-16` toolchain, whose `bin` directory must then be on `PATH`
(`export PATH="$(rustc +nightly-2026-11-16 --print sysroot)/bin:$PATH"`).

## Steps

```sh
git submodule update --init blueos/kernel kmiri
cd blueos
./run.sh setup   # once: patched std, Miri sysroot for the BlueOS target, gnrt
./run.sh build
./run.sh test    # prints the test log, then a summary line
```

Expected now: 169 tests pass and no undefined behavior is reported; the run
then stops at `stress_trap`, whose `ecall` in inline assembly KMiri cannot
interpret, so the tests after it are not reached.

## Differences from native `qemu_riscv64`

Listed in the descriptions of the board
([KMiri-rs/vivoblueos-kernel#25](https://github.com/KMiri-rs/vivoblueos-kernel/pull/25),
[#26](https://github.com/KMiri-rs/vivoblueos-kernel/pull/26)): one core
instead of SMP; clock and software interrupts come from KMiri's CLINT model,
which has no `mie` register and resets `mtimecmp` to the maximum.
Stacked Borrows is off by default (`KMIRI_FLAGS`).
