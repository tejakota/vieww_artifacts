//! The shell: chrome, the depth stack, and the tabs.
//!
//! Depth-based navigation, exactly as the Slint build had it. The flow is
//! linear, so the screen you are on *is* how far you have got. That makes the
//! transition derivable — deeper waits to the right, shallower to the left —
//! and makes it structurally impossible to reach the run without passing the
//! plan.
//!
//! # Why this is a `Stack` and not a `Navigator`
//!
//! vieww has a perfectly good `Navigator` with route transitions, and it is the
//! wrong shape for this app. A navigator owns a *stack of routes* and the app
//! owns a *depth*, which is a different thing: `Screen::Photo`'s back edge goes
//! to `Outcome` rather than to whatever was pushed before it, and the tab row
//! belongs to depth 0 alone. Reconciling those would mean keeping the
//! navigator's stack in step with a signal that is already the truth — two
//! sources for one fact, which is the bug the depth model exists to prevent.
//!
//! So: five stages side by side, offset by `(depth - current) * width`, with
//! one `Animated` driving the whole slide. An off-stage screen is not hidden,
//! it is elsewhere.

use std::cell::Cell;
use std::rc::Rc;

use vieww::element::ScrollController;
use vieww::foundation::Axis;
use vieww::gestures::ScrollPhysics;
use vieww::prelude::*;
use vieww::widget::widget_node_from;

use crate::model::{Screen, Tab};
use crate::screens;
use crate::state::VavltState;
use crate::theme::{motion, FormFactor, VavltTheme};
use crate::ui;

/// Adwaita's deep out-ease. Fast departure, long settle: a change of place has
/// mass.
const SETTLE: Curve = Curve::cubic(0.16, 1.0, 0.3, 1.0);

/// How far the arriving screen travels, in logical pixels.
///
/// Small on purpose. A screen that slides its whole width reads as a page turn
/// and takes as long as one; twenty points reads as the thing settling into
/// place, and finishes before it can get in the way of a second tap.
const ARRIVAL_TRAVEL: f32 = 20.0;

/// How visible the arriving screen is on its very first frame.
///
/// **Not zero, and this is the whole reason the constant exists.** An
/// `AnimationController` records its start on its first tick, so the frame the
/// navigation happens on is drawn at exactly the animation's beginning — and a
/// beginning of zero means one frame with the chrome up and nothing under it.
/// Sixteen milliseconds of empty screen is not much, and it is the one thing a
/// transition must never do: the report that started all of this was "the text
/// overlaid for a fraction of a second", and answering it with "the screen was
/// blank for a fraction of a second" is not an answer.
///
/// A third is enough to read the shape of the page through. Caught by
/// `tests/transition.rs`, which photographs the frame at 0ms.
const ARRIVAL_FLOOR: f32 = 0.34;

/// Every screen's scroll position, kept across a navigation.
///
/// One controller per screen rather than one shared: coming back to the plan
/// from the run should find it where you left it, and a shared offset would
/// scroll the outcome to wherever the plan happened to be.
#[derive(Debug)]
pub struct Scrolls {
    pub vault: ScrollController,
    pub activity: ScrollController,
    pub settings: ScrollController,
    pub plan: ScrollController,
    pub working: ScrollController,
    pub outcome: ScrollController,
    pub photo: ScrollController,
}

impl Scrolls {
    #[must_use]
    pub fn new(runtime: &Runtime) -> Self {
        let make = || ScrollController::new(runtime, ScrollPhysics::default());
        Self {
            vault: make(),
            activity: make(),
            settings: make(),
            plan: make(),
            working: make(),
            outcome: make(),
            photo: make(),
        }
    }

    /// Let every controller run its fling and its overscroll spring.
    ///
    /// Without this a released drag stops dead: the physics need a ticker, and
    /// a controller nobody attached simply never gets one.
    pub fn attach(&self, tickers: &mut vieww::animation::Tickers) {
        for controller in [
            &self.vault,
            &self.activity,
            &self.settings,
            &self.plan,
            &self.working,
            &self.outcome,
            &self.photo,
        ] {
            controller.attach(tickers);
        }
    }
}

/// The whole application.
#[derive(Debug)]
pub struct VavltApp {
    state: Rc<VavltState>,
    scrolls: Rc<Scrolls>,
    splash: Option<vieww::element::Animation<f32>>,
}

impl VavltApp {
    #[must_use]
    pub fn new(state: Rc<VavltState>, scrolls: Rc<Scrolls>) -> Self {
        Self {
            state,
            scrolls,
            splash: None,
        }
    }

    /// Mount the app with the branded vavlt startup animation.
    ///
    /// The animation is owned by the root so it remains alive for exactly the
    /// lifetime of the splash. The host must attach it to its frame tickers.
    #[must_use]
    pub fn new_with_splash(
        state: Rc<VavltState>,
        scrolls: Rc<Scrolls>,
        splash: vieww::element::Animation<f32>,
    ) -> Self {
        Self {
            state,
            scrolls,
            splash: Some(splash),
        }
    }
}

impl Widget for VavltApp {
    fn debug_name(&self) -> &'static str {
        "VavltApp"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let state = self.state.clone();
        let scrolls = self.scrolls.clone();
        let dark = state.dark.get();

        // The form factor is read from the surface, once, above everything —
        // so a phone-sized window on a laptop is genuinely the phone layout
        // rather than a desktop one squeezed.
        let splash = self.splash.clone();

        LayoutBuilder::new(move |constraints| {
            let form = FormFactor::for_width(constraints.max_width);
            let theme = VavltTheme::new(dark, form);

            let content = Container::new()
                .color(theme.colors.window)
                .child(shell(&theme, &state, &scrolls));

            let root: WidgetNode = if let Some(splash) = splash.clone() {
                let progress = splash.value();
                if progress < 0.999 {
                    Stack::new()
                        .fit(StackFit::Expand)
                        .children(children![
                            content,
                            vavlt_splash(progress, constraints.max_width, constraints.max_height),
                        ])
                        .into()
                } else {
                    content.into()
                }
            } else {
                content.into()
            };

            Theme::new(theme.framework())
                .child(Inherited::new(theme, root))
                .into()
        })
        // No breakpoints: this builder picks a form factor from a threshold —
        // which quantising would suit — but it also hands the exact width to
        // the stage stack, which offsets five screens by multiples of it. A
        // stale width there is a transition that lands the next screen short of
        // the edge. See `LayoutBuilder::breakpoints`.
        .into()
    }
}

widget_node_from!(VavltApp);

/// Branded startup: a checkerboard of random letters collapses to the five
/// letters in `vavlt`, then those letters align horizontally and the whole
/// layer fades away to reveal the already-mounted home screen.
fn vavlt_splash(progress: f32, width: f32, height: f32) -> WidgetNode {
    const GRID: [[char; 5]; 5] = [
        ['v', 'q', 'm', 'x', 'k'],
        ['z', 'a', 'c', 'p', 'b'],
        ['y', 'g', 'v', 'n', 'd'],
        ['r', 'j', 'q', 'm', 'l'],
        ['f', 'h', 's', 'w', 't'],
    ];
    const TARGETS: [(usize, usize, char, f32); 5] = [
        (0, 0, 'v', 1.16),
        (1, 1, 'a', 0.86),
        (2, 2, 'v', 1.28),
        (3, 4, 'l', 0.94),
        (4, 4, 't', 1.08),
    ];

    let blackout = smoothstep((progress - 0.10) / 0.42);
    let align = smoothstep((progress - 0.52) / 0.30);
    let fade = 1.0 - smoothstep((progress - 0.84) / 0.16);

    let grid_size = width.min(height) * 0.74;
    let cell = grid_size / 5.0;
    let left = (width - grid_size) * 0.5;
    let top = (height - grid_size) * 0.43;
    let final_gap = cell * 0.88;
    let final_width = final_gap * 4.0;
    let final_left = (width - final_width) * 0.5;
    let final_y = top + grid_size * 0.50;

    let target_set = |r: usize, c: usize| -> Option<(usize, usize, char, f32)> {
        TARGETS
            .iter()
            .position(|(tr, tc, _, _)| *tr == r && *tc == c)
            .map(|i| {
                let (_, _, ch, scale) = TARGETS[i];
                (i, r, ch, scale)
            })
    };

    let mut cells = Vec::with_capacity(25);
    for (r, row) in GRID.iter().enumerate() {
        for (c, ch) in row.iter().enumerate() {
            let target = target_set(r, c);
            let (alpha, x, y, scale) = if let Some((i, _, target_ch, start_scale)) = target {
                let x0 = left + c as f32 * cell + cell * 0.5;
                let y0 = top + r as f32 * cell + cell * 0.5;
                let x1 = final_left + i as f32 * final_gap;
                let y1 = final_y;
                let x = x0 + (x1 - x0) * align - cell * 0.5;
                let y = y0 + (y1 - y0) * align - cell * 0.5;
                let scale = start_scale + (1.0 - start_scale) * align;
                let _ = target_ch;
                (fade, x, y, scale)
            } else {
                let alpha = fade * (1.0 - blackout);
                (
                    alpha,
                    left + c as f32 * cell + cell * 0.5 - cell * 0.5,
                    top + r as f32 * cell + cell * 0.5 - cell * 0.5,
                    1.0,
                )
            };

            let color = if target.is_some() {
                match *ch {
                    'a' | 'l' => Color::hex(0x9D_7B_FF),
                    't' => Color::hex(0x69_B7_FF),
                    _ => Color::hex(0xC2_A5_FF),
                }
            } else {
                Color::hex(0x5E_68_7D)
            };

            let child = SizedBox::from_size(Size::new(cell, cell)).child(
                Center::new().child(
                    Text::new(ch.to_string())
                        .size(cell * 0.42)
                        .bold()
                        .color(color),
                ),
            );
            cells.push(
                Positioned::new()
                    .left(x)
                    .top(y)
                    .child(
                        Opacity::new(alpha.clamp(0.0, 1.0)).child(
                            Transformed::new(
                                Transform::translate(Offset::new(
                                    cell * 0.5 * (1.0 - scale),
                                    cell * 0.5 * (1.0 - scale),
                                ))
                                .then(Transform::scale(scale, scale)),
                            )
                            .child(child),
                        ),
                    )
                    .into(),
            );
        }
    }

    let mark = Icon::new(crate::ui::icons::vavlt_mark())
        .size(52.0)
        .color(Color::WHITE);

    let word = Text::new("vavlt").size(30.0).bold().color(Color::WHITE);

    Opacity::new(fade.clamp(0.0, 1.0))
        .child(Container::new().color(Color::hex(0x06_09_12)).child(
            Stack::new().fit(StackFit::Expand).children(children![
                            Stack::new().fit(StackFit::Expand).children(cells),
                            Positioned::new()
                                .left(0.0)
                                .right(0.0)
                                .bottom(height * 0.11)
                                .child(
                                    Flex::column()
                                        .main_axis_size(MainAxisSize::Min)
                                        .cross_axis_alignment(CrossAxisAlignment::Center)
                                        .spacing(8.0)
                                        .children(children![mark, word]),
                                ),
                        ]),
        ))
        .into()
}

#[must_use]
fn smoothstep(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn shell(theme: &VavltTheme, state: &Rc<VavltState>, scrolls: &Rc<Scrolls>) -> WidgetNode {
    let screen = state.screen.get();
    let tab = state.tab.get();
    let at_root = screen == Screen::Root;
    let running = state.running.get();

    // The back chevron is absent at the root, and absent mid-run: a control
    // that is offered and then refused is worse than one that was never there.
    let back = (!(at_root || (screen == Screen::Working && running))).then(|| {
        let state = state.clone();
        Box::new(move || state.back()) as Box<dyn Fn()>
    });

    let trailing = (at_root && tab == Tab::Vavlt).then(|| {
        let state = state.clone();
        ui::icon_button(
            theme,
            ui::icons::contrast(),
            theme.colors.fg,
            "Switch theme",
            move || state.toggle_theme(),
        )
    });

    // Chrome floats *over* the content rather than sitting in bars of its own.
    //
    // Two things fall out of that, and both were defects before. The tab bar no
    // longer occupies layout space, so appearing and disappearing between the
    // root and the flow does not resize the page under the user — it used to
    // grow the body by 66 points in one frame, mid-navigation. And the title
    // sits on the billboard it belongs to instead of in a strip above it, so
    // there is no bar left to hold a title that changed before its screen did.
    let stage = stage_stack(theme, state, scrolls);

    // On the three root tabs, horizontal swipes are a second navigation affordance
    // alongside the bottom bar. The drag is axis-locked so the vertical scroll
    // controllers inside Activity/Settings keep ownership of ordinary scrolling.
    let stage = if at_root {
        let swipe_dx = Rc::new(Cell::new(0.0_f32));
        let start_dx = swipe_dx.clone();
        let update_dx = swipe_dx.clone();
        let end_dx = swipe_dx.clone();
        let swipe_state = state.clone();

        GestureDetector::new()
            .drag_axis(Axis::Horizontal)
            .on_drag_start(move |_| start_dx.set(0.0))
            .on_drag_update(move |details| {
                update_dx.set(update_dx.get() + details.delta.dx);
            })
            .on_drag_end(move |details| {
                let distance = end_dx.get();
                let velocity = details.velocity.dx;
                let threshold = 56.0;
                let fast = 420.0;

                let direction = if distance <= -threshold || velocity <= -fast {
                    Some(1_i32)
                } else if distance >= threshold || velocity >= fast {
                    Some(-1_i32)
                } else {
                    None
                };

                if let Some(direction) = direction {
                    let current = swipe_state.tab.peek().index() as i32;
                    let next = (current + direction).clamp(0, (Tab::ALL.len() - 1) as i32);
                    if next != current {
                        swipe_state.set_tab(Tab::from_index(next as usize));
                    }
                }
                end_dx.set(0.0);
            })
            .child(stage)
            .into()
    } else {
        stage
    };

    let mut layers: Vec<WidgetNode> = children![
        stage,
        Positioned::new()
            .left(0.0)
            .right(0.0)
            .top(0.0)
            .child(ui::top_bar(
                theme,
                title_for(state, screen, tab),
                back,
                trailing
            )),
    ];

    if at_root {
        let state = state.clone();
        layers.push(
            Positioned::new()
                .left(0.0)
                .right(0.0)
                .bottom(0.0)
                .child(ui::tab_bar(theme, tab, move |tab| state.set_tab(tab)))
                .into(),
        );
    }

    SafeArea::new()
        .child(Stack::new().fit(StackFit::Expand).children(layers))
        .into()
}

/// The five stages.
///
/// # This does not animate, and that is deliberate for now
///
/// It used to slide: five stages side by side, offset by
/// `(depth - current) * width`, driven by one `Animated`. Two defects came out
/// of `tests/transition.rs`, in order:
///
/// 1. **The stages painted on top of each other.** `Positioned::fill()` wrapping
///    a `Transformed::translate` moved nothing, so a navigation drew the
///    outgoing and incoming screens in the *same place* for its whole duration —
///    which is the "text overlaid for a fraction of a second" that was reported
///    from a device. `target/transitions/01-root-to-plan-*.png` before the
///    change is two screens superimposed.
/// 2. **Placing by coordinate instead fixed the overlay and lost the travel.**
///    With `Positioned::new().left(dx)` the stages no longer overlap and no
///    longer move either: the outgoing screen sits still and the incoming one
///    never arrives. The geometry is right and something about the box the
///    stack is measured in is not.
///
/// So this renders **only the current screen**, with no transition at all. That
/// is worse than a slide and much better than a slide that draws two screens on
/// top of each other: it is never wrong, it is just plain. The animation comes
/// back when the second defect is understood, and `tests/transition.rs` is the
/// thing that will say when it has been — it is written and passing against the
/// static version, so a re-introduced slide has to survive it.
fn stage_stack(theme: &VavltTheme, state: &Rc<VavltState>, scrolls: &Rc<Scrolls>) -> WidgetNode {
    let screen = state.screen.get();
    let tab = state.tab.get();
    let body = match screen {
        Screen::Root => tabs(theme, state, scrolls, tab),
        Screen::Plan => screens::plan(theme, state, &scrolls.plan),
        Screen::Working => screens::working(theme, state, &scrolls.working),
        Screen::Outcome => screens::outcome(theme, state, &scrolls.outcome),
        Screen::Photo => screens::photo(theme, state, &scrolls.photo),
    };
    arrival(screen, tab, body)
}

/// The arriving screen fades up and settles the last few points into place.
///
/// # Only the arriving screen, and that is the fix
///
/// The first version of this cross-faded the *outgoing* screen against the
/// incoming one in a `Stack`, and the two were drawn on top of each other for
/// the whole transition — legible text over legible text, which reads as a
/// glitch rather than as a movement. It was disabled rather than shipped, and
/// the two things it was blamed on both turned out to be innocent:
/// `crates/vieww/tests/stack_transform.rs` proves a `Transformed` inside a
/// filled stack slot does move its child, and
/// `crates/vieww/tests/positioned_moves.rs` found the real defect — a parent
/// data change that never dirtied its parent's layout — and fixed it in the
/// framework.
///
/// This version does not need either. There is no outgoing screen in the tree
/// at all, so there is nothing to superimpose: the old one is gone the instant
/// the signal changes and the new one arrives under its own power. That is
/// Material's "fade through" rather than a cross-dissolve, and it is the
/// motion that suits a stack of screens rather than a carousel.
///
/// # How it restarts
///
/// The `Key`. A fresh `Animated` element starts at `from` and travels to
/// `target`; an element that is *reused* is already at its target and does not
/// move. Keying on the destination means every navigation is a new element and
/// therefore a new arrival, with no signal to reset and no "am I transitioning"
/// state to get out of step with the screen it describes.
fn arrival(screen: Screen, tab: Tab, body: WidgetNode) -> WidgetNode {
    Animated::new(1.0)
        .from(0.0)
        .duration(motion::NAV)
        .curve(SETTLE)
        .key(Key::from(format!("{screen:?}/{tab:?}")))
        .build(move |t| {
            Opacity::new(ARRIVAL_FLOOR + (1.0 - ARRIVAL_FLOOR) * t.clamp(0.0, 1.0))
                .child(
                    Transformed::translate(Offset::new(0.0, (1.0 - t) * ARRIVAL_TRAVEL))
                        .child(body.clone()),
                )
                .into()
        })
        .into()
}

/// Depth 0: whichever tab is up.
///
/// The tab change animates through [`arrival`] like any other navigation —
/// the key it is given carries the tab as well as the screen.
fn tabs(theme: &VavltTheme, state: &Rc<VavltState>, scrolls: &Rc<Scrolls>, tab: Tab) -> WidgetNode {
    match tab {
        Tab::Vavlt => screens::vault(theme, state, &scrolls.vault),
        Tab::Activity => screens::activity(theme, state, &scrolls.activity),
        Tab::Settings => screens::settings(theme, state, &scrolls.settings),
    }
}

fn title_for(state: &Rc<VavltState>, screen: Screen, tab: Tab) -> String {
    match screen {
        Screen::Root => tab.title().to_string(),
        Screen::Plan => "Plan".to_string(),
        Screen::Working => if state.running.get() {
            "Working"
        } else {
            "Finished"
        }
        .to_string(),
        Screen::Outcome => "What changed".to_string(),
        Screen::Photo => state
            .photos
            .with(|photos| photos.get(state.detail.get()).map(|p| p.name.clone()))
            .unwrap_or_else(|| "Photo".to_string()),
    }
}
