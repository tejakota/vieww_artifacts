//! GGUF: the file format llama.cpp's models are published in.
//!
//! ```text
//!  "GGUF"  version:u32  n_tensors:u64  n_kv:u64
//!  n_kv x  (key:string  type:u32  value)          metadata: hyper-parameters, the tokenizer
//!  n_tensors x (name:string  n_dims:u32  ne:[u64]  type:u32  offset:u64)
//!  padding to `general.alignment` (32)
//!  tensor data, each at its offset from here
//! ```
//!
//! Strings are a u64 length and UTF-8 bytes; everything is little-endian.
//! `ne[0]` is the innermost (fastest) dimension, so a weight matrix
//! `out x in` is stored as `ne = [in, out]`: `out` rows of `in` values.
//!
//! The whole file is read into memory once and tensors are views into it;
//! a 1.8 GB model is 1.8 GB of RAM, no copies.

use std::collections::BTreeMap;
use std::path::Path;

/// A metadata value.
#[derive(Debug, Clone, PartialEq)]
pub enum Meta {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Array(Vec<Meta>),
}

impl Meta {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Meta::Int(i) => Some(*i),
            _ => None,
        }
    }
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Meta::Float(f) => Some(*f),
            Meta::Int(i) => Some(*i as f64),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Meta::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[Meta]> {
        match self {
            Meta::Array(a) => Some(a),
            _ => None,
        }
    }
}

/// The ggml element types this reader understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    F32,
    F16,
    BF16,
    /// Blocks of 32: one f16 scale, 32 signed bytes (34 bytes).
    Q8_0,
}

impl DType {
    fn from_ggml(t: u32) -> Result<DType, String> {
        Ok(match t {
            0 => DType::F32,
            1 => DType::F16,
            8 => DType::Q8_0,
            30 => DType::BF16,
            other => return Err(format!("tensor type {other} is not supported (use an F32, F16, BF16 or Q8_0 file)")),
        })
    }

    /// Bytes for `n` elements.
    pub fn bytes(self, n: usize) -> usize {
        match self {
            DType::F32 => n * 4,
            DType::F16 | DType::BF16 => n * 2,
            DType::Q8_0 => n / 32 * 34,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TensorInfo {
    pub dtype: DType,
    /// `ne[0]` first (innermost).
    pub dims: Vec<usize>,
    /// Absolute offset in the file.
    pub start: usize,
    pub len: usize,
}

impl TensorInfo {
    pub fn elements(&self) -> usize {
        self.dims.iter().product()
    }
}

pub struct Gguf {
    pub meta: BTreeMap<String, Meta>,
    pub tensors: BTreeMap<String, TensorInfo>,
    pub data: Vec<u8>,
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|e| *e <= self.b.len()).ok_or("the file ends early (truncated download?)")?;
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(|_| "u32")?))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().map_err(|_| "u64")?))
    }
    fn len(&mut self) -> Result<usize, String> {
        let n = self.u64()?;
        // No real metadata item is bigger than the file it is in.
        usize::try_from(n).ok().filter(|n| *n <= self.b.len()).ok_or_else(|| format!("implausible length {n}"))
    }
    fn string(&mut self) -> Result<String, String> {
        let n = self.len()?;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
    fn value(&mut self, ty: u32, depth: u32) -> Result<Meta, String> {
        Ok(match ty {
            0 => Meta::Int(i64::from(self.take(1)?[0])),
            1 => Meta::Int(i64::from(self.take(1)?[0] as i8)),
            2 => Meta::Int(i64::from(u16::from_le_bytes(self.take(2)?.try_into().map_err(|_| "u16")?))),
            3 => Meta::Int(i64::from(i16::from_le_bytes(self.take(2)?.try_into().map_err(|_| "i16")?))),
            4 => Meta::Int(i64::from(self.u32()?)),
            5 => Meta::Int(i64::from(self.u32()? as i32)),
            6 => Meta::Float(f64::from(f32::from_le_bytes(self.take(4)?.try_into().map_err(|_| "f32")?))),
            7 => Meta::Bool(self.take(1)?[0] != 0),
            8 => Meta::Str(self.string()?),
            9 => {
                if depth > 2 {
                    return Err("metadata nested too deeply".into());
                }
                let inner = self.u32()?;
                let n = self.len()?;
                let mut v = Vec::with_capacity(n.min(1 << 20));
                for _ in 0..n {
                    v.push(self.value(inner, depth + 1)?);
                }
                Meta::Array(v)
            }
            10 => Meta::Int(self.u64()? as i64),
            11 => Meta::Int(self.u64()? as i64),
            12 => Meta::Float(f64::from_le_bytes(self.take(8)?.try_into().map_err(|_| "f64")?)),
            other => return Err(format!("unknown metadata type {other}")),
        })
    }
}

impl Gguf {
    pub fn open(path: &Path) -> Result<Gguf, String> {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Gguf::parse(data).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn parse(data: Vec<u8>) -> Result<Gguf, String> {
        let mut r = Reader { b: &data, at: 0 };
        if r.take(4)? != b"GGUF" {
            return Err("not a GGUF file".into());
        }
        let version = r.u32()?;
        if !(2..=3).contains(&version) {
            return Err(format!("GGUF version {version} is not supported (2 or 3)"));
        }
        let n_tensors = r.len()?;
        let n_kv = r.len()?;
        let mut meta = BTreeMap::new();
        for _ in 0..n_kv {
            let key = r.string()?;
            let ty = r.u32()?;
            meta.insert(key, r.value(ty, 0)?);
        }
        let mut infos = Vec::with_capacity(n_tensors);
        for _ in 0..n_tensors {
            let name = r.string()?;
            let n_dims = r.u32()? as usize;
            if n_dims > 4 {
                return Err(format!("{name}: {n_dims} dimensions"));
            }
            let mut dims = Vec::with_capacity(n_dims);
            for _ in 0..n_dims {
                dims.push(r.len()?);
            }
            let dtype = DType::from_ggml(r.u32()?).map_err(|e| format!("{name}: {e}"))?;
            let offset = r.len()?;
            infos.push((name, dims, dtype, offset));
        }
        let align = meta.get("general.alignment").and_then(Meta::as_int).unwrap_or(32).max(1) as usize;
        let base = r.at.div_ceil(align) * align;
        let mut tensors = BTreeMap::new();
        for (name, dims, dtype, offset) in infos {
            let n: usize = dims.iter().product();
            if dtype == DType::Q8_0 && !n.is_multiple_of(32) {
                return Err(format!("{name}: {n} elements is not a whole number of Q8_0 blocks"));
            }
            let len = dtype.bytes(n);
            let start = base.checked_add(offset).ok_or("offset overflow")?;
            if start.checked_add(len).is_none_or(|e| e > data.len()) {
                return Err(format!("{name} runs past the end of the file (truncated download?)"));
            }
            tensors.insert(name, TensorInfo { dtype, dims, start, len });
        }
        Ok(Gguf { meta, tensors, data })
    }

    pub fn get(&self, key: &str) -> Option<&Meta> {
        self.meta.get(key)
    }

    pub fn int(&self, key: &str) -> Result<i64, String> {
        self.get(key).and_then(Meta::as_int).ok_or_else(|| format!("the model file has no '{key}'"))
    }

    pub fn float(&self, key: &str) -> Result<f64, String> {
        self.get(key).and_then(Meta::as_float).ok_or_else(|| format!("the model file has no '{key}'"))
    }

    pub fn tensor(&self, name: &str) -> Result<(&TensorInfo, &[u8]), String> {
        let t = self.tensors.get(name).ok_or_else(|| format!("the model file has no tensor '{name}'"))?;
        Ok((t, &self.data[t.start..t.start + t.len]))
    }
}
