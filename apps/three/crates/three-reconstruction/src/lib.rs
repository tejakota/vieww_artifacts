//! Reconstruction layer for converting camera observations into temporal 3D.
//!
//! The actual computer-vision/ML algorithm is intentionally a plugin boundary.
//! This crate defines the stable contract and ships a tiny deterministic
//! implementation for development and tests.

use std::fmt;
use three_capture::{CameraFrame, TrackingSample};
use three_core::{Capture3D, CaptureSource, MeshFrame, PointFrame, TimestampNs, Vec3};

/// A bundle of synchronized observations for one moment in time.
#[derive(Clone, Debug)]
pub struct Observation {
    pub frame: CameraFrame,
    pub tracking: Option<TrackingSample>,
}

/// Reconstruction quality reported by an implementation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReconstructionQuality {
    pub geometry_confidence: f32,
    pub tracking_confidence: f32,
    pub completeness: f32,
}

/// Reconstruction errors.
#[derive(Debug)]
pub enum ReconstructionError { Invalid(&'static str), Algorithm(String) }
impl fmt::Display for ReconstructionError { fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result{match self{Self::Invalid(s)=>f.write_str(s),Self::Algorithm(s)=>write!(f,"reconstruction error: {s}")}} }
impl std::error::Error for ReconstructionError {}

/// A reconstruction engine consumes synchronized observations and emits a
/// temporal 3D representation.
pub trait Reconstructor {
    fn begin(&mut self) -> Result<(), ReconstructionError>;
    fn push(&mut self, observation: Observation) -> Result<(), ReconstructionError>;
    fn finish(&mut self) -> Result<ReconstructionResult, ReconstructionError>;
}

/// Result returned after the capture has been reconstructed.
pub struct ReconstructionResult {
    pub capture: Capture3D,
    pub quality: ReconstructionQuality,
}

/// Deterministic placeholder implementation.
///
/// It does **not** perform real 3D reconstruction. It creates a tiny animated
/// triangle so the complete capture -> reconstruction -> `.3` pipeline can be
/// tested before Android/iOS CV code is introduced.
pub struct DemoReconstructor {
    observations: Vec<Observation>,
}
impl DemoReconstructor { pub fn new() -> Self { Self { observations: Vec::new() } } }
impl Default for DemoReconstructor { fn default()->Self{Self::new()} }
impl Reconstructor for DemoReconstructor {
    fn begin(&mut self)->Result<(),ReconstructionError>{self.observations.clear();Ok(())}
    fn push(&mut self, observation:Observation)->Result<(),ReconstructionError>{self.observations.push(observation);Ok(())}
    fn finish(&mut self)->Result<ReconstructionResult,ReconstructionError>{
        if self.observations.is_empty(){return Err(ReconstructionError::Invalid("no camera observations"));}
        let mut meshes=Vec::with_capacity(self.observations.len());
        for (i,o) in self.observations.iter().enumerate(){
            let t=TimestampNs(o.frame.timestamp_ns);
            let dx=(i as f32)*0.01;
            meshes.push(MeshFrame{timestamp:t,vertices:vec![Vec3::new(-0.5+dx,0.0,0.0),Vec3::new(0.5+dx,0.0,0.0),Vec3::new(0.0,1.0,0.0)],indices:vec![0,1,2]});
        }
        let duration_ns=self.observations.last().unwrap().frame.timestamp_ns.max(1);
        Ok(ReconstructionResult{capture:Capture3D{duration_ns,source:CaptureSource::Synthetic,cameras:vec![],depth:vec![],meshes,points:Vec::<PointFrame>::new(),title:"3 capture".into()},quality:ReconstructionQuality{geometry_confidence:0.01,tracking_confidence:0.0,completeness:0.01}})
    }
}
