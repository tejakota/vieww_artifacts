//! Parse, validate, cross-compile and reflect one WGSL module.
//!
//! Every step here is pure-CPU `naga` — no GPU, no OS-specific toolchain,
//! no host target needed. That's the reason this crate's "cross-compile to
//! SPIR-V/MSL/HLSL" claim can be fully real and fully tested in this
//! delivery's sandbox, unlike the GPU backends themselves
//! (`vieww-hal::metal`/`d3d12`): naga's MSL and HLSL backends are text
//! generators, not compilers that need Metal.framework or `d3dcompiler.dll`
//! present to run. What they produce still needs a real Metal/D3D12
//! compiler downstream to turn into a pipeline object on the target OS —
//! this crate gets the shader *text* right, not the final GPU object.

use crate::pipeline_cache::PipelineCacheKey;
use naga::valid::{Capabilities, ValidationFlags, Validator};
pub use naga::ShaderStage;

/// Where [`ParsedShader::to_msl`] puts things, so a Metal backend can bind
/// them without reflecting the generated source.
///
/// Metal has three independent index spaces per stage — buffers, textures,
/// samplers — so the WGSL binding number is used directly in whichever space
/// the resource's type belongs to: `@group(0) @binding(3) var t: texture_2d`
/// is `texture(3)`, `@group(0) @binding(0) var<uniform>` is `buffer(0)`.
/// Two slots are reserved above any binding the built-in shaders use.
pub mod msl_slots {
    /// The buffer slot `var<immediate>` (push-constant) data is read from;
    /// bind it with `set_vertex_bytes`/`set_fragment_bytes`.
    pub const IMMEDIATES_BUFFER: u8 = 20;
    /// The buffer slot vertex attributes are fetched from: the index a
    /// pipeline's vertex descriptor layout must use for `[[stage_in]]`.
    pub const VERTEX_BUFFER: u8 = 30;
}

/// Where [`ParsedShader::to_hlsl`] puts things, for a D3D12 root signature.
///
/// naga maps `@group(g) @binding(b)` to register `b` in space `g` of the
/// register class the type implies (`b` for constant buffers, `t` for
/// textures, `s` for samplers), and vertex inputs to the semantic
/// [`VERTEX_SEMANTIC`](hlsl_slots::VERTEX_SEMANTIC) with the WGSL location as
/// its index.
pub mod hlsl_slots {
    /// The constant-buffer register `var<immediate>` data lives in — root
    /// constants in a D3D12 root signature.
    pub const IMMEDIATES_REGISTER: u32 = 15;
    /// The semantic name naga gives `@location(n)` vertex inputs (`LOC0`,
    /// `LOC1`, ...).
    pub const VERTEX_SEMANTIC: &str = "LOC";
}

#[derive(Debug)]
pub enum CompileError {
    Parse(String),
    Validate(String),
    Backend {
        target: &'static str,
        message: String,
    },
    /// The requested entry point does not exist in the module, or exists
    /// with a different stage than requested.
    MissingEntryPoint {
        name: String,
        stage: ShaderStage,
    },
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(message) => write!(f, "WGSL parse error: {message}"),
            Self::Validate(message) => write!(f, "WGSL validation error: {message}"),
            Self::Backend { target, message } => write!(f, "{target} codegen error: {message}"),
            Self::MissingEntryPoint { name, stage } => {
                write!(f, "no {stage:?} entry point named {name:?}")
            }
        }
    }
}

impl std::error::Error for CompileError {}

/// A parsed and validated WGSL module, ready to be cross-compiled or
/// reflected any number of times without re-parsing — the unit both
/// [`crate::pipeline_cache::PipelineCache`] and hot reload key off of.
pub struct ParsedShader {
    pub(crate) module: naga::Module,
    pub(crate) info: naga::valid::ModuleInfo,
    source: String,
}

impl std::fmt::Debug for ParsedShader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedShader")
            .field(
                "entry_points",
                &self
                    .module
                    .entry_points
                    .iter()
                    .map(|ep| &ep.name)
                    .collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

impl ParsedShader {
    /// Parse and validate `wgsl`. `Capabilities::all()` — not `::empty()`
    /// like `vieww-hal::vulkan`'s narrower clear/mesh shaders use — because
    /// this crate's built-ins (`image.wgsl`, `blur.wgsl`, …) use texture
    /// sampling and control flow the empty capability set doesn't need to
    /// forbid; naga's validator still rejects anything an actual target
    /// can't run, capabilities only gate *optional* hardware features
    /// (e.g. `PUSH_CONSTANT`) this library's shaders don't use anyway.
    pub fn parse(wgsl: &str) -> Result<Self, CompileError> {
        let module =
            naga::front::wgsl::parse_str(wgsl).map_err(|e| CompileError::Parse(e.to_string()))?;
        let info = Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .map_err(|e| CompileError::Validate(e.to_string()))?;
        Ok(Self {
            module,
            info,
            source: wgsl.to_string(),
        })
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    fn find_entry_point(
        &self,
        name: &str,
        stage: ShaderStage,
    ) -> Result<&naga::EntryPoint, CompileError> {
        self.module
            .entry_points
            .iter()
            .find(|ep| ep.name == name && ep.stage == stage)
            .ok_or_else(|| CompileError::MissingEntryPoint {
                name: name.to_string(),
                stage,
            })
    }

    /// Translate to SPIR-V — the same `naga::back::spv` call
    /// `vieww-hal::vulkan::translate_wgsl_to_spirv` makes, generalized to
    /// any entry point rather than the two hardcoded ones there. A whole
    /// module is written at once (SPIR-V's binary carries every entry
    /// point; `entry_point`/`stage` select which one a *caller* means to
    /// bind when building a pipeline, they don't change what bytes come
    /// out), so `entry_point`/`stage` here only need to exist for the
    /// error to be caught early rather than at pipeline-creation time.
    pub fn to_spirv(
        &self,
        entry_point: &str,
        stage: ShaderStage,
    ) -> Result<Vec<u32>, CompileError> {
        self.find_entry_point(entry_point, stage)?;
        // Bounds-checked image loads: naga's default leaves an out-of-range
        // `textureLoad` unchecked, which is undefined behaviour that software
        // rasterisers happen to forgive and real drivers do not. Out of range
        // reads as transparent — what the CPU compositor does.
        let options = naga::back::spv::Options {
            bounds_check_policies: naga::proc::BoundsCheckPolicies {
                index: naga::proc::BoundsCheckPolicy::Restrict,
                buffer: naga::proc::BoundsCheckPolicy::Restrict,
                image_load: naga::proc::BoundsCheckPolicy::ReadZeroSkipWrite,
                binding_array: naga::proc::BoundsCheckPolicy::Restrict,
            },
            ..naga::back::spv::Options::default()
        };
        let mut writer =
            naga::back::spv::Writer::new(&options).map_err(|e| CompileError::Backend {
                target: "spir-v",
                message: e.to_string(),
            })?;
        let mut spirv = Vec::new();
        writer
            .write(&self.module, &self.info, None, &None, &mut spirv)
            .map_err(|e| CompileError::Backend {
                target: "spir-v",
                message: e.to_string(),
            })?;
        Ok(spirv)
    }

    /// Translate one entry point to Metal Shading Language, with every
    /// resource bound to a real Metal slot under [`msl_slots`]'s convention.
    ///
    /// **Why the binding map is not optional.** With no map, naga emits
    /// `[[user(fake0)]]` for each resource — a placeholder that its own
    /// tests accept and that no Metal compiler does. This used to be the
    /// behaviour, and it was invisible for as long as nothing compiled the
    /// output; `vieww-hal`'s Metal backend is the first thing that does.
    ///
    /// # Errors
    ///
    /// The entry point is missing, a resource sits outside group 0 (the
    /// convention has one bind group, which is all the built-in shaders use),
    /// or naga's MSL writer refuses the module.
    pub fn to_msl(&self, entry_point: &str, stage: ShaderStage) -> Result<String, CompileError> {
        self.find_entry_point(entry_point, stage)?;
        let mut resources = naga::back::msl::BindingMap::new();
        for (_, var) in self.module.global_variables.iter() {
            let Some(binding) = var.binding else { continue };
            if binding.group != 0 {
                return Err(CompileError::Backend {
                    target: "msl",
                    message: format!(
                        "resource at group {} binding {}: the Metal slot convention has group 0 only",
                        binding.group, binding.binding
                    ),
                });
            }
            let slot = u8::try_from(binding.binding).map_err(|_| CompileError::Backend {
                target: "msl",
                message: format!("binding {} does not fit a Metal slot", binding.binding),
            })?;
            let mut target = naga::back::msl::BindTarget::default();
            match self.module.types[var.ty].inner {
                naga::TypeInner::Image { .. } => target.texture = Some(slot),
                naga::TypeInner::Sampler { .. } => {
                    target.sampler = Some(naga::back::msl::BindSamplerTarget::Resource(slot));
                }
                _ => {
                    target.buffer = Some(slot);
                    target.mutable = matches!(
                        var.space,
                        naga::AddressSpace::Storage {
                            access
                        } if access.contains(naga::StorageAccess::STORE)
                    );
                }
            }
            resources.insert(binding, target);
        }
        let mut per_entry_point_map = naga::back::msl::EntryPointResourceMap::new();
        per_entry_point_map.insert(
            entry_point.to_string(),
            naga::back::msl::EntryPointResources {
                resources,
                immediates_buffer: Some(msl_slots::IMMEDIATES_BUFFER),
                sizes_buffer: None,
            },
        );
        let options = naga::back::msl::Options {
            lang_version: (2, 1),
            per_entry_point_map,
            fake_missing_bindings: false,
            // The same policy as SPIR-V's, for the same reason: an
            // out-of-range `textureLoad` reads transparent, as on the CPU.
            bounds_check_policies: naga::proc::BoundsCheckPolicies {
                index: naga::proc::BoundsCheckPolicy::Restrict,
                buffer: naga::proc::BoundsCheckPolicy::Restrict,
                image_load: naga::proc::BoundsCheckPolicy::ReadZeroSkipWrite,
                binding_array: naga::proc::BoundsCheckPolicy::Restrict,
            },
            ..naga::back::msl::Options::default()
        };
        let pipeline_options = naga::back::msl::PipelineOptions {
            entry_point: Some((stage, entry_point.to_string())),
            ..Default::default()
        };
        let (source, _info) =
            naga::back::msl::write_string(&self.module, &self.info, &options, &pipeline_options)
                .map_err(|e| CompileError::Backend {
                    target: "msl",
                    message: e.to_string(),
                })?;
        Ok(source)
    }

    /// Translate one entry point to HLSL, for `vieww-hal::d3d12`'s eventual
    /// pipeline port — same relationship to that module as [`Self::to_msl`]
    /// has to `vieww-hal::metal`.
    pub fn to_hlsl(&self, entry_point: &str, stage: ShaderStage) -> Result<String, CompileError> {
        self.find_entry_point(entry_point, stage)?;
        // Push constants ("immediates") live in a constant buffer on D3D12, and
        // naga needs to be told which register. Space 0, register 15 — well
        // clear of the low registers the built-in shaders bind resources to.
        let options = naga::back::hlsl::Options {
            immediates_target: Some(naga::back::hlsl::BindTarget {
                space: 0,
                register: hlsl_slots::IMMEDIATES_REGISTER,
                binding_array_size: None,
                dynamic_storage_buffer_offsets_index: None,
                restrict_indexing: false,
            }),
            ..naga::back::hlsl::Options::default()
        };
        let pipeline_options = naga::back::hlsl::PipelineOptions {
            entry_point: Some((stage, entry_point.to_string())),
        };
        let mut buffer = String::new();
        let fragment_entry_point = if stage == ShaderStage::Fragment {
            naga::back::hlsl::FragmentEntryPoint::new(&self.module, entry_point)
        } else {
            None
        };
        let mut writer = naga::back::hlsl::Writer::new(&mut buffer, &options, &pipeline_options);
        writer
            .write(&self.module, &self.info, fragment_entry_point.as_ref())
            .map_err(|e| CompileError::Backend {
                target: "hlsl",
                message: e.to_string(),
            })?;
        Ok(buffer)
    }

    /// Every entry point in the module, with its stage, in declaration
    /// order — what a backend iterates to translate a whole shader.
    #[must_use]
    pub fn entry_points(&self) -> Vec<(String, ShaderStage)> {
        self.module
            .entry_points
            .iter()
            .map(|ep| (ep.name.clone(), ep.stage))
            .collect()
    }

    /// A cache key over this exact source text and requested entry
    /// point/stage — see [`crate::pipeline_cache`] for what it's for and
    /// why raw-source hashing (rather than hashing the parsed IR) is the
    /// documented, deliberate choice here.
    #[must_use]
    pub fn cache_key(&self, entry_point: &str, stage: ShaderStage) -> PipelineCacheKey {
        PipelineCacheKey::new(&self.source, entry_point, stage)
    }

    /// Reflect one entry point's resource bindings and (for a vertex stage)
    /// input attributes — see [`crate::reflection`].
    pub fn reflect(
        &self,
        entry_point: &str,
        stage: ShaderStage,
    ) -> Result<crate::reflection::Reflection, CompileError> {
        let ep = self.find_entry_point(entry_point, stage)?;
        Ok(crate::reflection::reflect(&self.module, ep))
    }
}

/// Convenience: parse and translate to all three backends plus reflection
/// in one call, for a caller (or test) that wants the full picture without
/// juggling a [`ParsedShader`] itself.
#[derive(Debug)]
pub struct CompiledShader {
    pub spirv: Vec<u32>,
    pub msl: String,
    pub hlsl: String,
    pub reflection: crate::reflection::Reflection,
    pub cache_key: PipelineCacheKey,
}

pub fn compile(
    wgsl: &str,
    entry_point: &str,
    stage: ShaderStage,
) -> Result<CompiledShader, CompileError> {
    let parsed = ParsedShader::parse(wgsl)?;
    Ok(CompiledShader {
        spirv: parsed.to_spirv(entry_point, stage)?,
        msl: parsed.to_msl(entry_point, stage)?,
        hlsl: parsed.to_hlsl(entry_point, stage)?,
        reflection: parsed.reflect(entry_point, stage)?,
        cache_key: parsed.cache_key(entry_point, stage),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::ShaderLibrary;

    /// Every entry point of every built-in shader translates to MSL that a
    /// Metal compiler can bind: no naga placeholder bindings, and each
    /// resource in the slot [`msl_slots`] documents.
    #[test]
    fn msl_carries_real_slots_for_every_built_in_shader() {
        let library = ShaderLibrary::new();
        for source in library.iter() {
            let parsed = ParsedShader::parse(&source.wgsl).unwrap();
            for ep in &parsed.module.entry_points {
                let msl = parsed
                    .to_msl(&ep.name, ep.stage)
                    .unwrap_or_else(|e| panic!("{} {}: {e}", source.name, ep.name));
                assert!(
                    !msl.contains("user(fake"),
                    "{} {} still has a placeholder binding:\n{msl}",
                    source.name,
                    ep.name
                );
            }
        }
        let scene = ParsedShader::parse(crate::library::SCENE_WGSL).unwrap();
        let vs = scene.to_msl("vs_main", ShaderStage::Vertex).unwrap();
        assert!(vs.contains("[[buffer(0)]]"), "{vs}");
        let fs = scene.to_msl("fs_main", ShaderStage::Fragment).unwrap();
        assert!(
            fs.contains("[[texture(1)]]") && fs.contains("[[texture(4)]]"),
            "{fs}"
        );
        let post = ParsedShader::parse(crate::library::POST_WGSL).unwrap();
        let fs = post.to_msl("fs_main", ShaderStage::Fragment).unwrap();
        assert!(fs.contains("[[buffer(20)]]"), "immediates slot:\n{fs}");
    }

    #[test]
    fn every_built_in_shader_parses_and_validates() {
        let library = ShaderLibrary::new();
        for source in library.iter() {
            ParsedShader::parse(&source.wgsl)
                .unwrap_or_else(|e| panic!("{} failed to parse/validate: {e}", source.name));
        }
    }

    #[test]
    fn every_built_in_shader_cross_compiles_to_all_three_backends() {
        let library = ShaderLibrary::new();
        for source in library.iter() {
            let parsed = ParsedShader::parse(&source.wgsl).unwrap();
            let spirv = parsed.to_spirv("vs_main", ShaderStage::Vertex).unwrap();
            assert!(!spirv.is_empty(), "{}: empty SPIR-V", source.name);
            let msl = parsed.to_msl("vs_main", ShaderStage::Vertex).unwrap();
            assert!(
                msl.contains("vertex"),
                "{}: MSL missing a vertex function: {msl}",
                source.name
            );
            let hlsl = parsed.to_hlsl("vs_main", ShaderStage::Vertex).unwrap();
            assert!(!hlsl.is_empty(), "{}: empty HLSL", source.name);

            let spirv_fs = parsed.to_spirv("fs_main", ShaderStage::Fragment).unwrap();
            assert!(
                !spirv_fs.is_empty(),
                "{}: empty fragment SPIR-V",
                source.name
            );
            let msl_fs = parsed.to_msl("fs_main", ShaderStage::Fragment).unwrap();
            assert!(
                msl_fs.contains("fragment"),
                "{}: MSL missing a fragment function",
                source.name
            );
            let hlsl_fs = parsed.to_hlsl("fs_main", ShaderStage::Fragment).unwrap();
            assert!(!hlsl_fs.is_empty(), "{}: empty fragment HLSL", source.name);
        }
    }

    #[test]
    fn a_missing_entry_point_is_a_compile_error_not_a_panic() {
        let parsed = ParsedShader::parse(crate::library::SOLID_WGSL).unwrap();
        let error = parsed
            .to_spirv("does_not_exist", ShaderStage::Vertex)
            .unwrap_err();
        assert!(matches!(error, CompileError::MissingEntryPoint { .. }));
    }

    #[test]
    fn a_syntax_error_is_a_parse_error_not_a_panic() {
        let error = ParsedShader::parse("this is not wgsl {{{").unwrap_err();
        assert!(matches!(error, CompileError::Parse(_)));
    }

    #[test]
    fn compile_returns_a_fully_populated_result_for_solid() {
        let compiled = compile(crate::library::SOLID_WGSL, "vs_main", ShaderStage::Vertex).unwrap();
        assert!(!compiled.spirv.is_empty());
        assert!(!compiled.msl.is_empty());
        assert!(!compiled.hlsl.is_empty());
        assert!(!compiled.reflection.vertex_attributes.is_empty());
    }
}
