//! The scheduler: the background service that fires schedules.
//!
//! * [`register`] — the Operator calls this at the end of a firing that
//!   has a `scheduling` chip. It records (project, cron, tz) in
//!   `~/.wright/schedules.json` and returns the next fire time. Idempotent:
//!   registering the same project again updates its entry.
//! * [`daemon`] — `wright daemon`: loop forever, sleep until the next due
//!   schedule (re-reading the registry at least every 30 s so new
//!   registrations are noticed), fire it, record that minute so a
//!   restart in the same minute never double-fires.
//! * [`install_unit`] — writes a systemd user unit (Linux) or a launchd
//!   agent (macOS) so the daemon starts at login. It does not enable it:
//!   the CLI prints the one command that does, so the person decides.
//!
//! A schedule is only fired when its entry is [`State::Active`]: a new
//! schedule waits for the person's go-ahead to run unattended
//! ([`State::Awaiting`], see `crate::unattended`), a paused one waits for
//! them to resume it, and the kill switch stops them all. An approval is
//! for one plan: when the blueprint changes, the schedule waits again.

use std::path::{Path, PathBuf};
use std::time::Duration;

use wright_core::events::now_ms;
use wright_core::schedule::{self, Cron};
use wright_core::wire::json::{self, Value};
use wright_runtime::config::wright_home;
use wright_runtime::store::write_atomic;
use wright_runtime::tz;

/// Whether a schedule fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Fires on its schedule.
    Active,
    /// Stopped by the person for this project.
    Paused,
    /// Registered, waiting for the person to approve unattended runs.
    Awaiting,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Active => "active",
            State::Paused => "paused",
            State::Awaiting => "awaiting",
        }
    }
    fn parse(s: &str) -> State {
        match s {
            "active" => State::Active,
            "paused" => State::Paused,
            _ => State::Awaiting,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub project: String,
    pub cron: String,
    pub tz: String,
    /// The last minute (Unix seconds / 60) this entry fired, if ever.
    pub last_fired_minute: i64,
    pub state: State,
}

fn registry_path() -> PathBuf {
    wright_home().join("schedules.json")
}

pub fn load() -> Vec<Entry> {
    let text = std::fs::read_to_string(registry_path()).unwrap_or_default();
    let Ok(v) = json::parse(&text) else {
        return Vec::new();
    };
    v.as_arr()
        .unwrap_or(&[])
        .iter()
        .filter_map(|e| {
            Some(Entry {
                project: e.str_of("project")?.to_string(),
                cron: e.str_of("cron")?.to_string(),
                tz: e.str_of("tz").unwrap_or("UTC").to_string(),
                last_fired_minute: e
                    .get("last_fired_minute")
                    .and_then(|n| n.as_f64())
                    .unwrap_or(0.0) as i64,
                // Registries from before states fired everything they held;
                // they keep doing so until someone pauses one.
                state: e.str_of("state").map(State::parse).unwrap_or(State::Active),
            })
        })
        .collect()
}

fn save(entries: &[Entry]) -> Result<(), String> {
    let v = Value::Arr(
        entries
            .iter()
            .map(|e| {
                Value::obj(vec![
                    ("project", Value::str(e.project.clone())),
                    ("cron", Value::str(e.cron.clone())),
                    ("tz", Value::str(e.tz.clone())),
                    ("last_fired_minute", Value::int(e.last_fired_minute)),
                    ("state", Value::str(e.state.as_str())),
                ])
            })
            .collect(),
    );
    write_atomic(&registry_path(), json::to_string(&v).as_bytes())
}

/// Next fire time of an entry, as Unix seconds.
pub fn next_fire(cron: &str, tz_name: &str, after: i64) -> Result<i64, String> {
    let c = Cron::parse(cron)?;
    let zone = tz::load(tz_name)?;
    c.next_after(after, &zone)
        .ok_or_else(|| format!("'{cron}' never fires"))
}

/// Human form of a UTC instant in a zone: "2026-10-21 08:00".
pub fn local_string(utc: i64, tz_name: &str) -> String {
    let off = tz::load(tz_name)
        .map(|z| z.offset_at(utc) as i64)
        .unwrap_or(0);
    schedule::format_minute(utc + off)
}

/// Record or update a project's schedule. Returns the next fire time.
/// A new schedule starts [`State::Awaiting`] unless the project already
/// holds a valid approval for unattended runs; an existing one keeps its
/// state.
pub fn register(project: &Path, cron: &str, tz_name: &str) -> Result<String, String> {
    let now = (now_ms() / 1000) as i64;
    let next = next_fire(cron, tz_name, now)?;
    let project = project.display().to_string();
    let mut entries = load();
    match entries.iter_mut().find(|e| e.project == project) {
        Some(e) => {
            e.cron = cron.into();
            e.tz = tz_name.into();
        }
        None => entries.push(Entry {
            state: if crate::unattended::current(Path::new(&project)).is_ok() {
                State::Active
            } else {
                State::Awaiting
            },
            project,
            cron: cron.into(),
            tz: tz_name.into(),
            last_fired_minute: 0,
        }),
    }
    save(&entries)?;
    Ok(local_string(next, tz_name))
}

/// The schedule entry for a project, if it has one.
pub fn entry_for(project: &Path) -> Option<Entry> {
    let project = project.display().to_string();
    load().into_iter().find(|e| e.project == project)
}

/// Set a project's schedule state. `Ok(false)` when it has no schedule.
pub fn set_state(project: &Path, state: State) -> Result<bool, String> {
    let project = project.display().to_string();
    let mut entries = load();
    let Some(e) = entries.iter_mut().find(|e| e.project == project) else {
        return Ok(false);
    };
    e.state = state;
    save(&entries)?;
    Ok(true)
}

/// Why a due entry would not fire right now, or `None` when it would.
pub fn hold_reason(e: &Entry) -> Option<String> {
    if let Some(why) = wright_runtime::killswitch::refusal() {
        return Some(why);
    }
    match e.state {
        State::Paused => Some("paused".into()),
        State::Awaiting => Some("waiting for your go-ahead to run unattended".into()),
        State::Active => crate::unattended::current(Path::new(&e.project))
            .err()
            .or_else(|| crate::org::approval(Path::new(&e.project)).err()),
    }
}

/// Remove a project's schedule.
pub fn unregister(project: &Path) -> Result<bool, String> {
    let project = project.display().to_string();
    let mut entries = load();
    let before = entries.len();
    entries.retain(|e| e.project != project);
    save(&entries)?;
    Ok(entries.len() != before)
}

/// The daemon loop. `fire` is called for each due project.
pub fn daemon(mut fire: impl FnMut(&Path), mut log: impl FnMut(&str)) -> ! {
    log("wright daemon started; watching ~/.wright/schedules.json");
    loop {
        let now = (now_ms() / 1000) as i64;
        let mut entries = load();
        let mut soonest = now + 30;
        let mut changed = false;
        for e in entries.iter_mut() {
            // Fire if a scheduled minute passed since we last fired it
            // (but never more than once, and not for minutes older than
            // 10 minutes — a laptop waking at noon does not run 8am late
            // without saying so; it logs the miss instead).
            let probe_from = now - 60 * 10;
            match next_fire(&e.cron, &e.tz, probe_from.max(e.last_fired_minute * 60)) {
                Ok(t) if t <= now => {
                    let minute = t / 60;
                    if minute > e.last_fired_minute {
                        e.last_fired_minute = minute;
                        changed = true;
                        if let Some(why) = hold_reason(e) {
                            log(&format!(
                                "not firing {} (scheduled {}): {why}",
                                e.project,
                                local_string(t, &e.tz)
                            ));
                            continue;
                        }
                        log(&format!(
                            "firing {} (scheduled {})",
                            e.project,
                            local_string(t, &e.tz)
                        ));
                        fire(Path::new(&e.project));
                    }
                }
                Ok(t) => soonest = soonest.min(t),
                Err(err) => log(&format!("{}: {err}", e.project)),
            }
        }
        if changed {
            if let Err(e) = save(&entries) {
                log(&format!("could not save the schedule registry: {e}"));
            }
        }
        let sleep_for = (soonest - (now_ms() / 1000) as i64).clamp(1, 30);
        std::thread::sleep(Duration::from_secs(sleep_for as u64));
    }
}

/// Write the login service file. Returns (path, the command to enable it).
pub fn install_unit(wright_binary: &Path) -> Result<(PathBuf, String), String> {
    let home = PathBuf::from(std::env::var("HOME").map_err(|_| "HOME is not set")?);
    if cfg!(target_os = "macos") {
        let p = home.join("Library/LaunchAgents/dev.wright.daemon.plist");
        let plist = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n  <key>Label</key><string>dev.wright.daemon</string>\n  <key>ProgramArguments</key><array><string>{}</string><string>daemon</string></array>\n  <key>RunAtLoad</key><true/>\n  <key>KeepAlive</key><true/>\n</dict></plist>\n",
            wright_binary.display()
        );
        write_atomic(&p, plist.as_bytes())?;
        Ok((p.clone(), format!("launchctl load -w {}", p.display())))
    } else if cfg!(target_os = "linux") {
        let p = home.join(".config/systemd/user/wright.service");
        let unit = format!(
            "[Unit]\nDescription=Wright scheduler (fires scheduled blueprints)\n\n[Service]\nExecStart={} daemon\nRestart=on-failure\nRestartSec=10\n\n[Install]\nWantedBy=default.target\n",
            wright_binary.display()
        );
        write_atomic(&p, unit.as_bytes())?;
        Ok((
            p,
            "systemctl --user daemon-reload && systemctl --user enable --now wright".into(),
        ))
    } else {
        Err("installing a login service is not wired for this OS yet; run `wright daemon` from your startup programs".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_round_trips_and_is_idempotent() {
        // WRIGHT_HOME is process-wide and the handoff drives flip it too:
        // hold the shared env lock so the registry file is this test's
        // alone for as long as it runs.
        let _env = crate::test_support::env_lock();
        let home = std::env::temp_dir().join(format!("wright-home-sched-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("WRIGHT_HOME", &home);
        let p = Path::new("/tmp/some-project");
        let next = register(p, "0 8 * * *", "UTC").unwrap();
        assert!(next.ends_with("08:00"), "{next}");
        register(p, "30 9 * * *", "UTC").unwrap();
        let e = load();
        assert_eq!(e.len(), 1, "same project registers once");
        assert_eq!(e[0].cron, "30 9 * * *");
        assert_eq!(
            e[0].state,
            State::Awaiting,
            "a new schedule waits for a go-ahead"
        );
        assert!(hold_reason(&e[0]).unwrap().contains("go-ahead"));
        assert!(set_state(p, State::Paused).unwrap());
        assert_eq!(entry_for(p).unwrap().state, State::Paused);
        register(p, "30 9 * * *", "UTC").unwrap();
        assert_eq!(
            entry_for(p).unwrap().state,
            State::Paused,
            "re-registering keeps the person's choice"
        );
        assert!(unregister(p).unwrap());
        assert!(load().is_empty());
        std::env::remove_var("WRIGHT_HOME");
    }
}
