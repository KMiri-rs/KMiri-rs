#!/bin/bash
# Runs the unit tests of the BlueOS kernel under KMiri. See README.md.
#
#   ./run.sh setup        once: patched std, Miri sysroot, gnrt
#   ./run.sh build        build the kernel's dependencies for the kmiri board
#   ./run.sh test [SECS]  interpret the unit-test kernel (default timeout 3000 s);
#                         exits 1 if undefined behavior is reported
#   ./run.sh list         print the names of the unit tests
#   ./run.sh sweep [SECS] [JOBS]
#                         interpret the kernel once per test, JOBS at a time
#                         (default 4), each with a timeout of SECS (default
#                         3000); results in $BLUEOS_WORK/sweep
#
# Environment: BLUEOS_KERNEL (default: ./kernel), BLUEOS_WORK (default: ./work),
# KMIRI_FLAGS (Miri flags, default when unset: -Zmiri-disable-stacked-borrows),
# BLUEOS_MIRI_TEST_FILTER (tests `test` runs, in libtest form: name filters,
# --exact; default: all).
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

# How the log says the run ended: panic, done or nothing yet.
log_end() {
    if grep -q "^Oops: " "$1" && grep -q "^Memory: total=" "$1"; then
        echo panic
    elif grep -q "^---- Done kernel unittests" "$1"; then
        echo done
    fi
}

# Interprets the unit-test kernel once, with the tests BLUEOS_MIRI_TEST_FILTER
# selects; the output goes to file $2, and to stdout as well if $3 is given.
# The kernel's panic handler ends in `loop {}` (natively too), and the kernel
# idles once the tests are done, so the run stops after the panic report or
# the "Done" line. Sets END to how the run ended: panic, done, or the exit
# status (124: timeout).
interpret() {
    local secs=$1 log=$2 cmd pid
    END=
    cmd=$(cd "$OUT" && ninja -t commands bin/kernel_unittest | tail -1)
    : >"$log"
    (cd "$OUT" && exec setsid timeout "$secs" bash -c "$cmd") >"$log" 2>&1 &
    pid=$!
    if [ -n "${3:-}" ]; then tail -f --pid=$pid "$log" & fi
    while kill -0 $pid 2>/dev/null; do
        END=$(log_end "$log")
        if [ -n "$END" ]; then
            sleep 2
            kill -- -$pid 2>/dev/null || true
            break
        fi
        sleep 2
    done
    END=${END:-$(log_end "$log")}
    wait $pid || END=${END:-$?}
    END=${END:-0}
    sleep 1
}

# Diagnostics are colored (the build passes --color=always).
plain() {
    sed 's/\x1b\[[0-9;]*m//g' "$1"
}

run_test() {
    local log ub
    log=$(mktemp)
    interpret "${1:-3000}" "$log" show
    ub=$(plain "$log" | grep -c '^error: Undefined Behavior' || true)
    echo "== passed: $(grep -c '^\[       OK \]' "$log"), undefined behavior reported: $ub, end: $END"
    rm -f "$log"
    [ "$ub" = 0 ]
}

list() {
    local log
    log=$(mktemp)
    BLUEOS_MIRI_TEST_FILTER=--list interpret 3000 "$log"
    if [ "$END" != done ]; then
        cat "$log" >&2
        rm -f "$log"
        echo "listing the tests failed (end: $END)" >&2
        exit 1
    fi
    sed -n 's/: test$//p' "$log"
    rm -f "$log"
}

# The outcome of one test, from its log and how the run ended. In this order
# (as Miri's crates.io evaluation ranks them): ub, unsupported (an operation
# KMiri cannot interpret), timeout, failed (the test panicked), ignored, ok;
# anything else is other (e.g. a deadlock or a build error).
outcome() {
    local text
    text=$(plain "$1")
    if grep -q '^error: Undefined Behavior' <<<"$text"; then
        echo ub
    elif grep -q '^error: unsupported operation' <<<"$text"; then
        echo unsupported
    elif [ "$2" = 124 ]; then
        echo timeout
    elif [ "$2" = panic ]; then
        echo failed
    elif [ "$2" = done ] && grep -q '^\[  IGNORED \]' <<<"$text"; then
        echo ignored
    elif [ "$2" = done ] && grep -q '^\[       OK \]' <<<"$text"; then
        echo ok
    else
        echo other
    fi
}

# Runs test $1 alone and appends "name, outcome, seconds" to the results.
sweep_one() {
    local name=$1 secs=$2 dir=$WORK/sweep start log
    log=$dir/logs/$name.log
    start=$(date +%s)
    BLUEOS_MIRI_TEST_FILTER="--exact $name" interpret "$secs" "$log"
    printf '%s\t%s\t%s\n' "$name" "$(outcome "$log" "$END")" "$(($(date +%s) - start))" >>"$dir/results.tsv"
}

sweep() {
    local secs=${1:-3000} jobs=${2:-4} dir=$WORK/sweep
    rm -rf "$dir"
    mkdir -p "$dir/logs"
    # KMiri appends a trace of every address translation to miri_debugger.log
    # in its working directory, one write per line; parallel runs share the
    # directory and then spend their time waiting on that file.
    ln -sf /dev/null "$OUT/miri_debugger.log"
    list >"$dir/tests"
    echo "== $(wc -l <"$dir/tests") tests, $jobs at a time"
    touch "$dir/results.tsv"
    xargs -P "$jobs" -I{} "$0" sweep-one {} "$secs" <"$dir/tests" || true
    sort -o "$dir/results.tsv" "$dir/results.tsv"
    cut -f2 "$dir/results.tsv" | sort | uniq -c | sort -rn
    echo "== $(wc -l <"$dir/results.tsv") results in $dir/results.tsv, logs in $dir/logs"
}

case ${1:-} in
setup) setup ;;
build) build ;;
test) run_test "${2:-}" ;;
list) list ;;
sweep) sweep "${2:-}" "${3:-}" ;;
sweep-one) sweep_one "$2" "$3" ;;
*)
    sed -n '2,19p' "$0" | sed '/^set /d'
    exit 2
    ;;
esac
