//! Sprite sheets and frame animation — Unity's Sprite Editor + Animator
//! clips, Godot's `AnimatedSprite2D`/`SpriteFrames`, Phaser's texture
//! atlases, Aseprite/TexturePacker JSON (§2.14, §2.12).
//!
//! A [`SpriteSheet`] is one image plus frame rectangles (each with an
//! optional pivot and per-frame duration) and named [`Clip`]s. Build one
//! from a uniform grid ([`SpriteSheet::grid`]), from TexturePacker /
//! Aseprite "JSON hash/array" exports ([`SpriteSheet::from_json`]), or pack
//! loose frames into a new atlas ([`SpriteSheet::pack`], via
//! [`crate::atlas::AtlasPacker`]). [`AnimatedSprite`] plays a
//! clip — loop, once, ping-pong, reverse — with speed, and reports frame
//! events (a footstep on frame 3).

use std::collections::BTreeMap;

use vieww_foundation::json::Json;
use vieww_foundation::Image;

use crate::atlas::{composite, AtlasPacker, AtlasRect};

/// One frame of a sheet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpriteFrame {
    pub rect: AtlasRect,
    /// Pivot, 0..1 of the frame (0.5, 1.0 = feet).
    pub pivot: (f32, f32),
    /// Seconds, when the source gave per-frame timing.
    pub duration: Option<f32>,
}

/// How a clip plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayMode {
    #[default]
    Loop,
    Once,
    PingPong,
    Reverse,
}

/// A named frame sequence.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub frames: Vec<usize>,
    pub fps: f32,
    pub mode: PlayMode,
    /// (frame index within the clip, event name).
    pub events: Vec<(usize, String)>,
}

impl Clip {
    #[must_use]
    pub fn new(frames: impl IntoIterator<Item = usize>, fps: f32) -> Self {
        Self {
            frames: frames.into_iter().collect(),
            fps,
            mode: PlayMode::Loop,
            events: Vec::new(),
        }
    }

    #[must_use]
    pub const fn mode(mut self, mode: PlayMode) -> Self {
        self.mode = mode;
        self
    }

    #[must_use]
    pub fn event(mut self, at: usize, name: &str) -> Self {
        self.events.push((at, name.to_owned()));
        self
    }
}

/// A sprite sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct SpriteSheet {
    pub image: Image,
    pub frames: Vec<SpriteFrame>,
    pub names: BTreeMap<String, usize>,
    pub clips: BTreeMap<String, Clip>,
}

impl SpriteSheet {
    /// A uniform grid of `cols × rows` cells, row-major.
    #[must_use]
    pub fn grid(image: Image, cols: u32, rows: u32) -> Self {
        let (w, h) = (image.width() / cols.max(1), image.height() / rows.max(1));
        let frames = (0..rows)
            .flat_map(|r| (0..cols).map(move |c| (c, r)))
            .map(|(c, r)| SpriteFrame {
                rect: AtlasRect { x: c * w, y: r * h, width: w, height: h },
                pivot: (0.5, 0.5),
                duration: None,
            })
            .collect();
        Self {
            image,
            frames,
            names: BTreeMap::new(),
            clips: BTreeMap::new(),
        }
    }

    /// Parse TexturePacker / Aseprite JSON (hash or array `frames`, with
    /// optional `duration` in ms, `pivot`, and Aseprite `meta.frameTags`).
    ///
    /// # Errors
    /// On malformed JSON or missing frame rectangles.
    pub fn from_json(image: Image, json: &str) -> Result<Self, String> {
        let j = Json::parse(json).map_err(|e| format!("{e:?}"))?;
        let frames_j = j.get("frames").ok_or("no frames")?;
        let entries: Vec<(String, &Json)> = match frames_j {
            Json::Object(o) => o.iter().map(|(k, v)| (k.clone(), v)).collect(),
            Json::Array(a) => a.iter().enumerate().map(|(i, v)| (v.get("filename").and_then(Json::as_str).map_or_else(|| i.to_string(), str::to_owned), v)).collect(),
            _ => return Err("frames must be an object or array".into()),
        };
        let mut sheet = Self {
            image,
            frames: Vec::new(),
            names: BTreeMap::new(),
            clips: BTreeMap::new(),
        };
        for (name, f) in entries {
            let r = f.get("frame").ok_or("frame without rect")?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let n = |k: &str| r.get(k).and_then(Json::as_f64).map(|v| v as u32).ok_or(format!("frame rect missing {k}"));
            let rect = AtlasRect { x: n("x")?, y: n("y")?, width: n("w")?, height: n("h")? };
            let pivot = f.get("pivot").map_or((0.5, 0.5), |p| (p.get("x").and_then(Json::as_f32).unwrap_or(0.5), p.get("y").and_then(Json::as_f32).unwrap_or(0.5)));
            let duration = f.get("duration").and_then(Json::as_f32).map(|ms| ms / 1000.0);
            sheet.names.insert(name, sheet.frames.len());
            sheet.frames.push(SpriteFrame { rect, pivot, duration });
        }
        if let Some(tags) = j.get("meta").and_then(|m| m.get("frameTags")).and_then(Json::as_array) {
            for t in tags {
                let (Some(name), Some(from), Some(to)) = (t.get("name").and_then(Json::as_str), t.get("from").and_then(Json::as_usize), t.get("to").and_then(Json::as_usize)) else {
                    continue;
                };
                let mode = match t.get("direction").and_then(Json::as_str) {
                    Some("reverse") => PlayMode::Reverse,
                    Some("pingpong") => PlayMode::PingPong,
                    _ => PlayMode::Loop,
                };
                let d = sheet.frames.get(from).and_then(|f| f.duration).unwrap_or(0.1);
                sheet.clips.insert(name.to_owned(), Clip::new(from..=to, 1.0 / d).mode(mode));
            }
        }
        Ok(sheet)
    }

    /// Pack loose frames into one atlas of `width × height` (1px padding).
    /// Returns `None` if they do not fit.
    #[must_use]
    pub fn pack(frames: &[(&str, &Image)], width: u32, height: u32) -> Option<Self> {
        let req: Vec<(u32, u32)> = frames.iter().map(|(_, i)| (i.width() + 1, i.height() + 1)).collect();
        let res = AtlasPacker::new(width, height).pack(&req);
        if !res.all_fit() {
            return None;
        }
        let rects: Vec<AtlasRect> = res
            .placements
            .iter()
            .zip(frames)
            .map(|(p, (_, i))| {
                let p = p.expect("all fit");
                AtlasRect { x: p.x, y: p.y, width: i.width(), height: i.height() }
            })
            .collect();
        let placements: Vec<(&Image, AtlasRect)> = frames.iter().zip(&rects).map(|((_, i), r)| (*i, *r)).collect();
        let image = composite(width, height, &placements);
        Some(Self {
            image,
            frames: rects.iter().map(|&rect| SpriteFrame { rect, pivot: (0.5, 0.5), duration: None }).collect(),
            names: frames.iter().enumerate().map(|(i, (n, _))| ((*n).to_owned(), i)).collect(),
            clips: BTreeMap::new(),
        })
    }

    /// Add a clip.
    pub fn add_clip(&mut self, name: &str, clip: Clip) -> &mut Self {
        self.clips.insert(name.to_owned(), clip);
        self
    }

    /// A frame index by name.
    #[must_use]
    pub fn index(&self, name: &str) -> Option<usize> {
        self.names.get(name).copied()
    }

    /// Crop frame `i` to its own image.
    #[must_use]
    pub fn frame_image(&self, i: usize) -> Option<Image> {
        let f = self.frames.get(i)?;
        let AtlasRect { x, y, width, height } = f.rect;
        let (iw, src) = (self.image.width(), self.image.pixels());
        let mut out = Vec::with_capacity((width * height * 4) as usize);
        for row in y..y + height {
            let s = ((row * iw + x) * 4) as usize;
            out.extend_from_slice(src.get(s..s + (width * 4) as usize)?);
        }
        Some(Image::from_rgba8(out, width, height))
    }
}

/// Playback of one clip.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimatedSprite {
    pub clip: String,
    pub time: f32,
    pub speed: f32,
    pub playing: bool,
    last_step: Option<usize>,
}

impl AnimatedSprite {
    #[must_use]
    pub fn new(clip: &str) -> Self {
        Self {
            clip: clip.to_owned(),
            time: 0.0,
            speed: 1.0,
            playing: true,
            last_step: None,
        }
    }

    /// Switch clip (restarting unless it is already playing).
    pub fn play(&mut self, clip: &str) {
        if self.clip != clip {
            self.clip = clip.to_owned();
            self.time = 0.0;
            self.last_step = None;
        }
        self.playing = true;
    }

    /// Clip step (index into `clip.frames`) at `time`, and whether a `Once`
    /// clip has finished.
    #[must_use]
    pub fn step_at(clip: &Clip, time: f32) -> (usize, bool) {
        let n = clip.frames.len().max(1);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let raw = (time.max(0.0) * clip.fps).floor() as usize;
        match clip.mode {
            PlayMode::Loop => (raw % n, false),
            PlayMode::Reverse => (n - 1 - raw % n, false),
            PlayMode::Once => (raw.min(n - 1), raw >= n),
            PlayMode::PingPong => {
                if n == 1 {
                    return (0, false);
                }
                let period = 2 * (n - 1);
                let k = raw % period;
                (if k < n { k } else { period - k }, false)
            }
        }
    }

    /// Advance by `dt` seconds; returns the events crossed.
    pub fn advance(&mut self, sheet: &SpriteSheet, dt: f32) -> Vec<String> {
        let Some(clip) = sheet.clips.get(&self.clip) else { return Vec::new() };
        if !self.playing {
            return Vec::new();
        }
        self.time += dt * self.speed;
        let (step, done) = Self::step_at(clip, self.time);
        let mut events = Vec::new();
        if self.last_step != Some(step) {
            events.extend(clip.events.iter().filter(|(at, _)| *at == step).map(|(_, e)| e.clone()));
            self.last_step = Some(step);
        }
        if done {
            self.playing = false;
            events.push("finished".to_owned());
        }
        events
    }

    /// Current frame index in the sheet.
    #[must_use]
    pub fn frame(&self, sheet: &SpriteSheet) -> Option<usize> {
        let clip = sheet.clips.get(&self.clip)?;
        clip.frames.get(Self::step_at(clip, self.time).0).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet_image(cols: u32, rows: u32, cell: u32) -> Image {
        let (w, h) = (cols * cell, rows * cell);
        let mut px = Vec::new();
        for y in 0..h {
            for x in 0..w {
                #[allow(clippy::cast_possible_truncation)]
                let idx = ((y / cell) * cols + x / cell) as u8;
                px.extend([idx * 20, 0, 0, 255]);
            }
        }
        Image::from_rgba8(px, w, h)
    }

    #[test]
    fn grid_frames_crop_correctly() {
        let s = SpriteSheet::grid(sheet_image(4, 2, 8), 4, 2);
        assert_eq!(s.frames.len(), 8);
        assert_eq!(s.frames[5].rect, AtlasRect { x: 8, y: 8, width: 8, height: 8 });
        let f = s.frame_image(5).unwrap();
        assert_eq!((f.width(), f.pixels()[0]), (8, 100));
    }

    #[test]
    fn play_modes() {
        let c = |m| Clip::new(0..4, 10.0).mode(m);
        let steps = |m| (0..8).map(|i| AnimatedSprite::step_at(&c(m), i as f32 / 10.0 + 0.01).0).collect::<Vec<_>>();
        assert_eq!(steps(PlayMode::Loop), [0, 1, 2, 3, 0, 1, 2, 3]);
        assert_eq!(steps(PlayMode::Reverse), [3, 2, 1, 0, 3, 2, 1, 0]);
        assert_eq!(steps(PlayMode::PingPong), [0, 1, 2, 3, 2, 1, 0, 1]);
        assert_eq!(steps(PlayMode::Once), [0, 1, 2, 3, 3, 3, 3, 3]);
    }

    #[test]
    fn events_and_finish() {
        let mut s = SpriteSheet::grid(sheet_image(4, 1, 4), 4, 1);
        s.add_clip("walk", Clip::new(0..4, 10.0).event(2, "step"));
        s.add_clip("die", Clip::new([3, 2], 10.0).mode(PlayMode::Once));
        let mut a = AnimatedSprite::new("walk");
        let mut ev = Vec::new();
        for _ in 0..8 {
            ev.extend(a.advance(&s, 0.1));
        }
        assert_eq!(ev, ["step", "step"]);
        a.play("die");
        let mut ev = Vec::new();
        for _ in 0..4 {
            ev.extend(a.advance(&s, 0.1));
        }
        assert!(ev.contains(&"finished".to_owned()));
        assert!(!a.playing);
        assert_eq!(a.frame(&s), Some(2));
    }

    #[test]
    fn texture_packer_and_aseprite_json() {
        let img = sheet_image(2, 1, 16);
        let hash = r#"{"frames": {"idle_0": {"frame": {"x": 0, "y": 0, "w": 16, "h": 16}, "pivot": {"x": 0.5, "y": 1.0}},
                                  "idle_1": {"frame": {"x": 16, "y": 0, "w": 16, "h": 16}}}}"#;
        let s = SpriteSheet::from_json(img.clone(), hash).unwrap();
        assert_eq!(s.index("idle_1"), Some(1));
        assert_eq!(s.frames[0].pivot, (0.5, 1.0));
        let ase = r#"{"frames": [{"filename": "a 0", "frame": {"x":0,"y":0,"w":16,"h":16}, "duration": 100},
                                 {"filename": "a 1", "frame": {"x":16,"y":0,"w":16,"h":16}, "duration": 100}],
                      "meta": {"frameTags": [{"name": "blink", "from": 0, "to": 1, "direction": "pingpong"}]}}"#;
        let s = SpriteSheet::from_json(img, ase).unwrap();
        let clip = &s.clips["blink"];
        assert_eq!((clip.frames.clone(), clip.mode), (vec![0, 1], PlayMode::PingPong));
        assert!((clip.fps - 10.0).abs() < 1e-4);
    }

    #[test]
    fn pack_loose_frames() {
        let a = Image::from_rgba8(vec![255; 10 * 6 * 4], 10, 6);
        let b = Image::from_rgba8(vec![9; 4 * 12 * 4], 4, 12);
        let s = SpriteSheet::pack(&[("a", &a), ("b", &b)], 32, 32).unwrap();
        let same = |x: &Image, y: &Image| x.width() == y.width() && x.pixels() == y.pixels();
        assert!(same(&s.frame_image(s.index("b").unwrap()).unwrap(), &b));
        assert!(same(&s.frame_image(0).unwrap(), &a));
        assert!(SpriteSheet::pack(&[("a", &a)], 4, 4).is_none());
    }
}
