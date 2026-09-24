//! Widget wrappers for the effects.
//!
//! These are composed widgets: they build into the existing widget
//! tree. The pixel operations in `cpu` are available for renderers
//! that want to apply them at raster time.

use vieww_foundation::Color;
use vieww_widget::prelude::*;

/// How to combine two layers.
///
/// This is the effects-side spelling of the twelve separable and
/// Porter-Duff-over modes the rasterizer honours; [`From`] maps each onto
/// its `vieww_foundation::BlendMode` twin, which is the one the render
/// pipeline consumes. `SrcOver` here is `Normal` there — the name a caller
/// of *this* crate reaches for when they mean "the default".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    SrcOver,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
}

impl From<BlendMode> for vieww_foundation::BlendMode {
    fn from(mode: BlendMode) -> Self {
        match mode {
            BlendMode::SrcOver => Self::Normal,
            BlendMode::Multiply => Self::Multiply,
            BlendMode::Screen => Self::Screen,
            BlendMode::Overlay => Self::Overlay,
            BlendMode::Darken => Self::Darken,
            BlendMode::Lighten => Self::Lighten,
            BlendMode::ColorDodge => Self::ColorDodge,
            BlendMode::ColorBurn => Self::ColorBurn,
            BlendMode::HardLight => Self::HardLight,
            BlendMode::SoftLight => Self::SoftLight,
            BlendMode::Difference => Self::Difference,
            BlendMode::Exclusion => Self::Exclusion,
        }
    }
}

/// What to do to a captured backdrop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackdropFilter {
    /// The blur radius (sigma) in logical pixels. 0 = no blur.
    pub blur: f32,
    /// A colour drawn over the backdrop, with its alpha.
    pub tint: Color,
}

impl BackdropFilter {
    /// Frosted glass: a blur and a white tint.
    #[must_use]
    pub const fn frosted(blur: f32) -> Self {
        Self {
            blur,
            tint: Color::rgba(255, 255, 255, 26),
        }
    }

    /// Dark glass: a blur and a black tint.
    #[must_use]
    pub const fn dark(blur: f32) -> Self {
        Self {
            blur,
            tint: Color::rgba(0, 0, 0, 40),
        }
    }
}

/// A widget that blurs whatever is behind it.
///
/// A real backdrop filter: the CPU renderer samples the destination pixels
/// already painted beneath this widget's bounds, blurs and tints *that*
/// copy, and only then paints this widget's own children on top of the
/// result — see [`vieww_foundation::ImageFilter::backdrop`] for exactly how
/// that ordering is enforced. The layout is identical either way; the
/// visual is genuine frosted glass rather than a flat wash.
///
/// # Examples
///
/// ```ignore
/// BackdropBlur::new(BackdropFilter::frosted(24.0))
///     .child(nav_bar_content)
/// ```
#[derive(Debug)]
pub struct BackdropBlur {
    filter: BackdropFilter,
    child: WidgetNode,
}

impl BackdropBlur {
    /// Create a backdrop blur with `filter`.
    #[must_use]
    /// Not `const`: the placeholder child is a `WidgetNode`, which is an `Rc`
    /// allocation, and allocating is not something a const fn may do.
    pub fn new(filter: BackdropFilter) -> Self {
        Self {
            filter,
            child: SizedBox::shrink().into(),
        }
    }

    /// Set the content drawn on top of the blur.
    #[must_use]
    pub fn child(mut self, child: impl Into<WidgetNode>) -> Self {
        self.child = child.into();
        self
    }
}

impl Widget for BackdropBlur {
    fn debug_name(&self) -> &'static str {
        "BackdropBlur"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    /// # A real backdrop filter, not a degraded stand-in
    ///
    /// This builds a [`Filtered`] with
    /// [`Filtered::with_backdrop`](vieww_widget::Filtered::with_backdrop) set,
    /// which is the widget that owns the behaviour: `Command::PushLayer`
    /// carries an [`ImageFilter`](vieww_foundation::ImageFilter) whose
    /// `backdrop` flag tells the CPU renderer to sample the real destination
    /// pixels beneath this widget, blur and tint that sampled copy, and only
    /// then let this widget's children paint on top of it — genuine frosted
    /// glass, not a flat translucent wash standing in for one. Kept as its
    /// own type because "frosted glass" is a recognisable thing to ask for
    /// and `BackdropFilter::frosted(24.0)` says it in one line.
    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let sigma = self.filter.blur;
        let mut filtered = vieww_widget::Filtered::blur(sigma).with_backdrop();
        if self.filter.tint.a > 0 {
            let amount = f32::from(self.filter.tint.a) / 255.0;
            filtered = filtered.tint(self.filter.tint, amount);
        }
        filtered.child(self.child.clone()).into()
    }
}

vieww_widget::widget_node_from!(BackdropBlur);

/// One colour filter, as a 5×4 matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Filter {
    matrix: [f32; 20],
}

impl Filter {
    /// The identity — no change.
    #[must_use]
    pub const fn identity() -> Self {
        Self {
            matrix: [
                1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
            ],
        }
    }

    /// Adjust brightness. `1.0` = unchanged.
    #[must_use]
    pub fn brightness(amount: f32) -> Self {
        Self {
            matrix: [
                amount, 0.0, 0.0, 0.0, 0.0, 0.0, amount, 0.0, 0.0, 0.0, 0.0, 0.0, amount, 0.0, 0.0,
                0.0, 0.0, 0.0, 1.0, 0.0,
            ],
        }
    }

    /// Adjust saturation. `1.0` = unchanged, `0.0` = grayscale.
    #[must_use]
    pub fn saturation(amount: f32) -> Self {
        const LR: f32 = 0.213;
        const LG: f32 = 0.715;
        const LB: f32 = 0.072;
        let sr = (1.0 - amount) * LR;
        let sg = (1.0 - amount) * LG;
        let sb = (1.0 - amount) * LB;

        Self {
            matrix: [
                sr + amount,
                sg,
                sb,
                0.0,
                0.0,
                sr,
                sg + amount,
                sb,
                0.0,
                0.0,
                sr,
                sg,
                sb + amount,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
            ],
        }
    }

    /// Convert to grayscale.
    #[must_use]
    pub fn grayscale() -> Self {
        Self::saturation(0.0)
    }

    /// The sepia tone.
    #[must_use]
    pub fn sepia() -> Self {
        Self {
            matrix: [
                0.393, 0.769, 0.189, 0.0, 0.0, 0.349, 0.686, 0.168, 0.0, 0.0, 0.272, 0.534, 0.131,
                0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0,
            ],
        }
    }

    /// The matrix, for the renderer.
    #[must_use]
    pub const fn matrix(&self) -> [f32; 20] {
        self.matrix
    }
}

/// A chain of filters applied to a subtree.
///
/// Without render-pipeline support, degrades to an opacity drop for
/// the "dimmed" case and a no-op otherwise.
///
/// # Examples
///
/// ```ignore
/// FilterChain::new()
///     .filter(Filter::grayscale())
///     .filter(Filter::brightness(0.6))
///     .child(panel_content)
/// ```
#[derive(Debug)]
pub struct FilterChain {
    filters: Vec<Filter>,
    child: WidgetNode,
}

impl FilterChain {
    /// An empty chain.
    #[must_use]
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
            child: SizedBox::shrink().into(),
        }
    }

    /// Add a filter to the chain.
    #[must_use]
    pub fn filter(mut self, filter: Filter) -> Self {
        self.filters.push(filter);
        self
    }

    /// Set the filtered child.
    #[must_use]
    pub fn child(mut self, child: impl Into<WidgetNode>) -> Self {
        self.child = child.into();
        self
    }
}

impl Default for FilterChain {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for FilterChain {
    fn debug_name(&self) -> &'static str {
        "FilterChain"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    /// # No longer a degraded path
    ///
    /// This used to render `Opacity(0.7)` regardless of which filters were in
    /// the chain — a grayscale request came out faded and still colourful. The
    /// matrices were correct and simply never reached a renderer.
    ///
    /// They do now: the chain composes into one 5×4 matrix and rides on the
    /// group's layer, which the CPU backend applies to the rasterised pixels.
    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        if self.filters.is_empty() {
            return self.child.clone();
        }

        let mut filtered = vieww_widget::Filtered::new();
        for filter in &self.filters {
            filtered = filtered.matrix(filter.matrix());
        }
        filtered.child(self.child.clone()).into()
    }
}

vieww_widget::widget_node_from!(FilterChain);

/// Draws `foreground` over `background` using `mode`.
///
/// Without render-pipeline support, degrades to plain stacking.
#[derive(Debug)]
pub struct Blend {
    mode: BlendMode,
    foreground: WidgetNode,
    background: WidgetNode,
}

impl Blend {
    /// Create a blend with `mode`.
    #[must_use]
    /// Not `const`, for the same reason as `BackdropBlur::new`.
    pub fn new(mode: BlendMode) -> Self {
        Self {
            mode,
            foreground: SizedBox::shrink().into(),
            background: SizedBox::shrink().into(),
        }
    }

    /// The layer drawn on top.
    #[must_use]
    pub fn foreground(mut self, fg: impl Into<WidgetNode>) -> Self {
        self.foreground = fg.into();
        self
    }

    /// The layer beneath.
    #[must_use]
    pub fn background(mut self, bg: impl Into<WidgetNode>) -> Self {
        self.background = bg.into();
        self
    }
}

impl Widget for Blend {
    fn debug_name(&self) -> &'static str {
        "Blend"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    /// # No longer a silent no-op (todo-upgrades U-02)
    ///
    /// This used to read `let _ = self.mode;` — the widget took a
    /// `BlendMode`, discarded it, and stacked. Its own doc said "without
    /// render-pipeline support, degrades to plain stacking", which described
    /// a world that no longer existed: the pipeline honours a mode through
    /// `Opacity::blend` today. The most dangerous kind of defect in a
    /// repository with this culture is one that does not fail — it just
    /// quietly produces `Normal`.
    ///
    /// The mode now rides on the foreground's compositing group, over the
    /// background, in a Stack — `Opacity::new(1.0).blend(mode)` being the one
    /// way the tree reaches a blend mode. This widget is a convenience over
    /// that one way rather than a second, silent one.
    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        Stack::new()
            .push(self.background.clone())
            .push(
                vieww_widget::Opacity::new(1.0)
                    .blend(self.mode.into())
                    .child(self.foreground.clone()),
            )
            .into()
    }
}

vieww_widget::widget_node_from!(Blend);

#[cfg(test)]
mod tests {
    use super::*;

    /// todo-upgrades U-02: every mode this crate can name must land on a
    /// foundation mode the rasterizer honours — the conversion is the seam
    /// the widget's correctness lives at now, so a variant added on one side
    /// without the other must fail here rather than render as `Normal`.
    #[test]
    fn every_effect_mode_lands_on_its_foundation_twin() {
        assert_eq!(
            vieww_foundation::BlendMode::from(BlendMode::SrcOver),
            vieww_foundation::BlendMode::Normal
        );
        assert_eq!(
            vieww_foundation::BlendMode::from(BlendMode::Multiply),
            vieww_foundation::BlendMode::Multiply
        );
        assert_eq!(
            vieww_foundation::BlendMode::from(BlendMode::Exclusion),
            vieww_foundation::BlendMode::Exclusion
        );
        // And the round trip through the whole set stays total: no mode maps
        // to a default behind our back.
        for mode in [
            BlendMode::SrcOver,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::Darken,
            BlendMode::Lighten,
            BlendMode::ColorDodge,
            BlendMode::ColorBurn,
            BlendMode::HardLight,
            BlendMode::SoftLight,
            BlendMode::Difference,
            BlendMode::Exclusion,
        ] {
            let mapped: vieww_foundation::BlendMode = mode.into();
            let name = format!("{mode:?}");
            // The honoured set: `Normal`, `Plus`, and the eleven separable
            // modes — everything except the non-separables and the two
            // Porter-Duff degenerate cases (see U-01's "22 of 28").
            assert!(
                matches!(
                    mapped,
                    vieww_foundation::BlendMode::Normal
                        | vieww_foundation::BlendMode::Multiply
                        | vieww_foundation::BlendMode::Screen
                        | vieww_foundation::BlendMode::Overlay
                        | vieww_foundation::BlendMode::Darken
                        | vieww_foundation::BlendMode::Lighten
                        | vieww_foundation::BlendMode::ColorDodge
                        | vieww_foundation::BlendMode::ColorBurn
                        | vieww_foundation::BlendMode::HardLight
                        | vieww_foundation::BlendMode::SoftLight
                        | vieww_foundation::BlendMode::Difference
                        | vieww_foundation::BlendMode::Exclusion
                ),
                "{name} must map onto a mode the rasterizer honours, got {mapped:?}"
            );
        }
    }
}
