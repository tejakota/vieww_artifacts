# Building

Two hosts you can build today: Linux (the development harness) and Android (the
product). iOS needs a Mac and is at the bottom.

## Before anything

**vieww must be checked out beside this repo.** It is a path dependency, not a
vendored copy — see the comment in `Cargo.toml`. Nothing builds without it:

```bash
git clone https://github.com/teja/vieww ../vieww   # sibling of this directory
ls ../vieww/crates/vieww/Cargo.toml                # this is what the path points at
```

The tree cargo expects:

```
somewhere/
├── vieww/          # the framework
└── vavlt/          # this repo
```

Rust 1.85 or newer, stable. `rustup show` to check.

---

## Linux

### The app

```bash
cargo run --release -p vavlt-desktop                 # opens on ~/Pictures
cargo run --release -p vavlt-desktop /some/folder    # walks that instead
```

**In release.** A debug `vello` is roughly twenty times slower, and motion
judged by eye in a debug build is a judgement of `rustc -O0`. (The workspace
already forces `opt-level = 2` on dependencies in dev builds, so a debug build
of the *app* is a reasonable compromise while iterating.)

`RUST_LOG=debug` for the run loop, the grants and the pump:

```bash
RUST_LOG=debug cargo run --release -p vavlt-desktop ~/Pictures
```

### The measurement CLI

No window, no GPU — this is the harness the codec ratios come from.

```bash
cargo build --release -p vavlt-cli
./target/release/vault scan ~/Pictures
./target/release/vault measure ~/Pictures --limit 200 --json report.json
```

### Everything

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

`vavlt-android` builds to an empty `cdylib` on a Linux host — its JNI half is
`cfg`-gated — so a workspace build does not need an SDK.

### Screenshots

Renders every screen through the real GPU backend to `target/screenshots/`:
phone and desktop, dark and light, forty PNGs.

```bash
cargo test -p vavlt-app --test screenshots --release -- --nocapture
```

It needs a Vulkan adapter and **skips rather than fails** without one. On a
headless box, software Vulkan is enough:

```bash
sudo apt-get install -y mesa-vulkan-drivers vulkan-tools
vulkaninfo --summary        # a device under "Devices:" means it will run
```

### System packages

On a bare Debian/Ubuntu, winit and wgpu want:

```bash
sudo apt-get install -y \
    build-essential pkg-config cmake \
    libx11-dev libxkbcommon-dev libwayland-dev \
    libvulkan1 mesa-vulkan-drivers
```

---

## Android

### One-time setup

```bash
export ANDROID_HOME=$HOME/Android/Sdk
export ANDROID_NDK_ROOT=$ANDROID_HOME/ndk/27.3.13750724
rustup target add aarch64-linux-android
cargo install cargo-apk
```

The NDK version above is the one this repo has been built with. Any recent one
works; `ci/android-env.sh` finds it and **skips a partial download** — the SDK
manager leaves a version directory with no `source.properties` behind when an
install is abandoned, and picking that one fails with an error that names the
NDK path rather than the problem.

### The two `cargo-apk` warnings, and why they are gone

A build used to print:

```
warning: ANDROID_PLATFORM environment variable is not set, using 'android-37.0'.
warning: ANDROID_BUILD_TOOLS_VERSION environment variable is not set, using '36.0.0'.
```

That is `android-build` picking the *newest installed* platform to compile the
picker shim against — API 37, while the manifest claims `targetSdk 34`. Two
machines with different SDKs then produce different APKs from the same commit,
and neither build says so.

`crates/android/build.rs` now pins the platform to `TARGET_SDK`, and falls back
to the newest with a `cargo:warning` naming what to install if that platform is
missing. `d8` desugars for `MIN_SDK` (24), which is the manifest's floor rather
than its target — the old value was 30, left over from when `min_sdk_version`
was 30.

The build-tools warning is `cargo-apk`'s own and is harmless: it picks the
newest installed, and the tools are backwards-compatible in the way the
platform jar is not. Pin it if you want reproducibility across machines:

```bash
export ANDROID_BUILD_TOOLS_VERSION=36.0.0
```

You need a JDK for `javac` and `d8`: `build.rs` compiles and dexes
`crates/android/java/VavltPickerShim.java` at build time. That is the whole
Java toolchain this repo uses — there is no Gradle project, no Kotlin and no
AGP.

### Build and install

```bash
cargo apk run -p vavlt-android --release
```

That builds, installs and launches on the attached device. Release, for the
same reason the desktop run is.

APK only, no device:

```bash
cargo apk build -p vavlt-android --release
ls target/release/apk/vault.apk
```

Install it by hand:

```bash
adb install -r target/release/apk/vault.apk
```

### Logs

`cargo apk run` installs, launches, and *then* sometimes reports a failure while
trying to attach logcat — on a device with a second user profile. The app is
already running when that prints. Ignore it and attach yourself:

```bash
adb logcat --pid=$(adb shell pidof dev.vavlt.app)
```

Or filter to this app's own lines:

```bash
adb logcat -s vavlt_android:V RustStdoutStderr:V
```

### Cross-checking without a device

```bash
cargo check -p vavlt-android --target aarch64-linux-android
```

**This does not exercise the linker.** A cross-check produces rlibs, which never
link, so it passes on a machine with no NDK at all. The first thing that
actually links is `cargo apk build`. `.cargo/config.toml` in this repo points
the Android targets at `ci/ndk-clang-*.sh` so that linking works from a clean
shell; without it cargo falls back to the host `cc` and reports missing
`-llog`, `-lunwind` and `-landroid`, which reads like a broken system rather
than an unconfigured build.

### If the build fails

| Symptom | Cause |
|---|---|
| `cannot find -llog` / `-landroid` | No linker configured. Check `.cargo/config.toml` is present and `ANDROID_NDK_ROOT` is set. |
| `Error detecting NDK version for path` | A partial NDK download. Point `ANDROID_NDK_ROOT` at a directory that has `source.properties`. |
| `no Android SDK at ...` | `ANDROID_HOME` unset or wrong. |
| `javac`/`d8` not found | No JDK, or `ANDROID_HOME` has no `build-tools` installed. |
| App launches to a black screen | The first frame costs ~480ms on a mid-range phone — vello compiling its shaders on device. Wait a second before believing it. |

### What the APK declares

No `<uses-permission>` entries at all. No `INTERNET`, no media permissions.
Files arrive only as content URIs from the out-of-process photo picker, and
`has_code="false"` is honest — the picker shim and `accesskit_android`'s
delegate are both dexed into the `.so` and loaded through
`InMemoryDexClassLoader`.

---

## iOS

Needs a Mac. The tree it mounts is the one the screenshot suite photographs, so
the *interface* is as checked as the other two hosts; the `PHPicker` bridge in
`crates/ios/src/picker.rs` is written and **has never run on a device**.

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cargo build --release -p vavlt-ios --target aarch64-apple-ios
```

The binary still needs a bundle — a directory with an `Info.plist` beside it,
signed for the device. vieww's `ci/ios-app.sh` builds one and is the thing to
copy.

It also runs on your laptop, deliberately, so the entry point cannot rot
between releases:

```bash
cargo run --release -p vavlt-ios
```

The picker reports that there is none, and tells you to use `vavlt-desktop`.
