//! Compiles and dexes the one Java file in the repo.
//!
//! Dexing at build time is what lets the picker shim exist without a Gradle
//! project: `jni_bridge.rs` embeds the resulting `classes.dex` with
//! `include_bytes!` and hands it to `InMemoryDexClassLoader` at startup.
//! `accesskit_android`, which vieww uses for TalkBack, loads its own delegate
//! the same way — so this pattern is now the framework's as well as ours.
//!
//! **There is no UI compilation step any more.** The Slint build ran the
//! `.slint` compiler here over eleven files; the interface is Rust now, and
//! `cargo` knows how to build Rust.

use std::env;
use std::path::PathBuf;

/// The API level the shim is compiled against.
///
/// **Kept equal to `target_sdk_version` in `Cargo.toml` by hand**, because
/// `cargo-apk` reads that key and this build script cannot. If they drift, the
/// shim is compiled against an API surface the manifest does not claim — which
/// javac will not complain about, and which shows up as a `NoSuchMethodError`
/// on a device rather than as a build failure.
const TARGET_SDK: u32 = 34;

/// The API level `d8` desugars for.
///
/// Equal to `min_sdk_version`, not to `TARGET_SDK`. This is the *floor* — the
/// oldest device the dex has to load on — and setting it too high is how a
/// build produces a `classes.dex` that the minimum-spec phone in the manifest
/// cannot read. It was 30 while the manifest said 30; the manifest now says 24.
const MIN_SDK: u32 = 24;

fn main() {
    // Nothing below applies to a host-side `cargo check`.
    if !env::var("TARGET").unwrap_or_default().contains("android") {
        return;
    }

    let java_src = "java/VavltPickerShim.java";
    println!("cargo:rerun-if-changed={java_src}");

    // `android_build` reads these, so a change to one has to re-run this
    // script. Without them, switching SDKs leaves a stale `classes.dex`
    // embedded in the binary and nothing says so.
    for key in [
        "ANDROID_HOME",
        "ANDROID_SDK_ROOT",
        "ANDROID_PLATFORM",
        "ANDROID_BUILD_TOOLS_VERSION",
        "JAVA_HOME",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
    }

    let out_dir: PathBuf = env::var_os("OUT_DIR").expect("OUT_DIR").into();
    let classes_dir = out_dir.join("java-classes");
    let _ = std::fs::remove_dir_all(&classes_dir);
    std::fs::create_dir_all(&classes_dir).expect("create classes dir");

    // Pinned rather than "whichever platform is newest".
    //
    // `android_jar(None)` picks the highest installed platform and *says so* in
    // a warning — `ANDROID_PLATFORM environment variable is not set, using
    // 'android-37.0'`. That is a build compiling against API 37 while the
    // manifest claims 34, decided by which SDK the developer happened to
    // install last. Two machines with different SDKs then produce different
    // APKs from the same commit, and neither build mentions it.
    //
    // Falling back to the newest is still better than failing: somebody with
    // only API 36 installed should get a working build and a warning, not a
    // wall.
    let platform = format!("android-{TARGET_SDK}");
    let android_jar = android_build::android_jar(Some(&platform))
        .or_else(|| {
            println!(
                "cargo:warning=no {platform} in the SDK — falling back to the newest installed \
                 platform. Install it with `sdkmanager \"platforms;{platform}\"` so this build \
                 matches its own manifest."
            );
            android_build::android_jar(None)
        })
        .expect("no Android platform found — set ANDROID_HOME and install a platform");

    let out = android_build::JavaBuild::new()
        .file(java_src)
        .class_path(&android_jar)
        .classes_out_dir(&classes_dir)
        .java_source_version(8)
        .java_target_version(8)
        // android.app.Fragment is deprecated; it is also the only fragment API
        // reachable without AndroidX, which would drag in Gradle.
        .nowarn(true)
        .command()
        .expect("javac command")
        .args(["-encoding", "UTF-8"])
        .output()
        .expect("run javac");
    if !out.status.success() {
        panic!(
            "javac failed on the picker shim:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let dex_dir = out_dir.join("dex");
    let _ = std::fs::remove_dir_all(&dex_dir);
    std::fs::create_dir_all(&dex_dir).expect("create dex dir");

    let out = android_build::Dexer::new()
        .android_jar(&android_jar)
        .class_path(&classes_dir)
        .android_min_api(MIN_SDK)
        .release(env::var("PROFILE").as_deref() == Ok("release"))
        .out_dir(&dex_dir)
        .collect_classes(&classes_dir)
        .expect("collect classes")
        .command()
        .expect("d8 command")
        .output()
        .expect("run d8");
    if !out.status.success() {
        panic!(
            "d8 failed on the picker shim:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    println!("cargo:rustc-env=VAVLT_DEX={}", dex_dir.join("classes.dex").display());
}
