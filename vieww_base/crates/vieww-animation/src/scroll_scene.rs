//! GSAP ScrollTrigger's full model (§ Core Capability Matrix, "Scroll-Driven
//! Animation": "watches scroll events, maps scroll progress to animation
//! playhead (scrub), locks elements in viewport (pin), and fires callbacks
//! at defined positions … snapping, markers for debugging").
//!
//! A [`ScrollScene`] is a pure function of the scroll offset (and, for
//! smoothed scrub and snapping, of time):
//!
//! * **start/end** are written GSAP's way — `"top center"` means *when the
//!   trigger element's top meets the viewport's centre* — with pixel and
//!   percentage offsets (`"top 80%"`, `"bottom top+=100"`), resolved against
//!   the element's layout box and the viewport height;
//! * **scrub**: `progress` maps linearly between start and end; with
//!   `scrub(seconds)`, the playhead *follows* it through a critically
//!   damped lag, GSAP's numeric scrub;
//! * **pin**: while between start and end the element is held at its
//!   start screen position ([`ScrollScene::pin_offset`]) and, with
//!   `pin_spacing`, later content is pushed by the pinned distance;
//! * **toggle actions** for onEnter / onLeave / onEnterBack / onLeaveBack
//!   (`"play pause resume reverse"`);
//! * **snap** to progress points, a step, or labels, with a duration, after
//!   the scroll settles;
//! * **markers**: the four debug lines GSAP draws, as positions.

/// Which edge of a box (or a fraction of it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edge {
    /// 0 = top, 0.5 = centre, 1 = bottom.
    pub fraction: f32,
    /// Extra pixels (`+=` / `-=`).
    pub offset: f32,
}

fn parse_edge(tok: &str) -> Option<Edge> {
    let (base, offset) = if let Some(i) = tok.find("+=") {
        (
            &tok[..i],
            tok[i + 2..].trim_end_matches("px").parse::<f32>().ok()?,
        )
    } else if let Some(i) = tok.find("-=") {
        (
            &tok[..i],
            -tok[i + 2..].trim_end_matches("px").parse::<f32>().ok()?,
        )
    } else {
        (tok, 0.0)
    };
    let fraction = match base {
        "top" => 0.0,
        "center" => 0.5,
        "bottom" => 1.0,
        "" => 0.0,
        b if b.ends_with('%') => b.trim_end_matches('%').parse::<f32>().ok()? / 100.0,
        b if b.ends_with("px") => {
            return Some(Edge {
                fraction: 0.0,
                offset: offset + b.trim_end_matches("px").parse::<f32>().ok()?,
            })
        }
        b => {
            return Some(Edge {
                fraction: 0.0,
                offset: offset + b.parse::<f32>().ok()?,
            })
        }
    };
    Some(Edge { fraction, offset })
}

/// `"<element edge> <viewport edge>"`, GSAP's position string.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub element: Edge,
    pub viewport: Edge,
}

impl Position {
    /// Parse `"top center"`, `"top 80%"`, `"bottom top+=100"`, `"center 50%"`.
    ///
    /// # Errors
    /// A string that is not two edges.
    pub fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() != 2 {
            return Err(format!("scroll position '{s}' needs two edges"));
        }
        Ok(Self {
            element: parse_edge(parts[0]).ok_or_else(|| format!("bad edge '{}'", parts[0]))?,
            viewport: parse_edge(parts[1]).ok_or_else(|| format!("bad edge '{}'", parts[1]))?,
        })
    }

    /// The scroll offset at which this position is reached, for an element
    /// at document y `top` with `height`, in a viewport `vh` tall.
    #[must_use]
    pub fn resolve(&self, top: f32, height: f32, vh: f32) -> f32 {
        let el = top + self.element.fraction * height + self.element.offset;
        let vp = self.viewport.fraction * vh + self.viewport.offset;
        el - vp
    }
}

/// What a toggle action does to the linked animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Play,
    Pause,
    Resume,
    Reverse,
    Restart,
    Reset,
    Complete,
    None,
}

impl Action {
    fn parse(s: &str) -> Self {
        match s {
            "play" => Self::Play,
            "pause" => Self::Pause,
            "resume" => Self::Resume,
            "reverse" => Self::Reverse,
            "restart" => Self::Restart,
            "reset" => Self::Reset,
            "complete" => Self::Complete,
            _ => Self::None,
        }
    }
}

/// A crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crossing {
    Enter,
    Leave,
    EnterBack,
    LeaveBack,
}

/// Snapping rules.
#[derive(Debug, Clone, PartialEq)]
pub enum Snap {
    /// To multiples of this progress step (0.25 = quarters).
    Step(f32),
    /// To these progress values.
    Points(Vec<f32>),
}

/// The scene. See the [module docs](self).
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollScene {
    pub start: f32,
    pub end: f32,
    /// `None` = progress follows the scroll exactly; `Some(s)` = smoothed
    /// over `s` seconds.
    pub scrub: Option<f32>,
    pub pin: bool,
    pub pin_spacing: bool,
    pub toggle: [Action; 4],
    pub snap: Option<(Snap, f32)>,
    /// The element's screen y at the moment pinning starts.
    pin_screen_y: f32,
    played: f32,
    last_offset: Option<f32>,
}

impl ScrollScene {
    /// A scene for an element at document `top`/`height` in a viewport of
    /// `vh`, between the `start` and `end` positions.
    ///
    /// # Errors
    /// Unparseable positions.
    pub fn new(top: f32, height: f32, vh: f32, start: &str, end: &str) -> Result<Self, String> {
        let s = Position::parse(start)?.resolve(top, height, vh);
        let e = Position::parse(end)?.resolve(top, height, vh);
        let sp = Position::parse(start)?;
        Ok(Self {
            start: s,
            end: e.max(s + 1e-3),
            scrub: None,
            pin: false,
            pin_spacing: true,
            toggle: [Action::Play, Action::None, Action::None, Action::None],
            snap: None,
            pin_screen_y: sp.viewport.fraction * vh + sp.viewport.offset
                - sp.element.fraction * height
                - sp.element.offset,
            played: 0.0,
            last_offset: None,
        })
    }

    #[must_use]
    pub fn scrub(mut self, seconds: Option<f32>) -> Self {
        self.scrub = seconds;
        self
    }

    #[must_use]
    pub fn pinned(mut self, spacing: bool) -> Self {
        self.pin = true;
        self.pin_spacing = spacing;
        self
    }

    /// GSAP's `toggleActions: "play pause resume reverse"`.
    #[must_use]
    pub fn toggle_actions(mut self, s: &str) -> Self {
        let v: Vec<Action> = s.split_whitespace().map(Action::parse).collect();
        for (i, a) in v.into_iter().take(4).enumerate() {
            self.toggle[i] = a;
        }
        self
    }

    #[must_use]
    pub fn snapping(mut self, snap: Snap, duration: f32) -> Self {
        self.snap = Some((snap, duration));
        self
    }

    /// Exact progress (0..1) at `offset`.
    #[must_use]
    pub fn progress(&self, offset: f32) -> f32 {
        ((offset - self.start) / (self.end - self.start)).clamp(0.0, 1.0)
    }

    /// Is the scene active (between start and end)?
    #[must_use]
    pub fn is_active(&self, offset: f32) -> bool {
        offset >= self.start && offset <= self.end
    }

    /// Feed a new offset; returns the crossings and their actions.
    pub fn update(&mut self, offset: f32) -> Vec<(Crossing, Action)> {
        let mut out = Vec::new();
        if let Some(prev) = self.last_offset {
            if prev < self.start && offset >= self.start {
                out.push((Crossing::Enter, self.toggle[0]));
            }
            if prev <= self.end && offset > self.end {
                out.push((Crossing::Leave, self.toggle[1]));
            }
            if prev > self.end && offset <= self.end {
                out.push((Crossing::EnterBack, self.toggle[2]));
            }
            if prev >= self.start && offset < self.start {
                out.push((Crossing::LeaveBack, self.toggle[3]));
            }
        }
        self.last_offset = Some(offset);
        out
    }

    /// Advance the scrubbed playhead toward the scroll's progress over
    /// `dt` seconds; returns the playhead.
    pub fn tick(&mut self, offset: f32, dt: f32) -> f32 {
        let target = self.progress(offset);
        self.played = match self.scrub {
            None => target,
            Some(lag) if lag <= 0.0 => target,
            Some(lag) => {
                // Exponential approach reaching ~98 % in `lag` seconds.
                let k = 1.0 - (-4.0 * dt / lag).exp();
                self.played + (target - self.played) * k
            }
        };
        self.played
    }

    /// Where an element should be drawn *relative to its normal scroll
    /// position*: while pinned, it moves down as the page moves up.
    #[must_use]
    pub fn pin_offset(&self, offset: f32) -> f32 {
        if !self.pin {
            return 0.0;
        }
        (offset.clamp(self.start, self.end) - self.start).max(0.0)
    }

    /// Extra space inserted after the pinned element (pin spacing).
    #[must_use]
    pub fn spacing(&self) -> f32 {
        if self.pin && self.pin_spacing {
            self.end - self.start
        } else {
            0.0
        }
    }

    /// The element's screen y while pinned.
    #[must_use]
    pub fn pinned_screen_y(&self) -> f32 {
        self.pin_screen_y
    }

    /// Where a settled scroll should glide to, and over how long:
    /// `(target offset, duration)`, or `None` without snapping.
    #[must_use]
    pub fn snap_target(&self, offset: f32) -> Option<(f32, f32)> {
        let (snap, duration) = self.snap.as_ref()?;
        if !self.is_active(offset) {
            return None;
        }
        let p = self.progress(offset);
        let q = match snap {
            Snap::Step(s) if *s > 0.0 => (p / s).round() * s,
            Snap::Step(_) => p,
            Snap::Points(pts) => *pts
                .iter()
                .min_by(|a, b| (**a - p).abs().total_cmp(&(**b - p).abs()))?,
        };
        Some((
            self.start + q.clamp(0.0, 1.0) * (self.end - self.start),
            *duration,
        ))
    }

    /// GSAP's debug markers: (scroller-start, scroller-end, start, end) as
    /// screen y's at `offset`.
    #[must_use]
    pub fn markers(&self, offset: f32, start_vp: f32, end_vp: f32) -> [f32; 4] {
        [
            start_vp,
            end_vp,
            self.start - offset + start_vp,
            self.end - offset + end_vp,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_resolve_like_gsap() {
        // Element at y=1000, 400 tall, viewport 800.
        let top_center = Position::parse("top center")
            .expect("p")
            .resolve(1000.0, 400.0, 800.0);
        assert!((top_center - 600.0).abs() < 1e-4);
        let bottom_top = Position::parse("bottom top+=100")
            .expect("p")
            .resolve(1000.0, 400.0, 800.0);
        assert!((bottom_top - 1300.0).abs() < 1e-4);
        let pct = Position::parse("top 80%")
            .expect("p")
            .resolve(1000.0, 400.0, 800.0);
        assert!((pct - 360.0).abs() < 1e-4);
        assert!(Position::parse("top").is_err());
    }

    #[test]
    fn progress_toggle_actions_fire_on_crossings() {
        let mut s = ScrollScene::new(1000.0, 400.0, 800.0, "top center", "bottom center")
            .expect("scene")
            .toggle_actions("play pause resume reverse");
        assert!(s.update(0.0).is_empty());
        assert_eq!(s.update(700.0), vec![(Crossing::Enter, Action::Play)]);
        assert!((s.progress(800.0) - 0.5).abs() < 1e-4);
        assert_eq!(s.update(1200.0), vec![(Crossing::Leave, Action::Pause)]);
        assert_eq!(s.update(900.0), vec![(Crossing::EnterBack, Action::Resume)]);
        assert_eq!(
            s.update(100.0),
            vec![(Crossing::LeaveBack, Action::Reverse)]
        );
    }

    #[test]
    fn smoothed_scrub_lags_then_catches_up() {
        let mut s = ScrollScene::new(0.0, 100.0, 100.0, "top top", "bottom top")
            .expect("s")
            .scrub(Some(0.5));
        let first = s.tick(100.0, 1.0 / 60.0);
        assert!(first > 0.0 && first < 0.2, "{first}");
        let mut p = first;
        for _ in 0..60 {
            p = s.tick(100.0, 1.0 / 60.0);
        }
        assert!(p > 0.99);
        let mut exact = s.clone().scrub(None);
        assert!((exact.tick(50.0, 0.016) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn pinning_holds_the_element_and_spaces_the_page() {
        let s = ScrollScene::new(500.0, 200.0, 800.0, "top top", "+=600 top")
            .expect("s")
            .pinned(true);
        assert!((s.start - 500.0).abs() < 1e-4);
        // While active the element moves down exactly as the page scrolls.
        assert!((s.pin_offset(700.0) - 200.0).abs() < 1e-4);
        assert!((s.pin_offset(2000.0) - (s.end - s.start)).abs() < 1e-4);
        assert!(s.pin_offset(0.0).abs() < 1e-6);
        assert!((s.spacing() - (s.end - s.start)).abs() < 1e-4);
        assert!(s.pinned_screen_y().abs() < 1e-4);
    }

    #[test]
    fn snapping_glides_to_the_nearest_point() {
        let s = ScrollScene::new(0.0, 1000.0, 1000.0, "top top", "bottom top")
            .expect("s")
            .snapping(Snap::Step(0.25), 0.3);
        let (t, d) = s.snap_target(370.0).expect("snap");
        assert!((t - 250.0).abs() < 1e-3 && (d - 0.3).abs() < 1e-6);
        let p = s.clone().snapping(Snap::Points(vec![0.0, 0.9]), 0.2);
        assert!((p.snap_target(700.0).expect("snap").0 - 900.0).abs() < 1e-3);
        let m = s.markers(100.0, 0.0, 0.0);
        assert!((m[2] + 100.0).abs() < 1e-4);
    }
}
