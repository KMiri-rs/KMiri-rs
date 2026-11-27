#!/bin/bash
# Runs the unit tests of the BlueOS kernel under KMiri. See README.md.
#
#   ./run.sh setup        once: patched std, Miri sysroot, gnrt
#   ./run.sh build        build the kernel's dependencies for the kmiri board
#   ./run.sh test [SECS]  interpret the unit-test kernel (default timeout 3000 s);
#                         exits 1 if undefined behavior is reported
#
# Environment: BLUEOS_KERNEL (default: ./kernel), BLUEOS_WORK (default: ./work),
# KMIRI_FLAGS (Miri flags, default when unset: -Zmiri-disable-stacked-borrows).
set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
KERNEL=${BLUEOS_KERNEL:-$HERE/kernel}
WORK=${BLUEOS_WORK:-$HERE/work}
export RUSTUP_TOOLCHAIN=nightly-2026-11-16
OUT=$KERNEL/out/kmiri.debug.dsc
SYSROOT=$WORK/miri-sysroot
LIB=$WORK/rust-lib
TARGET=$KERNEL/build/scripts/kmiri/riscv64-vivo-blueos.json

export GNRT_BIN=$KERNEL/out/host-tools/gnrt
export KMIRI_TOML=$HERE/kmiri.toml KMIRI_MIRI=miri KMIRI_HOST_RUSTC=rustc
export KMIRI_FLAGS=${KMIRI_FLAGS--Zmiri-disable-stacked-borrows}

setup() {
    mkdir -p "$WORK"
    # std for the BlueOS target: the nightly library with BlueOS's own std
    # patch, whose libc is the one in the kernel tree.
    rm -rf "$LIB"
    mkdir -p "$LIB"
    cp -r "$(rustc --print sysroot)/lib/rustlib/src/rust/library" "$LIB/"
    sed "s|@BLUEOS_LIBC@|$KERNEL/libs/libc|g" "$HERE/std/blueos-std.patch" |
        (cd "$LIB/library" && patch -p1 -s)
    (cd /tmp && MIRI_LIB_SRC=$LIB/library MIRI_SYSROOT=$SYSROOT \
        cargo miri setup --target "$TARGET")
    # gnrt writes the GN files of the vendored crates.
    cargo install --locked --root "$WORK/gnrt" --path "$KERNEL/external/gnrt" --bin gnrt
    mkdir -p "$(dirname "$GNRT_BIN")"
    cp "$WORK/gnrt/bin/gnrt" "$GNRT_BIN"
    (cd "$KERNEL" && "$GNRT_BIN" sync --all)
}

build() {
    (cd "$KERNEL" && gn gen "$OUT" --args="board=\"kmiri\" build_type=\"debug\" syscall_mode=\"dsc\" miri_sysroot=\"$SYSROOT\"")
    # Everything the unit-test kernel needs; building that target itself
    # would interpret it.
    local deps
    deps=$(ninja -C "$OUT" -t query bin/kernel_unittest |
        awk '/^  input:/ { f = 1; next } /^  outputs:/ { f = 0 } f { sub(/^ *(\|\| |\| )?/, ""); print }' |
        sort -u)
    # shellcheck disable=SC2086
    ninja -C "$OUT" $deps
}

# The kernel's panic handler ends in `loop {}` (natively too), so the run
# stops once the panic report is printed.
run_test() {
    local secs=${1:-3000} cmd log pid rc= ub
    cd "$OUT"
    cmd=$(ninja -t commands bin/kernel_unittest | tail -1)
    log=$(mktemp)
    setsid timeout "$secs" bash -c "$cmd" >"$log" 2>&1 &
    pid=$!
    tail -f --pid=$pid "$log" &
    while kill -0 $pid 2>/dev/null; do
        if grep -q '^Oops: ' "$log" && grep -q '^Memory: total=' "$log"; then
            sleep 2
            kill -- -$pid 2>/dev/null || true
            rc=panic
            break
        fi
        sleep 2
    done
    wait $pid || rc=${rc:-$?}
    sleep 1
    ub=$(grep -c '^error: Undefined Behavior' "$log" || true)
    echo "== passed: $(grep -c '^\[       OK \]' "$log"), undefined behavior reported: $ub, end: ${rc:-0}"
    rm -f "$log"
    [ "$ub" = 0 ]
}

case ${1:-} in
setup) setup ;;
build) build ;;
test) run_test "${2:-}" ;;
*)
    sed -n '2,10p' "$0" | sed '/^set /d'
    exit 2
    ;;
esac
