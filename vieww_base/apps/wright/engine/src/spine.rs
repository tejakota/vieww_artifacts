//! The spine sink: every event of a firing goes through here.
//!
//! One [`Spine`] per run. `emit` appends the event to the durable log
//! (fsynced — see `wright_runtime::store`), then hands it to any live
//! listeners (the CLI printer, an in-process UI). Agents run on several
//! threads at once; the mutex makes the log's order the single,
//! deterministic order everyone replays.

use std::sync::Mutex;

use wright_core::events::{Event, EventKind, RunId};
use wright_runtime::store::RunWriter;

type Listener = Box<dyn Fn(&Event) + Send + Sync>;

pub struct Spine {
    writer: Mutex<RunWriter>,
    run: RunId,
    listeners: Vec<Listener>,
}

impl Spine {
    pub fn new(writer: RunWriter) -> Spine {
        let run = writer.run();
        Spine {
            writer: Mutex::new(writer),
            run,
            listeners: Vec::new(),
        }
    }

    pub fn run(&self) -> RunId {
        self.run
    }

    /// Add a live listener (call before the firing starts).
    pub fn listen(&mut self, f: impl Fn(&Event) + Send + Sync + 'static) {
        self.listeners.push(Box::new(f));
    }

    /// Append an event durably, then notify listeners. If the disk
    /// write fails the event is still shown live, and the error is
    /// returned so the firing can stop: an unrecorded firing is not a
    /// firing Wright can stand behind.
    pub fn emit(
        &self,
        kind: EventKind,
        agent: Option<&str>,
        note: impl Into<String>,
        data: String,
    ) -> Result<Event, String> {
        let e = {
            let mut w = self.writer.lock().unwrap_or_else(|p| p.into_inner());
            w.append(kind, agent, note, data)?
        };
        for l in &self.listeners {
            l(&e);
        }
        Ok(e)
    }

    /// Emit, ignoring a storage failure (for best-effort notes).
    pub fn note(&self, kind: EventKind, agent: Option<&str>, note: impl Into<String>) {
        let _ = self.emit(kind, agent, note, String::new());
    }

    /// A snapshot of the events so far.
    pub fn events(&self) -> Vec<Event> {
        self.writer
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .log()
            .events()
            .to_vec()
    }
}
