//! Two-way data binding and view models — WPF's `{Binding Mode=TwoWay}`,
//! value converters, validation rules and `ICommand`; SwiftUI's `$binding`;
//! Rive's data-binding view models (§2.4, §2.9, §2.14).
//!
//! A [`Binding<T>`] is a get/set pair. Made from a [`Signal`] it reads
//! reactively (a widget that calls [`get`](Binding::get) in `build`
//! rebuilds when the source changes) and writes back through
//! [`set`](Binding::set) — that is two-way binding: a slider bound to
//! `volume` and a label bound to the same `volume` stay in step with no
//! glue. [`Binding::lens`] projects a field of a struct signal;
//! [`Binding::map`] converts in both directions (a WPF `IValueConverter`).
//!
//! [`TextBinding`] is the converter-plus-validation case every form field
//! needs: text in, parse, keep the last good value, surface the error.
//! [`Command`] is `ICommand`: an action with a reactive `can_execute`.
//!
//! [`ViewModel`] is a Rive/WPF-style bag of named, typed, observable
//! properties (number, text, bool, colour, enum, trigger, nested view
//! model), addressed by dotted paths, serialisable to and from JSON — the
//! contract a designer's file and the app's code agree on.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use vieww_foundation::json::Json;
use vieww_foundation::Color;

use crate::{Memo, Runtime, Signal};

/// A two-way binding.
pub struct Binding<T> {
    get: Rc<dyn Fn() -> T>,
    set: Rc<dyn Fn(T)>,
}

impl<T> Clone for Binding<T> {
    fn clone(&self) -> Self {
        Self {
            get: self.get.clone(),
            set: self.set.clone(),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Binding<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Binding").field(&(self.get)()).finish()
    }
}

impl<T: 'static> Binding<T> {
    /// From a getter and setter.
    pub fn new(get: impl Fn() -> T + 'static, set: impl Fn(T) + 'static) -> Self {
        Self {
            get: Rc::new(get),
            set: Rc::new(set),
        }
    }

    /// Bound to a signal (reads subscribe; writes notify).
    #[must_use]
    pub fn signal(s: &Signal<T>) -> Self
    where
        T: Clone,
    {
        let (a, b) = (s.clone(), s.clone());
        Self::new(move || a.get(), move |v| b.set(v))
    }

    /// A read-only constant (writes are ignored) — SwiftUI's `.constant`.
    #[must_use]
    pub fn constant(v: T) -> Self
    where
        T: Clone,
    {
        Self::new(move || v.clone(), |_| {})
    }

    #[must_use]
    pub fn get(&self) -> T {
        (self.get)()
    }

    pub fn set(&self, v: T) {
        (self.set)(v);
    }

    /// Read-modify-write.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        let mut v = self.get();
        f(&mut v);
        self.set(v);
    }

    /// Convert both ways.
    #[must_use]
    pub fn map<U: 'static>(&self, to: impl Fn(T) -> U + 'static, from: impl Fn(U) -> T + 'static) -> Binding<U> {
        let (g, s) = (self.get.clone(), self.set.clone());
        Binding::new(move || to(g()), move |u| s(from(u)))
    }

    /// Project a part of the value (`$user.name`).
    #[must_use]
    pub fn lens<U: 'static>(&self, get: impl Fn(&T) -> U + 'static, set: impl Fn(&mut T, U) + 'static) -> Binding<U> {
        let (g, g2, s) = (self.get.clone(), self.get.clone(), self.set.clone());
        Binding::new(
            move || get(&g()),
            move |u| {
                let mut whole = g2();
                set(&mut whole, u);
                s(whole);
            },
        )
    }
}

type Parser<T> = Rc<dyn Fn(&str) -> Result<T, String>>;

/// A text field's binding onto a typed value: parse on input, keep the last
/// valid value in the source, expose the error.
pub struct TextBinding<T: 'static> {
    source: Binding<T>,
    text: Signal<String>,
    error: Signal<Option<String>>,
    parse: Parser<T>,
    format: Rc<dyn Fn(&T) -> String>,
}

impl<T: 'static> fmt::Debug for TextBinding<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextBinding").field("text", &self.text.peek()).field("error", &self.error.peek()).finish()
    }
}

impl<T: 'static> TextBinding<T> {
    pub fn new(
        rt: &Runtime,
        source: Binding<T>,
        parse: impl Fn(&str) -> Result<T, String> + 'static,
        format: impl Fn(&T) -> String + 'static,
    ) -> Self {
        let text = rt.signal(format(&source.get()));
        Self {
            source,
            text,
            error: rt.signal(None),
            parse: Rc::new(parse),
            format: Rc::new(format),
        }
    }

    /// The user typed `s`.
    pub fn input(&self, s: &str) {
        self.text.set(s.to_owned());
        match (self.parse)(s) {
            Ok(v) => {
                self.source.set(v);
                self.error.set(None);
            }
            Err(e) => self.error.set(Some(e)),
        }
    }

    /// Re-read the source (the model changed underneath the field).
    pub fn refresh(&self) {
        self.text.set((self.format)(&self.source.get()));
        self.error.set(None);
    }

    #[must_use]
    pub fn text(&self) -> String {
        self.text.get()
    }

    #[must_use]
    pub fn error(&self) -> Option<String> {
        self.error.get()
    }
}

/// An action with a reactive enabled state — WPF's `ICommand`.
#[derive(Clone)]
pub struct Command {
    run: Rc<dyn Fn()>,
    can: Option<Memo<bool>>,
}

impl fmt::Debug for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Command").field("can_execute", &self.can_execute()).finish()
    }
}

impl Command {
    pub fn new(run: impl Fn() + 'static) -> Self {
        Self { run: Rc::new(run), can: None }
    }

    /// Enable only while `pred` holds (re-evaluated reactively).
    #[must_use]
    pub fn when(mut self, rt: &Runtime, pred: impl Fn() -> bool + 'static) -> Self {
        self.can = Some(rt.memo(pred));
        self
    }

    #[must_use]
    pub fn can_execute(&self) -> bool {
        self.can.as_ref().is_none_or(Memo::get)
    }

    /// Run if enabled; returns whether it ran.
    pub fn execute(&self) -> bool {
        if self.can_execute() {
            (self.run)();
            true
        } else {
            false
        }
    }
}

/// A view-model property value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f32),
    Text(String),
    Bool(bool),
    Color(Color),
    /// One of a fixed set of options.
    Enum { options: Vec<String>, selected: usize },
    /// Fire-and-forget; the count of firings.
    Trigger(u32),
}

impl Value {
    fn to_json(&self) -> Json {
        match self {
            Self::Number(n) => Json::Number(f64::from(*n)),
            Self::Text(s) => Json::String(s.clone()),
            Self::Bool(b) => Json::Bool(*b),
            Self::Color(c) => Json::String(format!("#{:02x}{:02x}{:02x}{:02x}", c.r, c.g, c.b, c.a)),
            Self::Enum { options, selected } => Json::String(options.get(*selected).cloned().unwrap_or_default()),
            Self::Trigger(n) => Json::Number(f64::from(*n)),
        }
    }

    fn assign_json(&mut self, j: &Json) -> bool {
        match (self, j) {
            (Self::Number(n), Json::Number(v)) => {
                #[allow(clippy::cast_possible_truncation)]
                {
                    *n = *v as f32;
                }
            }
            (Self::Text(s), Json::String(v)) => s.clone_from(v),
            (Self::Bool(b), Json::Bool(v)) => *b = *v,
            (Self::Color(c), Json::String(v)) => {
                let h = v.trim_start_matches('#');
                let byte = |i: usize| h.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok());
                match (byte(0), byte(2), byte(4)) {
                    (Some(r), Some(g), Some(b)) => *c = Color::rgba(r, g, b, byte(6).unwrap_or(255)),
                    _ => return false,
                }
            }
            (Self::Enum { options, selected }, Json::String(v)) => match options.iter().position(|o| o == v) {
                Some(i) => *selected = i,
                None => return false,
            },
            (Self::Trigger(_), _) => {}
            _ => return false,
        }
        true
    }
}

/// A named bag of observable properties, nestable.
#[derive(Clone)]
pub struct ViewModel {
    rt: Runtime,
    name: String,
    props: Rc<RefCell<BTreeMap<String, Signal<Value>>>>,
    children: Rc<RefCell<BTreeMap<String, ViewModel>>>,
}

impl fmt::Debug for ViewModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ViewModel({}) {}", self.name, self.to_json().pretty())
    }
}

impl ViewModel {
    #[must_use]
    pub fn new(rt: &Runtime, name: &str) -> Self {
        Self {
            rt: rt.clone(),
            name: name.to_owned(),
            props: Rc::default(),
            children: Rc::default(),
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Declare a property.
    #[must_use]
    pub fn with(self, key: &str, v: Value) -> Self {
        self.props.borrow_mut().insert(key.to_owned(), self.rt.signal(v));
        self
    }

    /// Declare a nested view model.
    #[must_use]
    pub fn nest(self, key: &str, child: Self) -> Self {
        self.children.borrow_mut().insert(key.to_owned(), child);
        self
    }

    fn resolve(&self, path: &str) -> Option<Signal<Value>> {
        match path.split_once('.') {
            Some((head, rest)) => self.children.borrow().get(head)?.resolve(rest),
            None => self.props.borrow().get(path).cloned(),
        }
    }

    /// The property at a dotted path (reactive read).
    #[must_use]
    pub fn get(&self, path: &str) -> Option<Value> {
        self.resolve(path).map(|s| s.get())
    }

    /// Set a property; false if absent or of a different kind.
    pub fn set(&self, path: &str, v: Value) -> bool {
        let Some(s) = self.resolve(path) else { return false };
        if std::mem::discriminant(&s.peek()) != std::mem::discriminant(&v) {
            return false;
        }
        s.set(v);
        true
    }

    /// Fire a trigger.
    pub fn fire(&self, path: &str) -> bool {
        let Some(s) = self.resolve(path) else { return false };
        let mut ok = false;
        s.update(|v| {
            if let Value::Trigger(n) = v {
                *n += 1;
                ok = true;
            }
        });
        ok
    }

    /// A two-way number binding (0 if absent).
    #[must_use]
    pub fn number(&self, path: &str) -> Binding<f32> {
        let s = self.resolve(path);
        let s2 = s.clone();
        Binding::new(
            move || match s.as_ref().map(Signal::get) {
                Some(Value::Number(n)) => n,
                _ => 0.0,
            },
            move |n| {
                if let Some(s) = &s2 {
                    s.set(Value::Number(n));
                }
            },
        )
    }

    /// A two-way text binding.
    #[must_use]
    pub fn text(&self, path: &str) -> Binding<String> {
        let s = self.resolve(path);
        let s2 = s.clone();
        Binding::new(
            move || match s.as_ref().map(Signal::get) {
                Some(Value::Text(t)) => t,
                _ => String::new(),
            },
            move |t| {
                if let Some(s) = &s2 {
                    s.set(Value::Text(t));
                }
            },
        )
    }

    /// A two-way bool binding.
    #[must_use]
    pub fn flag(&self, path: &str) -> Binding<bool> {
        let s = self.resolve(path);
        let s2 = s.clone();
        Binding::new(
            move || matches!(s.as_ref().map(Signal::get), Some(Value::Bool(true))),
            move |b| {
                if let Some(s) = &s2 {
                    s.set(Value::Bool(b));
                }
            },
        )
    }

    /// Current state as JSON (nested objects for nested models).
    #[must_use]
    pub fn to_json(&self) -> Json {
        let mut fields: Vec<(String, Json)> = self.props.borrow().iter().map(|(k, s)| (k.clone(), s.peek().to_json())).collect();
        fields.extend(self.children.borrow().iter().map(|(k, c)| (k.clone(), c.to_json())));
        Json::Object(fields)
    }

    /// Assign every matching field from JSON; returns the paths it could
    /// not apply (unknown or mistyped).
    pub fn apply_json(&self, j: &Json) -> Vec<String> {
        let mut rejected = Vec::new();
        let Some(obj) = j.as_object() else {
            return vec![self.name.clone()];
        };
        for (k, v) in obj {
            if let Some(child) = self.children.borrow().get(k) {
                rejected.extend(child.apply_json(v).into_iter().map(|p| format!("{k}.{p}")));
                continue;
            }
            let Some(s) = self.props.borrow().get(k).cloned() else {
                rejected.push(k.clone());
                continue;
            };
            let mut val = s.peek();
            if val.assign_json(v) {
                s.set(val);
            } else {
                rejected.push(k.clone());
            }
        }
        rejected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_way_binding_through_a_signal_notifies_both_sides() {
        let rt = Runtime::new();
        let volume = rt.signal(0.5f32);
        let slider = Binding::signal(&volume);
        let label = Binding::signal(&volume).map(|v| format!("{:.0}%", v * 100.0), |s: String| s.trim_end_matches('%').parse::<f32>().unwrap_or(0.0) / 100.0);
        assert_eq!(label.get(), "50%");
        slider.set(0.8);
        assert_eq!(label.get(), "80%");
        label.set("25%".into());
        assert!((slider.get() - 0.25).abs() < 1e-6);
        let doubled = rt.memo({
            let v = volume.clone();
            move || v.get() * 2.0
        });
        slider.set(0.4);
        rt.settle_memos();
        assert!((doubled.get() - 0.8).abs() < 1e-6, "a memo downstream of the bound signal recomputes");
    }

    #[test]
    fn lens_projects_a_field() {
        #[derive(Clone, Debug, PartialEq)]
        struct User {
            name: String,
            age: u32,
        }
        let rt = Runtime::new();
        let user = rt.signal(User { name: "Ada".into(), age: 36 });
        let name = Binding::signal(&user).lens(|u| u.name.clone(), |u, n| u.name = n);
        name.set("Grace".into());
        assert_eq!(user.peek(), User { name: "Grace".into(), age: 36 });
    }

    #[test]
    fn text_binding_validates() {
        let rt = Runtime::new();
        let age = rt.signal(30u32);
        let field = TextBinding::new(
            &rt,
            Binding::signal(&age),
            |s| s.parse::<u32>().map_err(|_| "enter a whole number".to_owned()).and_then(|v| if v > 150 { Err("too old".into()) } else { Ok(v) }),
            u32::to_string,
        );
        assert_eq!(field.text(), "30");
        field.input("4x");
        assert_eq!(field.error().as_deref(), Some("enter a whole number"));
        assert_eq!(age.peek(), 30, "invalid input never reaches the model");
        field.input("200");
        assert_eq!(field.error().as_deref(), Some("too old"));
        field.input("41");
        assert_eq!((age.peek(), field.error()), (41, None));
        age.set(7);
        field.refresh();
        assert_eq!(field.text(), "7");
    }

    #[test]
    fn command_can_execute_is_reactive() {
        let rt = Runtime::new();
        let text = rt.signal(String::new());
        let sent = Rc::new(RefCell::new(0));
        let s2 = sent.clone();
        let t2 = text.clone();
        let send = Command::new(move || *s2.borrow_mut() += 1).when(&rt, move || !t2.get().is_empty());
        assert!(!send.execute());
        text.set("hi".into());
        rt.settle_memos();
        assert!(send.execute());
        assert_eq!(*sent.borrow(), 1);
    }

    #[test]
    fn view_model_paths_bindings_triggers_and_json() {
        let rt = Runtime::new();
        let vm = ViewModel::new(&rt, "Player")
            .with("health", Value::Number(100.0))
            .with("name", Value::Text("Ada".into()))
            .with("shielded", Value::Bool(false))
            .with("tint", Value::Color(Color::rgb(255, 0, 0)))
            .with("mode", Value::Enum { options: vec!["idle".into(), "run".into()], selected: 0 })
            .with("hit", Value::Trigger(0))
            .nest("weapon", ViewModel::new(&rt, "Weapon").with("ammo", Value::Number(12.0)));
        let health = vm.number("health");
        health.update(|h| *h -= 30.0);
        assert_eq!(vm.get("health"), Some(Value::Number(70.0)));
        vm.number("weapon.ammo").set(11.0);
        assert_eq!(vm.get("weapon.ammo"), Some(Value::Number(11.0)));
        assert!(vm.fire("hit") && vm.fire("hit"));
        assert_eq!(vm.get("hit"), Some(Value::Trigger(2)));
        assert!(!vm.set("health", Value::Text("x".into())), "kinds are enforced");
        vm.flag("shielded").set(true);
        let j = vm.to_json();
        assert_eq!(j.get("weapon").and_then(|w| w.get("ammo")).and_then(Json::as_f32), Some(11.0));
        assert_eq!(j.get("tint").and_then(Json::as_str), Some("#ff0000ff"));
        let other = Json::parse(r##"{"health": 5, "mode": "run", "tint": "#00ff00", "weapon": {"ammo": 3, "bogus": 1}, "nope": true}"##).unwrap();
        let rejected = vm.apply_json(&other);
        assert_eq!(rejected, vec!["weapon.bogus".to_owned(), "nope".to_owned()]);
        assert_eq!(vm.get("mode"), Some(Value::Enum { options: vec!["idle".into(), "run".into()], selected: 1 }));
        assert_eq!(vm.get("tint"), Some(Value::Color(Color::rgb(0, 255, 0))));
        assert_eq!(vm.text("name").get(), "Ada");
    }
}
