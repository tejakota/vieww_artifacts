//! The Android entry point — a frame on a phone.
//!
//! ```console
//! # one device attached, ANDROID_HOME and ANDROID_NDK_ROOT set
//! cargo apk run -p vieww-platform-winit --example android --release
//!
//! # what it says while it runs
//! adb logcat -s vieww:V RustStdoutStderr:V
//! ```
//!
//! # Why this is an example and not a binary
//!
//! Android does not call `main`. The activity starts, loads a shared library
//! and calls `android_main` inside it — so this target is a `cdylib` (declared
//! in `Cargo.toml`, since a plain example would be built as an executable that
//! nothing on the device knows how to start).
//!
//! # Why it is almost empty
//!
//! Because the interesting work was already done. `App` opens its window on
//! `resumed` and drops the surface on `suspended`, which is the Android
//! lifecycle rather than a desktop one — a desktop window just never exercises
//! the second half. The only thing a phone genuinely needs that a desktop does
//! not is the `AndroidApp` handle, because the activity, its looper and its
//! surface all exist before any Rust runs and the event loop has to be built
//! around them rather than conjured.
//!
//! If this file ever grows, something has gone wrong in the layer below it.

/// Called by `android-activity`'s glue once the activity exists.
///
/// Plain Rust ABI rather than `extern "C"`: the glue looks the symbol up by
/// name and calls it as Rust, and `AndroidApp` is not `repr(C)` — declaring it
/// `extern "C"` would be an improper-ctypes bug that happens to work until it
/// does not.
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(android: vieww_platform_winit::AndroidApp) {
    demo::main(android);
}

// At the top level rather than inside `demo`: a `#[path]` on a module nested in
// an inline one resolves against *that* module's directory, so it would look
// for `examples/demo/shared/`.
#[cfg(target_os = "android")]
#[path = "shared/screen.rs"]
mod screen;

#[cfg(target_os = "android")]
#[path = "shared/device_tests.rs"]
mod device_tests;

#[cfg(target_os = "android")]
mod demo {
    use vieww_foundation::crash::{CrashKind, CrashReport, CrashSink, Crashes, FileSink};
    // For `screen()`'s platform argument. `ios.rs` has always imported this and
    // this file never did — the call was added to both entry points at once and
    // only one of them is built by anything that runs locally.
    use vieww_foundation::TargetPlatform;
    use vieww_foundation::{Clipboard, SharedServices};
    use vieww_platform_winit::{crash, AndroidApp, App};
    use vieww_widget::Inherited;

    use crate::device_tests::DeviceSuite;
    use crate::screen;

    /// Send whatever the last run died without being able to report.
    ///
    /// A real app would upload these. This one logs them, which on a phone is
    /// the same thing: logcat is where `adb` and every crash-collection SDK
    /// look, and the point being demonstrated is that a report *written by a
    /// process that then died* is readable by the next one.
    ///
    /// Before the reporter is installed, so this run's own crashes cannot be
    /// mistaken for the last one's. Returns how many there were, for the report.
    fn drain_previous_crashes(dir: &std::path::Path) -> usize {
        let pending = FileSink::pending(dir);
        for previous in &pending {
            log::error!("vieww: a previous run crashed:\n{}", previous.body);
            // Only after it has been logged. A report deleted on the way to a
            // failed send is gone; one sent twice is merely untidy.
            previous.remove();
        }
        pending.len()
    }

    /// Write a report into the app's own storage and read it straight back.
    ///
    /// # Why this is a device check and not a unit test
    ///
    /// `vieww-foundation` already tests the round trip, on a host, into a
    /// scratch directory. What that cannot establish is the part that is
    /// actually in doubt on a phone: that `internal_data_path` returns a real
    /// path, that the app is allowed to create a directory inside it, that a
    /// write survives, and that `pending` reads it back under Android's
    /// filesystem rather than a desktop one. Every one of those is a fact about
    /// the device, and all of them are load-bearing — a crash reporter whose
    /// directory silently does not exist is worse than none, because it reports
    /// nothing while looking installed.
    ///
    /// Self-contained on purpose: it writes, reads, asserts and deletes within
    /// one launch. The alternative — write now, check on the next launch —
    /// would fail on the first run after a fresh install, and a suite that
    /// fails on a legitimate state is a suite people learn to ignore.
    ///
    /// It does **not** test the panic hook. That half runs mid-unwind and is
    /// pure `std`, so it is tested where it can be tested properly: on the
    /// host, in `vieww_foundation::crash`.
    fn check_crash_storage(dir: &std::path::Path) -> (bool, String) {
        // Distinctive enough that this can only ever find its own file, so a
        // genuine crash report sitting in the directory is neither counted as a
        // success nor deleted by a diagnostic.
        const MARKER: &str = "vieww device storage check";

        FileSink::new(dir).deliver(&CrashReport {
            kind: CrashKind::Reported,
            message: MARKER.to_owned(),
            location: None,
            thread: String::from("android_main"),
            // Empty: capturing one costs milliseconds and this is a filesystem
            // check, not a report anybody will read.
            backtrace: String::new(),
            unix_millis: 0,
            context: Vec::new(),
        });

        let mine: Vec<_> = FileSink::pending(dir)
            .into_iter()
            .filter(|report| report.body.contains(MARKER))
            .collect();
        for report in &mine {
            report.remove();
        }

        match mine.len() {
            1 => (
                true,
                format!("wrote and read one back in {}", dir.display()),
            ),
            found => (
                false,
                format!("{found} of 1 readable back from {}", dir.display()),
            ),
        }
    }

    /// Will this process `dlopen` a library out of its own writable directory?
    ///
    /// # This is the entire Android hot-reload question
    ///
    /// The loader needs nothing new — `dlopen` exists, `android_main` is already
    /// a `cdylib`, `vieww-reload` is platform-agnostic. What stands in the way
    /// is policy: **Android 10 (API 29) enforces W^X, and an app targeting 29 or
    /// above cannot load executable code from a writable location.** This app
    /// targets 34. Hot reload is development-only, so a development APK may
    /// target 28 instead, and below 29 the restriction does not apply.
    ///
    /// So the experiment is: run this at `target_sdk_version = 34`, then at 28,
    /// and compare. If 28 is permitted, the rest is tooling that is already
    /// written. If neither is, hot reload is desktop-only and that gets
    /// *recorded* rather than attempted a second time.
    ///
    /// # Why the probe is deliberately not a real library
    ///
    /// The file written here is **garbage, not an ELF**, and that is what makes
    /// the answer unambiguous. The obvious probe — copy a system library into
    /// the data directory and open it — cannot distinguish success from bionic
    /// returning the copy of that library *already loaded in this process*: the
    /// linker de-duplicates by soname, so a "success" might never have touched
    /// the file at all.
    ///
    /// A file that is not an ELF cannot be de-duplicated with anything, so the
    /// two outcomes separate cleanly, **and the failure message is the result**:
    ///
    /// - complains about the **format** (not an ELF, too short, bad magic) — the
    ///   linker opened and read the file, so the path is permitted and hot
    ///   reload is viable at this target SDK.
    /// - complains about **permission, accessibility or a namespace** — it never
    ///   got that far. That is the W^X policy, and it is the blocking answer.
    ///
    /// Reported verbatim rather than reduced to a boolean, because the exact
    /// wording differs between Android versions and the heuristic below is a
    /// reading of it, not the evidence itself.
    ///
    /// # Why this does not go through `vieww-reload`
    ///
    /// It would be the more faithful probe and it would poison the thing being
    /// measured. `vieww-reload` enables `vieww-widget/hot-reload`, cargo unifies
    /// features across a build, and this example would then be running a
    /// `WidgetNode` with an extra field and reconciliation by type path — a
    /// different demo from the one every other check in this suite reports on.
    /// See `docs/HANDOFF.md` on `--exclude` and the feature graph.
    fn probe_writable_dlopen(files: &std::path::Path) -> String {
        let path = files.join("libvieww_probe.so");

        if let Err(error) = std::fs::write(&path, b"deliberately not an ELF file") {
            return format!("could not write a probe into {}: {error}", files.display());
        }

        // SAFETY: opening a library runs its initialisers, and this one has
        // none because it is 28 bytes of text that no linker will accept. The
        // call is expected to fail; the failure is the measurement.
        let opened = unsafe { libloading::Library::new(&path) };
        let _ = std::fs::remove_file(&path);

        match opened {
            // Cannot happen against a non-ELF, and is worth saying loudly rather
            // than reading as good news.
            Ok(_) => "a linker accepted 28 bytes of text as a library — do not \
                      trust this result"
                .to_owned(),
            Err(error) => {
                let text = error.to_string();
                let lower = text.to_lowercase();
                let blocked = lower.contains("not accessible")
                    || lower.contains("permission")
                    || lower.contains("not permitted")
                    || lower.contains("namespace");
                let verdict = if blocked {
                    "BLOCKED (policy, not content) — hot reload is not available at this target SDK"
                } else {
                    "PERMITTED (the linker read the file and rejected its contents)"
                };
                format!("{verdict}; dlopen said: {text}")
            }
        }
    }

    /// The name `ci/mobile/reload-guest-android.sh` pushes.
    const GUEST_LIBRARY: &str = "libreload_guest.so";

    /// A staging path `adb` can write and this app can read.
    ///
    /// World-readable by file mode, and reachable because `/data/local/tmp` is
    /// `0771` — an app cannot *list* it but can open an exact path through it.
    /// Second choice after the app's own external directory, because SELinux
    /// policy for `untrusted_app` against `shell_data_file` has tightened over
    /// releases and this may simply be denied. Both are tried; whichever works
    /// is named in the report.
    const GUEST_STAGING: &str = "/data/local/tmp/libreload_guest.so";

    /// Get the guest library into the one directory the experiment is about.
    ///
    /// # Why the app copies it rather than `adb` placing it there
    ///
    /// Because `adb push` cannot write an app's private directory, and the tool
    /// that can — `run-as` — **requires `android:debuggable`**. That is not a
    /// packaging inconvenience, it is the measurement being destroyed: a
    /// debuggable build is exactly the one the platform relaxes policy for, so
    /// "a debuggable app loaded code from its writable directory" answers an
    /// easier question than the one being asked, and it would not be the release
    /// APK every other check in this suite reports on.
    ///
    /// So `adb` leaves the library somewhere world-readable and **the app writes
    /// it into its own files directory itself**. That is closer to the real
    /// thing anyway: a hot-reload loop has the running app pull in a rebuilt
    /// library, and the file under test is then one this process wrote and this
    /// process loads — which is precisely the W^X pairing the whole question is
    /// about.
    ///
    /// Returns where it came from, for the report, or `None` if there is nothing
    /// staged anywhere.
    fn stage_guest_library(
        android: &AndroidApp,
        target: &std::path::Path,
    ) -> Option<(String, u64)> {
        let mut sources = Vec::new();
        // The app's own external directory. `adb` can write it without any
        // debuggable flag, and the app can always read it.
        if let Some(external) = android.external_data_path() {
            sources.push(external.join(GUEST_LIBRARY));
        }
        sources.push(std::path::PathBuf::from(GUEST_STAGING));

        let staged = sources.into_iter().find(|source| source.is_file());
        let existing = std::fs::metadata(target).ok().map(|meta| meta.len());

        let Some(source) = staged else {
            // Nothing staged, but a copy from an earlier run is still loadable
            // and still worth reporting on.
            return existing.map(|len| (String::from("already in place"), len));
        };

        // **Re-copy when the staged library differs**, rather than keeping the
        // first one ever seen. Otherwise the obvious iteration — edit the guest,
        // rebuild, push, relaunch — would silently keep loading the original,
        // and the probe would report a confident success about a library that is
        // no longer the one on the host. Comparing length is enough here: these
        // are two copies of one build artefact, not arbitrary files, and a
        // recompile that lands on exactly the same size has produced a library
        // that would load identically anyway.
        let source_len = std::fs::metadata(&source).map(|meta| meta.len()).ok();
        if let (Some(existing), Some(source_len)) = (existing, source_len) {
            if existing == source_len {
                return Some((String::from("already in place"), existing));
            }
        }

        let Ok(bytes) = std::fs::read(&source) else {
            return existing.map(|len| (String::from("already in place"), len));
        };

        match std::fs::write(target, &bytes) {
            Ok(()) => {
                let verb = if existing.is_some() {
                    "refreshed from"
                } else {
                    "copied from"
                };
                Some((format!("{verb} {}", source.display()), bytes.len() as u64))
            }
            // The read worked and the write did not, which is a different and
            // more interesting failure than "nothing staged" — it means the app
            // cannot write its own files directory, which is half the question
            // this probe exists to answer.
            Err(error) => Some((format!("FAILED to write {}: {error}", target.display()), 0)),
        }
    }

    /// Load a **real** guest `cdylib` from the writable directory, and run code
    /// out of it.
    ///
    /// # Why this exists when `probe_writable_dlopen` already said PERMITTED
    ///
    /// Because that probe proved less than it looked like it proved. It offers
    /// the linker 28 bytes of text, and the linker rejects it for its *contents*
    /// — which establishes that the file was opened and read, and therefore that
    /// no W^X policy stood in the way. What it cannot establish is that anything
    /// **after the ELF header is parsed** would also permit it. A check on the
    /// segment flags, the relocation pass, or an executable mapping of a
    /// writable file would never have fired against a file that failed at
    /// `e_ident`.
    ///
    /// So this one is the confirming experiment: a genuine `cdylib`, built from
    /// `examples/reload-guest` by `cargo ndk` for this exact ABI, pushed to the
    /// same directory the probe used, and loaded. Either it works — and the
    /// tooling in `vieww-reload` is already written — or the failure names
    /// something the garbage probe could not reach.
    ///
    /// Keeping both is the point rather than clutter: if this fails and
    /// `dlopen_from_a_writable_path` still says PERMITTED, the obstacle is
    /// something about *our* library, and if both fail the obstacle is the path.
    /// One observation cannot separate those.
    ///
    /// # It calls the fingerprint and never the root, and that is a soundness
    /// requirement rather than caution
    ///
    /// The guest exports two symbols. `__vieww_guest_root` returns
    /// `*mut WidgetNode`, and **calling it here would be undefined behaviour**:
    /// the guest is built with `vieww-widget/hot-reload` and this demo
    /// deliberately is not, so the two disagree about `WidgetNode`'s layout —
    /// the same feature-unification hazard that keeps `vieww-reload` out of this
    /// example's dependencies. Reading a value through that pointer would be
    /// reading one struct as another.
    ///
    /// `__vieww_guest_fingerprint` is `extern "C" fn() -> u64`. It takes nothing,
    /// returns a plain integer, and touches no type either side has an opinion
    /// about, so the ABI is fully pinned down whatever features are on. That
    /// makes it callable **and** worth calling: a resolved symbol proves only
    /// that the dynamic symbol table was read, whereas a returned value proves
    /// the library was mapped executable, its relocations applied, and its code
    /// run. That is the whole question, and it costs one call.
    ///
    /// The value is reported rather than compared against anything. The host
    /// half that would have an expectation is `vieww-reload`, which is not
    /// linked here for the reason above; a non-zero answer that took a real
    /// `Fingerprint::of` over `Taps` to compute is the evidence.
    fn probe_real_guest(android: &AndroidApp, files: &std::path::Path) -> String {
        let path = files.join(GUEST_LIBRARY);

        let Some((origin, size)) = stage_guest_library(android, &path) else {
            let external = match android.external_data_path() {
                Some(dir) => dir.join(GUEST_LIBRARY).display().to_string(),
                None => String::from("(this device reports no external path)"),
            };
            return format!(
                "not staged — found nothing at {external} or {GUEST_STAGING}. Run \
                 ci/mobile/reload-guest-android.sh to build and stage one; this is not a \
                 failure, only an unanswered question.",
            );
        };

        if origin.starts_with("FAILED") {
            return origin;
        }

        // SAFETY: opening a library runs its initialisers. This one is our own
        // guest, cross-compiled from this workspace for this ABI by
        // `ci/mobile/reload-guest-android.sh`. Nothing checkable at this line — the
        // guarantee is a property of the build, which is exactly what
        // `vieww_reload::Guest::load` documents about itself.
        let library = match unsafe { libloading::Library::new(&path) } {
            Ok(library) => library,
            Err(error) => return format!("FAILED to open a real cdylib: {error}"),
        };

        // SAFETY: the signature is the one `vieww_reload::guest!` gives this
        // symbol, and it is `extern "C" fn() -> u64` — no argument, and a return
        // type whose ABI does not depend on any feature either side enabled.
        let fingerprint = unsafe {
            match library.get::<unsafe extern "C" fn() -> u64>(b"__vieww_guest_fingerprint\0") {
                Ok(symbol) => symbol(),
                Err(error) => {
                    return format!(
                        "opened, but __vieww_guest_fingerprint is missing: {error}. \
                         The library loaded, so the path is fine — this is a build \
                         problem, most likely a guest without `vieww_reload::guest!` \
                         or a crate that is not crate-type = [\"cdylib\"]."
                    )
                }
            }
        };

        // Resolved to prove it is there, and **never called** — see the doc
        // comment. `*mut WidgetNode` across a feature mismatch is the one thing
        // in this function that would be undefined behaviour. Taking its address
        // is not: a symbol lookup reads the library's tables and runs none of
        // its code.
        //
        // Worth doing rather than assuming, because a guest that exports the
        // fingerprint and not the root would otherwise pass this probe and then
        // fail on a real host, where the failure is far more expensive to read.
        //
        // SAFETY: the signature is the one `guest!` gives this symbol. It is
        // only stored, so even a wrong signature could not be invoked here.
        let root_present = unsafe {
            library
                .get::<unsafe extern "C" fn() -> *mut std::ffi::c_void>(b"__vieww_guest_root\0")
                .is_ok()
        };

        // Deliberately leaked, for the reason `vieww_reload::guest` gives: a
        // loaded guest's vtable pointers outlive any handle to it. Nothing here
        // holds such a pointer, but closing the library would still be the wrong
        // habit to establish in the one place that demonstrates the pattern.
        std::mem::forget(library);

        let root = if root_present {
            "__vieww_guest_root resolved but not called — it returns \
             *mut WidgetNode and this demo is built without hot-reload"
        } else {
            "but __vieww_guest_root is MISSING, so this library would fail \
             against a real host"
        };

        format!(
            "LOADED and EXECUTED a real cdylib from {} ({size} bytes, {origin}); \
             __vieww_guest_fingerprint returned {fingerprint:#018x}. \
             Android hot reload is viable at this target SDK, in a NON-debuggable \
             release build. {root}.",
            path.display()
        )
    }

    /// A launch by Firebase Test Lab's **Game Loop** test.
    ///
    /// Test Lab starts the activity with action
    /// `com.google.intent.action.TEST_LOOP`, an integer extra `scenario`, and a
    /// data URI naming a results file the app may write
    /// (<https://firebase.google.com/docs/test-lab/android/game-loop>). The
    /// manifest's matching intent filter is in this crate's `Cargo.toml`.
    ///
    /// The demo already runs its whole device suite by itself, so a game loop
    /// needs no driver: scenario 1 is the CPU rasterizer, scenario 2 the
    /// planned GPU path, and the report goes into the results file as well as
    /// logcat — on every phone in the device matrix at once
    /// (`ci/mobile/test-lab.sh`).
    struct GameLoop {
        scenario: i32,
        /// The results file, opened for writing through the content resolver.
        results: Option<std::fs::File>,
    }

    fn game_loop_launch(android: &AndroidApp) -> Option<GameLoop> {
        use jni::objects::{JObject, JString, JValue};
        use jni::{jni_sig, jni_str};
        use std::os::fd::FromRawFd;

        if android.vm_as_ptr().is_null() || android.activity_as_ptr().is_null() {
            return None;
        }
        let vm = unsafe { jni::JavaVM::from_raw(android.vm_as_ptr().cast()) };
        let activity_ptr = android.activity_as_ptr();

        let found = vm.attach_current_thread(
            |env| -> Result<Option<(i32, Option<i32>)>, jni::errors::Error> {
                let activity = unsafe { JObject::from_raw(env, activity_ptr.cast()) };
                let intent = env
                    .call_method(
                        &activity,
                        jni_str!("getIntent"),
                        jni_sig!("()Landroid/content/Intent;"),
                        &[],
                    )?
                    .l()?;
                if intent.is_null() {
                    return Ok(None);
                }
                let action = env
                    .call_method(
                        &intent,
                        jni_str!("getAction"),
                        jni_sig!("()Ljava/lang/String;"),
                        &[],
                    )?
                    .l()?;
                if action.is_null() {
                    return Ok(None);
                }
                let action = env.cast_local::<JString>(action)?.try_to_string(env)?;
                if action != "com.google.intent.action.TEST_LOOP" {
                    return Ok(None);
                }
                let key = env.new_string("scenario")?;
                let scenario = env
                    .call_method(
                        &intent,
                        jni_str!("getIntExtra"),
                        jni_sig!("(Ljava/lang/String;I)I"),
                        &[JValue::Object(&key), JValue::Int(0)],
                    )?
                    .i()?;

                // The results file: `getContentResolver()
                // .openAssetFileDescriptor(uri, "w").getParcelFileDescriptor()`,
                // then `detachFd()` so the descriptor is ours to own and close.
                let uri = env
                    .call_method(&intent, jni_str!("getData"), jni_sig!("()Landroid/net/Uri;"), &[])?
                    .l()?;
                if uri.is_null() {
                    return Ok(Some((scenario, None)));
                }
                let resolver = env
                    .call_method(
                        &activity,
                        jni_str!("getContentResolver"),
                        jni_sig!("()Landroid/content/ContentResolver;"),
                        &[],
                    )?
                    .l()?;
                let mode = env.new_string("w")?;
                let asset = env
                    .call_method(
                        &resolver,
                        jni_str!("openAssetFileDescriptor"),
                        jni_sig!(
                            "(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/res/AssetFileDescriptor;"
                        ),
                        &[JValue::Object(&uri), JValue::Object(&mode)],
                    )?
                    .l()?;
                if asset.is_null() {
                    return Ok(Some((scenario, None)));
                }
                let parcel = env
                    .call_method(
                        &asset,
                        jni_str!("getParcelFileDescriptor"),
                        jni_sig!("()Landroid/os/ParcelFileDescriptor;"),
                        &[],
                    )?
                    .l()?;
                let fd = env
                    .call_method(&parcel, jni_str!("detachFd"), jni_sig!("()I"), &[])?
                    .i()?;
                Ok(Some((scenario, Some(fd))))
            },
        );
        match found {
            Ok(Some((scenario, fd))) => Some(GameLoop {
                scenario,
                // SAFETY: `detachFd` hands over sole ownership of a valid,
                // open descriptor; nothing on the Java side closes it now.
                results: fd
                    .filter(|&fd| fd >= 0)
                    .map(|fd| unsafe { std::fs::File::from_raw_fd(fd) }),
            }),
            Ok(None) => None,
            Err(error) => {
                log::warn!("vieww: reading the launch intent for a game loop failed: {error}");
                None
            }
        }
    }

    /// `Activity.finish()`: tells Test Lab this loop is done (recommended by
    /// its docs) instead of waiting out the timeout.
    fn finish_activity(vm_ptr: usize, activity_ptr: usize) {
        use jni::objects::JObject;
        use jni::{jni_sig, jni_str};
        let vm = unsafe { jni::JavaVM::from_raw(vm_ptr as *mut _) };
        let finished = vm.attach_current_thread(|env| -> Result<(), jni::errors::Error> {
            let activity = unsafe { JObject::from_raw(env, activity_ptr as *mut _) };
            env.call_method(&activity, jni_str!("finish"), jni_sig!("()V"), &[])?;
            Ok(())
        });
        if let Err(error) = finished {
            log::warn!("vieww: finishing the activity after the game loop failed: {error}");
        }
    }

    pub(crate) fn main(android: AndroidApp) {
        // Rust's panics and `println!` go nowhere on Android unless something
        // redirects them; without this a failure is an app that vanishes with
        // an empty logcat.
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("vieww"),
        );
        log::info!("vieww starting");

        // The frame graph's numbers come from the platform layer's `FrameLog`,
        // which only exists once the loop is running — so the hook is installed
        // now and pointed at the tree when `build` creates it.
        let perf = screen::PerfLink::new();
        // Runs against the same tree a human is looking at, and reports on a
        // sentinel line after 120 frames. `ci/mobile/device-suite.sh` reads it.
        let suite = DeviceSuite::new();

        // A Firebase Test Lab Game Loop launch, if this is one: its scenario
        // picks the rasterizer below, and the report also goes into the file
        // Test Lab collects, after which the activity finishes.
        let game_loop = game_loop_launch(&android);
        if let Some(game) = &game_loop {
            log::info!(
                "vieww: Firebase Test Lab game loop, scenario {}",
                game.scenario
            );
        }
        let game_loop_scenario = game_loop.as_ref().map(|game| game.scenario);
        if let Some(GameLoop { results, .. }) = game_loop {
            let (vm_ptr, activity_ptr) = (
                android.vm_as_ptr() as usize,
                android.activity_as_ptr() as usize,
            );
            suite.when_reported(move |report| {
                use std::io::Write;
                if let Some(mut file) = results {
                    if let Err(error) = file
                        .write_all(report.as_bytes())
                        .and_then(|()| file.flush())
                    {
                        log::warn!("vieww: writing the game-loop results file failed: {error}");
                    }
                }
                finish_activity(vm_ptr, activity_ptr);
            });
        }

        // A panic on a phone is otherwise silent: it prints to a stderr Android
        // discards, and the process is gone before anybody could have read it.
        // `LogSink` gets this run's panic into logcat; `FileSink` gets it onto
        // disk so the *next* run can report it even if nobody was watching.
        let crashes = crash::android_crash_dir(&android).map_or_else(
            || {
                log::warn!("vieww: no internal data path, so crashes cannot be persisted");
                suite.check(
                    "crash_reports_have_somewhere_to_go",
                    false,
                    "the activity gave us no internal data path",
                );
                Crashes::new().with_sink(crash::LogSink)
            },
            |dir| {
                // How many the last run died holding. An observation, not a
                // check: zero is the normal, healthy answer.
                let drained = drain_previous_crashes(&dir);
                suite.observe(
                    "crash_reports_recovered_from_the_last_run",
                    format!("{drained}"),
                );

                let (ok, detail) = check_crash_storage(&dir);
                suite.check("crash_reports_have_somewhere_to_go", ok, detail);

                Crashes::new()
                    .with_sink(crash::LogSink)
                    .with_sink(FileSink::new(&dir))
            },
        );

        // The pasteboard, the asset bundle, storage and deep links, as the
        // platform actually provides them.
        //
        // **The demo had none of this until 2026-08-14**, on any platform: it
        // built its tree and handed it straight to `set_root`, so there was no
        // `SharedServices` above it and `RenderEditableText` looked for a
        // `Clipboard` that was never registered. Every copy and paste in the
        // demo was therefore a no-op, and "cut, copy and paste, never run on a
        // phone" understated it — the wiring a device would have exercised was
        // not there to exercise.
        let registry = vieww_platform_winit::services::for_android(&android);
        // Taken out before the registry is consumed, so the suite can round-trip
        // the *same* instance the tree will use rather than a second one.
        let clipboard = registry.get::<dyn Clipboard>();
        suite.check(
            "the_platform_registered_a_clipboard",
            clipboard.is_some(),
            format!("{} service(s) registered", registry.len()),
        );
        let services = SharedServices::new(registry);

        // The one experiment that decides whether hot reload can exist on this
        // platform at all. An observation: both answers are real states of the
        // world, and neither is a suite failure.
        match android.internal_data_path() {
            Some(files) => {
                suite.observe("dlopen_from_a_writable_path", probe_writable_dlopen(&files));
                // The confirming half. Answered PERMITTED above against a file
                // that failed at `e_ident`; this one is a real library that has
                // to survive the whole load.
                suite.observe(
                    "dlopen_a_real_guest_library",
                    probe_real_guest(&android, &files),
                );
            }
            None => {
                suite.observe(
                    "dlopen_from_a_writable_path",
                    "the activity gave us no internal data path to probe",
                );
                suite.observe(
                    "dlopen_a_real_guest_library",
                    "the activity gave us no internal data path to probe",
                );
            }
        }

        // **Which rasterizer, chosen on the phone without a rebuild.** The CPU
        // rasterizer is the default and the bar; `adb shell setprop
        // debug.vieww.gpu 1` (what `ci/mobile/device-suite.sh --gpu` does)
        // asks for the planned GPU path instead, so the same build measures
        // both. Read through `getprop` rather than a new `libc` dependency: an
        // app may run it, and a `debug.` property needs no root to set. The
        // first real-phone run (Redmi Note 7 Pro, 2026-10-14) put the CPU
        // path at a 37ms median at 1080x2340 — this is how the GPU's number
        // is taken against it.
        // A Test Lab game loop says which with its scenario (2 = GPU), since a
        // device cloud cannot `setprop`; otherwise the property decides.
        let gpu_prop = std::process::Command::new("/system/bin/getprop")
            .arg("debug.vieww.gpu")
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
            .unwrap_or_default();
        let prefer_gpu = match game_loop_scenario {
            Some(scenario) => scenario == 2,
            None => gpu_prop == "1",
        };
        // `debug.vieww.gpu=auto`: measure both rasterizers on this phone and
        // keep the faster (`App::auto_rasterizer`), and
        // `debug.vieww.recalibrate=1` forgets an earlier answer first.
        let auto = game_loop_scenario.is_none() && gpu_prop == "auto";
        if auto
            && std::process::Command::new("/system/bin/getprop")
                .arg("debug.vieww.recalibrate")
                .output()
                .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).trim() == "1")
        {
            // Before the event loop and every worker the app starts, as below.
            std::env::set_var("VIEWW_RENDERER_RECALIBRATE", "1");
        }
        // **And which present, the same way:** `debug.vieww.present=vulkan`
        // presents CPU frames through the Vulkan swapchain instead of
        // `ANativeWindow_lock` (`native::PresentPath`). For A/B runs on one
        // phone: on 2026-10-16 the LG Stylo 5's CPU raster stage read ~30 ms
        // after the CPU present path landed against ~20 ms before it, on two
        // different units, and this is how the two paths are compared on one.
        // `PresentPath::choose` reads `VIEWW_PRESENT`; set before any thread
        // that could read the environment exists.
        let present = std::process::Command::new("/system/bin/getprop")
            .arg("debug.vieww.present")
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
            .unwrap_or_default();
        // `debug.vieww.hint=1`: an Android performance-hint session over the
        // UI thread (`VIEWW_PERF_HINT`, `vieww_platform_winit::perf_hint`) —
        // the test of whether the CPU path's slow raster is the scheduler's
        // choice of core and clock.
        let hint = std::process::Command::new("/system/bin/getprop")
            .arg("debug.vieww.hint")
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).trim() == "1");
        if hint {
            // Before the event loop and every worker the app starts, as below.
            std::env::set_var("VIEWW_PERF_HINT", "1");
        }
        // `debug.vieww.scale=<auto|0.25..1.0>`: the GPU path renders the plan
        // into targets a fraction of the swapchain's size and the final blit
        // enlarges (`VIEWW_GPU_SCALE`, `vieww_platform_winit::quality`) — the
        // test of whether a fill-rate cut is what lets the GPU path pass
        // where it currently loses (the LG Stylo 5: CPU 36.2 ms, GPU 40.1 ms,
        // Device Farm 2026-10-17). `auto` takes the probed device tier's
        // profile (`device_tier`); a number between 0.25 and 1.0 is that
        // scale; unset or `off` is full resolution.
        let scale = std::process::Command::new("/system/bin/getprop")
            .arg("debug.vieww.scale")
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
            .unwrap_or_default();
        if !scale.is_empty() && scale != "off" {
            // Before the event loop and every worker the app starts, as below.
            std::env::set_var("VIEWW_GPU_SCALE", &scale);
        }
        // `debug.vieww.bare=1`: the CPU rasterizer with the 2026-10-18/19
        // additions switched off — the row-parallel scanline walk
        // (`VIEWW_RASTER_THREADS=0`), the path-fill cache
        // (`VIEWW_PATH_CACHE_MB=0`) and the blur-band cache
        // (`VIEWW_BLUR_CACHE_MB=0`). Device Farm run f91d07c read the CPU
        // raster stage flat-to-slower (LG 30.6 → 34.8 ms, S26 4.7 → 6.6 ms)
        // and CPU-path PSS up 10–16 MB after they landed, against earlier
        // runs on other units; this is the same-unit, same-run A/B that says
        // which of those is theirs.
        let bare = std::process::Command::new("/system/bin/getprop")
            .arg("debug.vieww.bare")
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).trim() == "1");
        if bare {
            // Before the event loop and every worker the app starts, as below.
            std::env::set_var("VIEWW_RASTER_THREADS", "0");
            std::env::set_var("VIEWW_PATH_CACHE_MB", "0");
            std::env::set_var("VIEWW_BLUR_CACHE_MB", "0");
            suite.observe(
                "bare",
                "raster threads, path-fill cache and blur cache off (debug.vieww.bare=1)",
            );
        }
        if present == "vulkan" || present == "cpu" {
            // Before the event loop and every worker the app starts, so no
            // other thread is reading the environment yet.
            std::env::set_var("VIEWW_PRESENT", &present);
            suite.observe("present", &format!("debug.vieww.present={present}"));
        }
        suite.observe(
            "rasterizer",
            if auto {
                "measured on this phone (debug.vieww.gpu=auto)"
            } else if prefer_gpu {
                "planned GPU path requested (debug.vieww.gpu=1)"
            } else {
                "CPU rasterizer (set debug.vieww.gpu=1 for the GPU path)"
            },
        );
        let app = App::new()
            .title("vieww")
            .report_crashes(crashes)
            .on_frame(perf.hook())
            .on_frame(suite.hook())
            // Activates the button the way a screen reader does, and checks the
            // handler ran. `before_frame` because dispatching needs `&mut`.
            .before_frame(suite.semantic_activation())
            // Raises the drawer and asks whether the screen behind it left the
            // semantics tree — the mechanical half of the `BlockSemantics`
            // claim, which until now had nothing modal on this screen to test
            // against. `before_frame` for the same reason: it writes a signal,
            // and the rebuild that follows is then this frame's.
            .before_frame(suite.modal_semantics())
            // Round-trips the real pasteboard, after a frame is on screen —
            // Android returns an empty clipboard to a background read.
            .after_frame(suite.clipboard_round_trip(clipboard))
            // Reports where the button and the field ended up, so the
            // device script taps what is on screen rather than a constant
            // that only suited one phone. `after_frame`, not `on_frame`:
            // geometry does not exist until the frame has been laid out.
            .after_frame(suite.tap_targets());
        let app = if prefer_gpu { app.prefer_gpu() } else { app };
        let app = if auto { app.auto_rasterizer() } else { app };
        let result = app.run_android(android, |driver| {
            // `FrameDriver::set_root`, *not* `driver.elements().set_root`.
            // The driver keeps the root so it can re-publish it under an
            // `Inherited<ViewMetrics>` whenever the safe area changes; going
            // straight to the element tree leaves `FrameDriver::root` as
            // `None`, and the republish silently does nothing. That is how
            // the notch inset was computed correctly and ignored completely.
            //
            // The embedded faces come with `RenderTree::new`, so text shapes
            // without anybody opting into the system's fonts — which on a
            // phone would be the same 33-second scan it is everywhere else.
            let demo = screen::Demo::new(driver);
            perf.attach(&demo);
            let probe = suite.probe(demo.taps());
            // The latch `modal_semantics` pulls. Handed over here for the
            // reason the tap counter is: the hooks were installed before
            // this closure ran, so the signal did not exist yet.
            suite.watch_drawer(demo.drawer());
            // `Inherited<SharedServices>` above the tree is how a widget
            // reaches the pasteboard — `RenderEditableText` asks its scope
            // for a `Clipboard`, and finds one only if something published
            // it here.
            driver.set_root(Inherited::new(
                services,
                demo.screen("A frame, on a phone.", TargetPlatform::Android, probe),
            ));
        });

        match result {
            // Reached when the activity is finished rather than when something
            // went wrong — the report is what the desktop harness prints too.
            Ok(report) => log::info!("vieww stopped: {report}"),
            Err(error) => log::error!("vieww failed: {error}"),
        }
    }
}
