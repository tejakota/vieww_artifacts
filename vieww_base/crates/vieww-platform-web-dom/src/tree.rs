//! The walk: an element tree in, a virtual DOM tree out.
//!
//! This is the DOM's answer to `vieww_render::RenderObjectFactory`. The factory
//! reads the same primitives and builds render objects; this reads them and
//! builds elements and CSS. Neither knows about the other, and a widget written
//! outside this repository takes part in both on the same terms — by being one
//! of the primitives, or by building down to them.

use std::rc::Rc;

use vieww_element::{ElementId, ElementTree};
use vieww_foundation::TextAlign;
use vieww_widget::prelude::*;
use vieww_widget::prelude::{
    Align, AspectRatio, Clip, ClipShape, ColoredBox, Constrained, CursorArea, DecoratedBox, Flex,
    Flexible, GestureDetector, Image as ImageWidget, Offstage, Opacity, Padding, Positioned,
    RichText, Stack, Text, Transformed,
};

use crate::style::{px, text_css, Slot, Style};
use crate::widgets::{Canvas, Styled, Tag};

/// One DOM element, before it is a DOM element.
///
/// Kept as data rather than built straight into the document so a rebuild can
/// diff against the last one and touch only what changed — the same reason the
/// render tree exists between the widget tree and the pixels.
pub struct VNode {
    pub tag: &'static str,
    pub css: String,
    pub class: Option<String>,
    pub text: Option<String>,
    pub href: Option<String>,
    pub id: Option<String>,
    pub label: Option<String>,
    /// `Some` for a `<canvas>` island: the mount callback, run once, after the
    /// element is in the document.
    #[allow(clippy::type_complexity)]
    pub canvas: Option<(String, Rc<dyn Fn(&str)>)>,
    /// `Some` for an `<iframe>` embed: what to load (`src`) or render
    /// (`srcdoc`), and the semantics label.
    ///
    /// An `href` on an `iframe` would be ambiguous with a link's, so the
    /// embed carries its own field — see `walk`'s `WebView` case for why
    /// the DOM turn of an embed is a leaf with no subtree.
    pub iframe: Option<Iframe>,
    pub on_click: Option<Rc<dyn Fn()>>,
    pub children: Vec<VNode>,
}

/// An embedded web view's payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Iframe {
    /// The `src` to load, when the embed asked for a URL.
    pub src: Option<String>,
    /// The `srcdoc` to render, when the embed asked for inline HTML.
    pub srcdoc: Option<String>,
    /// What a screen reader is told, because an iframe is opaque to
    /// assistive technology and its `title` attribute is the one line
    /// of description that reaches the user.
    pub title: String,
}

impl VNode {
    fn div(css: String) -> Self {
        Self {
            tag: "div",
            css,
            class: None,
            text: None,
            href: None,
            id: None,
            label: None,
            canvas: None,
            iframe: None,
            on_click: None,
            children: Vec::new(),
        }
    }
}

/// What a walk has accumulated but not yet spent on an element.
#[derive(Default)]
struct Pending {
    style: Style,
    class: Option<String>,
    tag: Option<&'static str>,
    href: Option<String>,
    id: Option<String>,
    label: Option<String>,
    on_click: Option<Rc<dyn Fn()>>,
}

impl Pending {
    fn is_bare(&self) -> bool {
        self.style.is_empty()
            && self.class.is_none()
            && self.tag.is_none()
            && self.on_click.is_none()
            && self.id.is_none()
    }

    fn spend(self, tag: &'static str) -> VNode {
        VNode {
            tag: self.tag.unwrap_or(tag),
            css: self.style.to_css(),
            class: self.class,
            text: None,
            href: self.href,
            id: self.id,
            label: self.label,
            canvas: None,
            iframe: None,
            on_click: self.on_click,
            children: Vec::new(),
        }
    }
}

/// Translate the subtree at `id`.
///
/// Returns a list because a widget can be layout-transparent — an `Offstage`
/// that is hidden contributes nothing, and a `Composed` element contributes
/// whatever its one child does.
pub fn walk(tree: &ElementTree, id: ElementId) -> Vec<VNode> {
    walk_with(tree, id, Pending::default())
}

/// Carry on into the one child, or — when there is none — spend what has been
/// accumulated on a box of its own.
///
/// The second half is the whole point. A `Container` with a colour, a radius
/// and a size builds to `Constrained > DecoratedBox` with **nothing under it**,
/// so a walk that only recursed returned an empty list and the box vanished:
/// every dot, rule and coloured rectangle on the page disappeared, while the
/// boxes with children stayed. It looked like a styling bug and was a missing
/// base case.
fn descend(tree: &ElementTree, child: Option<ElementId>, pending: Pending) -> Vec<VNode> {
    match child {
        Some(id) => walk_with(tree, id, pending),
        None if pending.is_bare() => Vec::new(),
        None => vec![pending.spend("div")],
    }
}

#[allow(clippy::too_many_lines)]
fn walk_with(tree: &ElementTree, id: ElementId, mut pending: Pending) -> Vec<VNode> {
    let Some(element) = tree.get(id) else {
        return Vec::new();
    };
    let widget = element.widget();
    let children = element.children().to_vec();
    let only = || children.first().copied();

    // ── the escape hatches ────────────────────────────────────────────────
    if let Some(styled) = widget.downcast_ref::<Styled>() {
        if styled.leaf {
            if !styled.css.is_empty() {
                pending.style.raw.push(styled.css.clone());
            }
            if let Some(class) = &styled.class {
                pending.class = Some(class.clone());
            }
            return vec![pending.spend("div")];
        }
        if styled.block {
            let mut inner = Pending::default();
            inner.style.raw.push(styled.css.clone());
            inner.class = styled.class.clone();
            return vec![finish(tree, &children, pending.spend("div"), inner)];
        }
        if !styled.css.is_empty() {
            pending.style.raw.push(styled.css.clone());
        }
        if let Some(class) = &styled.class {
            pending.class = Some(match pending.class.take() {
                Some(existing) => format!("{existing} {class}"),
                None => class.clone(),
            });
        }
        return descend(tree, only(), pending);
    }
    if let Some(tagged) = widget.downcast_ref::<Tag>() {
        // A second tag on one box would be a nesting, so flush first.
        if pending.tag.is_some() {
            return vec![finish(
                tree,
                &children,
                pending.spend("div"),
                Pending {
                    tag: Some(tagged.tag),
                    href: tagged.href.clone(),
                    id: tagged.id.clone(),
                    label: tagged.label.clone(),
                    ..Pending::default()
                },
            )];
        }
        pending.tag = Some(tagged.tag);
        pending.href = tagged.href.clone().or(pending.href);
        pending.id = tagged.id.clone().or(pending.id);
        pending.label = tagged.label.clone().or(pending.label);
        return descend(tree, only(), pending);
    }
    if let Some(embed) = widget.downcast_ref::<vieww_embed::WebView>() {
        // The embed's DOM turn: a real `<iframe>`, sized as the widget asked
        // (the same contract `Canvas` gets), loaded from the embed's URL or
        // rendered from its HTML. A leaf: the browser owns the subtree, and
        // a walk that descended into it would find nothing it can express.
        let mut node = pending.spend("iframe");
        node.tag = "iframe";
        node.iframe = Some(Iframe {
            src: embed.content().as_url().map(str::to_string),
            srcdoc: embed.content().as_html().map(str::to_string),
            title: format!(
                "web content: {}",
                embed.content().as_url().unwrap_or("inline HTML")
            ),
        });
        node.css.push_str(&format!(
            "display:block;border:none;width:{};height:{};",
            px(embed.view_size().width),
            px(embed.view_size().height)
        ));
        return vec![node];
    }
    if let Some(canvas) = widget.downcast_ref::<Canvas>() {
        let mut node = pending.spend("canvas");
        node.tag = "canvas";
        node.canvas = Some((canvas.id.clone(), canvas.mount.clone()));
        node.id = Some(canvas.id.clone());
        node.css.push_str(&format!(
            "display:block;width:{};height:{};",
            px(canvas.width),
            px(canvas.height)
        ));
        return vec![node];
    }

    // ── layout-transparent primitives: fold in, keep descending ───────────
    if let Some(w) = widget.downcast_ref::<Padding>() {
        if pending.style.has(Slot::Padding) {
            return vec![flush(tree, &children, pending, |p| {
                p.style.padding = Some(w.insets())
            })];
        }
        pending.style.padding = Some(w.insets());
        return descend(tree, only(), pending);
    }
    if let Some(w) = widget.downcast_ref::<ColoredBox>() {
        if pending.style.has(Slot::Background) {
            return vec![flush(tree, &children, pending, |p| {
                p.style.background = Some(w.color());
            })];
        }
        pending.style.background = Some(w.color());
        return descend(tree, only(), pending);
    }
    if let Some(w) = widget.downcast_ref::<DecoratedBox>() {
        let decoration = w.decoration();
        if !pending.style.decorate(&decoration) {
            return vec![flush(tree, &children, pending, |p| {
                p.style.decorate(&decoration);
            })];
        }
        return descend(tree, only(), pending);
    }
    if let Some(w) = widget.downcast_ref::<Constrained>() {
        let c = w.constraints();
        if pending.style.has(Slot::Bounds) || pending.style.has(Slot::Size) {
            return vec![flush(tree, &children, pending, |p| apply_constraints(p, c))];
        }
        apply_constraints(&mut pending, c);
        return descend(tree, only(), pending);
    }
    if let Some(w) = widget.downcast_ref::<Opacity>() {
        if pending.style.has(Slot::Opacity) {
            return vec![flush(tree, &children, pending, |p| {
                p.style.opacity = Some(w.alpha());
            })];
        }
        pending.style.opacity = Some(w.alpha());
        return descend(tree, only(), pending);
    }
    if let Some(w) = widget.downcast_ref::<Transformed>() {
        if pending.style.has(Slot::Transform) {
            return vec![flush(tree, &children, pending, |p| {
                p.style.transform = Some(w.matrix());
            })];
        }
        pending.style.transform = Some(w.matrix());
        return descend(tree, only(), pending);
    }
    if let Some(w) = widget.downcast_ref::<Clip>() {
        pending.style.clip = true;
        if let ClipShape::RRect { radius } = w.shape() {
            if pending.style.radius.is_none() {
                pending.style.radius = Some(*radius);
            }
        }
        return descend(tree, only(), pending);
    }
    if let Some(w) = widget.downcast_ref::<AspectRatio>() {
        pending.style.aspect_ratio = Some(w.ratio());
        return descend(tree, only(), pending);
    }
    if let Some(w) = widget.downcast_ref::<Flexible>() {
        if pending.style.has(Slot::Flex) {
            return vec![flush(tree, &children, pending, |p| {
                p.style.flex = Some(f32::from(w.factor().flex));
            })];
        }
        pending.style.flex = Some(f32::from(w.factor().flex));
        return descend(tree, only(), pending);
    }
    if let Some(w) = widget.downcast_ref::<Positioned>() {
        let p = w.position();
        if p.is_positioned() {
            pending.style.position = Some((p.left, p.top, p.right, p.bottom));
            if let Some(width) = p.width {
                pending.style.width = Some(width);
            }
            if let Some(height) = p.height {
                pending.style.height = Some(height);
            }
        }
        return descend(tree, only(), pending);
    }
    if widget.downcast_ref::<CursorArea>().is_some() {
        pending.style.cursor = Some("pointer");
        return descend(tree, only(), pending);
    }
    if let Some(w) = widget.downcast_ref::<GestureDetector>() {
        let handlers = w.handlers();
        if let Some(tap) = handlers.on_tap.clone() {
            pending.on_click = Some(Rc::new(move || {
                tap(vieww_foundation::TapDetails {
                    position: vieww_foundation::Offset::ZERO,
                    local: vieww_foundation::Offset::ZERO,
                    button: vieww_foundation::PointerButton::Primary,
                    modifiers: vieww_foundation::Modifiers::default(),
                    timestamp: std::time::Duration::ZERO,
                });
            }));
            if pending.style.cursor.is_none() {
                pending.style.cursor = Some("pointer");
            }
        }
        return descend(tree, only(), pending);
    }
    if let Some(w) = widget.downcast_ref::<Offstage>() {
        if w.is_offstage() {
            return Vec::new();
        }
        return descend(tree, only(), pending);
    }
    if let Some(w) = widget.downcast_ref::<Align>() {
        // An `Align` is a one-child flex box in CSS, which is also how `Center`
        // — the widget that produces most of them — is meant to read.
        let a = w.alignment();
        pending.style.raw.push(format!(
            "display:flex;justify-content:{};align-items:{};",
            place(a.x),
            place(a.y)
        ));
        return descend(tree, only(), pending);
    }

    // ── the primitives that need a box of their own ───────────────────────
    if let Some(w) = widget.downcast_ref::<Flex>() {
        let sized = pending.style.has(Slot::Size);
        let mut node = pending.spend("div");
        node.css.push_str(&format!(
            "display:flex;flex-direction:{};justify-content:{};align-items:{};",
            if w.direction() == Axis::Horizontal {
                "row"
            } else {
                "column"
            },
            main_axis(w.main_axis()),
            cross_axis(w.cross_axis()),
        ));
        if w.child_spacing() > 0.0 {
            node.css
                .push_str(&format!("gap:{};", px(w.child_spacing())));
        }
        if w.axis_size() == MainAxisSize::Min && !sized {
            node.css.push_str(if w.direction() == Axis::Horizontal {
                "width:fit-content;"
            } else {
                "height:fit-content;"
            });
        }
        node.children = children.iter().flat_map(|c| walk(tree, *c)).collect();
        return vec![node];
    }
    if let Some(w) = widget.downcast_ref::<Stack>() {
        // Read before spending: a `Stack` that shares a box with the
        // `Container` above it must not restate the size that box already has.
        // `width:100%` after `width:22px` is what turned a 22-point mark into a
        // full-width one, because the last declaration wins.
        let sized = pending.style.has(Slot::Size);
        let mut node = pending.spend("div");
        node.css.push_str("position:relative;display:grid;");
        if w.stack_fit() == StackFit::Expand && !sized {
            node.css.push_str("width:100%;height:100%;");
        }
        node.children = children
            .iter()
            .flat_map(|c| walk(tree, *c))
            .map(|mut child| {
                // Non-positioned children of a grid stack share one cell, which
                // is what `Stack` means and what `position:absolute` would lose
                // for the child that decides the size.
                if !child.css.contains("position:absolute") {
                    child.css.push_str("grid-area:1/1;");
                }
                child
            })
            .collect();
        return vec![node];
    }
    if let Some(w) = widget.downcast_ref::<Text>() {
        let mut node = pending.spend("span");
        node.css.push_str(&text_css(w.text_style(), w.text_align()));
        // **The author's `display` wins.** `spend` has already written the
        // accumulated style, and a `Styled` that asked for `display:flex` —
        // a badge centring a digit, say — is in there. Appending `display:block`
        // unconditionally overrode it, and the digit sat at the top of its
        // badge on a line box two thirds the badge's height. Anything the tree
        // did not ask for is still a block.
        if !node.css.contains("display:") {
            node.css.push_str("display:block;");
        }
        node.css.push_str("white-space:pre-wrap;");
        if let Some(limit) = w.line_limit() {
            node.css.push_str(&format!(
                "display:-webkit-box;-webkit-line-clamp:{limit};-webkit-box-orient:vertical;overflow:hidden;"
            ));
        }
        node.text = Some(w.data().to_owned());
        return vec![node];
    }
    if let Some(w) = widget.downcast_ref::<RichText>() {
        let base = w.base_style().unwrap_or_else(|| TextStyle::new(15.0));
        let mut node = pending.spend("span");
        node.css.push_str(&text_css(&base, TextAlign::Start));
        if !node.css.contains("display:") {
            node.css.push_str("display:block;");
        }
        node.css.push_str("white-space:pre-wrap;");
        node.children = w
            .spans()
            .iter()
            .map(|span| {
                let style = span.resolved(base);
                let mut child = VNode::div(text_css(&style, TextAlign::Start));
                child.tag = "span";
                child.css.push_str("display:inline;");
                child.text = Some(span.text().to_owned());
                child
            })
            .collect();
        return vec![node];
    }
    if let Some(w) = widget.downcast_ref::<ImageWidget>() {
        let mut node = pending.spend("img");
        node.tag = "img";
        if !node.css.contains("display:") {
            node.css.push_str("display:block;");
        }
        node.css.push_str("object-fit:contain;");
        if let Some(width) = w.image_width() {
            node.css.push_str(&format!("width:{};", px(width)));
        }
        if let Some(height) = w.image_height() {
            node.css.push_str(&format!("height:{};", px(height)));
        }
        node.href = crate::images::data_url(w.data());
        node.label = w.image_label().map(str::to_owned);
        return vec![node];
    }

    // ── anything else: a `Composed` element, or a primitive this backend has
    //    no opinion about. Descend, keeping whatever has accumulated. ──────
    if children.len() == 1 {
        return walk_with(tree, children[0], pending);
    }
    if children.is_empty() {
        return if pending.is_bare() {
            Vec::new()
        } else {
            vec![pending.spend("div")]
        };
    }
    let mut node = pending.spend("div");
    node.children = children.iter().flat_map(|c| walk(tree, *c)).collect();
    vec![node]
}

/// Emit the accumulated box, then start a fresh one for the property that did
/// not fit, and carry on into the children.
fn flush(
    tree: &ElementTree,
    children: &[ElementId],
    pending: Pending,
    set: impl FnOnce(&mut Pending),
) -> VNode {
    let mut next = Pending::default();
    set(&mut next);
    finish(tree, children, pending.spend("div"), next)
}

fn finish(tree: &ElementTree, children: &[ElementId], mut outer: VNode, inner: Pending) -> VNode {
    let mut node = inner.spend("div");
    node.children = children.iter().flat_map(|c| walk(tree, *c)).collect();
    outer.children = vec![node];
    outer
}

fn apply_constraints(pending: &mut Pending, c: Constraints) {
    if (c.min_width - c.max_width).abs() < 0.01 && c.max_width.is_finite() {
        pending.style.width = Some(c.max_width);
    } else {
        if c.min_width > 0.0 {
            pending.style.min_width = Some(c.min_width);
        }
        if c.max_width.is_finite() {
            pending.style.max_width = Some(c.max_width);
        }
    }
    if (c.min_height - c.max_height).abs() < 0.01 && c.max_height.is_finite() {
        pending.style.height = Some(c.max_height);
    } else {
        if c.min_height > 0.0 {
            pending.style.min_height = Some(c.min_height);
        }
        if c.max_height.is_finite() {
            pending.style.max_height = Some(c.max_height);
        }
    }
}

fn place(value: f32) -> &'static str {
    if value < -0.5 {
        "flex-start"
    } else if value > 0.5 {
        "flex-end"
    } else {
        "center"
    }
}

fn main_axis(alignment: MainAxisAlignment) -> &'static str {
    match alignment {
        MainAxisAlignment::Start => "flex-start",
        MainAxisAlignment::End => "flex-end",
        MainAxisAlignment::Center => "center",
        MainAxisAlignment::SpaceBetween => "space-between",
        MainAxisAlignment::SpaceAround => "space-around",
        MainAxisAlignment::SpaceEvenly => "space-evenly",
    }
}

fn cross_axis(alignment: CrossAxisAlignment) -> &'static str {
    match alignment {
        CrossAxisAlignment::Start => "flex-start",
        CrossAxisAlignment::End => "flex-end",
        CrossAxisAlignment::Center => "center",
        CrossAxisAlignment::Stretch => "stretch",
        CrossAxisAlignment::Baseline => "baseline",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vieww_embed::{WebContent, WebView};

    /// Walk a widget tree to its VNodes, the way `DomApp` does.
    fn walked(widget: impl Into<vieww_widget::WidgetNode>) -> Vec<VNode> {
        let mut tree = ElementTree::new();
        let root = tree.mount(widget);
        walk(&tree, root)
    }

    #[test]
    fn a_web_view_becomes_an_iframe() {
        let nodes = walked(WebView::new(WebContent::url("https://example.test/embed")));
        assert_eq!(nodes.len(), 1, "a leaf: the browser owns the subtree");
        let node = &nodes[0];
        assert_eq!(node.tag, "iframe");
        let iframe = node.iframe.as_ref().expect("the embed payload");
        assert_eq!(iframe.src.as_deref(), Some("https://example.test/embed"));
        assert!(iframe.srcdoc.is_none());
        assert!(
            node.css.contains("width:320"),
            "sized as asked: {}",
            node.css
        );
        assert!(
            node.label.is_none(),
            "the title rides the iframe payload, not the label slot"
        );
    }

    #[test]
    fn inline_html_takes_the_srcdoc_spelling() {
        let nodes = walked(WebView::new(WebContent::html("<strong>hi</strong>")));
        let iframe = nodes[0].iframe.as_ref().expect("the embed payload");
        assert!(iframe.src.is_none());
        assert_eq!(iframe.srcdoc.as_deref(), Some("<strong>hi</strong>"));
    }

    #[test]
    fn a_sized_embed_sizes_the_frame() {
        let nodes = walked(
            WebView::new(WebContent::url("https://example.test/"))
                .size(vieww_foundation::Size::new(640.0, 360.0)),
        );
        assert!(nodes[0].css.contains("width:640"), "{}", nodes[0].css);
        assert!(nodes[0].css.contains("height:360"), "{}", nodes[0].css);
    }

    #[test]
    fn an_iframe_node_is_comparable_for_patch_decisions() {
        // The patcher diffs `iframe` by equality; two embeds of one URL are
        // equal, an embed of another URL is not.
        let a = walked(WebView::new(WebContent::url("https://a.test/")));
        let b = walked(WebView::new(WebContent::url("https://a.test/")));
        let c = walked(WebView::new(WebContent::url("https://c.test/")));
        assert_eq!(a[0].iframe, b[0].iframe);
        assert_ne!(a[0].iframe, c[0].iframe);
    }
}
