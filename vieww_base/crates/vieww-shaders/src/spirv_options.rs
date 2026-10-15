//! The one place SPIR-V translation is configured.
//!
//! Shared, by `#[path]`, between [`crate::compiler`] (runtime translation,
//! behind the `compiler` feature) and this crate's `build.rs` (which bakes the
//! built-in library to SPIR-V at compile time — see [`crate::baked`]). One
//! file, so a shader baked at build time and the same shader translated at
//! run time cannot be configured differently; `baked`'s tests check the bytes
//! agree.
//!
//! Only `naga` types appear here, so `build.rs` can include it without the
//! rest of the crate.

/// Parse and validate `wgsl` exactly as every vieww shader is.
///
/// `Capabilities::all()`: capabilities only gate optional hardware features,
/// and naga's validator still rejects anything no target can run.
pub(crate) fn parse_and_validate(
    wgsl: &str,
) -> Result<(naga::Module, naga::valid::ModuleInfo), String> {
    let module = naga::front::wgsl::parse_str(wgsl).map_err(|e| format!("parse: {e}"))?;
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|e| format!("validate: {e}"))?;
    Ok((module, info))
}

/// SPIR-V writer options.
///
/// Bounds-checked image loads: naga's default leaves an out-of-range
/// `textureLoad` unchecked, which is undefined behaviour that software
/// rasterisers happen to forgive and real drivers do not. Out of range reads
/// as transparent — what the CPU compositor does.
pub(crate) fn spirv_options() -> naga::back::spv::Options<'static> {
    naga::back::spv::Options {
        bounds_check_policies: naga::proc::BoundsCheckPolicies {
            index: naga::proc::BoundsCheckPolicy::Restrict,
            buffer: naga::proc::BoundsCheckPolicy::Restrict,
            image_load: naga::proc::BoundsCheckPolicy::ReadZeroSkipWrite,
            binding_array: naga::proc::BoundsCheckPolicy::Restrict,
        },
        ..naga::back::spv::Options::default()
    }
}

/// The whole module as SPIR-V words. SPIR-V carries every entry point, so
/// there is one blob per WGSL module, not one per entry point.
pub(crate) fn write_spirv(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
) -> Result<Vec<u32>, String> {
    let mut writer = naga::back::spv::Writer::new(&spirv_options()).map_err(|e| e.to_string())?;
    let mut spirv = Vec::new();
    writer
        .write(module, info, None, &None, &mut spirv)
        .map_err(|e| e.to_string())?;
    Ok(spirv)
}
