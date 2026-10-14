//! A WPF/Avalonia-style property system (§2.13 L3: "`StyledProperty<T>`,
//! `DirectProperty<T>`, `AttachedProperty<T>` — similar to WPF's
//! DependencyProperty"; L2's logical tree; styles with selectors).
//!
//! vieww's own state model is signals outside the tree; this module is the
//! other model, available for code ported from XAML frameworks and for
//! theming that wants CSS-like cascades:
//!
//! * a [`PropertyTree`] of nodes with a **type name**, **classes**, a name
//!   and a parent (the logical tree);
//! * [`Property`] keys declaring a default, whether the value **inherits**
//!   down the tree (FontSize, Foreground), and whether it is attached (owned
//!   by another type, settable on any node: `Grid.Row`);
//! * values with **priority** — local value > animation > style setter
//!   (by selector specificity, later styles winning ties) > inherited >
//!   default — exactly Avalonia's `BindingPriority` order;
//! * [`Style`]s with a [`Selector`] grammar (`Button`, `.primary`,
//!   `#ok`, `:pointerover`, descendant `A B` and child `A > B`
//!   combinators);
//! * change notification: every effective-value change is reported, so a
//!   binding or a render invalidation can follow it.

use std::collections::{BTreeMap, BTreeSet};

/// A property value (dynamically typed, as XAML's are).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Bool(bool),
    Text(String),
    Color(u32),
}

/// A declared property.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub name: &'static str,
    pub default: Value,
    pub inherits: bool,
    /// `Some(owner)` for attached properties (`Grid.Row`).
    pub owner: Option<&'static str>,
}

impl Property {
    #[must_use]
    pub const fn styled(name: &'static str, default: Value, inherits: bool) -> Self {
        Self {
            name,
            default,
            inherits,
            owner: None,
        }
    }
    #[must_use]
    pub const fn attached(owner: &'static str, name: &'static str, default: Value) -> Self {
        Self {
            name,
            default,
            inherits: false,
            owner: Some(owner),
        }
    }
    /// The key values are stored under.
    #[must_use]
    pub fn key(&self) -> String {
        match self.owner {
            Some(o) => format!("{o}.{}", self.name),
            None => self.name.to_owned(),
        }
    }
}

/// Where a value came from, highest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Default = 0,
    Inherited = 1,
    Style = 2,
    Animation = 3,
    Local = 4,
}

/// A node id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, Default)]
struct Node {
    type_name: String,
    name: Option<String>,
    classes: BTreeSet<String>,
    pseudo: BTreeSet<String>,
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    local: BTreeMap<String, Value>,
    animated: BTreeMap<String, Value>,
}

/// One compound selector step: type, `#name`, `.class`es, `:pseudo`s.
#[derive(Debug, Clone, PartialEq, Default)]
struct Compound {
    type_name: Option<String>,
    name: Option<String>,
    classes: Vec<String>,
    pseudo: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Comb {
    Descendant,
    Child,
}

/// A parsed selector.
#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    /// Rightmost (subject) last.
    steps: Vec<(Compound, Comb)>,
}

impl Selector {
    /// Parse `Button.primary:pointerover`, `StackPanel > TextBlock`,
    /// `#ok`, `Window .card Button`.
    ///
    /// # Errors
    /// An empty selector or a dangling combinator.
    pub fn parse(s: &str) -> Result<Self, String> {
        let mut steps = Vec::new();
        let mut comb = Comb::Descendant;
        for tok in s.replace('>', " > ").split_whitespace() {
            if tok == ">" {
                if steps.is_empty() {
                    return Err("selector starts with '>'".into());
                }
                comb = Comb::Child;
                continue;
            }
            let mut c = Compound::default();
            let mut cur = String::new();
            let mut kind = 't';
            let flush = |kind: char, cur: &mut String, c: &mut Compound| {
                if cur.is_empty() {
                    return;
                }
                match kind {
                    't' => c.type_name = Some(std::mem::take(cur)),
                    '#' => c.name = Some(std::mem::take(cur)),
                    '.' => c.classes.push(std::mem::take(cur)),
                    _ => c.pseudo.push(std::mem::take(cur)),
                }
            };
            for ch in tok.chars() {
                if matches!(ch, '#' | '.' | ':') {
                    flush(kind, &mut cur, &mut c);
                    kind = ch;
                } else {
                    cur.push(ch);
                }
            }
            flush(kind, &mut cur, &mut c);
            steps.push((c, comb));
            comb = Comb::Descendant;
        }
        if steps.is_empty() || comb == Comb::Child {
            return Err(format!("bad selector '{s}'"));
        }
        Ok(Self { steps })
    }

    /// CSS specificity: (ids, classes + pseudo, types).
    #[must_use]
    pub fn specificity(&self) -> (u32, u32, u32) {
        let mut sp = (0, 0, 0);
        for (c, _) in &self.steps {
            sp.0 += u32::from(c.name.is_some());
            sp.1 += (c.classes.len() + c.pseudo.len()) as u32;
            sp.2 += u32::from(c.type_name.is_some());
        }
        sp
    }
}

/// A style: a selector and its setters.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub selector: Selector,
    pub setters: Vec<(String, Value)>,
}

impl Style {
    /// # Errors
    /// A bad selector.
    pub fn new(selector: &str, setters: &[(&Property, Value)]) -> Result<Self, String> {
        Ok(Self {
            selector: Selector::parse(selector)?,
            setters: setters.iter().map(|(p, v)| (p.key(), v.clone())).collect(),
        })
    }
}

/// An effective-value change.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub node: NodeId,
    pub property: String,
    pub old: Value,
    pub new: Value,
}

/// The logical tree with its property store. See the [module docs](self).
#[derive(Debug, Clone, Default)]
pub struct PropertyTree {
    nodes: Vec<Node>,
    properties: BTreeMap<String, Property>,
    styles: Vec<Style>,
    /// Last computed effective values, for change detection.
    cache: BTreeMap<(NodeId, String), Value>,
}

impl PropertyTree {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, p: &Property) {
        self.properties.insert(p.key(), p.clone());
    }

    pub fn add_style(&mut self, s: Style) -> Vec<Change> {
        self.styles.push(s);
        self.refresh()
    }

    /// Add a node of `type_name` under `parent`.
    pub fn add(&mut self, type_name: &str, parent: Option<NodeId>) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node {
            type_name: type_name.to_owned(),
            parent,
            ..Node::default()
        });
        if let Some(p) = parent {
            self.nodes[p.0].children.push(id);
        }
        self.refresh();
        id
    }

    pub fn set_name(&mut self, n: NodeId, name: &str) -> Vec<Change> {
        self.nodes[n.0].name = Some(name.to_owned());
        self.refresh()
    }

    pub fn add_class(&mut self, n: NodeId, class: &str) -> Vec<Change> {
        self.nodes[n.0].classes.insert(class.to_owned());
        self.refresh()
    }

    pub fn remove_class(&mut self, n: NodeId, class: &str) -> Vec<Change> {
        self.nodes[n.0].classes.remove(class);
        self.refresh()
    }

    /// Set or clear a pseudo-class (`:pointerover`, `:pressed`, `:focus`).
    pub fn set_pseudo(&mut self, n: NodeId, pseudo: &str, on: bool) -> Vec<Change> {
        if on {
            self.nodes[n.0].pseudo.insert(pseudo.to_owned());
        } else {
            self.nodes[n.0].pseudo.remove(pseudo);
        }
        self.refresh()
    }

    /// Set a local value (highest priority).
    pub fn set(&mut self, n: NodeId, p: &Property, v: Value) -> Vec<Change> {
        self.nodes[n.0].local.insert(p.key(), v);
        self.refresh()
    }

    pub fn clear(&mut self, n: NodeId, p: &Property) -> Vec<Change> {
        self.nodes[n.0].local.remove(&p.key());
        self.refresh()
    }

    /// Set an animated value (above styles, below local).
    pub fn animate(&mut self, n: NodeId, p: &Property, v: Option<Value>) -> Vec<Change> {
        match v {
            Some(v) => {
                self.nodes[n.0].animated.insert(p.key(), v);
            }
            None => {
                self.nodes[n.0].animated.remove(&p.key());
            }
        }
        self.refresh()
    }

    fn compound_matches(&self, n: NodeId, c: &Compound) -> bool {
        let node = &self.nodes[n.0];
        c.type_name
            .as_ref()
            .is_none_or(|t| *t == node.type_name || t == "*")
            && c.name
                .as_ref()
                .is_none_or(|nm| node.name.as_ref() == Some(nm))
            && c.classes.iter().all(|cl| node.classes.contains(cl))
            && c.pseudo.iter().all(|ps| node.pseudo.contains(ps))
    }

    /// Does `sel` match node `n`?
    #[must_use]
    pub fn matches(&self, n: NodeId, sel: &Selector) -> bool {
        fn go(t: &PropertyTree, n: NodeId, steps: &[(Compound, Comb)]) -> bool {
            let Some(((c, _), rest)) = steps.split_last() else {
                return true;
            };
            if !t.compound_matches(n, c) {
                return false;
            }
            if rest.is_empty() {
                return true;
            }
            // The combinator recorded on *this* step relates it to the one
            // before it.
            let comb = steps[steps.len() - 1].1;
            let mut p = t.nodes[n.0].parent;
            match comb {
                Comb::Child => p.is_some_and(|p| go(t, p, rest)),
                Comb::Descendant => {
                    while let Some(a) = p {
                        if go(t, a, rest) {
                            return true;
                        }
                        p = t.nodes[a.0].parent;
                    }
                    false
                }
            }
        }
        go(self, n, &sel.steps)
    }

    /// The effective value and where it came from.
    #[must_use]
    pub fn get_with_priority(&self, n: NodeId, p: &Property) -> (Value, Priority) {
        let key = p.key();
        let node = &self.nodes[n.0];
        if let Some(v) = node.local.get(&key) {
            return (v.clone(), Priority::Local);
        }
        if let Some(v) = node.animated.get(&key) {
            return (v.clone(), Priority::Animation);
        }
        let mut best: Option<((u32, u32, u32), usize, Value)> = None;
        for (i, s) in self.styles.iter().enumerate() {
            if let Some((_, v)) = s.setters.iter().find(|(k, _)| *k == key) {
                if self.matches(n, &s.selector) {
                    let sp = s.selector.specificity();
                    if best
                        .as_ref()
                        .is_none_or(|(bsp, bi, _)| (sp, i) >= (*bsp, *bi))
                    {
                        best = Some((sp, i, v.clone()));
                    }
                }
            }
        }
        if let Some((_, _, v)) = best {
            return (v, Priority::Style);
        }
        if p.inherits {
            if let Some(parent) = node.parent {
                let (v, _) = self.get_with_priority(parent, p);
                return (v, Priority::Inherited);
            }
        }
        (p.default.clone(), Priority::Default)
    }

    #[must_use]
    pub fn get(&self, n: NodeId, p: &Property) -> Value {
        self.get_with_priority(n, p).0
    }

    /// Recompute every registered property everywhere; report changes.
    fn refresh(&mut self) -> Vec<Change> {
        let mut changes = Vec::new();
        let props: Vec<Property> = self.properties.values().cloned().collect();
        for i in 0..self.nodes.len() {
            for p in &props {
                let v = self.get(NodeId(i), p);
                let k = (NodeId(i), p.key());
                match self.cache.get(&k) {
                    Some(old) if *old == v => {}
                    Some(old) => {
                        changes.push(Change {
                            node: NodeId(i),
                            property: p.key(),
                            old: old.clone(),
                            new: v.clone(),
                        });
                        self.cache.insert(k, v);
                    }
                    None => {
                        self.cache.insert(k, v);
                    }
                }
            }
        }
        changes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONT_SIZE: Property = Property::styled("FontSize", Value::Number(14.0), true);
    const BACKGROUND: Property = Property::styled("Background", Value::Color(0xFFFF_FFFF), false);
    const ROW: Property = Property::attached("Grid", "Row", Value::Number(0.0));

    fn tree() -> (PropertyTree, NodeId, NodeId, NodeId) {
        let mut t = PropertyTree::new();
        t.register(&FONT_SIZE);
        t.register(&BACKGROUND);
        t.register(&ROW);
        let win = t.add("Window", None);
        let panel = t.add("StackPanel", Some(win));
        let button = t.add("Button", Some(panel));
        (t, win, panel, button)
    }

    #[test]
    fn inherited_values_flow_down_and_local_values_win() {
        let (mut t, win, _, button) = tree();
        assert_eq!(t.get(button, &FONT_SIZE), Value::Number(14.0));
        t.set(win, &FONT_SIZE, Value::Number(20.0));
        assert_eq!(
            t.get_with_priority(button, &FONT_SIZE),
            (Value::Number(20.0), Priority::Inherited)
        );
        // Non-inheriting properties do not flow.
        t.set(win, &BACKGROUND, Value::Color(1));
        assert_eq!(t.get(button, &BACKGROUND), Value::Color(0xFFFF_FFFF));
        t.set(button, &FONT_SIZE, Value::Number(9.0));
        assert_eq!(t.get(button, &FONT_SIZE), Value::Number(9.0));
    }

    #[test]
    fn styles_cascade_by_specificity_then_order() {
        let (mut t, _, _, button) = tree();
        t.add_style(Style::new("Button", &[(&BACKGROUND, Value::Color(1))]).expect("style"));
        t.add_style(
            Style::new("Button.primary", &[(&BACKGROUND, Value::Color(2))]).expect("style"),
        );
        t.add_style(Style::new("Button", &[(&BACKGROUND, Value::Color(3))]).expect("style"));
        assert_eq!(t.get(button, &BACKGROUND), Value::Color(3));
        t.add_class(button, "primary");
        assert_eq!(t.get(button, &BACKGROUND), Value::Color(2));
        t.add_style(Style::new("#ok", &[(&BACKGROUND, Value::Color(4))]).expect("style"));
        t.set_name(button, "ok");
        assert_eq!(t.get(button, &BACKGROUND), Value::Color(4));
    }

    #[test]
    fn pseudo_classes_and_combinators_select() {
        let (mut t, _, panel, button) = tree();
        t.add_style(
            Style::new("Button:pointerover", &[(&BACKGROUND, Value::Color(7))]).expect("s"),
        );
        t.add_style(Style::new("Window Button", &[(&FONT_SIZE, Value::Number(30.0))]).expect("s"));
        t.add_style(
            Style::new("Window > Button", &[(&FONT_SIZE, Value::Number(99.0))]).expect("s"),
        );
        assert_eq!(
            t.get(button, &FONT_SIZE),
            Value::Number(30.0),
            "descendant, not child"
        );
        let ch = t.set_pseudo(button, "pointerover", true);
        assert!(ch
            .iter()
            .any(|c| c.property == "Background" && c.new == Value::Color(7)));
        assert_eq!(t.get(button, &BACKGROUND), Value::Color(7));
        let direct = t.add("Button", Some(NodeId(0)));
        assert_eq!(t.get(direct, &FONT_SIZE), Value::Number(99.0));
        let _ = panel;
    }

    #[test]
    fn animation_sits_between_local_and_style() {
        let (mut t, _, _, button) = tree();
        t.add_style(Style::new("Button", &[(&BACKGROUND, Value::Color(1))]).expect("s"));
        t.animate(button, &BACKGROUND, Some(Value::Color(5)));
        assert_eq!(
            t.get_with_priority(button, &BACKGROUND).1,
            Priority::Animation
        );
        t.set(button, &BACKGROUND, Value::Color(9));
        assert_eq!(t.get(button, &BACKGROUND), Value::Color(9));
        t.clear(button, &BACKGROUND);
        t.animate(button, &BACKGROUND, None);
        assert_eq!(t.get(button, &BACKGROUND), Value::Color(1));
    }

    #[test]
    fn attached_properties_live_on_any_node_under_their_owner_key() {
        let (mut t, _, panel, button) = tree();
        assert_eq!(ROW.key(), "Grid.Row");
        t.set(button, &ROW, Value::Number(2.0));
        assert_eq!(t.get(button, &ROW), Value::Number(2.0));
        assert_eq!(t.get(panel, &ROW), Value::Number(0.0));
    }

    #[test]
    fn changes_are_reported_once_per_effective_change() {
        let (mut t, win, _, button) = tree();
        let ch = t.set(win, &FONT_SIZE, Value::Number(16.0));
        // Window, panel and button all change effective FontSize.
        assert_eq!(ch.iter().filter(|c| c.property == "FontSize").count(), 3);
        let again = t.set(win, &FONT_SIZE, Value::Number(16.0));
        assert!(again.is_empty());
        let _ = button;
    }

    #[test]
    fn bad_selectors_are_refused() {
        assert!(Selector::parse("").is_err());
        assert!(Selector::parse("> Button").is_err());
        assert!(Selector::parse("Window >").is_err());
        assert_eq!(
            Selector::parse("Button.a.b:hover#x")
                .expect("ok")
                .specificity(),
            (1, 3, 1)
        );
    }
}
