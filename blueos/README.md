# BlueOS under KMiri

Runs the unit tests of the [vivo BlueOS](https://github.com/vivoblueos/kernel)
kernel (riscv64, `qemu_riscv64` debug or release configuration) in the KMiri interpreter,
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
interpret, so the tests after it are not reached. On 2 cores this needs three
BlueOS fixes that the board branch carries (upstream PRs
[vivoblueos/kernel#592](https://github.com/vivoblueos/kernel/pull/592),
[#593](https://github.com/vivoblueos/kernel/pull/593),
[#594](https://github.com/vivoblueos/kernel/pull/594)); without them KMiri
stops at the first one (two data races and a failing test).

## One test at a time

KMiri stops at the first undefined behavior or unsupported operation, so a run
of all tests ends at the first such test and hides the ones after it. The
kernel can run a single test instead: it reads a libtest-style selection from
the semihosting command line, which `BLUEOS_MIRI_TEST_FILTER` sets (e.g.
`BLUEOS_MIRI_TEST_FILTER="--exact tests::stress_trap" ./run.sh test`).

```sh
./run.sh list            # the test names
./run.sh sweep 1800 6    # each test alone, at most 1800 s each, 6 at a time
```

`sweep` writes one line per test, `name  outcome  seconds`, to
`work/sweep/results.tsv` and its log to `work/sweep/logs/`. The outcome is the
first that applies of `ub`, `unsupported` (an operation KMiri cannot
interpret), `timeout`, `failed` (the test panicked), `ignored`, `ok` and
`other`.

Expected now (about 15 minutes on 32 cores): 240 tests, 235 `ok`, 1
`unsupported` (`tests::stress_trap`, see above) and 4 `ub`:
`vfs::dirent::tests::test_dirent`, `vfs::dirent::tests::test_dirent_long_name`,
`vfs::syscalls::tests::test_getdents_current_dir` and
`vfs::syscalls::tests::test_getdents_parent_dir` make a `&Dirent` from a byte
array that is only 1-aligned. On 1 core (`CONFIG_SMP` off, no `[smp]`) there
is one more test, `time::timer::tests::test_timer_edge_cases`, which passes.

## Release build

`BLUEOS_BUILD_TYPE=release` selects the kernel's release build type for all
commands (`kernel/kconfig/config/kmiri/release`, the `qemu_riscv64` release
configuration on 2 cores; output in `kernel/out/kmiri.release.dsc`):

```sh
BLUEOS_BUILD_TYPE=release ./run.sh build
BLUEOS_BUILD_TYPE=release ./run.sh sweep 1800 6
```

As in the native release build, debug assertions, overflow checks and
`ub_checks` are off and code under `cfg(debug_assertions)` is left out (BlueOS
then uses half the thread stack sizes and smaller stress tests). GN's Miri mode
appends `-Copt-level=0`, which would turn debug assertions on; the kernel's
`build/scripts/kmiri/kmiri_rustc.py` turns them off again. `sweep` writes to
the same `work/sweep` for both build types.

Expected now: 239 tests (`vfs::procfs::stat::tests::test_running_idle_cycles`
needs `CONFIG_THREAD_STATS`, which release leaves off), 230 `ok`, the same 4
`ub` and 1 `unsupported` as in debug, and 4 `other`:
`allocator::tests::coalescing_test` and the buddy tests
`wrapper_reads_and_writes_real_backing_pages`,
`wrapper_translates_addresses_and_handles_overflow` and
`shared_local_heap_serializes_concurrent_operations` run out of thread stack
(`resource exhaustion: there are no more free addresses in the address
space`). KMiri interprets unoptimized MIR, where every local and temporary has
its own stack slot; natively all four pass.

## Differences from native `qemu_riscv64`

Listed in the descriptions of the board
([KMiri-rs/vivoblueos-kernel#25](https://github.com/KMiri-rs/vivoblueos-kernel/pull/25),
[#26](https://github.com/KMiri-rs/vivoblueos-kernel/pull/26),
[#27](https://github.com/KMiri-rs/vivoblueos-kernel/pull/27)): 2 cores
instead of 32; clock and software interrupts come from KMiri's CLINT model,
which has no `mie` register and resets `mtimecmp` to the maximum.
Stacked Borrows is off by default (`KMIRI_FLAGS`).
