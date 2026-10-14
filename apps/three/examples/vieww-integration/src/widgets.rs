//! The viewer's widgets: screen, media surface, and controls.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use three_format::LazyCapture;
use three_vieww::RenderedFrame;

use vieww::foundation::{BoxFit, Image as ImageData, Offset, ScrollEvent, Shadow};
use vieww::prelude::*;
use vieww::{DragDetails, LongPressDetails, ScaleDetails};

use crate::state::PlaybackState;

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
        f.debug_struct("StateHandle(<PlaybackState>)").finish()
    }
}

/// The root screen: title, the depth-video surface, and the playback
/// controls.
///
/// The durable playback state lives on this widget's element; both the media
/// view and the controls are rebuilt from a render of it on every frame
/// that changes something. That is the Vieww shape for shared view state:
/// one element owns it, and the children are pure descriptions of it.
pub struct ViewerScreen {
    /// The capture being viewed, shared with the state.
    pub capture: Rc<LazyCapture>,
}

impl fmt::Debug for ViewerScreen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Not the capture's own Debug — its bytes would drown the dump. The
        // title and frame count are what a dump needs.
        f.debug_struct("ViewerScreen")
            .field("title", &self.capture.meta().title)
            .field("frames", &self.capture.meta().frame_count)
            .finish()
    }
}

#[widget]
impl ViewerScreen {
    fn create_state(&self) -> Option<Box<dyn ElementState>> {
        Some(Box::new(PlaybackState::new(Rc::clone(&self.capture))))
    }

    fn build(&self, ctx: &BuildContext) -> impl Into<WidgetNode> {
        let theme = ThemeData::of(ctx);
        let (rendered, meta, controls) = ctx
            .state::<PlaybackState, (RenderedFrame, three_format::CaptureMeta, ControlsData)>(
                |state| {
                    (
                        state.rendered().clone(),
                        state.view().meta().clone(),
                        ControlsData::from_state(state),
                    )
                },
            )
            .unwrap_or_else(|| {
                (
                    RenderedFrame::blank(16, 12),
                    self.capture.meta().clone(),
                    ControlsData::unmounted(),
                )
            });
        let handle = ctx.state_handle().map(StateHandle::new);

        Container::new()
            .color(theme.colors.surface)
            .padding(EdgeInsets::all(16.0))
            .child(
                Flex::column()
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .spacing(12.0)
                    .push(viewer_header(&theme, &meta))
                    .push(Flexible::expanded(1).child(ThreeMediaView {
                        rendered,
                        frame_count: meta.frame_count,
                        handle: handle.clone(),
                    }))
                    .push(ControlsBar { controls, handle }),
            )
    }
}

/// The title row: capture name and a one-line description.
fn viewer_header(theme: &ThemeData, meta: &three_format::CaptureMeta) -> WidgetNode {
    let meta_line = format!(
        "{} frames · {} · {} s · depth: {}",
        meta.frame_count,
        meta.source,
        meta.duration_ns as f32 / 1_000_000_000.0,
        if meta.source.is_measured() {
            "measured"
        } else if meta.source == three_core::DepthSourceKind::None {
            "none"
        } else {
            "generated"
        },
    );

    Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .spacing(2.0)
        .push(
            Text::new(meta.title.clone())
                .style(theme.text.title)
                .max_lines(1),
        )
        .push(Text::new(meta_line).style(theme.text.label).max_lines(1))
        .into()
}

/// The gesture-driven depth-video surface.
///
/// The pixels come from the state — the warp ran on the element's tick, on
/// the frame clock, not during build — and this widget is only their
/// description: an [`Image`](vieww::Image) fitted to the chamber, inside
/// the gesture recognizer that drives the camera.
///
/// The gestures, and what each one means:
///
/// * **Pan** — orbit the virtual camera around the subject; the depth
///   makes this a real re-photograph, with near geometry sweeping past far.
/// * **Pinch** — dolly in and out *in depth*: closer is genuinely closer,
///   the foreground overtaking the background as it would if you walked.
/// * **Hold** — the recenter spring, returning the view to the one angle
///   the footage was actually filmed from.
#[derive(Debug)]
pub struct ThreeMediaView {
    pub rendered: RenderedFrame,
    pub frame_count: usize,
    handle: Option<StateHandle>,
}

#[widget]
impl ThreeMediaView {
    fn build(&self, ctx: &BuildContext) -> impl Into<WidgetNode> {
        let theme = ThemeData::of(ctx);
        let _ = theme;

        // The pixels as the framework's image type: shared, not copied —
        // the render already lives on the element, the widget description
        // only borrows it for this build.
        let pixels = ImageData::from_shared_rgba8(
            self.rendered.rgba.clone(),
            self.rendered.width,
            self.rendered.height,
        );
        let media: WidgetNode = vieww::prelude::Image::new(pixels)
            .fit(BoxFit::Contain)
            .label(format!(
                "Depth video, {} frames. Pan to orbit, pinch to move in depth, hold to recenter.",
                self.frame_count
            ))
            .into();

        let drag = self.handle.clone();
        let scroll = self.handle.clone();
        let pinch = self.handle.clone();
        let pinch_end = self.handle.clone();
        let hold = self.handle.clone();

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
            // Hold: press without moving, and the view comes home.
            .on_long_press(move |_: LongPressDetails| {
                if let Some(handle) = &hold {
                    handle.write(|state| state.hold());
                }
            })
            .child(
                // The chamber, lifted: a shadow-casting frame *outside* the
                // clip (a shadow drawn inside its own `Clip` is clipped to
                // the very corner it is meant to soften). The capture is
                // this screen's one subject, and on a flat surface colour the
                // difference between "a viewport into a moment" and "a
                // rectangle of dark pixels" is exactly this lift.
                Container::new()
                    .color(vieww::foundation::Color::rgb(12, 14, 20))
                    .radius(MEDIA_CORNER)
                    .shadow(Shadow::new(
                        vieww::foundation::Color::rgba(0, 0, 0, 96),
                        Offset::new(0.0, 14.0),
                        36.0,
                    ))
                    .child(Clip::rounded(MEDIA_CORNER).child(media)),
            )
    }
}

/// What the controls bar needs to draw itself: an immutable read of the
/// player, taken during a build.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ControlsData {
    playing: bool,
    looping: bool,
    progress: f32,
    time_seconds: f32,
    duration_seconds: f32,
}

impl ControlsData {
    fn from_state(state: &PlaybackState) -> Self {
        let player = state.view().player();
        Self {
            playing: state.playing(),
            looping: player.is_looping(),
            progress: player.progress(),
            time_seconds: player.time_ns() as f32 / 1_000_000_000.0,
            duration_seconds: state.view().meta().duration_ns as f32 / 1_000_000_000.0,
        }
    }

    /// The controls an unmounted tree draws — the fallback of a
    /// `debug_tree` dump: honest zeros, no media.
    fn unmounted() -> Self {
        Self {
            playing: false,
            looping: true,
            progress: 0.0,
            time_seconds: 0.0,
            duration_seconds: 0.0,
        }
    }
}

/// The playback controls: transport row, scrub slider, and the status line.
#[derive(Debug)]
pub struct ControlsBar {
    controls: ControlsData,
    handle: Option<StateHandle>,
}

#[widget]
impl ControlsBar {
    fn build(&self, ctx: &BuildContext) -> impl Into<WidgetNode> {
        let theme = ThemeData::of(ctx);

        let play_pause = self.handle.clone();
        let looping = self.handle.clone();
        let recenter = self.handle.clone();
        let scrub = self.handle.clone();

        // Play/Pause is the one action the thumb goes to every time, so it
        // alone is `Filled`; Recenter is the button form of the hold
        // gesture; Loop is a setting. Equal-width transport slots keep the
        // row inside a narrow window instead of overflowing it.
        let transport = Flex::row()
            .spacing(8.0)
            .push(
                Flexible::expanded(1).child(
                    Button::new(if self.controls.playing {
                        "Pause"
                    } else {
                        "Play"
                    })
                    .style(ButtonStyle::Filled)
                    .on_pressed(move || {
                        if let Some(handle) = &play_pause {
                            handle.write(|state| state.toggle_play());
                        }
                    }),
                ),
            )
            .push(
                Flexible::expanded(1).child(
                    Button::new(if self.controls.looping {
                        "Loop on"
                    } else {
                        "Loop off"
                    })
                    .style(ButtonStyle::Text)
                    .on_pressed(move || {
                        if let Some(handle) = &looping {
                            handle.write(|state| {
                                state.set_looping(!state.view().player().is_looping())
                            });
                        }
                    }),
                ),
            )
            .push(
                Flexible::expanded(1).child(
                    Button::new("Hold recentres")
                        .style(ButtonStyle::Text)
                        .on_pressed(move || {
                            if let Some(handle) = &recenter {
                                handle.write(|state| state.hold());
                            }
                        }),
                ),
            );

        // The timeline: a fixed clock readout and a flexible scrub track.
        let timeline = Flex::row()
            .spacing(10.0)
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .push(
                Text::new(format!(
                    "{:.1} / {:.1} s",
                    self.controls.time_seconds, self.controls.duration_seconds
                ))
                .style(theme.text.label),
            )
            .push(Flexible::expanded(1).child(
                Slider::new(self.controls.progress.clamp(0.0, 1.0)).on_changed(Rc::new(
                    move |progress: f32| {
                        if let Some(handle) = &scrub {
                            handle.write(|state| state.scrub(progress));
                        }
                    },
                )),
            ));

        // The caption: what the gestures do, because no gesture explains
        // itself on first use.
        let caption = Text::new("Pan orbits · Pinch moves in depth · Hold recentres")
            .style(theme.text.label)
            .max_lines(1);

        Flex::column()
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .spacing(10.0)
            .push(transport)
            .push(timeline)
            .push(caption)
    }
}
