//! Live MIDI input and output.
//!
//! [`midi`](crate::midi) is everything above the port; this is the port. A
//! MIDI device is, at bottom, a byte stream, so [`MidiInput`] runs a reader
//! thread over any `Read` — on Linux an ALSA rawmidi device
//! (`/dev/snd/midiC{card}D{device}`, opened by [`MidiInput::open_rawmidi`]
//! with nothing but the file system: rawmidi needs no ioctls to read) —
//! parses it with the running-status [`Parser`], and timestamps each
//! message on arrival against the input's own start instant. The audio
//! thread drains messages with [`MidiInput::poll`], which never blocks.
//!
//! [`MidiOutput`] writes messages to any `Write`, the rawmidi device
//! included. CoreMIDI, WinMM and Web MIDI are the same two seams on other
//! platforms; [`list_rawmidi`] enumerates the Linux devices present.

use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Instant;

use crate::midi::{Message, Parser};

/// A message and when it arrived, in seconds since the input opened.
#[derive(Debug, Clone, PartialEq)]
pub struct Timed {
    /// Arrival time.
    pub time: f64,
    /// The message.
    pub message: Message,
}

/// A live MIDI input.
#[derive(Debug)]
pub struct MidiInput {
    rx: mpsc::Receiver<Timed>,
    started: Instant,
}

impl MidiInput {
    /// Start reading `source` on a thread of its own.
    pub fn from_reader<R: Read + Send + 'static>(mut source: R) -> Self {
        let (tx, rx) = mpsc::channel();
        let started = Instant::now();
        std::thread::Builder::new()
            .name("vieww-midi-in".into())
            .spawn(move || {
                // Arrival timestamps are only as good as the wake-up.
                let _rt = crate::rt::promote_helper_thread();
                let mut parser = Parser::new();
                let mut buf = [0u8; 256];
                loop {
                    match source.read(&mut buf) {
                        Ok(0) | Err(_) => return,
                        Ok(n) => {
                            let t = started.elapsed().as_secs_f64();
                            for message in parser.feed(&buf[..n]) {
                                if tx.send(Timed { time: t, message }).is_err() {
                                    return;
                                }
                            }
                        }
                    }
                }
            })
            .ok();
        Self { rx, started }
    }

    /// Open an ALSA rawmidi device.
    ///
    /// # Errors
    /// The device does not exist or cannot be opened (no Linux, no device,
    /// no permission) — said as the OS said it.
    pub fn open_rawmidi(card: u32, device: u32) -> std::io::Result<Self> {
        let f = std::fs::File::open(format!("/dev/snd/midiC{card}D{device}"))?;
        Ok(Self::from_reader(f))
    }

    /// Every message received since the last poll. Never blocks.
    pub fn poll(&self) -> Vec<Timed> {
        self.rx.try_iter().collect()
    }

    /// Seconds since this input opened (the timestamps' clock).
    #[must_use]
    pub fn now(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
}

/// A live MIDI output over any byte sink.
#[derive(Debug)]
pub struct MidiOutput<W: Write> {
    sink: W,
}

impl<W: Write> MidiOutput<W> {
    /// Wrap a sink.
    pub const fn new(sink: W) -> Self {
        Self { sink }
    }

    /// Send one message.
    ///
    /// # Errors
    /// The sink's write error.
    pub fn send(&mut self, m: &Message) -> std::io::Result<()> {
        self.sink.write_all(&m.to_bytes())?;
        self.sink.flush()
    }

    /// The sink back.
    pub fn into_inner(self) -> W {
        self.sink
    }
}

impl MidiOutput<std::fs::File> {
    /// Open an ALSA rawmidi device for output.
    ///
    /// # Errors
    /// As [`MidiInput::open_rawmidi`].
    pub fn open_rawmidi(card: u32, device: u32) -> std::io::Result<Self> {
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(format!("/dev/snd/midiC{card}D{device}"))?;
        Ok(Self::new(f))
    }
}

/// The ALSA rawmidi devices present, as `(card, device)`.
#[must_use]
pub fn list_rawmidi() -> Vec<(u32, u32)> {
    let Ok(dir) = std::fs::read_dir("/dev/snd") else {
        return Vec::new();
    };
    let mut out: Vec<(u32, u32)> = dir
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let rest = name.strip_prefix("midiC")?;
            let (c, d) = rest.split_once('D')?;
            Some((c.parse().ok()?, d.parse().ok()?))
        })
        .collect();
    out.sort_unstable();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_arrive_timestamped_through_a_pipe() {
        let (reader, writer) = std::io::pipe().unwrap();
        let input = MidiInput::from_reader(reader);
        let mut out = MidiOutput::new(writer);
        out.send(&Message::NoteOn {
            channel: 2,
            key: 64,
            velocity: 99,
        })
        .unwrap();
        out.send(&Message::ControlChange {
            channel: 2,
            controller: 1,
            value: 7,
        })
        .unwrap();
        out.send(&Message::TimingClock).unwrap();
        drop(out);
        let mut got = Vec::new();
        let t0 = std::time::Instant::now();
        while got.len() < 3 && t0.elapsed().as_secs() < 5 {
            got.extend(input.poll());
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let msgs: Vec<Message> = got.iter().map(|t| t.message.clone()).collect();
        assert_eq!(
            msgs,
            vec![
                Message::NoteOn {
                    channel: 2,
                    key: 64,
                    velocity: 99
                },
                Message::ControlChange {
                    channel: 2,
                    controller: 1,
                    value: 7
                },
                Message::TimingClock
            ]
        );
        assert!(got.windows(2).all(|w| w[0].time <= w[1].time));
        assert!(input.now() >= got[2].time);
    }

    #[test]
    fn missing_devices_are_reported_not_faked() {
        if list_rawmidi().contains(&(99, 99)) {
            return;
        }
        assert!(MidiInput::open_rawmidi(99, 99).is_err());
        assert!(MidiOutput::open_rawmidi(99, 99).is_err());
    }
}
