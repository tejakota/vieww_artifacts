//! The render queue — After Effects' Render Queue and output modules
//! (§2.15 L6), Blender's render jobs, Nuke's Write nodes.
//!
//! A [`RenderQueue`] holds [`Job`]s. Each job is a frame source (any
//! function `frame index → Image`, a composition's `render`, a 3D scene, a
//! film's FrameDriver), a frame range, a frame rate and one or more
//! [`OutputModule`]s — every module receives every frame, so one pass can
//! write a PNG master sequence *and* an MJPEG proxy *and* a GIF preview.
//! Jobs run in order; each reports [`JobStatus`] and per-job timing, and a
//! progress callback sees every frame. A job that fails stops alone — the
//! queue carries on, as the AE queue does.

use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use vieww_foundation::Image;

use crate::avi::MjpegWriter;
use crate::export::{FrameSink, GifWriter, PngSequence, Y4mWriter};

/// Where a job's frames go.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputModule {
    /// `dir/prefix-00000.png` …
    PngSequence { dir: PathBuf, prefix: String },
    /// An animated GIF file.
    Gif { path: PathBuf },
    /// An uncompressed Y4M (4:2:0) file.
    Y4m { path: PathBuf },
    /// A Motion-JPEG AVI at `quality`.
    Mjpeg { path: PathBuf, quality: u8 },
    /// Keep the frames in memory (tests, thumbnails, further processing).
    Memory,
}

/// A job's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Queued,
    Done { frames: u64, elapsed: Duration },
    Failed(String),
}

/// One render.
pub struct Job {
    pub name: String,
    pub source: Box<dyn Fn(u64) -> Image>,
    pub range: std::ops::Range<u64>,
    pub fps: u32,
    pub outputs: Vec<OutputModule>,
    pub status: JobStatus,
    /// Frames kept by an [`OutputModule::Memory`] output.
    pub memory: Vec<Image>,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job")
            .field("name", &self.name)
            .field("range", &self.range)
            .field("fps", &self.fps)
            .field("outputs", &self.outputs)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl Job {
    pub fn new(name: &str, range: std::ops::Range<u64>, fps: u32, source: impl Fn(u64) -> Image + 'static) -> Self {
        Self {
            name: name.to_owned(),
            source: Box::new(source),
            range,
            fps,
            outputs: Vec::new(),
            status: JobStatus::Queued,
            memory: Vec::new(),
        }
    }

    #[must_use]
    pub fn output(mut self, o: OutputModule) -> Self {
        self.outputs.push(o);
        self
    }
}

enum Sink {
    Png(PngSequence),
    Gif(GifWriter<std::io::BufWriter<std::fs::File>>),
    Y4m(Y4mWriter<std::io::BufWriter<std::fs::File>>),
    Mjpeg(MjpegWriter<std::io::BufWriter<std::fs::File>>),
    Memory,
}

/// Progress: `(job index, frames done, frames total)`.
pub type Progress<'a> = &'a mut dyn FnMut(usize, u64, u64);

/// Jobs in order.
#[derive(Debug, Default)]
pub struct RenderQueue {
    pub jobs: Vec<Job>,
}

impl RenderQueue {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, job: Job) -> usize {
        self.jobs.push(job);
        self.jobs.len() - 1
    }

    /// Render every queued job.
    pub fn render(&mut self, progress: Progress<'_>) {
        for (ji, job) in self.jobs.iter_mut().enumerate() {
            if job.status != JobStatus::Queued {
                continue;
            }
            let start = Instant::now();
            job.status = match run(job, ji, progress) {
                Ok(n) => JobStatus::Done {
                    frames: n,
                    elapsed: start.elapsed(),
                },
                Err(e) => JobStatus::Failed(e.to_string()),
            };
        }
    }
}

fn file(p: &PathBuf) -> io::Result<std::io::BufWriter<std::fs::File>> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    Ok(std::io::BufWriter::new(std::fs::File::create(p)?))
}

fn run(job: &mut Job, ji: usize, progress: Progress<'_>) -> io::Result<u64> {
    let total = job.range.end.saturating_sub(job.range.start);
    if total == 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty frame range"));
    }
    let first = (job.source)(job.range.start);
    let (w, h) = (first.width(), first.height());
    let mut sinks = Vec::new();
    for o in &job.outputs {
        sinks.push(match o {
            OutputModule::PngSequence { dir, prefix } => Sink::Png(PngSequence::new(dir, prefix)?),
            OutputModule::Gif { path } => Sink::Gif(GifWriter::new(file(path)?, w, h, job.fps)?),
            OutputModule::Y4m { path } => Sink::Y4m(Y4mWriter::new(file(path)?, w, h, (job.fps, 1))),
            OutputModule::Mjpeg { path, quality } => {
                Sink::Mjpeg(MjpegWriter::new(file(path)?, w, h, job.fps, *quality))
            }
            OutputModule::Memory => Sink::Memory,
        });
    }
    let mut frame = Some(first);
    for (k, f) in job.range.clone().enumerate() {
        let img = frame.take().unwrap_or_else(|| (job.source)(f));
        for s in &mut sinks {
            match s {
                Sink::Png(x) => x.push(&img)?,
                Sink::Gif(x) => x.push(&img)?,
                Sink::Y4m(x) => x.push(&img)?,
                Sink::Mjpeg(x) => x.push(&img)?,
                Sink::Memory => job.memory.push(img.clone()),
            }
        }
        progress(ji, k as u64 + 1, total);
    }
    for s in &mut sinks {
        match s {
            Sink::Png(x) => x.finish()?,
            Sink::Gif(x) => x.finish()?,
            Sink::Y4m(x) => x.finish()?,
            Sink::Mjpeg(x) => x.finish()?,
            Sink::Memory => {}
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avi::MjpegAvi;
    use crate::VideoSource;

    fn src(f: u64) -> Image {
        #[allow(clippy::cast_possible_truncation)]
        let v = (f * 40) as u8;
        Image::from_rgba8([v, 255 - v, 128, 255].repeat(16 * 8), 16, 8)
    }

    #[test]
    fn one_pass_feeds_every_output_module() {
        let dir = std::env::temp_dir().join(format!("vieww-queue-{}", std::process::id()));
        let mut q = RenderQueue::new();
        q.add(
            Job::new("master", 0..5, 24, src)
                .output(OutputModule::PngSequence { dir: dir.join("png"), prefix: "f".into() })
                .output(OutputModule::Mjpeg { path: dir.join("proxy.avi"), quality: 85 })
                .output(OutputModule::Gif { path: dir.join("preview.gif") })
                .output(OutputModule::Memory),
        );
        q.add(Job::new("empty", 3..3, 24, src));
        let mut seen = Vec::new();
        q.render(&mut |j, d, t| seen.push((j, d, t)));
        assert!(matches!(q.jobs[0].status, JobStatus::Done { frames: 5, .. }));
        assert!(matches!(q.jobs[1].status, JobStatus::Failed(_)), "the bad job fails alone");
        assert_eq!(seen.last(), Some(&(0, 5, 5)));
        assert_eq!(q.jobs[0].memory.len(), 5);
        assert!(dir.join("png/f-00004.png").exists());
        let avi = MjpegAvi::parse(std::fs::read(dir.join("proxy.avi")).unwrap()).unwrap();
        assert_eq!(avi.frame_count(), 5);
        let gif = vieww_image::codec::gif::decode(&std::fs::read(dir.join("preview.gif")).unwrap()).unwrap();
        assert_eq!(gif.frames.len(), 5);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
