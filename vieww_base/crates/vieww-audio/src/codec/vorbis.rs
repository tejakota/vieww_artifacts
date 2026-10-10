//! Ogg Vorbis decoding (stage 5 of the audio roadmap fills this in).

/// A decoded stream.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    /// Interleaved samples.
    pub data: Vec<f32>,
    /// Channels.
    pub channels: u8,
    /// Sample rate.
    pub rate: u32,
}

/// Decode an Ogg Vorbis stream.
///
/// # Errors
/// Always, until the decoder lands.
pub fn decode(_bytes: &[u8]) -> Result<Decoded, String> {
    Err("the Vorbis decoder is not implemented yet".into())
}
