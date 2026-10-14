#!/usr/bin/env bash
#
# Close the hardware-only items in PENDING.md on one Linux laptop and one
# Android phone, and collect everything into one tarball.
#
#   ci/hardware/close-remaining.sh            # all steps
#   ci/hardware/close-remaining.sh gpu        # only some: gpu nvidia gallery android talkback monitors
#
# Run from vieww_base/. Every step runs even when an earlier one failed; each
# writes its own log, and the summary at the end says which passed. Attach the
# tarball it prints (or paste summary.txt) back into the conversation.
#
# What each step closes:
#   gpu       PENDING §1.1  Vulkan suites + a GPU frame in a real window, on the
#                           laptop's own driver (not lavapipe, not SwiftShader)
#   nvidia    PENDING §1.1  the same suites on the discrete NVIDIA GPU (PRIME offload)
#   gallery   PENDING §4.1  the fixture gallery's timings on this machine
#   android   PENDING §1.5  the in-app device suite on a real phone: real GPU frame
#                           times count here, unlike on the emulator
#   talkback  PENDING §1.5  the semantics tree with TalkBack actually bound, the
#                           modal check, Maestro on the phone, and one minute of
#                           listening (prompted)
#   monitors  PENDING §3.2  drag the demo window between two monitors; the
#                           refresh-rate line must reappear (prompted)
#
# Needs: Rust (the pinned toolchain), Vulkan drivers (vulkaninfo), and for the
# phone steps adb, the Android SDK + NDK (ANDROID_HOME, default ~/Android/Sdk),
# cargo-apk (`cargo install cargo-apk`), Java 17+ for Maestro, USB debugging on.

set -uo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root" || exit 1
stamp="$(date +%Y%m%d-%H%M%S)"
out="$root/hardware-results-$stamp"
mkdir -p "$out"
summary="$out/summary.txt"

steps=("$@")
[ ${#steps[@]} -eq 0 ] && steps=(gpu nvidia gallery android talkback monitors)
want() { local s; for s in "${steps[@]}"; do [ "$s" = "$1" ] && return 0; done; return 1; }

interactive=0
[ -t 0 ] && interactive=1
pause() {
    if [ "$interactive" = 1 ]; then
        read -r -p "$1 — press Enter to continue " _
    else
        echo "(not interactive: skipping the prompt \"$1\")"
    fi
}

# run NAME CMD... — run, tee to NAME.log, record pass/fail.
run() {
    local name="$1"; shift
    echo
    echo "==> $name: $*"
    if "$@" 2>&1 | tee "$out/$name.log"; then
        echo "PASS  $name" | tee -a "$summary"
    else
        echo "FAIL  $name (see $name.log)" | tee -a "$summary"
    fi
}

{
    echo "vieww hardware run $stamp"
    echo "commit: $(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
    echo "kernel: $(uname -r)"
    echo "session: ${XDG_SESSION_TYPE:-unknown} ${XDG_CURRENT_DESKTOP:-}"
    rustc -V 2>/dev/null
    echo
} > "$summary"
command -v vulkaninfo >/dev/null && vulkaninfo --summary > "$out/vulkaninfo.txt" 2>&1
command -v lspci >/dev/null && lspci | grep -Ei 'vga|3d|display' > "$out/gpus.txt" 2>&1
# The real driver, never a leftover software ICD from a CI-style shell.
unset VK_ICD_FILENAMES VK_DRIVER_FILES

if want gpu; then
    run gpu-hal    cargo test --release -p vieww-hal --features vulkan -- --ignored --test-threads=1
    run gpu-3d     cargo test --release -p vieww-3d  --features vulkan -- --ignored --test-threads=1
    run gpu-window env VIEWW_REQUIRE_HARDWARE=display,gpu \
                   cargo test --release -p vieww-platform-winit --test wait_loop
fi

if want nvidia; then
    if command -v nvidia-smi >/dev/null 2>&1 || lspci 2>/dev/null | grep -qi nvidia; then
        run nvidia-hal env __NV_PRIME_RENDER_OFFLOAD=1 __VK_LAYER_NV_optimus=NVIDIA_only \
                       cargo test --release -p vieww-hal --features vulkan -- --ignored --test-threads=1
    else
        echo "SKIP  nvidia (no NVIDIA GPU found)" | tee -a "$summary"
    fi
fi

if want gallery; then
    run gallery cargo run --release -p fixtures -- "$out/fixtures"
fi

phone=0
if want android || want talkback; then
    if command -v adb >/dev/null && adb get-state >/dev/null 2>&1; then
        phone=1
        {
            echo "model: $(adb shell getprop ro.product.model | tr -d '\r')"
            echo "android: $(adb shell getprop ro.build.version.release | tr -d '\r')"
            echo "abi: $(adb shell getprop ro.product.cpu.abi | tr -d '\r')"
            echo "gpu: $(adb shell dumpsys SurfaceFlinger 2>/dev/null | grep -m1 GLES | tr -d '\r')"
        } > "$out/phone.txt"
        cat "$out/phone.txt" >> "$summary"
    else
        echo "SKIP  android, talkback (no phone on adb — USB debugging on and authorised?)" | tee -a "$summary"
    fi
fi

if [ "$phone" = 1 ]; then
    # The first real run (2026-10-14) stopped here twice, on setup rather than
    # on vieww: the pinned toolchain had no `aarch64-linux-android` std, and an
    # older build of the app was installed only for a second user profile
    # (user 10) while the scripts ask about user 0.
    abi="$(adb shell getprop ro.product.cpu.abi | tr -d '\r')"
    case "$abi" in
        arm64-v8a) rustup target add aarch64-linux-android > "$out/rustup-target.log" 2>&1 ;;
        x86_64) rustup target add x86_64-linux-android > "$out/rustup-target.log" 2>&1 ;;
    esac
fi

if want android && [ "$phone" = 1 ]; then
    # Never on an emulator here: the point is the real GPU's frame times, which
    # device-suite.sh counts fully on a phone.
    run android-suite ci/mobile/device-suite.sh
    # The same build again, on the planned GPU rasterizer: the CPU path above
    # was 37ms median on the first real run. Its gpu_frames line says whether
    # the GPU actually drew (a frame it cannot plan, or a GPU that will not
    # build, falls back to the CPU).
    run android-suite-gpu ci/mobile/device-suite.sh --gpu
    adb shell setprop debug.vieww.gpu 0 >/dev/null 2>&1 || true
    for f in android-suite android-suite-gpu; do
        {
            echo "$f: $(grep -o 'frame_time: median [0-9.]*ms, worst [0-9.]*ms' "$out/$f.log" | head -1)"
            echo "$f: $(grep -o 'gpu_frames: .*' "$out/$f.log" | head -1)"
        } >> "$summary"
    done
fi

if want talkback && [ "$phone" = 1 ]; then
    # Installed for some user but not user 0 (a work profile / second space):
    # make it available to user 0, which is who the scripts launch as.
    if ! adb shell pm list packages --user 0 2>/dev/null | grep -q 'dev.vieww.demo'; then
        adb shell pm install-existing --user 0 dev.vieww.demo > "$out/install-existing.log" 2>&1 || true
    fi
    run a11y-talkback ci/mobile/a11y-android.sh --keep
    run a11y-modal    ci/mobile/a11y-android.sh --modal
    # Maestro taps through TalkBack differently; run it with TalkBack off.
    ci/mobile/a11y-android.sh --off > "$out/a11y-off.log" 2>&1
    VIEWW_MAESTRO_OUT="$out/maestro-phone" run maestro-phone ci/mobile/maestro.sh android
    echo
    echo "Listening check: TalkBack is being switched on for one minute of use."
    ci/mobile/a11y-android.sh --keep > /dev/null 2>&1
    # The first listening check heard nothing at all. Writing TalkBack into
    # the settings is not the same as TalkBack running and speaking, so record
    # what the phone says about each link in that chain.
    sleep 3
    {
        echo "--- accessibility services (is TalkBack bound?)"
        adb shell settings get secure enabled_accessibility_services
        adb shell settings get secure accessibility_enabled
        adb shell dumpsys accessibility 2>/dev/null | grep -iE "talkback|bound services|enabled services|touch exploration" | head -20
        echo "--- talkback package"
        adb shell dumpsys package com.google.android.marvin.talkback 2>/dev/null | grep -E "versionName|enabled=" | head -4
        echo "--- speech engine"
        adb shell settings get secure tts_default_synth
        adb shell pm list packages 2>/dev/null | grep -iE "tts|speech|google.android.tts" | head
        echo "--- volumes (accessibility stream is 10, media 3)"
        adb shell cmd media_session volume --stream 10 --get 2>&1 | tail -2
        adb shell cmd media_session volume --stream 3 --get 2>&1 | tail -2
    } > "$out/talkback-state.txt" 2>&1
    if grep -qi "talkback" <(adb shell dumpsys accessibility 2>/dev/null | grep -i "bound services"); then
        echo "talkback-state: bound" | tee -a "$summary"
    else
        echo "talkback-state: NOT bound — the setting was written but the service is not running (see talkback-state.txt)" | tee -a "$summary"
    fi
    echo "  If you hear nothing at all, even on the status bar: check the phone's"
    echo "  accessibility volume (Settings > Accessibility > TalkBack > Settings), and"
    echo "  try turning TalkBack on by hand there once — some phones refuse a service"
    echo "  enabled only through settings."
    echo "  On the phone: swipe right through the demo screen, double-tap 'Tap me',"
    echo "  open the menu and swipe through it. Does every control announce sensibly?"
    pause "Done listening"
    if [ "$interactive" = 1 ]; then
        read -r -p "Did TalkBack announce everything sensibly? [y/n/notes] " heard
        echo "talkback-by-ear: ${heard:-no answer}" | tee -a "$summary"
    fi
    ci/mobile/a11y-android.sh --off >> "$out/a11y-off.log" 2>&1
fi

if want monitors; then
    echo
    echo "Two-monitor check: the demo opens now. Drag its window to the other monitor,"
    echo "wait two seconds, drag it back, then close the window."
    RUST_LOG=info timeout 300 cargo run --release -p vieww-platform-winit --example desktop \
        > "$out/monitors.log" 2>&1
    count="$(grep -c 'reports .*Hz' "$out/monitors.log" || true)"
    rates="$(grep -o 'reports [0-9.]*Hz' "$out/monitors.log" | sort -u | tr '\n' ' ')"
    if [ "${count:-0}" -ge 2 ]; then
        echo "PASS  monitors ($count refresh-rate reports: $rates)" | tee -a "$summary"
    else
        echo "FAIL  monitors (only ${count:-0} refresh-rate report(s): $rates — was the window moved?)" | tee -a "$summary"
    fi
fi

echo
echo "================ summary ================"
cat "$summary"
tar -czf "$out.tar.gz" -C "$(dirname "$out")" "$(basename "$out")"
echo
echo "Everything is in $out.tar.gz — attach it (or paste summary.txt) in the conversation."
