//! AIFF and AIFF-C — Apple's (and Pro Tools') interchange format.
//!
//! Big-endian IFF: a `FORM` of type `AIFF` or `AIFC` holding a `COMM` chunk
//! (channels, frames, bits, and the sample rate as an 80-bit IEEE extended
//! float) and an `SSND` chunk of samples. AIFF samples are big-endian
//! signed integers — including 8-bit, which unlike WAV's is *signed*.
//! AIFF-C adds a compression code; the uncompressed ones are read here:
//! `NONE`, `twos` (big-endian), `sowt` (little-endian, as Logic writes),
//! `fl32`/`FL32` and `fl64`/`FL64` (IEEE floats).

use super::{CodecError, Decoded};

/// Decode an 80-bit IEEE 754 extended float (big-endian).
#[must_use]
pub fn extended_to_f64(b: &[u8; 10]) -> f64 {
    let sign = if b[0] & 0x80 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from(u16::from_be_bytes([b[0] & 0x7F, b[1]]));
    let mant = u64::from_be_bytes([b[2], b[3], b[4], b[5], b[6], b[7], b[8], b[9]]);
    if exp == 0 && mant == 0 {
        return 0.0;
    }
    #[expect(clippy::cast_precision_loss, reason = "the format's own precision")]
    let m = mant as f64;
    sign * m * 2f64.powi(exp - 16_383 - 63)
}

/// Encode a positive integer-valued rate as an 80-bit extended float.
#[must_use]
pub fn f64_to_extended(v: f64) -> [u8; 10] {
    let mut out = [0u8; 10];
    if v <= 0.0 {
        return out;
    }
    #[expect(clippy::cast_possible_truncation, reason = "exponent of a sample rate")]
    let e = v.log2().floor() as i32;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "normalised mantissa"
    )]
    let mant = (v / 2f64.powi(e) * 2f64.powi(63)) as u64;
    let exp = u16::try_from(e + 16_383).unwrap_or(0);
    out[..2].copy_from_slice(&exp.to_be_bytes());
    out[2..].copy_from_slice(&mant.to_be_bytes());
    out
}

/// Decode AIFF / AIFF-C.
///
/// # Errors
/// Malformed files, compressed AIFF-C types, unknown depths.
pub fn decode(bytes: &[u8]) -> Result<Decoded, CodecError> {
    if bytes.len() < 12 || &bytes[0..4] != b"FORM" {
        return Err(CodecError::NotThisFormat);
    }
    let aifc = match &bytes[8..12] {
        b"AIFF" => false,
        b"AIFC" => true,
        _ => return Err(CodecError::NotThisFormat),
    };
    let mut comm: Option<&[u8]> = None;
    let mut ssnd: Option<&[u8]> = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let len = u32::from_be_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]])
            as usize;
        let s = at + 8;
        let e = s
            .checked_add(len)
            .ok_or(CodecError::Truncated)?
            .min(bytes.len());
        match id {
            b"COMM" => comm = Some(&bytes[s..e]),
            b"SSND" => ssnd = Some(&bytes[s..e]),
            _ => {}
        }
        at = s + len + (len & 1);
    }
    let c = comm.ok_or(CodecError::Malformed("no COMM chunk"))?;
    if c.len() < 18 {
        return Err(CodecError::Truncated);
    }
    let channels = u16::from_be_bytes([c[0], c[1]]);
    let frames = u32::from_be_bytes([c[2], c[3], c[4], c[5]]) as usize;
    let bits = u16::from_be_bytes([c[6], c[7]]);
    let mut ext = [0u8; 10];
    ext.copy_from_slice(&c[8..18]);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "sample rates"
    )]
    let rate = extended_to_f64(&ext).round() as u32;
    let kind: [u8; 4] = if aifc && c.len() >= 22 {
        [c[18], c[19], c[20], c[21]]
    } else {
        *b"NONE"
    };
    let s = ssnd.ok_or(CodecError::Malformed("no SSND chunk"))?;
    if s.len() < 8 {
        return Err(CodecError::Truncated);
    }
    let offset = u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as usize;
    let pcm = s.get(8 + offset..).ok_or(CodecError::Truncated)?;
    let ch = usize::from(channels.max(1));
    let width = usize::from(bits).div_ceil(8);
    let (width, decode_one): (usize, fn(&[u8]) -> f32) = match (&kind, width) {
        (b"NONE" | b"twos", 1) => (1, |b| f32::from(b[0].cast_signed()) / 128.0),
        (b"NONE" | b"twos", 2) => (2, |b| {
            f32::from(i16::from_be_bytes([b[0], b[1]])) / 32_767.0
        }),
        (b"NONE" | b"twos", 3) => (3, |b| {
            (i32::from_be_bytes([b[0], b[1], b[2], 0]) >> 8) as f32 / 8_388_607.0
        }),
        (b"NONE" | b"twos", 4) => (4, |b| {
            (f64::from(i32::from_be_bytes([b[0], b[1], b[2], b[3]])) / 2_147_483_647.0) as f32
        }),
        (b"sowt", 2) => (2, |b| {
            f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_767.0
        }),
        (b"sowt", 3) => (3, |b| {
            (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_607.0
        }),
        (b"fl32" | b"FL32", _) => (4, |b| f32::from_be_bytes([b[0], b[1], b[2], b[3]])),
        (b"fl64" | b"FL64", _) => (8, |b| {
            f64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32
        }),
        _ => return Err(CodecError::Unsupported("this AIFF-C compression type")),
    };
    let n = (frames * ch).min(pcm.len() / width);
    let data = pcm[..n * width]
        .chunks_exact(width)
        .map(decode_one)
        .collect();
    Ok(Decoded {
        data,
        channels: u8::try_from(ch).unwrap_or(u8::MAX),
        rate,
    })
}

/// Encode big-endian integer AIFF (`bits` 16 or 24).
///
/// # Errors
/// Unsupported depths.
pub fn encode(d: &Decoded, bits: u16) -> Result<Vec<u8>, CodecError> {
    if bits != 16 && bits != 24 {
        return Err(CodecError::Unsupported("AIFF depths other than 16 and 24"));
    }
    let ch = usize::from(d.channels.max(1));
    let frames = d.data.len() / ch;
    let width = usize::from(bits / 8);
    let mut ssnd = vec![0u8; 8];
    for v in &d.data {
        let c = f64::from(v.clamp(-1.0, 1.0));
        if bits == 16 {
            #[expect(clippy::cast_possible_truncation, reason = "clamped")]
            ssnd.extend(((c * 32_767.0).round() as i16).to_be_bytes());
        } else {
            #[expect(clippy::cast_possible_truncation, reason = "clamped")]
            let q = (c * 8_388_607.0).round() as i32;
            ssnd.extend(&q.to_be_bytes()[1..]);
        }
    }
    let mut comm = Vec::new();
    comm.extend(u16::from(d.channels).to_be_bytes());
    comm.extend(u32::try_from(frames).unwrap_or(u32::MAX).to_be_bytes());
    comm.extend(bits.to_be_bytes());
    comm.extend(f64_to_extended(f64::from(d.rate)));
    let body_len = 4 + 8 + comm.len() + 8 + ssnd.len() + (ssnd.len() & 1);
    let mut out = b"FORM".to_vec();
    out.extend(u32::try_from(body_len).unwrap_or(u32::MAX).to_be_bytes());
    out.extend(b"AIFF");
    out.extend(b"COMM");
    out.extend(u32::try_from(comm.len()).unwrap_or(0).to_be_bytes());
    out.extend(&comm);
    out.extend(b"SSND");
    out.extend(u32::try_from(ssnd.len()).unwrap_or(0).to_be_bytes());
    out.extend(&ssnd);
    if ssnd.len() & 1 == 1 {
        out.push(0);
    }
    let _ = width;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_floats_round_trip_common_rates() {
        for r in [8_000.0, 22_050.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0] {
            assert_eq!(extended_to_f64(&f64_to_extended(r)), r);
        }
        // 44.1 kHz as every AIFF writer encodes it.
        assert_eq!(
            f64_to_extended(44_100.0),
            [0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn reads_ffmpegs_24_bit_aiff_like_the_wav() {
        let a = decode(include_bytes!("../../tests/fixtures/src24.aiff")).unwrap();
        let w = crate::wav::read_wav(include_bytes!("../../tests/fixtures/source16.wav")).unwrap();
        assert_eq!((a.channels, a.rate), (2, 44_100));
        assert_eq!(a.data.len(), w.samples.data.len());
        let worst = a
            .data
            .iter()
            .zip(&w.samples.data)
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f32::max);
        assert!(worst < 1e-4, "{worst}");
    }

    #[test]
    fn encode_decode_round_trip() {
        let d = Decoded {
            data: (0..400).map(|i| ((i as f32) * 0.05).sin() * 0.8).collect(),
            channels: 2,
            rate: 48_000,
        };
        for (bits, tol) in [(16, 1.0 / 30_000.0), (24, 1.0 / 8_000_000.0)] {
            let back = decode(&encode(&d, bits).unwrap()).unwrap();
            assert_eq!((back.channels, back.rate), (2, 48_000));
            let worst = back
                .data
                .iter()
                .zip(&d.data)
                .map(|(x, y)| (x - y).abs())
                .fold(0.0, f32::max);
            assert!(worst <= tol, "{bits}: {worst}");
        }
    }

    #[test]
    fn rejects_other_forms() {
        assert_eq!(
            decode(b"RIFF....WAVE").err(),
            Some(CodecError::NotThisFormat)
        );
    }
}
