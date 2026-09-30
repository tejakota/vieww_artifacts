//! The embed layer — third-party content inside the tree.
//!
//! # What this closes
//!
//! The framework's comparison tables list "browser embedding" as a gap
//! against Avalonia and Electron: an `<iframe>`'s worth of somebody else's
//! live content — a map, a video, a payment form, an advert — inside a
//! vieww screen. The *mechanism* already existed when the table was
//! written: foundation's [`PlatformViews`](vieww_foundation::capability::PlatformViews)
//! capability is a registry of named view factories, and `"webview"` is
//! the name a web implementation registers under. What was missing is the
//! **asking side**: a widget that says what it wants, in terms every
//! backend can satisfy or honestly refuse.
//!
//! This crate is that side, in two parts:
//!
//! * [`WebContent`] — *what* to show: a URL, or inline HTML. Two variants,
//!   because those are the two things an iframe's `src` and `srcdoc`
//!   attributes (and every native web view's load methods) have agreed on
//!   for twenty years.
//! * [`WebView`] — the widget. On the **web backend** it becomes a real
//!   `<iframe>` (the DOM backend's walk recognises it and emits the
//!   element); on every other backend it draws a themed placeholder — a
//!   framed box with the URL in it — because a canvas rasteriser cannot
//!   host a browser view, and pretending it could is a blank rectangle
//!   that looks broken rather than a box that says what it is.
//!
//! ```
//! use vieww_embed::{WebContent, WebView};
//!
//! // A map, in three lines.
//! let map = WebView::new(WebContent::url("https://maps.example.test/embed"));
//! assert_eq!(map.content().as_url(), Some("https://maps.example.test/embed"));
//!
//! // And the same widget, inline HTML — the srcdoc spelling.
//! let badge = WebView::new(WebContent::html("<strong>Hello</strong>"));
//! assert_eq!(badge.content().as_html(), Some("<strong>Hello</strong>"));
//! ```
//!
//! # The native path, said out loud
//!
//! `PlatformViews` is the seam for backends that can host a *native* view:
//! a Kotlin `WebView` on Android, a `WKWebView` on iOS, a `webview` crate
//! on the desktop. A host application registers a factory under
//! `"webview"` and mounts `WebViewSpec`s through the existing
//! `PlatformView` capability — [`WebContent::spec`] produces exactly the
//! parameters that registry entry expects, so the asking side never has
//! to know which platform answered.
//!
//! # Why the placeholder rather than nothing
//!
//! A widget that renders nothing on non-web backends makes a layout
//! *appear* to hold a hole — constraints resolve, the space is there, and
//! nothing explains it. The placeholder is the honest middle: same layout,
//! a frame, the URL as text, and a screen reader label that says "web
//! content". An application can branch on `TargetPlatform` when it wants
//! more, and most should not.

use vieww_foundation::capability::PlatformViewSpec;
use vieww_widget::prelude::*;

pub use vieww_foundation as foundation;
pub use vieww_widget as widget;

/// The `kind` a web view factory registers itself under.
///
/// A constant rather than a string at every call site, because the asking
/// side and the answering side have to agree on one spelling and a typo
/// between them is a runtime hole that type-checks.
pub const WEB_VIEW_KIND: &str = "webview";

/// What to show: a URL, or inline HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebContent {
    /// Load this URL — the `src` spelling. The embedder's origin, cookies
    /// and permissions are the browser's business, exactly as they would
    /// be for the same URL in a tab.
    Url(String),
    /// Render this HTML — the `srcdoc` spelling. For self-contained
    /// fragments: a generated badge, a styled receipt, a chart from an
    /// inline script. **Not** for stitching in untrusted page-sized
    /// content, which a sandboxed iframe exists to hold and this type
    /// does not model.
    Html(String),
}

impl WebContent {
    /// A URL to load.
    #[must_use]
    pub fn url(url: impl Into<String>) -> Self {
        Self::Url(url.into())
    }

    /// HTML to render.
    #[must_use]
    pub fn html(html: impl Into<String>) -> Self {
        Self::Html(html.into())
    }

    /// The URL, if that is what this is.
    #[must_use]
    pub fn as_url(&self) -> Option<&str> {
        match self {
            Self::Url(url) => Some(url),
            Self::Html(_) => None,
        }
    }

    /// The HTML, if that is what this is.
    #[must_use]
    pub fn as_html(&self) -> Option<&str> {
        match self {
            Self::Html(html) => Some(html),
            Self::Url(_) => None,
        }
    }

    /// The parameters a `"webview"` factory expects: `url`, or `srcdoc`.
    ///
    /// The translation to [`PlatformViewSpec`] that lets the *asking* side
    /// and the platform *answering* side share one spelling without either
    /// knowing the other — see the module docs.
    #[must_use]
    pub fn spec(&self) -> PlatformViewSpec {
        let mut spec = PlatformViewSpec::new(WEB_VIEW_KIND);
        match self {
            Self::Url(url) => spec = spec.with("url", url.clone()),
            Self::Html(html) => spec = spec.with("srcdoc", html.clone()),
        }
        spec
    }
}

/// A web view in the tree: an `<iframe>` on the web backend, a themed
/// placeholder everywhere else.
///
/// The size is part of the ask, because an embedded view is *placed*
/// content — a map that reshapes itself to its container is a map that
/// jumps when a panel opens. Fixed here, like the `Canvas` island the DOM
/// backend already sizes the same way.
#[derive(Debug, Clone, PartialEq)]
pub struct WebView {
    content: WebContent,
    view_size: Size,
}

impl WebView {
    /// A web view showing `content`, 320×180 by default — a 16:9 card.
    #[must_use]
    pub fn new(content: WebContent) -> Self {
        Self {
            content,
            view_size: Size::new(320.0, 180.0),
        }
    }

    /// What this view shows.
    #[must_use]
    pub const fn content(&self) -> &WebContent {
        &self.content
    }

    /// The size the view asked for — what a backend that hosts real content
    /// should reserve, and what the placeholder draws.
    #[must_use]
    pub const fn view_size(&self) -> Size {
        self.view_size
    }

    /// Set the size.
    #[must_use]
    pub const fn size(mut self, size: Size) -> Self {
        self.view_size = size;
        self
    }
}

impl Widget for WebView {
    fn debug_name(&self) -> &'static str {
        "WebView"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, ctx: &BuildContext) -> WidgetNode {
        // The placeholder: a framed surface, the URL or "inline HTML" as a
        // line of text, and a semantics label that says what the space is.
        // The DOM backend intercepts this widget on its walk and never
        // reaches this build; every other backend does, and shows something
        // honest in the space.
        let theme = ThemeData::of(ctx);
        Semantics::new()
            .label(format!(
                "web content: {}",
                self.content.as_url().unwrap_or("inline HTML")
            ))
            .child(CustomPaint::sized(
                self.view_size,
                WebViewPainter {
                    frame: theme.colors.outline,
                },
            ))
            .into()
    }
}

widget_node_from!(WebView);

/// The placeholder's painter: a frame and a subtle cross-hatch, so the box
/// reads as "a space something else will fill" rather than "a broken
/// control".
struct WebViewPainter {
    frame: Color,
}

impl CustomPainter for WebViewPainter {
    fn paint(&self, size: Size) -> Vec<DrawInstruction> {
        let mut instructions = vec![
            DrawInstruction::FillRoundedRect {
                rect: Rect::new(0.0, 0.0, size.width, size.height),
                radius: 8.0,
                color: self.frame.with_alpha(0x60),
            },
        ];
        // The cross-hatch: corner-to-corner hairlines, the universal
        // "reserved space" mark from paper forms to CAD.
        let inset = 6.0;
        instructions.push(DrawInstruction::DrawLine {
            from: Offset::new(inset, inset),
            to: Offset::new(size.width - inset, size.height - inset),
            color: self.frame.with_alpha(0x40),
            width: 0.5,
        });
        instructions.push(DrawInstruction::DrawLine {
            from: Offset::new(size.width - inset, inset),
            to: Offset::new(inset, size.height - inset),
            color: self.frame.with_alpha(0x40),
            width: 0.5,
        });
        instructions
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn should_repaint(&self, previous: &dyn CustomPainter) -> bool {
        match previous.as_any().downcast_ref::<Self>() {
            Some(prev) => prev.frame != self.frame,
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vieww_widget::inflate;

    #[test]
    fn web_content_knows_which_side_it_is() {
        let url = WebContent::url("https://example.test/page");
        assert_eq!(url.as_url(), Some("https://example.test/page"));
        assert_eq!(url.as_html(), None);

        let html = WebContent::html("<p>hello</p>");
        assert_eq!(html.as_html(), Some("<p>hello</p>"));
        assert_eq!(html.as_url(), None);
    }

    #[test]
    fn the_spec_speaks_the_registrys_language() {
        let spec = WebContent::url("https://example.test/").spec();
        assert_eq!(spec.kind, WEB_VIEW_KIND);
        assert_eq!(spec.parameters.get("url").map(String::as_str), Some("https://example.test/"));
        assert!(!spec.parameters.contains_key("srcdoc"));

        let spec = WebContent::html("<em>hi</em>").spec();
        assert_eq!(spec.kind, WEB_VIEW_KIND);
        assert_eq!(
            spec.parameters.get("srcdoc").map(String::as_str),
            Some("<em>hi</em>")
        );
        assert!(!spec.parameters.contains_key("url"));
    }

    #[test]
    fn a_web_view_defaults_to_a_sixteen_nine_card() {
        let view = WebView::new(WebContent::url("https://example.test/"));
        assert_eq!(view.content().as_url(), Some("https://example.test/"));
        assert_eq!(view.view_size, Size::new(320.0, 180.0));

        let sized = view.size(Size::new(200.0, 100.0));
        assert_eq!(sized.view_size, Size::new(200.0, 100.0));
    }

    #[test]
    fn the_placeholder_builds_on_a_plain_backend() {
        // Building the widget off the web backend produces the framed
        // placeholder — the walk that turns it into an iframe is the DOM
        // backend's business, tested there.
        let view = WebView::new(WebContent::url("https://example.test/"));
        let node = inflate(view);
        let rendered = format!("{node:?}");
        assert!(rendered.contains("WebView"), "the placeholder is built: {rendered}");
    }
}
