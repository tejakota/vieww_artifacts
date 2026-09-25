//! Tier definitions. A tier is *policy only* — one pipeline executes all of them.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tier {
    /// Byte-exact reversible. Lepton, oxipng, zstd, whole-file dedup.
    Move,
    /// Irreversible, quality-gated to visually lossless.
    DeepMove,
    /// Irreversible, visible on close inspection. Opt-in, not shipped in v1.
    DeepMovePlus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PhotoPolicy {
    /// Bit-exact recode; original bytes reconstructable.
    Lossless,
    /// Re-encode, verified against a perceptual floor.
    Perceptual { min_ssimulacra2: f32 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum VideoPolicy {
    /// Store as-is. The only correct action for HEVC/AV1 sources in Move.
    Passthrough,
    Transcode {
        min_vmaf: f32,
        max_height: Option<u32>,
        /// Re-encode audio to Opus. Negligible on 4K, ~10% on low-bitrate clips.
        opus_audio_kbps: Option<u32>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Policy {
    pub tier: Tier,
    pub photo: PhotoPolicy,
    pub video: VideoPolicy,
    pub dedup: bool,
    pub retain_originals_days: u32,
}

impl Tier {
    pub fn policy(self) -> Policy {
        match self {
            Tier::Move => Policy {
                tier: self,
                photo: PhotoPolicy::Lossless,
                video: VideoPolicy::Passthrough,
                dedup: true,
                retain_originals_days: 0,
            },
            Tier::DeepMove => Policy {
                tier: self,
                photo: PhotoPolicy::Perceptual {
                    min_ssimulacra2: 90.0,
                },
                video: VideoPolicy::Transcode {
                    min_vmaf: 95.0,
                    max_height: None,
                    opus_audio_kbps: Some(64),
                },
                dedup: true,
                retain_originals_days: 14,
            },
            Tier::DeepMovePlus => Policy {
                tier: self,
                photo: PhotoPolicy::Perceptual {
                    min_ssimulacra2: 82.0,
                },
                video: VideoPolicy::Transcode {
                    min_vmaf: 90.0,
                    max_height: Some(1080),
                    opus_audio_kbps: Some(48),
                },
                dedup: true,
                retain_originals_days: 14,
            },
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tier::Move => "Move",
            Tier::DeepMove => "Deep Move",
            Tier::DeepMovePlus => "Deep Move+",
        }
    }

    pub fn reversible(self) -> bool {
        matches!(self, Tier::Move)
    }

    pub fn all() -> [Tier; 3] {
        [Tier::Move, Tier::DeepMove, Tier::DeepMovePlus]
    }
}
