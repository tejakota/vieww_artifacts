//! Compiles and dexes the camera shim, following vavlt's build script to the
//! letter — the pattern is proven across that app's CI.

use std::env;
use std::path::PathBuf;

/// The API level the shim is compiled against, kept equal to
/// `target_sdk_version` in the app manifests.
const TARGET_SDK: u32 = 34;

fn main() {
    // Nothing below applies to a host-side `cargo check` or `cargo test`.
    if !env::var("TARGET").unwrap_or_default().contains("android") {
        return;
    }

    let java_src = "java/ThreeCameraShim.java";
    println!("cargo:rerun-if-changed={java_src}");
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
        .nowarn(true)
        .command()
        .expect("javac command")
        .args(["-encoding", "UTF-8"])
        .output()
        .expect("run javac");
    if !out.status.success() {
        panic!(
            "javac failed on the camera shim:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let dex_dir = out_dir.join("dex");
    let _ = std::fs::remove_dir_all(&dex_dir);
    std::fs::create_dir_all(&dex_dir).expect("create dex dir");

    let out = android_build::Dexer::new()
        .android_jar(&android_jar)
        .class_path(&classes_dir)
        .android_min_api(24)
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
            "d8 failed on the camera shim:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    println!(
        "cargo:rustc-env=THREE_DEX={}",
        dex_dir.join("classes.dex").display()
    );
}
