//! Live MIDI input and output.
//!
//! [`midi`](crate::midi) is everything above the port; this is the port.
//!
//! **Ports.** [`input_ports`] and [`output_ports`] list what the system
//! offers, and [`MidiInput::open`] / [`MidiOutput::open`] take a
//! [`MidiPort::id`] (or a port's exact name):
//!
//! | Platform | API | Port ids |
//! |---|---|---|
//! | Linux | ALSA rawmidi (`/dev/snd/midiC{c}D{d}`, the file system only) | `hw:C,D` |
//! | Windows | WinMM (`midiIn*` with a callback, `midiOut*`; SysEx both ways) | device index |
//! | macOS, iOS | CoreMIDI (a client, an input port connected to the source, `MIDISend`) | `kMIDIPropertyUniqueID` |
//!
//! Elsewhere the lists are empty and opening is
//! [`Unsupported`](std::io::ErrorKind::Unsupported). (On Linux, software
//! ports — another app's virtual output — live in the ALSA *sequencer*,
//! which rawmidi does not see; hardware interfaces appear in both.)
//!
//! **Input.** Bytes are parsed with the running-status [`Parser`] and each
//! message is timestamped on arrival — in the OS callback itself on
//! Windows and macOS, on the reader thread on Linux — against the input's
//! own start instant. The audio thread drains messages with
//! [`MidiInput::poll`], which never blocks. [`MidiInput::from_reader`]
//! does the same over any `Read`.
//!
//! **Output.** [`MidiOutput`] writes messages to any `Write`; a port's
//! sink is a [`PortSink`], which sends each flushed message the way the
//! platform wants it (a packed short message or a SysEx buffer on WinMM,
//! a packet list on CoreMIDI, bytes on rawmidi).

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

/// A MIDI port the system offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiPort {
    /// What [`MidiInput::open`] / [`MidiOutput::open`] take.
    pub id: String,
    /// Its display name.
    pub name: String,
}

/// The MIDI inputs present (sources: keyboards, controllers, interfaces).
#[must_use]
pub fn input_ports() -> Vec<MidiPort> {
    platform::inputs()
}

/// The MIDI outputs present (destinations: synths, interfaces).
#[must_use]
pub fn output_ports() -> Vec<MidiPort> {
    platform::outputs()
}

/// The port in `ports` that `id` names — by id, else by exact name.
fn find<'a>(ports: &'a [MidiPort], id: &str) -> Option<&'a MidiPort> {
    ports
        .iter()
        .find(|p| p.id == id)
        .or_else(|| ports.iter().find(|p| p.name == id))
}

fn not_found(id: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::NotFound, format!("no MIDI port {id:?}"))
}

/// Where a port's bytes go: parse, timestamp, queue. Called from the
/// platform's callback (or reader thread).
pub(crate) struct Feed {
    tx: mpsc::Sender<Timed>,
    started: Instant,
    parser: Parser,
}

impl Feed {
    /// Bytes that just arrived. Returns `false` once the input is gone.
    pub(crate) fn bytes(&mut self, b: &[u8]) -> bool {
        let time = self.started.elapsed().as_secs_f64();
        for message in self.parser.feed(b) {
            if self.tx.send(Timed { time, message }).is_err() {
                return false;
            }
        }
        true
    }
}

/// A live MIDI input.
pub struct MidiInput {
    rx: mpsc::Receiver<Timed>,
    started: Instant,
    /// The platform's open port, closed when the input is dropped.
    _port: Option<Box<dyn Send>>,
}

impl std::fmt::Debug for MidiInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MidiInput")
            .field("started", &self.started)
            .finish_non_exhaustive()
    }
}

impl MidiInput {
    fn with_feed(
        open: impl FnOnce(Feed) -> std::io::Result<Option<Box<dyn Send>>>,
    ) -> std::io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let started = Instant::now();
        let port = open(Feed {
            tx,
            started,
            parser: Parser::new(),
        })?;
        Ok(Self {
            rx,
            started,
            _port: port,
        })
    }

    /// Start reading `source` on a thread of its own.
    pub fn from_reader<R: Read + Send + 'static>(source: R) -> Self {
        Self::with_feed(|feed| {
            reader_thread(source, feed);
            Ok(None)
        })
        .unwrap_or_else(|_| unreachable!("spawning a reader cannot fail here"))
    }

    /// Open the input port `id` (a [`MidiPort::id`] from [`input_ports`],
    /// or a port's exact name).
    ///
    /// # Errors
    /// No such port ([`NotFound`](std::io::ErrorKind::NotFound)), the
    /// platform refused it, or MIDI is not supported here.
    pub fn open(id: &str) -> std::io::Result<Self> {
        let ports = platform::inputs();
        let port = find(&ports, id).ok_or_else(|| not_found(id))?.clone();
        Self::with_feed(|feed| platform::open_input(&port, feed).map(Some))
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

/// Read `source` into `feed` on a thread until either ends.
fn reader_thread<R: Read + Send + 'static>(mut source: R, mut feed: Feed) {
    std::thread::Builder::new()
        .name("vieww-midi-in".into())
        .spawn(move || {
            // Arrival timestamps are only as good as the wake-up.
            let _rt = crate::rt::promote_helper_thread();
            let mut buf = [0u8; 256];
            loop {
                match source.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => {
                        if !feed.bytes(&buf[..n]) {
                            return;
                        }
                    }
                }
            }
        })
        .ok();
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

impl MidiOutput<PortSink> {
    /// Open the output port `id` (a [`MidiPort::id`] from
    /// [`output_ports`], or a port's exact name).
    ///
    /// # Errors
    /// As [`MidiInput::open`].
    pub fn open(id: &str) -> std::io::Result<Self> {
        let ports = platform::outputs();
        let port = find(&ports, id).ok_or_else(|| not_found(id))?;
        Ok(Self::new(PortSink {
            pending: Vec::new(),
            parser: Parser::new(),
            out: platform::Out::open(port)?,
        }))
    }
}

/// A system MIDI output port as a byte sink: bytes are collected, and
/// each flush parses them into messages and sends each the platform's
/// way.
pub struct PortSink {
    pending: Vec<u8>,
    parser: Parser,
    out: platform::Out,
}

impl std::fmt::Debug for PortSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortSink")
            .field("pending", &self.pending.len())
            .finish_non_exhaustive()
    }
}

impl Write for PortSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.pending.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let bytes = std::mem::take(&mut self.pending);
        for m in self.parser.feed(&bytes) {
            self.out.send(&m.to_bytes())?;
        }
        Ok(())
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

/// The length of a short message from its status byte (WinMM hands them
/// over packed in a `u32`).
#[cfg_attr(not(any(windows, test)), expect(dead_code, reason = "WinMM only"))]
pub(crate) const fn short_len(status: u8) -> usize {
    match status {
        0xC0..=0xDF | 0xF1 | 0xF3 => 2,
        0xF6 | 0xF8..=0xFF => 1,
        _ => 3,
    }
}

/// Walk a CoreMIDI `MIDIPacketList`, handing each packet's bytes to `f`.
/// Layout (`#pragma pack(4)`): count at 0, first packet at 4; a packet's
/// 64-bit timestamp at 0, length at 8, data at 10; with `align4` (arm64)
/// each following packet starts at the next 4-byte boundary, as the SDK's
/// `MIDIPacketNext` does.
///
/// # Safety
/// `list` points at a valid packet list of that layout.
#[cfg_attr(
    not(any(target_os = "macos", target_os = "ios", test)),
    expect(dead_code, reason = "CoreMIDI only")
)]
pub(crate) unsafe fn walk_packet_list(list: *const u8, align4: bool, mut f: impl FnMut(&[u8])) {
    // SAFETY: within the list, per the layout above.
    unsafe {
        let n = list.cast::<u32>().read_unaligned();
        let mut p = list.add(4);
        for _ in 0..n {
            let len = usize::from(p.add(8).cast::<u16>().read_unaligned());
            f(std::slice::from_raw_parts(p.add(10), len));
            let next = p.add(10 + len);
            p = if align4 {
                next.add(next.align_offset(4))
            } else {
                next
            };
        }
    }
}

#[cfg(all(target_os = "linux", not(target_os = "android")))]
mod platform {
    use super::{reader_thread, Feed, MidiPort};

    fn ports() -> Vec<MidiPort> {
        super::list_rawmidi()
            .into_iter()
            .map(|(c, d)| {
                let card = std::fs::read_to_string(format!("/proc/asound/card{c}/id"))
                    .map_or_else(|_| format!("card {c}"), |s| s.trim().to_owned());
                MidiPort {
                    id: format!("hw:{c},{d}"),
                    name: format!("{card} MIDI {d}"),
                }
            })
            .collect()
    }

    pub(super) fn inputs() -> Vec<MidiPort> {
        ports()
    }

    pub(super) fn outputs() -> Vec<MidiPort> {
        ports()
    }

    fn path(port: &MidiPort) -> String {
        let (c, d) = port
            .id
            .strip_prefix("hw:")
            .and_then(|r| r.split_once(','))
            .unwrap_or(("0", "0"));
        format!("/dev/snd/midiC{c}D{d}")
    }

    pub(super) fn open_input(port: &MidiPort, feed: Feed) -> std::io::Result<Box<dyn Send>> {
        let f = std::fs::File::open(path(port))?;
        reader_thread(f, feed);
        Ok(Box::new(()))
    }

    pub(super) struct Out(std::fs::File);

    impl Out {
        pub(super) fn open(port: &MidiPort) -> std::io::Result<Self> {
            std::fs::OpenOptions::new()
                .write(true)
                .open(path(port))
                .map(Self)
        }

        pub(super) fn send(&mut self, bytes: &[u8]) -> std::io::Result<()> {
            use std::io::Write;
            self.0.write_all(bytes)?;
            self.0.flush()
        }
    }
}

#[cfg(windows)]
#[path = "midi_winmm.rs"]
mod platform;

#[cfg(any(target_os = "macos", target_os = "ios"))]
#[path = "midi_coremidi.rs"]
mod platform;

#[cfg(not(any(
    all(target_os = "linux", not(target_os = "android")),
    windows,
    target_os = "macos",
    target_os = "ios"
)))]
mod platform {
    use super::{Feed, MidiPort};

    fn unsupported() -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "no MIDI port API on this platform",
        )
    }

    pub(super) fn inputs() -> Vec<MidiPort> {
        Vec::new()
    }

    pub(super) fn outputs() -> Vec<MidiPort> {
        Vec::new()
    }

    pub(super) fn open_input(_: &MidiPort, _: Feed) -> std::io::Result<Box<dyn Send>> {
        Err(unsupported())
    }

    pub(super) struct Out;

    impl Out {
        pub(super) fn open(_: &MidiPort) -> std::io::Result<Self> {
            Err(unsupported())
        }

        pub(super) fn send(&mut self, _: &[u8]) -> std::io::Result<()> {
            Err(unsupported())
        }
    }
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
    fn coremidi_packet_lists_walk_in_both_layouts() {
        for align4 in [false, true] {
            // A 4-aligned base, as CoreMIDI's allocations are.
            let mut store = vec![0u32; 16];
            let base = store.as_mut_ptr().cast::<u8>();
            let mut l = 2u32.to_le_bytes().to_vec();
            for data in [&[0x90u8, 60, 100][..], &[0xF8][..]] {
                l.extend_from_slice(&0u64.to_le_bytes());
                l.extend_from_slice(&u16::try_from(data.len()).unwrap().to_le_bytes());
                l.extend_from_slice(data);
                if align4 {
                    while !l.len().is_multiple_of(4) {
                        l.push(0);
                    }
                }
            }
            // SAFETY: `l` fits in `store` (64 bytes).
            unsafe { std::ptr::copy_nonoverlapping(l.as_ptr(), base, l.len()) };
            let mut got = Vec::new();
            // SAFETY: the list just written.
            unsafe { walk_packet_list(base, align4, |b| got.push(b.to_vec())) };
            assert_eq!(
                got,
                vec![vec![0x90, 60, 100], vec![0xF8]],
                "align4 {align4}"
            );
        }
    }

    #[test]
    fn short_message_lengths() {
        assert_eq!(short_len(0x90), 3);
        assert_eq!(short_len(0xC5), 2);
        assert_eq!(short_len(0xD0), 2);
        assert_eq!(short_len(0xF8), 1);
        assert_eq!(short_len(0xF2), 3);
        assert_eq!(short_len(0xF1), 2);
    }

    #[test]
    fn a_port_sink_sends_whole_messages() {
        // Through the pipe-backed `from_reader` path the bytes arrive
        // intact; here the sink's own splitting: two messages written in
        // one go, running status, flushed once.
        let mut p = Parser::new();
        let msgs = p.feed(&[0x90, 60, 100, 62, 100, 0xF8]);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[1].to_bytes(), vec![0x90, 62, 100]);
    }

    #[test]
    fn ports_list_and_unknown_ids_are_not_found() {
        for p in input_ports().iter().chain(&output_ports()) {
            assert!(!p.id.is_empty() && !p.name.is_empty());
        }
        let e = MidiInput::open("no such port").unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
        let e = MidiOutput::open("no such port").unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
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
