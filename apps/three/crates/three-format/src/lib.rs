//! Reference `.3` binary container.
//!
//! Version 0 is intentionally simple and deterministic. It is suitable for
//! prototyping and tests, while the public API leaves room for chunked,
//! compressed and streamable versions later.

use std::{fmt, io::{self, Cursor, Read, Write}};
use three_core::{CameraSample, Capture3D, CaptureSource, DepthSample, MeshFrame, PointFrame, Quaternion, TimestampNs, Transform, Vec3};

pub const MAGIC: [u8; 4] = *b"DOT3";
pub const VERSION: u16 = 0;

/// File header for a `.3` file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: u16,
    pub flags: u16,
    pub payload_len: u64,
}

/// Errors returned by `.3` encoding and decoding.
#[derive(Debug)]
pub enum FormatError {
    Io(io::Error),
    InvalidMagic,
    UnsupportedVersion(u16),
    InvalidData(&'static str),
    Core(three_core::CoreError),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::InvalidMagic => f.write_str("not a .3 file (bad magic)"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported .3 version {v}"),
            Self::InvalidData(s) => f.write_str(s),
            Self::Core(e) => write!(f, "invalid capture: {e}"),
        }
    }
}
impl std::error::Error for FormatError {}
impl From<io::Error> for FormatError { fn from(e: io::Error) -> Self { Self::Io(e) } }
impl From<three_core::CoreError> for FormatError { fn from(e: three_core::CoreError) -> Self { Self::Core(e) } }

/// Encode a complete capture into `.3` bytes.
pub fn encode(capture: &Capture3D) -> Result<Vec<u8>, FormatError> {
    capture.validate()?;
    let mut payload = Vec::new();
    write_string(&mut payload, &capture.title)?;
    payload.write_all(&capture.duration_ns.to_le_bytes())?;
    payload.write_all(&[source_to_u8(capture.source)])?;
    write_vec(&mut payload, &capture.cameras, write_camera)?;
    write_vec(&mut payload, &capture.depth, write_depth)?;
    write_vec(&mut payload, &capture.meshes, write_mesh)?;
    write_vec(&mut payload, &capture.points, write_points)?;

    let mut out = Vec::with_capacity(16 + payload.len());
    out.write_all(&MAGIC)?;
    out.write_all(&VERSION.to_le_bytes())?;
    out.write_all(&0u16.to_le_bytes())?;
    out.write_all(&(payload.len() as u64).to_le_bytes())?;
    out.write_all(&payload)?;
    Ok(out)
}

/// Decode `.3` bytes into a validated temporal capture.
pub fn decode(bytes: &[u8]) -> Result<Capture3D, FormatError> {
    let mut c = Cursor::new(bytes);
    let mut magic = [0; 4]; c.read_exact(&mut magic)?;
    if magic != MAGIC { return Err(FormatError::InvalidMagic); }
    let version = read_u16(&mut c)?;
    if version != VERSION { return Err(FormatError::UnsupportedVersion(version)); }
    let _flags = read_u16(&mut c)?;
    let payload_len = read_u64(&mut c)? as usize;
    if payload_len != bytes.len().saturating_sub(16) { return Err(FormatError::InvalidData("payload length mismatch")); }

    let title = read_string(&mut c)?;
    let duration_ns = read_u64(&mut c)?;
    let source = u8_to_source(read_u8(&mut c)?)?;
    let cameras = read_vec(&mut c, read_camera)?;
    let depth = read_vec(&mut c, read_depth)?;
    let meshes = read_vec(&mut c, read_mesh)?;
    let points = read_vec(&mut c, read_points)?;
    let capture = Capture3D { duration_ns, source, cameras, depth, meshes, points, title };
    capture.validate()?;
    Ok(capture)
}

pub fn header(bytes: &[u8]) -> Result<Header, FormatError> {
    if bytes.len() < 16 { return Err(FormatError::InvalidData("file is shorter than header")); }
    if bytes[0..4] != MAGIC { return Err(FormatError::InvalidMagic); }
    Ok(Header {
        version: u16::from_le_bytes([bytes[4], bytes[5]]),
        flags: u16::from_le_bytes([bytes[6], bytes[7]]),
        payload_len: u64::from_le_bytes(bytes[8..16].try_into().unwrap()),
    })
}

fn source_to_u8(s: CaptureSource) -> u8 { match s { CaptureSource::Rgb => 0, CaptureSource::RgbWithDepth => 1, CaptureSource::RgbWithLidar => 2, CaptureSource::MultiCamera => 3, CaptureSource::Synthetic => 4 } }
fn u8_to_source(v: u8) -> Result<CaptureSource, FormatError> { Ok(match v { 0 => CaptureSource::Rgb, 1 => CaptureSource::RgbWithDepth, 2 => CaptureSource::RgbWithLidar, 3 => CaptureSource::MultiCamera, 4 => CaptureSource::Synthetic, _ => return Err(FormatError::InvalidData("unknown capture source")) }) }

fn write_vec<T, F: FnMut(&mut Vec<u8>, &T) -> Result<(), FormatError>>(out: &mut Vec<u8>, values: &[T], mut f: F) -> Result<(), FormatError> {
    out.write_all(&(values.len() as u32).to_le_bytes())?;
    for v in values { f(out, v)?; }
    Ok(())
}
fn read_vec<T, F: FnMut(&mut Cursor<&[u8]>) -> Result<T, FormatError>>(c: &mut Cursor<&[u8]>, mut f: F) -> Result<Vec<T>, FormatError> {
    let count = read_u32(c)? as usize;
    if count > 10_000_000 { return Err(FormatError::InvalidData("vector count is unreasonably large")); }
    (0..count).map(|_| f(c)).collect()
}
fn write_f32(out: &mut Vec<u8>, v: f32) -> Result<(), FormatError> { out.write_all(&v.to_le_bytes()).map_err(Into::into) }
fn read_f32(c: &mut Cursor<&[u8]>) -> Result<f32, FormatError> { Ok(f32::from_le_bytes(read_exact::<4>(c)?)) }
fn write_vec3(out: &mut Vec<u8>, v: Vec3) -> Result<(), FormatError> { write_f32(out,v.x)?; write_f32(out,v.y)?; write_f32(out,v.z) }
fn read_vec3(c: &mut Cursor<&[u8]>) -> Result<Vec3, FormatError> { Ok(Vec3::new(read_f32(c)?,read_f32(c)?,read_f32(c)?)) }
fn write_quat(out: &mut Vec<u8>, q: Quaternion) -> Result<(), FormatError> { write_f32(out,q.x)?; write_f32(out,q.y)?; write_f32(out,q.z)?; write_f32(out,q.w) }
fn read_quat(c: &mut Cursor<&[u8]>) -> Result<Quaternion, FormatError> { Ok(Quaternion { x: read_f32(c)?, y: read_f32(c)?, z: read_f32(c)?, w: read_f32(c)? }) }
fn write_transform(out: &mut Vec<u8>, t: Transform) -> Result<(), FormatError> { write_vec3(out,t.translation)?; write_quat(out,t.rotation) }
fn read_transform(c: &mut Cursor<&[u8]>) -> Result<Transform, FormatError> { Ok(Transform { translation: read_vec3(c)?, rotation: read_quat(c)? }) }
fn write_camera(out: &mut Vec<u8>, s: &CameraSample) -> Result<(), FormatError> { out.write_all(&s.timestamp.0.to_le_bytes())?; write_transform(out,s.pose)?; write_f32(out,s.focal_length_px.0)?; write_f32(out,s.focal_length_px.1)?; write_f32(out,s.principal_point_px.0)?; write_f32(out,s.principal_point_px.1)?; out.write_all(&s.image_size_px.0.to_le_bytes())?; out.write_all(&s.image_size_px.1.to_le_bytes())?; Ok(()) }
fn read_camera(c: &mut Cursor<&[u8]>) -> Result<CameraSample, FormatError> { Ok(CameraSample { timestamp: TimestampNs(read_u64(c)?), pose: read_transform(c)?, focal_length_px: (read_f32(c)?,read_f32(c)?), principal_point_px: (read_f32(c)?,read_f32(c)?), image_size_px: (read_u32(c)?,read_u32(c)?) }) }
fn write_depth(out: &mut Vec<u8>, s: &DepthSample) -> Result<(), FormatError> { out.write_all(&s.timestamp.0.to_le_bytes())?; out.write_all(&s.width.to_le_bytes())?; out.write_all(&s.height.to_le_bytes())?; out.write_all(&(s.millimeters.len() as u32).to_le_bytes())?; for v in &s.millimeters { out.write_all(&v.to_le_bytes())?; } Ok(()) }
fn read_depth(c: &mut Cursor<&[u8]>) -> Result<DepthSample, FormatError> { let timestamp=TimestampNs(read_u64(c)?); let width=read_u32(c)?; let height=read_u32(c)?; let n=read_u32(c)? as usize; let mut millimeters=Vec::with_capacity(n); for _ in 0..n { millimeters.push(read_u16(c)?); } Ok(DepthSample{timestamp,width,height,millimeters}) }
fn write_mesh(out: &mut Vec<u8>, m: &MeshFrame) -> Result<(), FormatError> { out.write_all(&m.timestamp.0.to_le_bytes())?; write_vec(out,&m.vertices,|o,v|write_vec3(o,*v))?; out.write_all(&(m.indices.len() as u32).to_le_bytes())?; for i in &m.indices { out.write_all(&i.to_le_bytes())?; } Ok(()) }
fn read_mesh(c: &mut Cursor<&[u8]>) -> Result<MeshFrame, FormatError> { let timestamp=TimestampNs(read_u64(c)?); let vertices=read_vec(c,read_vec3)?; let n=read_u32(c)? as usize; let mut indices=Vec::with_capacity(n); for _ in 0..n { indices.push(read_u32(c)?); } Ok(MeshFrame{timestamp,vertices,indices}) }
fn write_points(out: &mut Vec<u8>, p: &PointFrame) -> Result<(), FormatError> { out.write_all(&p.timestamp.0.to_le_bytes())?; write_vec(out,&p.points,|o,v|write_vec3(o,*v))?; Ok(()) }
fn read_points(c: &mut Cursor<&[u8]>) -> Result<PointFrame, FormatError> { Ok(PointFrame{timestamp:TimestampNs(read_u64(c)?),points:read_vec(c,read_vec3)?}) }
fn write_string(out: &mut Vec<u8>, s: &str) -> Result<(), FormatError> { let bytes=s.as_bytes(); if bytes.len()>1_000_000{return Err(FormatError::InvalidData("string too long"))}; out.write_all(&(bytes.len() as u32).to_le_bytes())?; out.write_all(bytes)?; Ok(()) }
fn read_string(c: &mut Cursor<&[u8]>) -> Result<String, FormatError> { let n=read_u32(c)? as usize; let mut b=vec![0;n]; c.read_exact(&mut b)?; String::from_utf8(b).map_err(|_|FormatError::InvalidData("invalid UTF-8 string")) }
fn read_u8(c:&mut Cursor<&[u8]>)->Result<u8,FormatError>{Ok(read_exact::<1>(c)?[0])}
fn read_u16(c:&mut Cursor<&[u8]>)->Result<u16,FormatError>{Ok(u16::from_le_bytes(read_exact::<2>(c)?))}
fn read_u32(c:&mut Cursor<&[u8]>)->Result<u32,FormatError>{Ok(u32::from_le_bytes(read_exact::<4>(c)?))}
fn read_u64(c:&mut Cursor<&[u8]>)->Result<u64,FormatError>{Ok(u64::from_le_bytes(read_exact::<8>(c)?))}
fn read_exact<const N:usize>(c:&mut Cursor<&[u8]>)->Result<[u8;N],FormatError>{let mut b=[0;N];c.read_exact(&mut b)?;Ok(b)}

#[cfg(test)]
mod tests {
    use super::*;
    use three_core::*;

    fn sample() -> Capture3D {
        Capture3D {
            duration_ns: 1_000_000_000,
            source: CaptureSource::Synthetic,
            cameras: vec![CameraSample { timestamp: TimestampNs(0), pose: Transform::default(), focal_length_px:(500.0,500.0), principal_point_px:(320.0,240.0), image_size_px:(640,480) }],
            depth: vec![],
            meshes: vec![MeshFrame { timestamp:TimestampNs(0), vertices:vec![Vec3::new(0.0,0.0,0.0),Vec3::new(1.0,0.0,0.0),Vec3::new(0.0,1.0,0.0)], indices:vec![0,1,2] }],
            points: vec![],
            title:"demo".into(),
        }
    }

    #[test]
    fn round_trip_is_lossless() { let value=sample(); let bytes=encode(&value).unwrap(); let decoded=decode(&bytes).unwrap(); assert_eq!(decoded,value); }
    #[test]
    fn magic_is_checked() { assert!(matches!(decode(b"nope"),Err(FormatError::InvalidMagic))); }
}
