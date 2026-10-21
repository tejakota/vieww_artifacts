//! Settings: what the agents need (models and keys), defaults, the look,
//! the workspace, and the open project's own limits.
//!
//! ```text
//!  Models & keys   the built-in models (download, remove, fall back to them),
//!                  providers, their models, key Ready / Missing / Not needed
//!  Defaults        the interview model, the voice model
//!  Appearance      dark / light, any accent colour
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
    Defaults,
    Appearance,
    Workspace,
    Project,
}

impl Tab {
    const ALL: [Tab; 5] = [Tab::Models, Tab::Defaults, Tab::Appearance, Tab::Workspace, Tab::Project];
    fn label(self) -> &'static str {
        match self {
            Tab::Models => "Models & keys",
            Tab::Defaults => "Defaults",
            Tab::Appearance => "Appearance",
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
            Tab::Defaults => defaults(&ui, &p),
            Tab::Appearance => appearance(&ui, &p),
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
                if msg.is_empty() { SizedBox::shrink().into() } else { t(msg, 12.5, p.muted) },
            ]))
            .into()
    })
    .into()
}

fn field(ui: &Ui, p: &Pal, label: &str, value: &Signal<TextEditingValue>, placeholder: &str, secret: bool) -> WidgetNode {
    let v = value.clone();
    column(5.0, vec![
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
            Some(Pull::Failed(e)) => block.push(para(e.clone(), 11.5, p.wait)),
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
            mono(format!("{}  ·  {}", prov.base_url, prov.models.join(", ")), 11.5, p.muted),
        ];
        if let KeyState::Missing(why) = &state {
            block.push(para(why.clone(), 11.5, p.muted));
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
        row(10.0, vec![mono(home.display().to_string(), 12.5, p.ink), button(p, "Open", Look::Quiet, true, move || super::picker::reveal(&reveal))]),
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
        column(5.0, vec![
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
