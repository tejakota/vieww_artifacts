//! The only ingress. Everything the engine ever sees comes through here.
//!
//! SPEC §6: access comes exclusively from out-of-process OS pickers, and the
//! descriptor *is* the capability. This module is deliberately small and
//! deliberately the only place in the crate that can produce a file descriptor
//! — if it isn't here, the engine cannot read it.
//!
//! The Java side is loaded from a dex embedded in this binary rather than from
//! the APK's classes.dex, which is what lets the shim exist without a Gradle
//! project. `accesskit_android` — which vieww uses to reach TalkBack — loads its
//! own delegate exactly the same way, so this is now the framework's pattern as
//! well as ours.

use anyhow::{anyhow, Context, Result};
use jni::objects::{GlobalRef, JObject, JObjectArray, JString, JValue};
use jni::{JavaVM, JNIEnv};
use std::fs::File;
use std::io::Read;
use std::os::fd::FromRawFd;

/// Compiled from `java/VavltPickerShim.java` by `build.rs`.
const DEX: &[u8] = include_bytes!(env!("VAVLT_DEX"));

const CLASS_NAME: &str = "dev.vavlt.app.VavltPickerShim";

/// Mirrors the constants in the shim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickState {
    Idle,
    Pending,
    Done,
    Cancelled,
}

impl PickState {
    fn from_i32(v: i32) -> Self {
        match v {
            1 => PickState::Pending,
            2 => PickState::Done,
            3 => PickState::Cancelled,
            _ => PickState::Idle,
        }
    }
}

/// What the picker handed back. Metadata only — nothing has been opened.
#[derive(Debug, Clone)]
pub struct PickedItem {
    pub uri: String,
    pub name: String,
    pub bytes: u64,
}

/// Handle onto the shim. `JavaVM` and `GlobalRef` are both `Send + Sync`, so
/// this crosses to the codec worker thread intact — which is the point, since
/// that is the thread that needs to open descriptors.
pub struct Picker {
    vm: JavaVM,
    activity: GlobalRef,
    class: GlobalRef,
}

impl Picker {
    /// Loads the dex and resolves the shim class. Called once, at startup.
    pub fn new(app: &vieww_platform_winit::AndroidApp) -> Result<Self> {
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }
            .context("JavaVM from android-activity")?;
        // Scoped: the attach guard borrows the VM, and the VM is moved into
        // `Self` at the end. Both refs it produces are global, so nothing here
        // outlives the guard anyway.
        let (activity, class) = {
        let mut env = vm.attach_current_thread().context("attach main thread")?;

        // SAFETY: android-activity owns this reference for the lifetime of the
        // process; we immediately promote it to a global ref.
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
        let activity = env.new_global_ref(&activity).context("global activity ref")?;

        // The dex is 'static, so the direct buffer it backs stays valid for as
        // long as the class loader might touch it.
        let buf = unsafe {
            env.new_direct_byte_buffer(DEX.as_ptr() as *mut u8, DEX.len())
                .context("direct byte buffer over the dex")?
        };

        let parent = env
            .call_method(
                activity.as_obj(),
                "getClassLoader",
                "()Ljava/lang/ClassLoader;",
                &[],
            )
            .context("getClassLoader")?
            .l()?;

        let loader = env
            .new_object(
                "dalvik/system/InMemoryDexClassLoader",
                "(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V",
                &[JValue::Object(&buf), JValue::Object(&parent)],
            )
            .context("InMemoryDexClassLoader")?;

        let name = env.new_string(CLASS_NAME)?;
        let class = env
            .call_method(
                &loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&name)],
            )
            .context("loadClass VavltPickerShim")?
            .l()?;
        let class = env.new_global_ref(&class).context("global class ref")?;

            (activity, class)
        };

        Ok(Self {
            vm,
            activity,
            class,
        })
    }

    fn with_env<T>(&self, f: impl FnOnce(&mut JNIEnv) -> Result<T>) -> Result<T> {
        let mut env = self.vm.attach_current_thread()?;
        let r = f(&mut env);
        if r.is_err() {
            // A pending Java exception poisons every later JNI call on this
            // thread, so it is cleared here rather than left for the next one.
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
        r
    }

    /// Opens the system picker. Returns as soon as the intent is posted.
    pub fn launch(&self, max: i32) -> Result<()> {
        self.with_env(|env| {
            env.call_static_method(
                &self.class,
                "launch",
                "(Landroid/app/Activity;I)V",
                &[JValue::Object(self.activity.as_obj()), JValue::Int(max)],
            )?;
            Ok(())
        })
    }

    pub fn state(&self) -> PickState {
        self.with_env(|env| {
            Ok(PickState::from_i32(
                env.call_static_method(&self.class, "state", "()I", &[])?.i()?,
            ))
        })
        .unwrap_or(PickState::Idle)
    }

    pub fn reset(&self) {
        let _ = self.with_env(|env| {
            env.call_static_method(&self.class, "reset", "()V", &[])?;
            Ok(())
        });
    }

    /// The picked selection, as metadata. No file is opened by this call.
    pub fn take_selection(&self) -> Result<Vec<PickedItem>> {
        self.with_env(|env| {
            let uris = string_array(env, &self.class, "uris")?;
            let names = string_array(env, &self.class, "names")?;

            let sizes_obj = env
                .call_static_method(&self.class, "sizes", "()[J", &[])?
                .l()?;
            let sizes_arr: jni::objects::JLongArray = sizes_obj.into();
            let len = env.get_array_length(&sizes_arr)? as usize;
            let mut sizes = vec![0i64; len];
            env.get_long_array_region(&sizes_arr, 0, &mut sizes)?;

            Ok(uris
                .into_iter()
                .enumerate()
                .map(|(i, uri)| PickedItem {
                    name: names.get(i).cloned().unwrap_or_else(|| "unnamed".into()),
                    bytes: sizes.get(i).copied().unwrap_or(0).max(0) as u64,
                    uri,
                })
                .collect())
        })
    }

    /// A display thumbnail as JPEG bytes, rendered by the system.
    ///
    /// Not a read of the original: this is the same downscaled image the picker
    /// already showed the user, and it never reaches the engine. It exists so
    /// the plan screen can show photographs rather than filenames — you should
    /// be able to *see* what you are about to change.
    pub fn thumbnail(&self, uri: &str, size: i32) -> Option<Vec<u8>> {
        self.with_env(|env| {
            let juri = env.new_string(uri)?;
            let obj = env
                .call_static_method(
                    &self.class,
                    "thumbnail",
                    "(Landroid/app/Activity;Ljava/lang/String;I)[B",
                    &[
                        JValue::Object(self.activity.as_obj()),
                        JValue::Object(&juri),
                        JValue::Int(size),
                    ],
                )?
                .l()?;

            if obj.is_null() {
                return Ok(None);
            }
            let arr: jni::objects::JByteArray = obj.into();
            Ok(Some(env.convert_byte_array(&arr)?))
        })
        .ok()
        .flatten()
    }

    /// Reads one picked item in full.
    ///
    /// This is the moment the capability is exercised, and it happens only
    /// after the Transform grant — everything before this point in the app runs
    /// on metadata the picker already showed the user.
    pub fn read(&self, uri: &str) -> Result<Vec<u8>> {
        let fd = self.with_env(|env| {
            let juri = env.new_string(uri)?;
            Ok(env
                .call_static_method(
                    &self.class,
                    "openFd",
                    "(Landroid/app/Activity;Ljava/lang/String;)I",
                    &[
                        JValue::Object(self.activity.as_obj()),
                        JValue::Object(&juri),
                    ],
                )?
                .i()?)
        })?;

        if fd < 0 {
            return Err(anyhow!("the picker's grant for this item is no longer valid"));
        }

        // The fd was detached on the Java side, so this File owns it and
        // closing it here is the whole lifetime.
        let mut file = unsafe { File::from_raw_fd(fd) };
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).context("read picked item")?;
        Ok(buf)
    }
}

fn string_array(env: &mut JNIEnv, class: &GlobalRef, method: &str) -> Result<Vec<String>> {
    let obj = env
        .call_static_method(class, method, "()[Ljava/lang/String;", &[])?
        .l()?;
    let arr: JObjectArray = obj.into();
    let len = env.get_array_length(&arr)?;

    let mut out = Vec::with_capacity(len as usize);
    for i in 0..len {
        let item = env.get_object_array_element(&arr, i)?;
        let s: JString = item.into();
        out.push(env.get_string(&s)?.into());
    }
    Ok(out)
}
