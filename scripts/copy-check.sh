#!/usr/bin/env bash
#
# Does a copy outlive the editor?
#
#   scripts/copy-check.sh [binary] [image]
#
# Opens the image in the editor, lets the editor end the way Ctrl+C does
# (COSMIC_EXT_SNIP_DUMP_COPY), then watches the process that holds the copy
# for WAIT seconds (default 30) and compares the clipboard with the PNG the
# editor copies. The wait is the test: the fault this guards against killed
# that process 4 to 12 seconds after the copy.
#
# Needs a COSMIC Wayland session and target/release/examples/clipboard_check
# (`just copy-check` builds it). It opens a window for a moment and replaces
# what is on the clipboard; the clipboard is empty afterwards.
set -euo pipefail

bin=${1:-target/release/cosmic-ext-snip}
image=${2:-data/screenshots/editor.png}
wait_s=${WAIT:-30}
check=target/release/examples/clipboard_check

[ -x "$check" ] || { echo "copy-check: $check is missing: cargo build --release --example clipboard_check" >&2; exit 1; }
command -v "$bin" >/dev/null || { echo "copy-check: no binary $bin" >&2; exit 1; }

tmp=$(mktemp -d)
pid=
cleanup() {
    if [ -n "$pid" ]; then kill "$pid" 2>/dev/null || true; fi
    rm -rf "$tmp"
}
trap cleanup EXIT

COSMIC_SINGLE_INSTANCE=0 COSMIC_EXT_SNIP_DUMP="$tmp/window.png" COSMIC_EXT_SNIP_DUMP_COPY=1 \
    "$bin" "$image" 2>"$tmp/log" &
pid=$!

# The editor copies right after it has saved its own rendering.
for _ in $(seq 1 100); do
    [ -s "$tmp/window.png" ] && break
    sleep 0.2
done
[ -s "$tmp/window.png" ] || { echo "copy-check: the editor never rendered $image" >&2; cat "$tmp/log" >&2; exit 1; }

for second in $(seq 1 "$wait_s"); do
    sleep 1
    if ! kill -0 "$pid" 2>/dev/null; then
        status=0
        wait "$pid" || status=$?
        pid=
        echo "copy-check: the process holding the copy ended $second s after it (status $status; 139 is SIGSEGV)" >&2
        tail -3 "$tmp/log" >&2
        exit 1
    fi
done

served_by="this process, after the app ended (the fallback)"
if tr '\0' ' ' <"/proc/$pid/cmdline" | grep -q -- '--serve-clipboard'; then
    served_by="a fresh image of the process"
fi

"$check" "$image"
echo "copy-check: held for $wait_s s by $served_by"
