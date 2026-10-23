//! CoreMIDI (macOS and iOS): a client per open port, an input port
//! connected to the chosen source with a read procedure, an output port
//! sending packet lists with `MIDISend`.
//!
//! Packet lists are walked by hand: `MIDIPacketList` and `MIDIPacket` are
//! declared under `#pragma pack(4)` (count at 0, first packet at 4; a
//! packet's 64-bit timestamp at 0, length at 8, data at 10), and on arm64
//! each following packet starts at the next 4-byte boundary — what the
//! SDK's `MIDIPacketNext` does. Port ids are the endpoints'
//! `kMIDIPropertyUniqueID`, stable across launches and replugging.

use std::ffi::{c_char, c_void};
use std::sync::Mutex;

use super::{Feed, MidiPort};

type OsStatus = i32;
type Ref = u32;
type CfString = *const c_void;
type ReadProc = unsafe extern "C" fn(*const u8, *mut c_void, *mut c_void);

const UTF8: u32 = 0x0800_0100;

#[link(name = "CoreMIDI", kind = "framework")]
unsafe extern "C" {
    fn MIDIClientCreate(
        name: CfString,
        notify: *const c_void,
        refcon: *mut c_void,
        out: *mut Ref,
    ) -> OsStatus;
    fn MIDIClientDispose(client: Ref) -> OsStatus;
    fn MIDIInputPortCreate(
        client: Ref,
        name: CfString,
        read: ReadProc,
        refcon: *mut c_void,
        out: *mut Ref,
    ) -> OsStatus;
    fn MIDIOutputPortCreate(client: Ref, name: CfString, out: *mut Ref) -> OsStatus;
    fn MIDIPortConnectSource(port: Ref, source: Ref, conn: *mut c_void) -> OsStatus;
    fn MIDIPortDisconnectSource(port: Ref, source: Ref) -> OsStatus;
    fn MIDIPortDispose(port: Ref) -> OsStatus;
    fn MIDIGetNumberOfSources() -> usize;
    fn MIDIGetSource(i: usize) -> Ref;
    fn MIDIGetNumberOfDestinations() -> usize;
    fn MIDIGetDestination(i: usize) -> Ref;
    fn MIDIObjectGetStringProperty(obj: Ref, prop: CfString, out: *mut CfString) -> OsStatus;
    fn MIDIObjectGetIntegerProperty(obj: Ref, prop: CfString, out: *mut i32) -> OsStatus;
    fn MIDISend(port: Ref, dest: Ref, list: *const u8) -> OsStatus;
    static kMIDIPropertyDisplayName: CfString;
    static kMIDIPropertyUniqueID: CfString;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithBytes(
        alloc: *const c_void,
        bytes: *const u8,
        len: isize,
        encoding: u32,
        external: u8,
    ) -> CfString;
    fn CFStringGetCString(s: CfString, buf: *mut c_char, len: isize, encoding: u32) -> u8;
    fn CFRelease(p: *const c_void);
}

/// An owned `CFStringRef`.
struct Cf(CfString);

impl Cf {
    fn new(s: &str) -> Self {
        // SAFETY: valid UTF-8 bytes and length; the result is released in
        // `drop` (null on failure, which CoreMIDI treats as no name).
        Self(unsafe {
            CFStringCreateWithBytes(
                std::ptr::null(),
                s.as_ptr(),
                isize::try_from(s.len()).unwrap_or(0),
                UTF8,
                0,
            )
        })
    }
}

impl Drop for Cf {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: a string we own.
            unsafe { CFRelease(self.0) };
        }
    }
}

fn status(s: OsStatus, what: &str) -> std::io::Result<()> {
    if s == 0 {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "{what} failed (OSStatus {s})"
        )))
    }
}

fn describe(endpoint: Ref) -> MidiPort {
    let mut id = 0i32;
    let mut cf: CfString = std::ptr::null();
    let mut name = String::new();
    // SAFETY: property reads on a live endpoint; the returned string is
    // ours to release, and the buffer is sized as passed.
    unsafe {
        MIDIObjectGetIntegerProperty(endpoint, kMIDIPropertyUniqueID, &raw mut id);
        if MIDIObjectGetStringProperty(endpoint, kMIDIPropertyDisplayName, &raw mut cf) == 0
            && !cf.is_null()
        {
            let mut buf = [0 as c_char; 512];
            if CFStringGetCString(cf, buf.as_mut_ptr(), 512, UTF8) != 0 {
                name = std::ffi::CStr::from_ptr(buf.as_ptr())
                    .to_string_lossy()
                    .into_owned();
            }
            CFRelease(cf);
        }
    }
    MidiPort {
        id: id.to_string(),
        name: if name.is_empty() {
            format!("endpoint {id}")
        } else {
            name
        },
    }
}

pub(super) fn inputs() -> Vec<MidiPort> {
    // SAFETY: CoreMIDI enumeration.
    unsafe {
        (0..MIDIGetNumberOfSources())
            .map(|i| describe(MIDIGetSource(i)))
            .collect()
    }
}

pub(super) fn outputs() -> Vec<MidiPort> {
    // SAFETY: CoreMIDI enumeration.
    unsafe {
        (0..MIDIGetNumberOfDestinations())
            .map(|i| describe(MIDIGetDestination(i)))
            .collect()
    }
}

fn endpoint(port: &MidiPort, sources: bool) -> std::io::Result<Ref> {
    // SAFETY: CoreMIDI enumeration.
    unsafe {
        let n = if sources {
            MIDIGetNumberOfSources()
        } else {
            MIDIGetNumberOfDestinations()
        };
        (0..n)
            .map(|i| {
                if sources {
                    MIDIGetSource(i)
                } else {
                    MIDIGetDestination(i)
                }
            })
            .find(|&e| describe(e).id == port.id)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "endpoint gone"))
    }
}

fn client() -> std::io::Result<Ref> {
    let name = Cf::new("vieww-audio");
    let mut c = 0;
    // SAFETY: no notification callback; `c` written on success.
    status(
        unsafe { MIDIClientCreate(name.0, std::ptr::null(), std::ptr::null_mut(), &raw mut c) },
        "MIDIClientCreate",
    )?;
    Ok(c)
}

unsafe extern "C" fn on_packets(list: *const u8, refcon: *mut c_void, _conn: *mut c_void) {
    // SAFETY: `refcon` is the input's `Mutex<Feed>`, alive until the port
    // is disposed (which stops the read procedure).
    let feed = unsafe { &*(refcon as *const Mutex<Feed>) };
    if let Ok(mut f) = feed.lock() {
        // SAFETY: CoreMIDI's packet list for this call.
        unsafe {
            super::walk_packet_list(list, cfg!(target_arch = "aarch64"), |b| {
                f.bytes(b);
            })
        };
    }
}

struct Input {
    client: Ref,
    port: Ref,
    source: Ref,
    _feed: Box<Mutex<Feed>>,
}

impl Drop for Input {
    fn drop(&mut self) {
        // SAFETY: objects we created; disposing the port ends the read
        // procedure before the feed is freed.
        unsafe {
            MIDIPortDisconnectSource(self.port, self.source);
            MIDIPortDispose(self.port);
            MIDIClientDispose(self.client);
        }
    }
}

pub(super) fn open_input(port: &MidiPort, feed: Feed) -> std::io::Result<Box<dyn Send>> {
    let source = endpoint(port, true)?;
    let feed = Box::new(Mutex::new(feed));
    let c = client()?;
    let name = Cf::new("vieww-audio in");
    let mut p = 0;
    // SAFETY: the read procedure's refcon is the boxed feed, which the
    // input keeps until after the port is disposed.
    unsafe {
        if let Err(e) = status(
            MIDIInputPortCreate(
                c,
                name.0,
                on_packets,
                std::ptr::from_ref::<Mutex<Feed>>(&feed).cast_mut().cast(),
                &raw mut p,
            ),
            "MIDIInputPortCreate",
        ) {
            MIDIClientDispose(c);
            return Err(e);
        }
    }
    let input = Input {
        client: c,
        port: p,
        source,
        _feed: feed,
    };
    // SAFETY: a live port and source.
    status(
        unsafe { MIDIPortConnectSource(p, source, std::ptr::null_mut()) },
        "MIDIPortConnectSource",
    )?;
    Ok(Box::new(input))
}

pub(super) struct Out {
    client: Ref,
    port: Ref,
    dest: Ref,
    list: Vec<u8>,
}

impl Out {
    pub(super) fn open(port: &MidiPort) -> std::io::Result<Self> {
        let dest = endpoint(port, false)?;
        let c = client()?;
        let name = Cf::new("vieww-audio out");
        let mut p = 0;
        // SAFETY: `p` written on success.
        if let Err(e) = status(
            unsafe { MIDIOutputPortCreate(c, name.0, &raw mut p) },
            "MIDIOutputPortCreate",
        ) {
            // SAFETY: the client we just made.
            unsafe { MIDIClientDispose(c) };
            return Err(e);
        }
        Ok(Self {
            client: c,
            port: p,
            dest,
            list: Vec::new(),
        })
    }

    pub(super) fn send(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let len = u16::try_from(bytes.len())
            .map_err(|_| std::io::Error::other("a MIDI packet over 64 KiB"))?;
        // One packet, timestamp 0 ("now").
        self.list.clear();
        self.list.extend_from_slice(&1u32.to_le_bytes());
        self.list.extend_from_slice(&0u64.to_le_bytes());
        self.list.extend_from_slice(&len.to_le_bytes());
        self.list.extend_from_slice(bytes);
        // SAFETY: a well-formed one-packet list (layout in the module
        // docs) that lives for the call.
        status(
            unsafe { MIDISend(self.port, self.dest, self.list.as_ptr()) },
            "MIDISend",
        )
    }
}

impl Drop for Out {
    fn drop(&mut self) {
        // SAFETY: objects we created.
        unsafe {
            MIDIPortDispose(self.port);
            MIDIClientDispose(self.client);
        }
    }
}
