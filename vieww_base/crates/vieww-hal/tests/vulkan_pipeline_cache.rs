//! The pipeline cache survives a "launch": a device that created the scene
//! renderer's pipelines saves them, and the next device starts from them.
//!
//! `#[ignore]`d like every Vulkan suite here: needs a loader and an ICD
//! (`lavapipe`, or real hardware).

#![cfg(feature = "vulkan")]

use vieww_hal::vulkan::{set_pipeline_cache_file, SceneRenderer, VulkanDevice};

#[test]
#[ignore = "needs a Vulkan loader + ICD (lavapipe or a real GPU); run with `cargo test --features vulkan -- --ignored`"]
fn a_second_device_starts_from_the_first_ones_pipelines() {
    let dir = std::env::temp_dir().join(format!("vieww-pipeline-cache-{}", std::process::id()));
    let file = dir.join("pipelines.bin");
    let _ = std::fs::remove_dir_all(&dir);
    set_pipeline_cache_file(Some(file.clone()));

    let Ok(first) = VulkanDevice::new() else {
        eprintln!("no Vulkan device; skipping");
        set_pipeline_cache_file(None);
        return;
    };
    assert_eq!(first.pipeline_cache_seed(), 0, "nothing to start from yet");
    let renderer = SceneRenderer::new(&first).expect("scene renderer");
    let written = first.save_pipeline_cache().expect("saved");
    // At least the header. Lavapipe stops there — it keeps no compiled code
    // between devices — so a real driver's volume is Device Farm's to show;
    // what this checks is that a real driver's own header is accepted back.
    assert!(
        written >= 32,
        "the cache holds at least its header: {written} bytes"
    );
    drop(renderer);
    drop(first);

    let second = VulkanDevice::new().expect("a second device");
    assert_eq!(
        second.pipeline_cache_seed(),
        written,
        "the second device started from everything the first saved"
    );

    // Data from another GPU or driver is not handed to the driver at all.
    std::fs::write(&file, vec![0u8; 64]).expect("overwrite");
    let third = VulkanDevice::new().expect("a third device");
    assert_eq!(
        third.pipeline_cache_seed(),
        0,
        "a foreign header is dropped"
    );

    drop(second);
    drop(third);
    set_pipeline_cache_file(None);
    let _ = std::fs::remove_dir_all(&dir);
}
