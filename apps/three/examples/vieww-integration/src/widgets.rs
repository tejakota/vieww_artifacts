//! The viewer's widgets: screen, media surface, and controls.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use three_core::Capture3D;
use three_vieww::OrbitCamera;

use vieww::prelude::*;
use vieww::{DragDetails, ScaleDetails};
use vieww::foundation::{Offset, ScrollEvent, Shadow};

use crate::painter::{MeshPainter, ViewPalette};
use crate::state::{PlaybackState, Snapshot};

/// The corner the media chamber and its frame share.
const MEDIA_CORNER: f32 = 14.0;

/// A shareable write path into the mounted [`PlaybackState`].
///
/// Widget descriptions are immutable, and a build must not write state — so
/// this handle is what a gesture handler or a button callback holds instead.
/// It is the mechanism `BuildContext::state_handle` exists for: the closure
/// runs during input dispatch, long after the build has returned, and writes
/// through the element's own state cell.
///
/// Writes go through [`write`](Self::write) rather than exposing the
/// `RefCell`: every write lands inside one borrow, so a handler can never
/// hand out a `RefMut` that outlives the next tick.
#[derive(Clone)]
pub struct StateHandle(Rc<RefCell<dyn ElementState>>);

impl StateHandle {
    /// Wrap the handle a build context handed out.
    pub fn new(state: Rc<RefCell<dyn ElementState>>) -> Self {
        Self(state)
    }

    /// Apply one edit to the playback state, if this handle still points at
    /// one. Handles do not keep the element alive — a handler whose element
    /// went away writes to nothing, which is the intended silence.
    pub fn write(&self, edit: impl FnOnce(&mut PlaybackState)) {
        let mut state = self.0.borrow_mut();
        if let Some(playback) = state.as_any_mut().downcast_mut::<PlaybackState>() {
            edit(playback);
        }
    }
}

impl fmt::Debug for StateHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Not the state's Debug: printing a capture's geometry into a tree
        // dump would drown it. The type is what a dump needs to say.
        f.write_str("StateHandle(<PlaybackState>)")
    }
}

/// The root screen: title, the 3D media surface, and the playback controls.
///
/// The durable playback state lives on this widget's element; both the media
/// view and the controls are rebuilt from a [`Snapshot`] of it on every frame
/// that changes something. That is the Vieww shape for shared view state: one
/// element owns it, and the children are pure descriptions of it.
#[derive(Debug)]
pub struct ViewerScreen {
    /// The capture being viewed, shared with the state and the painter.
    pub capture: Rc<Capture3D>,
}

#[widget]
impl ViewerScreen {
    fn create_state(&self) -> Option<Box<dyn ElementState>> {
        Some(Box::new(PlaybackState::new(Rc::clone(&self.capture))))
    }

    fn build(&self, ctx: &BuildContext) -> impl Into<WidgetNode> {
        let theme = ThemeData::of(ctx);
        let snapshot = ctx
            .state::<PlaybackState, Snapshot>(Snapshot::from_state)
            .unwrap_or_else(|| Snapshot::initial(&self.capture));
        let handle = ctx.state_handle().map(StateHandle::new);

        Container::new()
            .color(theme.colors.surface)
            .padding(EdgeInsets::all(16.0))
            .child(
                Flex::column()
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .spacing(12.0)
                    .push(viewer_header(&theme, &snapshot))
                    .push(Flexible::expanded(1).child(ThreeMediaView {
                        mesh_index: snapshot.mesh_index,
                        points_index: snapshot.points_index,
                        camera: snapshot.camera,
                        wireframe: snapshot.wireframe,
                        show_points: snapshot.show_points,
                        capture: Rc::clone(&snapshot.capture),
                        frame_count: snapshot.frame_count,
                        handle: handle.clone(),
                    }))
                    .push(ControlsBar {
                        snapshot: snapshot.clone(),
                        handle,
                    }),
            )
    }
}

/// The title row: capture name and a one-line description.
fn viewer_header(theme: &ThemeData, snapshot: &Snapshot) -> WidgetNode {
    let capture = &snapshot.capture;
    let meta = if snapshot.frame_count > 0 {
        format!(
            "{} frames · {} · {} s",
            snapshot.frame_count,
            capture.source,
            format_seconds(snapshot.duration_seconds)
        )
    } else {
        "no spatial data".to_string()
    };

    Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .spacing(2.0)
        .push(
            Text::new(capture.title.clone())
                .style(theme.text.title)
                .max_lines(1),
        )
        .push(Text::new(meta).style(theme.text.label).max_lines(1))
        .into()
}

/// The gesture-driven 3D surface.
///
/// Everything drawn is decided by the fields — the widget itself holds no
/// state that survives a rebuild. Drag orbits, scroll and pinch zoom, and the
/// [`MeshPainter`] records the frame under the current camera when the
/// `Painting` render leaf lays out.
#[derive(Debug)]
pub struct ThreeMediaView {
    pub capture: Rc<Capture3D>,
    pub mesh_index: Option<usize>,
    pub points_index: Option<usize>,
    pub camera: OrbitCamera,
    pub wireframe: bool,
    pub show_points: bool,
    pub frame_count: usize,
    handle: Option<StateHandle>,
}

#[widget]
impl ThreeMediaView {
    fn build(&self, ctx: &BuildContext) -> impl Into<WidgetNode> {
        let theme = ThemeData::of(ctx);
        let palette = ViewPalette::from_theme(&theme);

        let painter = MeshPainter {
            capture: Rc::clone(&self.capture),
            mesh_index: self.mesh_index,
            points_index: self.points_index,
            camera: self.camera,
            wireframe: self.wireframe,
            show_points: self.show_points,
            palette,
            shading: three_vieww::MeshShading::default(),
            lens: three_vieww::Perspective::default(),
        };

        let drag = self.handle.clone();
        let scroll = self.handle.clone();
        let pinch = self.handle.clone();
        let pinch_end = self.handle.clone();

        // What a screen reader is told: the media itself, not the gestures.
        // An ink rectangle with no text announces as nothing, so this label
        // is the only way a reader knows a capture is even here.
        let label = format!(
            "Temporal 3D capture, {} frames, {:.1} seconds",
            self.frame_count,
            self.capture.duration_ns as f32 / 1_000_000_000.0
        );

        GestureDetector::new()
            .on_drag_update(move |details: DragDetails| {
                if let Some(handle) = &drag {
                    handle.write(|state| state.orbit(details.delta.dx, details.delta.dy));
                }
            })
            .on_scroll(move |event: ScrollEvent| {
                if let Some(handle) = &scroll {
                    handle.write(|state| state.zoom(event.delta.dy));
                }
            })
            .on_scale_update(move |details: ScaleDetails| {
                if let Some(handle) = &pinch {
                    handle.write(|state| state.pinch_step(details.scale));
                }
            })
            .on_scale_end(move |_: ScaleDetails| {
                if let Some(handle) = &pinch_end {
                    handle.write(|state| state.pinch_end());
                }
            })
            .child(
                // The chamber, lifted: a shadow-casting frame *outside* the
                // clip (a shadow drawn inside its own `Clip` is clipped to
                // the very corner it is meant to soften). The capture is
                // this screen's one subject, and on a flat surface colour the
                // difference between "a viewport into a 3D scene" and "a
                // rectangle of dark pixels" is exactly this lift.
                Container::new()
                    .radius(MEDIA_CORNER)
                    .shadow(Shadow::new(
                        vieww::foundation::Color::rgba(0, 0, 0, 96),
                        Offset::new(0.0, 14.0),
                        36.0,
                    ))
                    .child(
                        Clip::rounded(MEDIA_CORNER)
                            .child(Semantics::container(label).child(Painting::new(painter))),
                    ),
            )
    }
}

/// The playback controls: transport row, scrub slider, and the status line.
///
/// The status line doubles as the viewer's instrumentation in tests — the
/// frame counter and camera readout are asserted on through the element
/// tree's debug dump, which is cheaper and more honest than a screenshot.
#[derive(Debug)]
pub struct ControlsBar {
    pub snapshot: Snapshot,
    pub handle: Option<StateHandle>,
}

#[widget]
impl ControlsBar {
    fn build(&self, ctx: &BuildContext) -> impl Into<WidgetNode> {
        let theme = ThemeData::of(ctx);
        let snapshot = &self.snapshot;

        let play_pause = self.handle.clone();
        let wireframe = self.handle.clone();
        let motes = self.handle.clone();
        let looping = self.handle.clone();
        let reset = self.handle.clone();
        let scrub = self.handle.clone();

        // Four toggles, each filling an equal slot of the transport row.
        // Tight `Flexible::expanded` rather than natural-size buttons: a
        // natural row of five buttons was 73 px too wide for a 480 px window
        // (the harness tests caught it as a `RenderRow` overflow before any
        // human saw it), and equal-width transport buttons are what a media
        // player's row looks like anyway. Labels stay short enough that the
        // squeeze on a narrow window never has to wrap them.
        //
        // Hierarchy, not just equal slots: Play/Pause is the one action the
        // thumb goes to every time, so it alone is `Filled` — the view-mode
        // toggles beside it are `Text`, which reads as the secondary settings
        // they are. Four identical filled buttons said "pick at random".
        let transport = Flex::row()
            .spacing(8.0)
            .push(Flexible::expanded(1).child(
                Button::new(if snapshot.playing { "Pause" } else { "Play" })
                    .style(ButtonStyle::Filled)
                    .on_pressed(move || {
                        if let Some(handle) = &play_pause {
                            handle.write(|state| state.toggle_play());
                        }
                    }),
            ))
            .push(Flexible::expanded(1).child(
                Button::new(if snapshot.wireframe { "Shaded" } else { "Wire" })
                    .style(ButtonStyle::Text)
                    .on_pressed(
                        move || {
                            if let Some(handle) = &wireframe {
                                handle.write(|state| state.toggle_wireframe());
                            }
                        },
                    ),
            ))
            .push(Flexible::expanded(1).child(
                Button::new(if snapshot.show_points { "Motes on" } else { "Motes off" })
                    .style(ButtonStyle::Text)
                    .on_pressed(
                        move || {
                            if let Some(handle) = &motes {
                                handle.write(|state| state.toggle_points());
                            }
                        },
                    ),
            ))
            .push(Flexible::expanded(1).child(
                Button::new(if snapshot.looping { "Loop on" } else { "Loop off" })
                    .style(ButtonStyle::Text)
                    .on_pressed(
                        move || {
                            if let Some(handle) = &looping {
                                handle.write(|state| state.set_looping(!state.view().is_looping()));
                            }
                        },
                    ),
            ));

        // The timeline: a fixed clock readout, a flexible scrub track, and
        // the camera reset tucked at the end where a thumb rests.
        let timeline = Flex::row()
            .spacing(10.0)
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .push(
                Text::new(format!(
                    "{} / {}",
                    format_seconds(snapshot.time_seconds),
                    format_seconds(snapshot.duration_seconds)
                ))
                .style(theme.text.label),
            )
            .push(Flexible::expanded(1).child(
                Slider::new(snapshot.progress.clamp(0.0, 1.0))
                    .on_changed(Rc::new(move |progress: f32| {
                        if let Some(handle) = &scrub {
                            handle.write(|state| state.scrub(progress));
                        }
                    })),
            ))
            .push(Button::new("Reset view").on_pressed(move || {
                if let Some(handle) = &reset {
                    handle.write(|state| state.reset_camera());
                }
            }));

        let shown_frame = snapshot
            .mesh_index
            .map(|index| (index + 1).to_string())
            .unwrap_or_else(|| "-".into());
        let yaw_degrees = snapshot.camera.yaw * 180.0 / std::f32::consts::PI;
        let pitch_degrees = snapshot.camera.pitch * 180.0 / std::f32::consts::PI;
        let status = format!(
            "frame {} / {} · cam {:.0}° / {:.0}° · {:.2} m",
            shown_frame,
            snapshot.frame_count,
            yaw_degrees,
            pitch_degrees,
            snapshot.camera.distance
        );

        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(10.0)
            .push(transport)
            .push(timeline)
            .push(Text::new(status).style(theme.text.label).max_lines(1))
    }
}

/// Seconds as a media clock prints them: `1.4 s`, never `1.400000000 s`.
fn format_seconds(seconds: f32) -> String {
    format!("{seconds:.1} s")
}
