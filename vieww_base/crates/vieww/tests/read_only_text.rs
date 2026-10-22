//! Read-only text a person can still select and copy, and a password field
//! that never gives its contents to the pasteboard.
//!
//! ```console
//! cargo test -p vieww --test read_only_text
//! ```
//!
//! Driven through the whole production path: a real `Services` registry
//! published above the root, the factory reading it, the render object
//! acting on it — only the platform's pasteboard is swapped for a
//! `MemoryClipboard`.

use std::rc::Rc;
use std::time::Duration;

use vieww::foundation::{
    Clipboard, KeyEvent, LogicalKey, MemoryClipboard, Modifiers, NamedKey, Offset, PointerEvent,
    PointerId, Services, SharedServices, Size, TargetPlatform, TextEditingValue,
};
use vieww::prelude::*;
use vieww::{FrameDriver, Role};

const SURFACE: Size = Size {
    width: 400.0,
    height: 300.0,
};

#[derive(Debug)]
struct Screen {
    value: Signal<TextEditingValue>,
    obscure: bool,
    read_only: bool,
}

impl Widget for Screen {
    fn debug_name(&self) -> &'static str {
        "Screen"
    }

    fn kind(&self) -> WidgetKind<'_> {
        WidgetKind::Composed
    }

    fn build(&self, _ctx: &BuildContext) -> WidgetNode {
        let apply = self.value.clone();
        let mut field = TextField::new(self.value.get())
            .size(16.0)
            .obscure(self.obscure)
            .on_changed(Rc::new(move |next| apply.set(next)));
        if self.read_only {
            field = field.read_only();
        }
        field.into()
    }
}

vieww::widget::widget_node_from!(Screen);

struct Harness {
    driver: FrameDriver,
    value: Signal<TextEditingValue>,
    clipboard: Rc<MemoryClipboard>,
}

impl Harness {
    fn new(text: &str, read_only: bool, obscure: bool) -> Self {
        let clipboard = Rc::new(MemoryClipboard::new());
        let mut registry = Services::new();
        registry.provide::<dyn Clipboard>(Rc::clone(&clipboard) as Rc<dyn Clipboard>);
        let mut driver = FrameDriver::new(SURFACE);
        let value = driver
            .elements()
            .runtime()
            .signal(TextEditingValue::new(text));
        driver.set_root(Inherited::new(
            SharedServices::new(registry),
            Screen {
                value: value.clone(),
                obscure,
                read_only,
            },
        ));
        driver.draw_frame();
        if read_only {
            // Text to read is not a Tab stop: a press is what focuses it.
            assert!(
                !driver.focus_next(true),
                "Tab must pass over read-only text"
            );
            let node = driver
                .semantics()
                .nodes()
                .iter()
                // Alone on screen, the text's node is folded into the
                // window's; find it by its words.
                .find(|n| n.label.as_deref() == Some(text))
                .expect("the text is in the tree")
                .bounds;
            let at = Offset::new(node.left + 4.0, node.top + node.height() / 2.0);
            driver.handle_pointer(&PointerEvent::down(PointerId(1), at, Duration::ZERO));
            driver.handle_pointer(&PointerEvent::up(
                PointerId(1),
                at,
                Duration::from_millis(40),
            ));
        } else {
            driver.focus_next(true);
        }
        driver.draw_frame();
        Self {
            driver,
            value,
            clipboard,
        }
    }

    fn key(&mut self, key: LogicalKey, modifiers: Modifiers) -> bool {
        let handled = self
            .driver
            .handle_key(&KeyEvent::down(key, Duration::ZERO).with_modifiers(modifiers));
        self.driver.draw_frame();
        handled
    }

    fn shortcut(&mut self, c: &str) -> bool {
        self.key(
            LogicalKey::Character(c.into()),
            Modifiers::shortcut_for(TargetPlatform::current()),
        )
    }

    fn pasteboard(&self) -> Option<String> {
        self.clipboard
            .read_text()
            .expect("a memory pasteboard cannot fail")
    }
}

#[test]
fn read_only_text_selects_and_copies() {
    let mut h = Harness::new("a reply worth keeping", true, false);
    assert!(h.shortcut("a"), "select all is the field's");
    assert!(h.shortcut("c"), "copy is the field's");
    assert_eq!(h.pasteboard().as_deref(), Some("a reply worth keeping"));
    assert_eq!(h.value.get().text, "a reply worth keeping");
}

#[test]
fn read_only_text_lets_editing_keys_through_to_the_app() {
    let mut h = Harness::new("hello", true, false);
    h.shortcut("a");
    // Typing, deleting, cutting and pasting are not the field's: they go on
    // to whatever is above it (the application's own shortcuts).
    assert!(!h.key(LogicalKey::Character("e".into()), Modifiers::NONE));
    assert!(!h.key(LogicalKey::Character(" ".into()), Modifiers::NONE));
    assert!(!h.key(LogicalKey::Named(NamedKey::Backspace), Modifiers::NONE));
    assert!(!h.shortcut("x"));
    h.clipboard.write_text("pasted").unwrap();
    assert!(!h.shortcut("v"));
    assert_eq!(h.value.get().text, "hello", "nothing changed the text");
}

#[test]
fn read_only_text_reads_as_text_not_as_a_field() {
    let h = Harness::new("hello", true, false);
    let semantics = h.driver.semantics();
    assert!(
        semantics.nodes().iter().all(|n| n.role != Role::TextField),
        "a screen reader must not offer to type into it"
    );
    assert!(semantics
        .nodes()
        .iter()
        .any(|n| n.label.as_deref() == Some("hello")));
}

#[test]
fn a_password_field_never_copies_or_cuts_its_contents() {
    let mut h = Harness::new("hunter2", false, true);
    h.clipboard.write_text("what was there").unwrap();
    h.shortcut("a");
    h.shortcut("c");
    assert_eq!(h.pasteboard().as_deref(), Some("what was there"));
    h.shortcut("x");
    assert_eq!(h.pasteboard().as_deref(), Some("what was there"));
    assert_eq!(
        h.value.get().text,
        "hunter2",
        "a refused cut deletes nothing"
    );
}

#[test]
fn an_editable_field_still_pastes() {
    let mut h = Harness::new("", false, false);
    h.clipboard.write_text("sk-pasted-key").unwrap();
    h.shortcut("v");
    assert_eq!(h.value.get().text, "sk-pasted-key");
}
