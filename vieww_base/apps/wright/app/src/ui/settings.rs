//! Settings: what the agents need (models and keys), defaults, the look,
//! the workspace, and the open project's own limits.
//!
//! ```text
//!  Models & keys   the built-in models (download, remove, fall back to them),
//!                  providers, their models, key Ready / Missing / Not needed
//!  Defaults        the interview model, the voice model
//!  Appearance      dark / light, any accent colour
//!  Network         office machines: an extra certificate file (PEM) for
//!                  networks that inspect HTTPS, environment variables
//!                  (proxies, mirrors), and what is in effect right now
//!  Fleet           everything this computer runs on its own: state, next
//!                  run, last run, time to trust; pause, resume, approve;
//!                  the kill switch
//!  Organization    the signed organization policy, this project against
//!                  it, administrator approval, every receipt verified
//!  Workspace       where new projects go, background schedules
//!  This project    model tiers and spending limits (the blueprint's own)
//! ```
//!
//! Global settings are `~/.wright/config.kdl` (written through
//! `Config::save_file`, which keeps a `.bak`). Keys go to the OS keychain,
//! never to a file. Project settings are the blueprint: saving writes
//! `blueprint.kdl` in canonical form and records the version in history,
//! exactly as an edit by hand would be.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use vieww::prelude::*;
use wright_runtime::config::{Config, ProviderEntry, ProviderKind};
use wright_runtime::local::{self, BuiltinModel};

use super::kit::*;
use super::session::WakeHandle;
use super::theme::{self, Mode, Pal};
use super::Ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Models,
    AgentsTools,
    Defaults,
    Appearance,
    Network,
    Fleet,
    Organization,
    Workspace,
    Project,
}

impl Tab {
    const ALL: [Tab; 9] = [Tab::Models, Tab::AgentsTools, Tab::Defaults, Tab::Appearance, Tab::Network, Tab::Fleet, Tab::Organization, Tab::Workspace, Tab::Project];
    fn label(self) -> &'static str {
        match self {
            Tab::Models => "Models & keys",
            Tab::AgentsTools => "Agents & tools",
            Tab::Defaults => "Defaults",
            Tab::Appearance => "Appearance",
            Tab::Network => "Network",
            Tab::Fleet => "Fleet",
            Tab::Organization => "Organization",
            Tab::Workspace => "Workspace",
            Tab::Project => "This project",
        }
    }
}

/// Key state per provider, read once when Settings opens (reading a
/// keychain runs a process; not something to do every frame).
#[derive(Debug, Clone, PartialEq)]
pub enum KeyState {
    Ready,
    Missing(String),
    NotNeeded,
}

/// A built-in model download started from Settings.
#[derive(Debug, Clone, PartialEq)]
pub enum Pull {
    Working { line: String, percent: u32 },
    Done,
    Failed(String),
}

/// A file-dialog answer.
type Picked = Result<Vec<std::path::PathBuf>, String>;

/// The form state Settings edits.
#[derive(Clone)]
pub struct Form {
    /// Downloads by model id, as the UI shows them…
    pub pulls: Signal<BTreeMap<String, Pull>>,
    /// …and as the download threads report them (copied over in `pump`).
    pulled: Arc<Mutex<BTreeMap<String, Pull>>>,
    pub tab: Signal<Tab>,
    pub config: Signal<Option<Config>>,
    pub keys: Signal<Vec<(String, KeyState)>>,
    pub key_for: Signal<Option<String>>,
    pub key_value: Signal<TextEditingValue>,
    pub new_name: Signal<TextEditingValue>,
    pub new_base: Signal<TextEditingValue>,
    pub new_models: Signal<TextEditingValue>,
    pub new_anthropic: Signal<bool>,
    pub interview: Signal<TextEditingValue>,
    pub voice: Signal<TextEditingValue>,
    pub projects_dir: Signal<TextEditingValue>,
    pub tiers: Signal<[TextEditingValue; 4]>,
    pub per_run: Signal<TextEditingValue>,
    pub per_day: Signal<TextEditingValue>,
    /// Network: the certificate file path being edited.
    pub ca_path: Signal<TextEditingValue>,
    /// Network: a new environment variable.
    pub env_name: Signal<TextEditingValue>,
    pub env_value: Signal<TextEditingValue>,
    /// A file chosen in the system dialog, from its worker thread.
    picked: Arc<Mutex<Option<Picked>>>,
    /// Fleet: every scheduled project, read when Settings opens.
    pub fleet: Signal<Vec<wright_engine::fleet::Row>>,
    /// Organization: the policy's sentences, or why it cannot be trusted.
    pub policy: Signal<Option<Result<Vec<String>, String>>>,
    /// Organization: the last fleet-wide verification.
    pub verified: Signal<Option<Vec<wright_engine::org::RunCheck>>>,
    pub message: Signal<String>,
    pub scroll: ScrollState,
}

fn tv(s: impl Into<String>) -> TextEditingValue {
    TextEditingValue::new(s.into())
}

impl Form {
    pub fn new(rt: &vieww::element::Runtime) -> Form {
        Form {
            pulls: rt.signal(BTreeMap::new()),
            pulled: Arc::new(Mutex::new(BTreeMap::new())),
            tab: rt.signal(Tab::Models),
            config: rt.signal(None),
            keys: rt.signal(Vec::new()),
            key_for: rt.signal(None),
            key_value: rt.signal(tv("")),
            new_name: rt.signal(tv("")),
            new_base: rt.signal(tv("")),
            new_models: rt.signal(tv("")),
            new_anthropic: rt.signal(false),
            interview: rt.signal(tv("")),
            voice: rt.signal(tv("")),
            projects_dir: rt.signal(tv("")),
            tiers: rt.signal([tv(""), tv(""), tv(""), tv("")]),
            per_run: rt.signal(tv("")),
            per_day: rt.signal(tv("")),
            ca_path: rt.signal(tv("")),
            env_name: rt.signal(tv("")),
            env_value: rt.signal(tv("")),
            picked: Arc::new(Mutex::new(None)),
            fleet: rt.signal(Vec::new()),
            policy: rt.signal(None),
            verified: rt.signal(None),
            message: rt.signal(String::new()),
            scroll: ScrollState::new(rt),
        }
    }

    /// Fill the form from disk (called when Settings opens, and after a save).
    pub fn reload(&self, project: Option<&Path>) {
        let merged = Config::load().unwrap_or_else(|_| Config::builtin());
        let keys = merged
            .providers
            .iter()
            .map(|p| {
                let st = match &p.key {
                    None => KeyState::NotNeeded,
                    Some(r) => match wright_runtime::secrets::resolve(r) {
                        Ok(_) => KeyState::Ready,
                        Err(e) => KeyState::Missing(e),
                    },
                };
                (p.name.clone(), st)
            })
            .collect();
        self.keys.set(keys);
        self.interview.set(tv(merged.interview_model.clone()));
        self.voice.set(tv(merged.voice_model.clone()));
        self.config.set(Some(merged));
        self.projects_dir.set(tv(super::store::projects_dir().display().to_string()));
        self.ca_path.set(tv(Config::load_file().ok().and_then(|c| c.ca_file).unwrap_or_default()));
        self.fleet.set(wright_engine::fleet::rows());
        self.policy.set(match wright_runtime::policy::load() {
            Ok(Some(p)) => Some(Ok(p.sentences())),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        });
        if let Some(bp) = project.and_then(|p| wright_engine::load_blueprint(p).ok()) {
            let m = &bp.models;
            self.tiers.set([tv(m.cheap.prefer.clone()), tv(m.cheap.fallback.clone()), tv(m.quality.prefer.clone()), tv(m.quality.fallback.clone())]);
            self.per_run.set(tv(format!("{:.2}", bp.guardrails.per_firing_micro_usd as f64 / 1e6)));
            self.per_day.set(tv(format!("{:.2}", bp.guardrails.daily_micro_usd as f64 / 1e6)));
        }
    }

    /// Download a built-in model in the background.
    pub fn pull(&self, m: &'static BuiltinModel) {
        if matches!(self.pulls.get().get(m.id), Some(Pull::Working { .. })) {
            return;
        }
        let shared = Arc::clone(&self.pulled);
        let set = move |state: Pull| {
            shared.lock().unwrap_or_else(|p| p.into_inner()).insert(m.id.to_string(), state);
        };
        set(Pull::Working { line: format!("Starting the download of {}…", m.id), percent: 0 });
        self.pump();
        let wake = WakeHandle::current();
        std::thread::spawn(move || {
            let mut last = u32::MAX;
            let r = local::pull(m, &mut |p| {
                if p.percent() != last {
                    last = p.percent();
                    set(Pull::Working { line: local::progress_line(m, p), percent: p.percent() });
                    wake.wake();
                }
            });
            set(match r {
                Ok(_) => Pull::Done,
                Err(e) => Pull::Failed(e),
            });
            wake.wake();
        });
    }

    /// Bring download progress from the worker threads into view.
    pub fn pump(&self) {
        let now = self.pulled.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if now != self.pulls.get() {
            self.pulls.set(now);
        }
        if let Some(r) = self.picked.lock().unwrap_or_else(|p| p.into_inner()).take() {
            match r {
                Ok(paths) => {
                    if let Some(first) = paths.first() {
                        self.ca_path.set(tv(first.display().to_string()));
                        self.message.set("Chosen. Save to start using it.".into());
                    }
                }
                Err(e) => self.message.set(e),
            }
        }
    }

    /// Ask the system's file dialog for the certificate file.
    pub fn choose_ca_file(&self) {
        let (slot, wake) = (Arc::clone(&self.picked), WakeHandle::current());
        std::thread::spawn(move || {
            let r = super::picker::pick(false);
            *slot.lock().unwrap_or_else(|p| p.into_inner()) = Some(r);
            wake.wake();
        });
    }

    /// Change the file config (no built-ins) and save it.
    fn edit_file(&self, change: impl FnOnce(&mut Config)) {
        let result = Config::load_file().and_then(|mut c| {
            change(&mut c);
            c.save_file()
        });
        match result {
            Ok(path) => self.message.set(format!("Saved to {}", path.display())),
            Err(e) => self.message.set(format!("Not saved: {e}")),
        }
    }
}

// ── the page ─────────────────────────────────────────────────────

pub fn page(ui: &Ui) -> WidgetNode {
    let ui = ui.clone();
    Live::new("settings", move || {
        let p = ui.pal();
        let f = ui.form.clone();
        let tab = f.tab.get();
        let tabs: Vec<WidgetNode> = Tab::ALL
            .iter()
            .map(|&tb| {
                let set = f.tab.clone();
                let on = tb == tab;
                Pressable::sensed(move |s: Sense| {
                    Container::new()
                        .color(if on { p.accent_soft } else { p.surface.lerp(p.surface2, s.hover) })
                        .radius(corner::SMALL)
                        .padding(EdgeInsets::symmetric(12.0, 8.0))
                        .child(t(tb.label(), 13.5, if on { p.ink } else { p.muted }))
                        .into()
                })
                .on_tap({
                    let top = f.scroll.offset.clone();
                    move || {
                        set.set(tb);
                        top.set(0.0);
                    }
                })
                .into()
            })
            .collect();
        let body = match tab {
            Tab::Models => models(&ui, &p),
            Tab::AgentsTools => agents_and_tools(&ui, &p),
            Tab::Defaults => defaults(&ui, &p),
            Tab::Appearance => appearance(&ui, &p),
            Tab::Network => network(&ui, &p),
            Tab::Fleet => fleet(&ui, &p),
            Tab::Organization => organization(&ui, &p),
            Tab::Workspace => workspace(&ui, &p),
            Tab::Project => project(&ui, &p),
        };
        let close = ui.settings.clone();
        let msg = f.message.get();
        Container::new()
            .color(p.surface)
            .radius(corner::PANEL)
            .border(Border::new(p.line_strong, 1.0))
            .shadow(Shadow::new(Color::rgba(0, 0, 0, 90), Offset::new(0.0, 20.0), 60.0))
            .padding(EdgeInsets::all(22.0))
            .child(stretch(16.0, vec![
                row(10.0, vec![Glyph::Gear.draw(20.0, p.accent_ink), b("Settings", 20.0, p.ink), gap(), t("Esc closes", 12.0, p.muted), icon_button(&p, Glyph::Close, "Close settings", false, move || close.set(false))]),
                grow(Flex::row().cross_axis_alignment(CrossAxisAlignment::Start).spacing(22.0).children(vec![
                    SizedBox::width(170.0).child(column(4.0, tabs)).into(),
                    grow(scroll_area(&ui, &f.scroll, pad(EdgeInsets::only(0.0, 0.0, 14.0, 0.0), body))),
                ])),
                if msg.is_empty() { SizedBox::shrink().into() } else { copyable(msg, 12.5, p.muted) },
            ]))
            .into()
    })
    .into()
}

fn field(ui: &Ui, p: &Pal, label: &str, value: &Signal<TextEditingValue>, placeholder: &str, secret: bool) -> WidgetNode {
    let v = value.clone();
    // Stretch, not start: a text box fills the room it was given (a box
    // that shrinks to the placeholder's width cuts off what is typed).
    stretch(5.0, vec![
        t(label.to_string(), 12.0, p.muted),
        Container::new()
            .color(p.surface2)
            .radius(corner::FIELD)
            .border(Border::new(p.line_strong, 1.0))
            .padding(EdgeInsets::symmetric(12.0, 8.0))
            .child(
                TextField::new(value.get())
                    .show_cursor(ui.is_typing_in(placeholder))
                    .single_line()
                    .obscure(secret)
                    .placeholder(placeholder.to_string())
                    .placeholder_color(p.muted)
                    .size(13.5)
                    .color(p.ink)
                    .cursor(p.accent_ink, 2.0)
                    .on_changed(std::rc::Rc::new(move |x: TextEditingValue| v.set(x))),
            )
            .into(),
    ])
}

fn section(p: &Pal, title: &str, sub: &str) -> WidgetNode {
    column(4.0, vec![b(title.to_string(), 15.0, p.ink), para(sub.to_string(), 12.5, p.muted)])
}

/// The built-in models: what they are, whether they are here, and the
/// switch for falling back to them.
fn builtin(ui: &Ui, p: &Pal, cfg: &Config) -> Vec<WidgetNode> {
    let f = &ui.form;
    let pulls = f.pulls.get();
    let mut rows = vec![section(
        p,
        "Built-in models",
        "Two small open models Wright runs on this machine: no key, no account, nothing leaves it. A project with no key set up runs on them; voice uses the built-in Whisper unless you choose another model.",
    )];
    for m in &local::CATALOG {
        let installed = m.installed();
        let pull = pulls.get(m.id).cloned();
        let badge = match (&pull, installed) {
            (Some(Pull::Working { percent, .. }), _) => pill(format!("Downloading {percent}%"), p.accent_ink, p.accent_soft),
            (_, true) => pill("Ready", p.good, p.good_soft),
            (Some(Pull::Failed(_)), _) => pill("Download failed", p.wait, p.wait_soft),
            _ if m.partial() > 0 => pill(format!("{} of {}", local::human_bytes(m.partial()), m.size_text()), p.wait, p.wait_soft),
            _ => pill("Downloads on first use", p.muted, p.surface2),
        };
        let mut actions = Vec::new();
        let working = matches!(pull, Some(Pull::Working { .. }));
        if !installed {
            let form = f.clone();
            actions.push(button(p, if m.partial() > 0 { "Resume download" } else { "Download now" }, Look::Secondary, !working, move || form.pull(m)));
        } else {
            let (form, project) = (f.clone(), ui.current_session().map(|s| (*s.project).clone()));
            actions.push(button(p, "Remove", Look::Quiet, true, move || {
                match local::remove(m) {
                    Ok(()) => form.message.set(format!("Removed {}. It downloads again when it is next needed.", m.id)),
                    Err(e) => form.message.set(format!("Not removed: {e}")),
                }
                form.pulled.lock().unwrap_or_else(|p| p.into_inner()).remove(m.id);
                form.pump();
                form.reload(project.as_deref());
            }));
        }
        let kind = if m.kind == local::Kind::Text { "text and tool calls" } else { "speech to text" };
        let mut block = vec![
            row(10.0, vec![b(m.id.to_string(), 14.0, p.ink), badge, gap(), row(8.0, actions)]),
            para(m.about.to_string(), 12.5, p.muted),
            mono(format!("{kind}  ·  {}  ·  {}  ·  sha256 {}…", m.size_text(), m.license, &m.sha256[..12]), 11.5, p.muted),
        ];
        match &pull {
            Some(Pull::Working { line, percent }) => {
                block.push(progress_bar(p, *percent as f32 / 100.0));
                block.push(t(line.clone(), 11.5, p.muted));
            }
            Some(Pull::Failed(e)) => block.push(copyable(e.clone(), 11.5, p.wait)),
            _ => {}
        }
        rows.push(Container::new().color(p.surface2.with_alpha(110)).radius(corner::CARD).border(Border::new(p.line, 1.0)).padding(EdgeInsets::all(14.0)).child(stretch(6.0, block)).into());
    }
    let on = cfg.falls_back_to_builtin();
    let (form_on, form_off) = (f.clone(), f.clone());
    let project = ui.current_session().map(|s| (*s.project).clone());
    let project2 = project.clone();
    rows.push(row(10.0, vec![
        t("When a model has no key:", 12.5, p.ink),
        button(p, "Use the built-in model", if on { Look::Primary } else { Look::Secondary }, true, move || {
            form_on.edit_file(|c| c.builtin_fallback = Some(true));
            form_on.reload(project.as_deref());
        }),
        button(p, "Stop and ask for a key", if on { Look::Secondary } else { Look::Primary }, true, move || {
            form_off.edit_file(|c| c.builtin_fallback = Some(false));
            form_off.reload(project2.as_deref());
        }),
    ]));
    rows
}

/// A thin bar, `done` of the way across.
fn progress_bar(p: &Pal, done: f32) -> WidgetNode {
    let done = done.clamp(0.0, 1.0);
    Container::new()
        .color(p.surface2)
        .radius(corner::SMALL)
        .height(6.0)
        .child(Flex::row().children(vec![
            Flexible::expanded((done * 1000.0).round().max(1.0) as u16).child(Container::new().color(p.accent).radius(corner::SMALL).height(6.0)).into(),
            Flexible::expanded(((1.0 - done) * 1000.0).round().max(1.0) as u16).child(SizedBox::shrink()).into(),
        ]))
        .into()
}

fn models(ui: &Ui, p: &Pal) -> WidgetNode {
    let f = &ui.form;
    let cfg = f.config.get().unwrap_or_else(Config::builtin);
    let mine: Vec<String> = Config::load_file().map(|c| c.providers.iter().map(|p| p.name.clone()).collect()).unwrap_or_default();
    let keys = f.keys.get();
    let editing = f.key_for.get();
    let mut seen = Vec::new();
    let mut rows = builtin(ui, p, &cfg);
    rows.push(divider(p));
    rows.push(section(p, "Model providers", "Agents use the first provider whose models match. Keys live in your system's keychain; Wright never writes them to a file."));
    for prov in &cfg.providers {
        if seen.contains(&prov.name) || prov.kind == ProviderKind::Builtin {
            continue;
        }
        seen.push(prov.name.clone());
        let state = keys.iter().find(|(n, _)| n == &prov.name).map(|(_, s)| s.clone()).unwrap_or(KeyState::NotNeeded);
        let badge = match &state {
            KeyState::Ready => pill("Key ready", p.good, p.good_soft),
            KeyState::Missing(_) => pill("Key missing", p.wait, p.wait_soft),
            KeyState::NotNeeded => pill("No key needed", p.muted, p.surface2),
        };
        let mut actions = vec![];
        if prov.key.is_some() {
            let (kf, kv, name) = (f.key_for.clone(), f.key_value.clone(), prov.name.clone());
            actions.push(button(p, if state == KeyState::Ready { "Replace key" } else { "Set key" }, Look::Secondary, true, move || {
                kv.set(tv(""));
                kf.set(Some(name.clone()));
            }));
        }
        if mine.contains(&prov.name) {
            let (form, name) = (f.clone(), prov.name.clone());
            let project = ui.current_session().map(|s| (*s.project).clone());
            actions.push(button(p, "Remove", Look::Quiet, true, move || {
                form.edit_file(|c| c.providers.retain(|x| x.name != name));
                form.reload(project.as_deref());
            }));
        }
        let mut block = vec![
            row(10.0, vec![b(prov.name.clone(), 14.0, p.ink), badge, if mine.contains(&prov.name) { pill("yours", p.accent_ink, p.accent_soft) } else { SizedBox::shrink().into() }, gap(), row(8.0, actions)]),
            copyable_mono(format!("{}  ·  {}", prov.base_url, prov.models.join(", ")), 11.5, p.muted),
        ];
        if let KeyState::Missing(why) = &state {
            block.push(copyable(why.clone(), 11.5, p.muted));
        }
        if editing.as_deref() == Some(prov.name.as_str()) {
            let (form, reference) = (f.clone(), prov.key.clone().unwrap_or_default());
            let cancel = f.key_for.clone();
            let project = ui.current_session().map(|s| (*s.project).clone());
            block.push(row(10.0, vec![
                grow(field(ui, p, "Paste the key", &f.key_value, "sk-…", true)),
                button(p, "Save to keychain", Look::Primary, true, move || {
                    let value = form.key_value.get().text.trim().to_string();
                    form.key_value.set(tv(""));
                    match wright_runtime::secrets::store(&reference, &value) {
                        Ok(()) => form.message.set(format!("Saved {reference} to the keychain.")),
                        Err(e) => form.message.set(format!("Not saved: {e}")),
                    }
                    form.key_for.set(None);
                    form.reload(project.as_deref());
                }),
                button(p, "Cancel", Look::Quiet, true, move || cancel.set(None)),
            ]));
        }
        rows.push(Container::new().color(p.surface2.with_alpha(110)).radius(corner::CARD).border(Border::new(p.line, 1.0)).padding(EdgeInsets::all(14.0)).child(stretch(6.0, block)).into());
    }

    // Add a provider (a local server, a gateway, another vendor).
    let anthropic = f.new_anthropic.get();
    let (kind_a, kind_o) = (f.new_anthropic.clone(), f.new_anthropic.clone());
    let form = f.clone();
    let project = ui.current_session().map(|s| (*s.project).clone());
    rows.push(divider(p));
    rows.push(section(p, "Add a provider", "A local server (Ollama, vLLM, LM Studio), a gateway, or another vendor. Most speak the OpenAI format."));
    rows.push(row(10.0, vec![
        button(p, "OpenAI-compatible", if anthropic { Look::Secondary } else { Look::Primary }, true, move || kind_o.set(false)),
        button(p, "Anthropic", if anthropic { Look::Primary } else { Look::Secondary }, true, move || kind_a.set(true)),
    ]));
    rows.push(row(10.0, vec![
        grow(field(ui, p, "Name", &f.new_name, "local", false)),
        grow(field(ui, p, "Address", &f.new_base, "http://127.0.0.1:11434/v1", false)),
    ]));
    rows.push(field(ui, p, "Models it serves (patterns, comma separated)", &f.new_models, "llama*, qwen*", false));
    rows.push(row(10.0, vec![
        button(p, "Add provider", Look::Primary, true, move || {
            let name = form.new_name.get().text.trim().to_string();
            let base = form.new_base.get().text.trim().to_string();
            let models: Vec<String> = form.new_models.get().text.split(',').map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).collect();
            if name.is_empty() || base.is_empty() {
                form.message.set("A provider needs a name and an address.".into());
                return;
            }
            let local = base.contains("127.0.0.1") || base.contains("localhost");
            let entry = ProviderEntry {
                name: name.clone(),
                kind: if form.new_anthropic.get() { ProviderKind::Anthropic } else { ProviderKind::OpenAi },
                base_url: base,
                key: if local { None } else { Some(format!("keychain:wright/{name}")) },
                models: if models.is_empty() { vec!["*".into()] } else { models },
            };
            form.edit_file(move |c| {
                c.providers.retain(|x| x.name != entry.name);
                c.providers.insert(0, entry);
            });
            for s in [&form.new_name, &form.new_base, &form.new_models] {
                s.set(tv(""));
            }
            form.reload(project.as_deref());
        }),
        t("Local addresses need no key; anything else gets a keychain entry to fill in.", 12.0, p.muted),
    ]));
    stretch(12.0, rows)
}

/// What Wright works with, out of the box: its own agents (the workforce
/// that drafts, builds, checks and explains every project) and the tools a
/// plan can hand to an agent. Nothing to install or set up; this page is
/// for knowing what is there.
fn agents_and_tools(ui: &Ui, p: &Pal) -> WidgetNode {
    use wright_core::palette::{Capability, Effect, PALETTE};
    let mut rows = vec![section(
        p,
        "Wright's own agents",
        "Built in, always there. You never write these: Wright brings each one in when a plan needs it, and every one follows a fixed contract. The agents of your own project are the ones your plan names; they appear on the plan card and in the Engine view (hold Space).",
    )];
    for a in wright_agents::WORKFORCE {
        rows.push(card(p, column(4.0, vec![
            row(8.0, vec![b(a.name, 14.0, p.ink), gap(), pill(format!("joins at {}", a.injected_by), p.muted, p.surface2)]),
            para(a.duty, 13.0, p.muted),
        ])));
    }
    rows.push(divider(p));
    rows.push(section(
        p,
        "Tools",
        "What an agent can be given. A plan grants each agent only the tools it needs, inside the project folder; anything that reaches outside this machine stops and asks you first.",
    ));
    for tool in PALETTE {
        let (effect, fg, bg) = match tool.effect {
            Effect::None => ("only reads", p.good, p.good_soft),
            Effect::Workspace => ("changes the project (undoable)", p.accent_ink, p.accent_soft),
            Effect::World => ("touches the world · asks first", p.wait, p.wait_soft),
        };
        let needs: Vec<&str> = tool
            .caps
            .iter()
            .map(|c| match c {
                Capability::ReadFiles => "read files",
                Capability::WriteFiles => "write files",
                Capability::Network => "network",
                Capability::Shell => "shell",
                Capability::Spawn => "run programs",
            })
            .collect();
        let mut head = vec![b(tool.name, 13.5, p.ink), pill(effect, fg, bg)];
        if !tool.implemented {
            head.push(pill("not yet", p.bad, p.surface2));
        }
        let mut lines = vec![row(8.0, head), para(tool.summary, 12.5, p.muted)];
        if !needs.is_empty() {
            lines.push(t(format!("Needs: {}", needs.join(", ")), 11.5, p.muted));
        }
        rows.push(column(4.0, lines));
    }
    if let Some(plan) = ui.current_session().and_then(|s| s.plan.get()) {
        rows.push(divider(p));
        rows.push(section(p, &format!("In this project: {}", plan.name), "The agents this plan names, and the tools each one was given."));
        for c in plan.cards() {
            let tools = if c.tools.is_empty() { "no tools".to_string() } else { c.tools.join(", ") };
            rows.push(column(3.0, vec![row(8.0, vec![b(c.name.clone(), 13.5, p.ink), pill(c.role, p.muted, p.surface2)]), t(format!("{} · {tools}", c.model), 12.0, p.muted)]));
        }
    }
    stretch(12.0, rows)
}

fn defaults(ui: &Ui, p: &Pal) -> WidgetNode {
    let f = ui.form.clone();
    let project = ui.current_session().map(|s| (*s.project).clone());
    stretch(14.0, vec![
        section(p, "Drafting plans", "The model the interview uses to turn your description into a blueprint. It talks to you, so a capable model pays off."),
        field(ui, p, "Interview model", &f.interview, "claude-sonnet-5-5", false),
        section(p, "Voice", "The speech-to-text model the mic uses. whisper-base is built in and runs on this machine; any other model is sent to an OpenAI-compatible provider (OpenAI's whisper-1, Groq, or a Whisper server)."),
        field(ui, p, "Voice model", &f.voice, "whisper-base", false),
        row(10.0, vec![button(p, "Save defaults", Look::Primary, true, move || {
            let (i, v) = (f.interview.get().text.trim().to_string(), f.voice.get().text.trim().to_string());
            f.edit_file(|c| {
                c.interview_model = i;
                c.voice_model = v;
            });
            f.reload(project.as_deref());
        })]),
    ])
}

fn appearance(ui: &Ui, p: &Pal) -> WidgetNode {
    let mode = ui.mode.get();
    let accent = ui.accent.get();
    let (dark, light) = (ui.clone(), ui.clone());
    let pick = ui.clone();
    let presets: Vec<Color> = theme::SWATCHES.iter().map(|&h| Color::hex(h)).collect();
    stretch(16.0, vec![
        section(p, "Theme", "VS Code's Dark Modern and Light Modern."),
        row(10.0, vec![
            button(p, "Dark", if mode == Mode::Dark { Look::Primary } else { Look::Secondary }, true, move || dark.set_look(Mode::Dark, dark.accent.get())),
            button(p, "Light", if mode == Mode::Light { Look::Primary } else { Look::Secondary }, true, move || light.set_look(Mode::Light, light.accent.get())),
        ]),
        section(p, "Accent colour", "Any colour. The logo, the background mark, buttons, highlights and the live parts of the engine all follow it."),
        row(24.0, vec![
            ColorPicker::new(accent).presets(presets).show_sliders(true).on_color_changed(move |c| pick.set_look(pick.mode.get(), c)).into(),
            column(12.0, vec![
                super::logo::tile(88.0, p.accent, p.on_accent),
                row(4.0, vec![super::logo::Mark::solid(p.accent_ink).widget(22.0), b("wright", 17.0, p.ink)]),
                mono(theme::hex(accent), 12.5, p.muted),
            ]),
        ]),
    ])
}

/// Office machines. A network that inspects HTTPS re-signs every site
/// with the organization's own root certificate; without that root, every
/// model call and download fails verification. Proxies and package mirrors
/// arrive as environment variables. Both are written to config.kdl and put
/// into effect at once (and at every start).
fn network(ui: &Ui, p: &Pal) -> WidgetNode {
    use wright_runtime::config::{self as rc, check_ca_file, expand_home};
    let f = ui.form.clone();
    let file_cfg = Config::load_file().unwrap_or_else(|_| Config::builtin());
    let mut rows = vec![section(
        p,
        "Certificates",
        "If your organization inspects HTTPS (common on office laptops), add its root certificate here so Wright can reach model providers and downloads. It is trusted in addition to this computer's own certificates, never instead of them. The file is PEM text: blocks that begin with -----BEGIN CERTIFICATE-----. Your IT team can give you one; a .cer/.der file converts with `openssl x509 -inform der -in corp.cer -out corp.pem`.",
    )];
    let typed = f.ca_path.get().text.trim().to_string();
    let status: WidgetNode = if typed.is_empty() {
        t("No extra certificates. This computer's own trust store is used.", 12.5, p.muted)
    } else {
        match check_ca_file(&expand_home(&typed)) {
            Ok(n) => row(8.0, vec![pill(format!("{n} certificate{}", if n == 1 { "" } else { "s" }), p.good, p.good_soft), grow(t("found in this file", 12.5, p.muted))]),
            Err(e) => copyable(e, 12.5, p.wait),
        }
    };
    let (choose, save, clear) = (f.clone(), f.clone(), f.clone());
    let project = ui.current_session().map(|s| (*s.project).clone());
    let project2 = project.clone();
    rows.push(row(10.0, vec![
        grow(field(ui, p, "Certificate file (paste a path, or choose one)", &f.ca_path, "/path/to/company-root.pem", false)),
        button(p, "Choose file…", Look::Secondary, true, move || choose.choose_ca_file()),
    ]));
    rows.push(status);
    rows.push(row(10.0, vec![
        button(p, "Save certificates", Look::Primary, !typed.is_empty(), move || {
            let path = save.ca_path.get().text.trim().to_string();
            if let Err(e) = check_ca_file(&expand_home(&path)) {
                save.message.set(format!("Not saved: {e}"));
                return;
            }
            save.edit_file(|c| c.ca_file = Some(path.clone()));
            apply_now(&save);
            save.reload(project.as_deref());
        }),
        button(p, "Remove", Look::Quiet, file_cfg.ca_file.is_some(), move || {
            clear.edit_file(|c| c.ca_file = None);
            apply_now(&clear);
            clear.reload(project2.as_deref());
        }),
    ]));
    if std::env::var_os(rc::CA_ENV).is_some() && file_cfg.ca_file.is_none() {
        rows.push(t(format!("{} is set in your environment, so that file is in use for this session.", rc::CA_ENV), 12.0, p.muted));
    }

    rows.push(divider(p));
    rows.push(section(
        p,
        "Environment variables",
        "For Wright and every program it runs: a proxy (HTTPS_PROXY, NO_PROXY), a package mirror (PIP_INDEX_URL, npm_config_registry), a model mirror (WRIGHT_MODEL_MIRROR). A variable you already set in your own shell wins over these. Values whose names say they are secret (…KEY, …TOKEN, …SECRET, …PASSWORD), and proxy addresses with a password in them, go to the system keychain, not to a file.",
    ));
    for (k, v) in &file_cfg.env {
        let shown = if v.starts_with("keychain:") { format!("from the keychain ({v})") } else { v.clone() };
        let (form, name) = (f.clone(), k.clone());
        let project = ui.current_session().map(|s| (*s.project).clone());
        let live = std::env::var_os(k).is_some();
        rows.push(Container::new().color(p.surface2.with_alpha(110)).radius(corner::FIELD).border(Border::new(p.line, 1.0)).padding(EdgeInsets::symmetric(12.0, 8.0)).child(row(10.0, vec![
            b(k.clone(), 13.0, p.ink),
            grow(line(shown, 12.5, p.muted)),
            if live { pill("in effect", p.good, p.good_soft) } else { pill("not set", p.wait, p.wait_soft) },
            button(p, "Remove", Look::Quiet, true, move || {
                form.edit_file(|c| c.env.retain(|(n, _)| n != &name));
                apply_now(&form);
                form.reload(project.as_deref());
            }),
        ])).into());
    }
    let add = f.clone();
    let project3 = ui.current_session().map(|s| (*s.project).clone());
    rows.push(row(10.0, vec![
        SizedBox::width(220.0).child(field(ui, p, "Name", &f.env_name, "HTTPS_PROXY", false)).into(),
        grow(field(ui, p, "Value", &f.env_value, "http://proxy.example.com:3128", rc::looks_secret(f.env_name.get().text.trim()))),
    ]));
    rows.push(row(10.0, vec![button(p, "Add variable", Look::Primary, true, move || {
        let name = add.env_name.get().text.trim().to_string();
        let value = add.env_value.get().text.trim().to_string();
        if name.is_empty() {
            add.message.set("A variable needs a name.".into());
            return;
        }
        let stored = match store_env_value(&name, &value) {
            Ok(v) => v,
            Err(e) => {
                add.message.set(format!("Not saved: {e}"));
                return;
            }
        };
        add.edit_file(|c| {
            c.env.retain(|(n, _)| n != &name);
            c.env.push((name.clone(), stored.clone()));
        });
        add.env_name.set(tv(""));
        add.env_value.set(tv(""));
        apply_now(&add);
        add.reload(project3.as_deref());
    })]));

    rows.push(divider(p));
    rows.push(section(p, "In effect now", "What Wright sees in this session. Passwords inside proxy addresses are hidden."));
    for k in ["HTTPS_PROXY", "HTTP_PROXY", "NO_PROXY", rc::CA_ENV, "SSL_CERT_FILE", "WRIGHT_MODEL_MIRROR", "WRIGHT_HOME"] {
        let v = std::env::var(k).or_else(|_| std::env::var(k.to_ascii_lowercase())).ok();
        rows.push(row(10.0, vec![
            SizedBox::width(190.0).child(mono(k.to_string(), 12.0, p.ink)).into(),
            grow(match v {
                Some(v) => copyable_mono(hide_password(&v), 12.0, p.muted),
                None => t("not set", 12.0, p.muted),
            }),
        ]));
    }
    stretch(12.0, rows)
}

/// Put the saved settings into effect for this session, and say so.
fn apply_now(f: &Form) {
    match Config::load() {
        Ok(cfg) => {
            let problems = wright_runtime::config::apply_to_process(&cfg);
            if !problems.is_empty() {
                f.message.set(problems.join(" "));
            }
        }
        Err(e) => f.message.set(e),
    }
}

/// A value for `env`: secrets go to the keychain and come back as a
/// reference; everything else is stored as written.
pub fn store_env_value(name: &str, value: &str) -> Result<String, String> {
    use wright_runtime::config::{check_env_pair, looks_secret};
    if !wright_runtime::config::valid_env_name(name) {
        return check_env_pair(name, value).map(|_| value.to_string());
    }
    let secret = !value.starts_with("keychain:") && (looks_secret(name) || check_env_pair(name, value).is_err());
    if secret {
        let reference = format!("keychain:wright/env-{}", name.to_ascii_lowercase().replace('_', "-"));
        wright_runtime::secrets::store(&reference, value)?;
        return Ok(reference);
    }
    check_env_pair(name, value)?;
    Ok(value.to_string())
}

/// `http://user:pass@host` -> `http://user:•••@host`.
fn hide_password(v: &str) -> String {
    if let Some((scheme, rest)) = v.split_once("://") {
        if let Some((cred, host)) = rest.split_once('@') {
            if let Some((user, _)) = cred.split_once(':') {
                return format!("{scheme}://{user}:•••@{host}");
            }
        }
    }
    v.to_string()
}

/// Everything this computer runs on its own, and the switch that stops it.
fn fleet(ui: &Ui, p: &Pal) -> WidgetNode {
    use wright_engine::scheduler::State;
    let f = ui.form.clone();
    let stopped = ui.stopped.get();
    let mut rows = vec![section(
        p,
        "Everything that runs on its own",
        "Projects with a schedule. A new schedule waits until you let it run unattended — after you have watched a run pass — and waits again whenever its plan changes.",
    )];
    let (stop, start) = (ui.clone(), ui.clone());
    rows.push(match &stopped {
        None => row(10.0, vec![
            button(p, "Stop everything", Look::Secondary, true, move || stop.stop_everything("stopped from Settings")),
            grow(t("One switch: no firing starts, firings in flight stop, approvals are refused — here and in the background service.", 12.5, p.muted)),
        ]),
        Some(reason) => row(10.0, vec![
            pill("STOPPED", p.bad, p.surface2),
            grow(copyable(reason.clone(), 12.5, p.ink)),
            button(p, "Start again", Look::Primary, true, move || start.start_again()),
        ]),
    });
    let list = f.fleet.get();
    if list.is_empty() {
        rows.push(t("Nothing is scheduled on this computer.", 13.0, p.muted));
    }
    for r in list {
        let (label, fg, bg) = if r.firing_now {
            ("firing now", p.accent_ink, p.accent_soft)
        } else {
            match (&r.held, r.state) {
                (None, _) => ("runs on its own", p.good, p.good_soft),
                (Some(_), State::Paused) => ("paused", p.muted, p.surface2),
                (Some(_), State::Awaiting) => ("waiting for you", p.wait, p.wait_soft),
                (Some(_), State::Active) => ("held", p.wait, p.wait_soft),
            }
        };
        let mut actions: Vec<WidgetNode> = Vec::new();
        let dir = r.dir.clone();
        let form = f.clone();
        let refresh = move |form: &Form, msg: String| {
            form.message.set(msg);
            form.fleet.set(wright_engine::fleet::rows());
        };
        if r.state == State::Paused {
            let (d, fm, rf) = (dir.clone(), form.clone(), refresh);
            actions.push(button(p, "Resume", Look::Secondary, true, move || {
                let m = match wright_engine::scheduler::set_state(&d, State::Active) {
                    Ok(_) => "Resumed.".to_string(),
                    Err(e) => e,
                };
                rf(&fm, m);
            }));
        } else {
            let (d, fm, rf) = (dir.clone(), form.clone(), refresh);
            actions.push(button(p, "Pause", Look::Quiet, true, move || {
                let m = match wright_engine::scheduler::set_state(&d, State::Paused) {
                    Ok(_) => "Paused. It will not run until you resume it.".to_string(),
                    Err(e) => e,
                };
                rf(&fm, m);
            }));
        }
        if r.trust.is_none() {
            let (d, fm, rf) = (dir.clone(), form.clone(), refresh);
            actions.push(button(p, "Let it run unattended", Look::Primary, true, move || {
                let m = match wright_engine::unattended::approve(&d, "window") {
                    Ok(a) => wright_engine::unattended::describe(&a),
                    Err(e) => format!("Not yet: {e}"),
                };
                rf(&fm, m);
            }));
        } else {
            let (d, fm, rf) = (dir.clone(), form.clone(), refresh);
            actions.push(button(p, "Take it back", Look::Quiet, true, move || {
                let m = match wright_engine::unattended::revoke(&d) {
                    Ok(_) => "It will not run unattended until you let it again.".to_string(),
                    Err(e) => e,
                };
                rf(&fm, m);
            }));
        }
        let mut block = vec![
            row(10.0, vec![b(r.name.clone(), 14.0, p.ink), pill(label, fg, bg), gap(), row(8.0, actions)]),
            copyable_mono(format!("{} {} · next {} · {}", r.cron, r.tz, r.next, r.dir.display()), 11.5, p.muted),
        ];
        if let Some(why) = &r.held {
            block.push(copyable(why.clone(), 12.5, p.wait));
        }
        match &r.last {
            Some(l) => block.push(copyable(
                format!(
                    "Last run {} {} — {} — ${:.4} — receipt {}",
                    l.run,
                    if l.ok { "passed" } else { "failed" },
                    l.headline,
                    l.cost_micro_usd as f64 / 1e6,
                    if l.verified { "verifies" } else { "does NOT verify" }
                ),
                12.5,
                if l.ok && l.verified { p.ink } else { p.bad },
            )),
            None => block.push(t("Never run yet.", 12.5, p.muted)),
        }
        if let Some(tr) = &r.trust {
            block.push(copyable(tr.clone(), 12.5, p.good));
        }
        if r.waiting_on_you > 0 {
            block.push(t(format!("{} approval(s) waiting on you in its session.", r.waiting_on_you), 12.5, p.wait));
        }
        rows.push(Container::new().color(p.surface2.with_alpha(110)).radius(corner::CARD).border(Border::new(p.line, 1.0)).padding(EdgeInsets::all(14.0)).child(stretch(6.0, block)).into());
    }
    stretch(12.0, rows)
}

/// The administrator's side, as a person on the machine sees it.
fn organization(ui: &Ui, p: &Pal) -> WidgetNode {
    let f = ui.form.clone();
    let mut rows = vec![section(
        p,
        "Your organization's rules",
        "An IT team can set rules for every Wright on its computers: budgets, tools, the network, model providers, external agents, and who approves unattended runs. The rules are signed by an administrator; a policy that was changed or is not signed stops everything until IT fixes it.",
    )];
    match f.policy.get() {
        None => {
            rows.push(t(format!("No organization policy on this computer (IT installs one in {}).", wright_runtime::policy::system_dir().display()), 13.0, p.muted));
            rows.push(t("For administrators, from a terminal:", 12.5, p.muted));
            for c in ["wright admin keygen --name \"IT security\"", "wright admin policy init policy.kdl", "wright admin policy sign policy.kdl --name \"IT security\"", "wright admin approve <project> --name \"IT security\"", "wright admin fleet-verify <projects folder> --json"] {
                rows.push(copyable_mono(c, 12.0, p.ink));
            }
        }
        Some(Err(e)) => rows.push(copyable(e, 13.0, p.bad)),
        Some(Ok(lines)) => {
            for l in lines {
                rows.push(row(8.0, vec![Glyph::Done.draw(14.0, p.good), grow(copyable(l, 13.0, p.ink))]));
            }
        }
    }
    if let Some(s) = ui.current_session() {
        if let Ok(bp) = wright_engine::load_blueprint(&s.project) {
            rows.push(divider(p));
            rows.push(b(format!("This project: {}", bp.meta.name), 14.0, p.ink));
            let cfg = Config::load().unwrap_or_else(|_| Config::builtin());
            rows.push(match wright_engine::org::enforce(&bp, &cfg) {
                Ok(None) => t("No policy to check it against.", 12.5, p.muted),
                Ok(Some(pol)) => t(format!("{}'s policy allows this plan.", pol.org), 12.5, p.good),
                Err(e) => copyable(e, 12.5, p.bad),
            });
            if let Err(e) = wright_engine::org::approval(&s.project) {
                rows.push(copyable(format!("Unattended runs: {e}."), 12.5, p.wait));
            }
        }
    }
    rows.push(divider(p));
    rows.push(section(p, "Every receipt on this computer", "Each run of every project here and every scheduled one, through all six checks: signature and signer, the event log, artifacts, inputs, the plan, and the agents' identities."));
    let go = f.clone();
    rows.push(row(10.0, vec![button(p, "Verify every receipt", Look::Secondary, true, move || {
        go.verified.set(Some(wright_engine::org::fleet_verify(&[super::store::projects_dir()])));
    })]));
    if let Some(checks) = f.verified.get() {
        let report = wright_engine::org::fleet_report(&checks, false);
        let all_ok = checks.iter().all(|c| c.ok);
        rows.push(copyable(report.trim_end().to_string(), 12.5, if all_ok { p.good } else { p.bad }));
    }
    stretch(12.0, rows)
}

fn workspace(ui: &Ui, p: &Pal) -> WidgetNode {
    let f = ui.form.clone();
    let save_dir = f.clone();
    let daemon = f.clone();
    let home = wright_runtime::config::wright_home();
    let reveal = home.clone();
    stretch(14.0, vec![
        section(p, "New projects", "Each new session gets its own folder here, named from your first words."),
        field(ui, p, "Projects folder", &f.projects_dir, "~/Wright", false),
        row(10.0, vec![button(p, "Save", Look::Primary, true, move || {
            let dir = std::path::PathBuf::from(save_dir.projects_dir.get().text.trim());
            if dir.as_os_str().is_empty() {
                return;
            }
            super::store::set_projects_dir(&dir);
            save_dir.message.set(format!("New projects will be created in {}", dir.display()));
        })]),
        divider(p),
        section(p, "Background schedules", "Scheduled projects run while Wright's background service is installed, even with this window closed."),
        row(10.0, vec![button(p, "Run schedules in the background", Look::Secondary, true, move || {
            let exe = std::env::current_exe().unwrap_or_else(|_| "wright".into());
            match wright_engine::scheduler::install_unit(&exe) {
                Ok((path, next)) => daemon.message.set(format!("Installed {}. To start it now: {next}", path.display())),
                Err(e) => daemon.message.set(format!("Not installed: {e}")),
            }
        })]),
        divider(p),
        section(p, "Wright's own records", "Run history, cache and receipts live outside your projects, here:"),
        row(10.0, vec![grow(copyable_mono(home.display().to_string(), 12.5, p.ink)), button(p, "Open", Look::Quiet, true, move || super::picker::reveal(&reveal))]),
    ])
}

fn project(ui: &Ui, p: &Pal) -> WidgetNode {
    let Some(s) = ui.current_session() else {
        return section(p, "No project open", "Open a session to change its models and limits.");
    };
    if s.plan.get().is_none() {
        return section(p, "No blueprint yet", "Describe the project first; its models and limits appear here once the plan exists.");
    }
    let f = ui.form.clone();
    let tiers = f.tiers.get();
    let tier_field = |i: usize, label: &str, ph: &str| -> WidgetNode {
        let sig = f.tiers.clone();
        let v = tiers[i].clone();
        stretch(5.0, vec![
            t(label.to_string(), 12.0, p.muted),
            Container::new()
                .color(p.surface2)
                .radius(corner::FIELD)
                .border(Border::new(p.line_strong, 1.0))
                .padding(EdgeInsets::symmetric(12.0, 8.0))
                .child(TextField::new(v).show_cursor(ui.is_typing_in(ph)).single_line().placeholder(ph.to_string()).placeholder_color(p.muted).size(13.5).color(p.ink).cursor(p.accent_ink, 2.0).on_changed(std::rc::Rc::new(move |x: TextEditingValue| {
                    let mut all = sig.get();
                    all[i] = x;
                    sig.set(all);
                })))
                .into(),
        ])
    };
    let (form, dir, sess) = (f.clone(), (*s.project).clone(), s.clone());
    stretch(14.0, vec![
        section(p, &s.title(), "These live in the project's blueprint. Saving records a new blueprint version; the next run uses it, and only the work it affects runs again."),
        b("Models", 14.0, p.ink),
        row(10.0, vec![grow(tier_field(0, "Everyday work: prefer", "deepseek-v4-flash")), grow(tier_field(1, "…fall back to", "glm-5.3-flash"))]),
        row(10.0, vec![grow(tier_field(2, "Careful work (review, delivery): prefer", "claude-sonnet-5-5")), grow(tier_field(3, "…fall back to", "gpt-5.5"))]),
        b("Spending limits (USD)", 14.0, p.ink),
        row(10.0, vec![grow(field(ui, p, "Per run", &f.per_run, "1.00", false)), grow(field(ui, p, "Per day", &f.per_day, "5.00", false))]),
        row(10.0, vec![button(p, "Save to the blueprint", Look::Primary, true, move || {
            let t4 = form.tiers.get();
            let tiers = [t4[0].text.trim().to_string(), t4[1].text.trim().to_string(), t4[2].text.trim().to_string(), t4[3].text.trim().to_string()];
            match save_project(&dir, tiers, &form.per_run.get().text, &form.per_day.get().text) {
                Ok(commit) => {
                    form.message.set(format!("Saved as blueprint {commit}."));
                    sess.plan.set(super::session::load_plan(&dir));
                }
                Err(e) => form.message.set(format!("Not saved: {e}")),
            }
        })]),
    ])
}

/// Write new model tiers and limits into a project's blueprint.
pub fn save_project(dir: &Path, tiers: [String; 4], per_run: &str, per_day: &str) -> Result<String, String> {
    let money = |s: &str, what: &str| -> Result<u64, String> {
        let v: f64 = s.trim().trim_start_matches('$').parse().map_err(|_| format!("{what} must be a number of dollars, like 1.50"))?;
        if !(0.0..=10_000.0).contains(&v) {
            return Err(format!("{what} must be between 0 and 10000"));
        }
        Ok((v * 1e6).round() as u64)
    };
    let mut bp = wright_engine::load_blueprint(dir)?;
    let [cp, cf, qp, qf] = tiers;
    if cp.is_empty() || qp.is_empty() {
        return Err("each tier needs a preferred model".into());
    }
    bp.models.cheap.prefer = cp;
    bp.models.cheap.fallback = cf;
    bp.models.quality.prefer = qp;
    bp.models.quality.fallback = qf;
    bp.guardrails.per_firing_micro_usd = money(per_run, "The per-run limit")?;
    bp.guardrails.daily_micro_usd = money(per_day, "The daily limit")?;
    if bp.guardrails.daily_micro_usd < bp.guardrails.per_firing_micro_usd {
        return Err("the daily limit cannot be lower than one run's".into());
    }
    wright_core::compiler::compile(&bp).map_err(|e| format!("{e:?}"))?;
    let text = wright_core::blueprint::canon::canonical(&bp);
    wright_runtime::store::write_atomic(&dir.join("blueprint.kdl"), text.as_bytes())?;
    let store = wright_runtime::store::Store::open(dir)?;
    store.commit_blueprint(&text, "settings: models and limits")?;
    Ok(wright_core::hash::short(&wright_core::blueprint::canon::content_hash(&bp)).to_string())
}
