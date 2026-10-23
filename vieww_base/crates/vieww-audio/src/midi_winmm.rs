//! WinMM MIDI: `midiIn*` with a function callback, `midiOut*`.
//!
//! Short messages arrive packed in the callback's first parameter
//! (`MIM_DATA`); SysEx arrives in buffers the input owns (`MIM_LONGDATA`),
//! four of 1 KiB kept queued and re-queued from the callback, as RtMidi
//! does. Output sends a short message with `midiOutShortMsg` and SysEx with
//! a prepared header and `midiOutLongMsg`, waiting for the driver to hand
//! the buffer back. Structures follow `mmeapi.h` (64-bit layout; the test
//! checks sizes and offsets). Port ids are device indices — WinMM has no
//! stable identifier — and [`MidiInput::open`](super::MidiInput::open)
//! also matches a port's exact name, which is.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use super::{short_len, Feed, MidiPort};

type Handle = *mut c_void;
type MmResult = u32;

const CALLBACK_FUNCTION: u32 = 0x0003_0000;
const CALLBACK_NULL: u32 = 0;
const MIM_DATA: u32 = 0x3C3;
const MIM_LONGDATA: u32 = 0x3C4;
const MIDIERR_STILLPLAYING: MmResult = 65;
const SYSEX_BUFFERS: usize = 4;
const SYSEX_BYTES: usize = 1024;

/// `MIDIINCAPSW`.
#[repr(C)]
struct InCaps {
    mid: u16,
    pid: u16,
    driver_version: u32,
    name: [u16; 32],
    support: u32,
}

/// `MIDIOUTCAPSW`.
#[repr(C)]
struct OutCaps {
    mid: u16,
    pid: u16,
    driver_version: u32,
    name: [u16; 32],
    technology: u16,
    voices: u16,
    notes: u16,
    channel_mask: u16,
    support: u32,
}

/// `MIDIHDR`.
#[repr(C)]
struct MidiHdr {
    data: *mut u8,
    buffer_length: u32,
    bytes_recorded: u32,
    user: usize,
    flags: u32,
    next: *mut MidiHdr,
    reserved: usize,
    offset: u32,
    reserved2: [usize; 8],
}

impl MidiHdr {
    const fn new(data: *mut u8, len: u32) -> Self {
        Self {
            data,
            buffer_length: len,
            bytes_recorded: len,
            user: 0,
            flags: 0,
            next: std::ptr::null_mut(),
            reserved: 0,
            offset: 0,
            reserved2: [0; 8],
        }
    }
}

const HDR: u32 = std::mem::size_of::<MidiHdr>() as u32;

#[link(name = "winmm")]
unsafe extern "system" {
    fn midiInGetNumDevs() -> u32;
    fn midiInGetDevCapsW(id: usize, caps: *mut InCaps, size: u32) -> MmResult;
    fn midiOutGetNumDevs() -> u32;
    fn midiOutGetDevCapsW(id: usize, caps: *mut OutCaps, size: u32) -> MmResult;
    fn midiInOpen(
        h: *mut Handle,
        id: u32,
        callback: usize,
        instance: usize,
        flags: u32,
    ) -> MmResult;
    fn midiInStart(h: Handle) -> MmResult;
    fn midiInStop(h: Handle) -> MmResult;
    fn midiInReset(h: Handle) -> MmResult;
    fn midiInClose(h: Handle) -> MmResult;
    fn midiInPrepareHeader(h: Handle, hdr: *mut MidiHdr, size: u32) -> MmResult;
    fn midiInUnprepareHeader(h: Handle, hdr: *mut MidiHdr, size: u32) -> MmResult;
    fn midiInAddBuffer(h: Handle, hdr: *mut MidiHdr, size: u32) -> MmResult;
    fn midiOutOpen(
        h: *mut Handle,
        id: u32,
        callback: usize,
        instance: usize,
        flags: u32,
    ) -> MmResult;
    fn midiOutShortMsg(h: Handle, msg: u32) -> MmResult;
    fn midiOutLongMsg(h: Handle, hdr: *mut MidiHdr, size: u32) -> MmResult;
    fn midiOutPrepareHeader(h: Handle, hdr: *mut MidiHdr, size: u32) -> MmResult;
    fn midiOutUnprepareHeader(h: Handle, hdr: *mut MidiHdr, size: u32) -> MmResult;
    fn midiOutReset(h: Handle) -> MmResult;
    fn midiOutClose(h: Handle) -> MmResult;
}

fn wide(name: &[u16]) -> String {
    let n = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    String::from_utf16_lossy(&name[..n])
}

fn mm(r: MmResult, what: &str) -> std::io::Result<()> {
    if r == 0 {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "{what} failed (MMRESULT {r})"
        )))
    }
}

pub(super) fn inputs() -> Vec<MidiPort> {
    // SAFETY: plain queries; each caps struct is sized and passed with
    // its size.
    unsafe {
        (0..midiInGetNumDevs())
            .filter_map(|i| {
                let mut c: InCaps = std::mem::zeroed();
                (midiInGetDevCapsW(i as usize, &raw mut c, size_of::<InCaps>() as u32) == 0).then(
                    || MidiPort {
                        id: i.to_string(),
                        name: wide(&c.name),
                    },
                )
            })
            .collect()
    }
}

pub(super) fn outputs() -> Vec<MidiPort> {
    // SAFETY: as in `inputs`.
    unsafe {
        (0..midiOutGetNumDevs())
            .filter_map(|i| {
                let mut c: OutCaps = std::mem::zeroed();
                (midiOutGetDevCapsW(i as usize, &raw mut c, size_of::<OutCaps>() as u32) == 0).then(
                    || MidiPort {
                        id: i.to_string(),
                        name: wide(&c.name),
                    },
                )
            })
            .collect()
    }
}

fn index(port: &MidiPort) -> std::io::Result<u32> {
    port.id
        .parse()
        .map_err(|_| std::io::Error::other(format!("bad WinMM port id {:?}", port.id)))
}

/// What the callback reaches through its instance pointer.
struct Shared {
    feed: Mutex<Feed>,
    closing: AtomicBool,
}

/// An open input: the handle, the callback's state and the SysEx buffers
/// the driver holds pointers into (all boxed, so they do not move).
struct Input {
    handle: Handle,
    shared: Box<Shared>,
    headers: Box<[MidiHdr; SYSEX_BUFFERS]>,
    _buffers: Box<[[u8; SYSEX_BYTES]; SYSEX_BUFFERS]>,
}

// SAFETY: the handle and the header pointers are only used by `drop`,
// which runs once on whichever thread owns the input; WinMM handles are
// not thread-affine.
unsafe impl Send for Input {}

impl Drop for Input {
    fn drop(&mut self) {
        self.shared.closing.store(true, Ordering::Release);
        // SAFETY: a handle we opened; Reset returns every queued buffer
        // (the callback sees `closing` and does not re-queue), after which
        // the headers can be unprepared and the buffers freed.
        unsafe {
            midiInStop(self.handle);
            midiInReset(self.handle);
            for h in self.headers.iter_mut() {
                midiInUnprepareHeader(self.handle, h, HDR);
            }
            midiInClose(self.handle);
        }
    }
}

unsafe extern "system" fn on_input(h: Handle, msg: u32, instance: usize, p1: usize, _p2: usize) {
    // SAFETY: `instance` is the `Shared` the input keeps alive until after
    // `midiInClose`, which stops callbacks.
    let shared = unsafe { &*(instance as *const Shared) };
    match msg {
        MIM_DATA => {
            #[expect(clippy::cast_possible_truncation, reason = "a packed u32 message")]
            let b = (p1 as u32).to_le_bytes();
            if let Ok(mut f) = shared.feed.lock() {
                f.bytes(&b[..short_len(b[0])]);
            }
        }
        MIM_LONGDATA => {
            let hdr = p1 as *mut MidiHdr;
            // SAFETY: the driver hands back one of our prepared headers;
            // `bytes_recorded` bytes of its buffer are valid.
            unsafe {
                let n = (*hdr).bytes_recorded as usize;
                if n > 0 {
                    let data = std::slice::from_raw_parts((*hdr).data, n);
                    if let Ok(mut f) = shared.feed.lock() {
                        f.bytes(data);
                    }
                }
                if !shared.closing.load(Ordering::Acquire) {
                    midiInAddBuffer(h, hdr, HDR);
                }
            }
        }
        _ => {}
    }
}

pub(super) fn open_input(port: &MidiPort, feed: Feed) -> std::io::Result<Box<dyn Send>> {
    let id = index(port)?;
    let shared = Box::new(Shared {
        feed: Mutex::new(feed),
        closing: AtomicBool::new(false),
    });
    let mut buffers = Box::new([[0u8; SYSEX_BYTES]; SYSEX_BUFFERS]);
    let headers = Box::new(std::array::from_fn::<_, SYSEX_BUFFERS, _>(|i| {
        MidiHdr::new(buffers[i].as_mut_ptr(), SYSEX_BYTES as u32)
    }));
    let mut handle: Handle = std::ptr::null_mut();
    // SAFETY: the callback and its instance pointer stay valid for the
    // handle's life (see `Input::drop`).
    unsafe {
        mm(
            midiInOpen(
                &raw mut handle,
                id,
                on_input as *const () as usize,
                std::ptr::from_ref::<Shared>(&shared) as usize,
                CALLBACK_FUNCTION,
            ),
            "midiInOpen",
        )?;
    }
    let mut input = Input {
        handle,
        shared,
        headers,
        _buffers: buffers,
    };
    // SAFETY: an open handle; headers point into boxed buffers that live
    // as long as `input`.
    unsafe {
        for h in input.headers.iter_mut() {
            h.bytes_recorded = 0;
            mm(midiInPrepareHeader(handle, h, HDR), "midiInPrepareHeader")?;
            mm(midiInAddBuffer(handle, h, HDR), "midiInAddBuffer")?;
        }
        mm(midiInStart(handle), "midiInStart")?;
    }
    Ok(Box::new(input))
}

pub(super) struct Out {
    handle: Handle,
}

// SAFETY: a WinMM output handle may be used from any thread.
unsafe impl Send for Out {}

impl Out {
    pub(super) fn open(port: &MidiPort) -> std::io::Result<Self> {
        let id = index(port)?;
        let mut handle: Handle = std::ptr::null_mut();
        // SAFETY: no callback; the handle is written on success.
        unsafe {
            mm(
                midiOutOpen(&raw mut handle, id, 0, 0, CALLBACK_NULL),
                "midiOutOpen",
            )?;
        }
        Ok(Self { handle })
    }

    pub(super) fn send(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        if bytes.len() <= 3 && bytes[0] != 0xF0 {
            let mut w = [0u8; 4];
            w[..bytes.len()].copy_from_slice(bytes);
            // SAFETY: an open output handle.
            return mm(
                unsafe { midiOutShortMsg(self.handle, u32::from_le_bytes(w)) },
                "midiOutShortMsg",
            );
        }
        let mut data = bytes.to_vec();
        let len =
            u32::try_from(data.len()).map_err(|_| std::io::Error::other("SysEx over 4 GiB"))?;
        let mut hdr = MidiHdr::new(data.as_mut_ptr(), len);
        // SAFETY: `hdr` and `data` outlive the exchange: we wait until the
        // driver releases the buffer (unprepare stops reporting
        // STILLPLAYING) before either is dropped.
        unsafe {
            mm(
                midiOutPrepareHeader(self.handle, &raw mut hdr, HDR),
                "midiOutPrepareHeader",
            )?;
            let sent = midiOutLongMsg(self.handle, &raw mut hdr, HDR);
            let start = std::time::Instant::now();
            loop {
                let r = midiOutUnprepareHeader(self.handle, &raw mut hdr, HDR);
                if r != MIDIERR_STILLPLAYING {
                    break;
                }
                if start.elapsed() > std::time::Duration::from_secs(10) {
                    // A driver that never returns the buffer: take it back
                    // by force rather than free memory it still holds.
                    midiOutReset(self.handle);
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            mm(sent, "midiOutLongMsg")
        }
    }
}

impl Drop for Out {
    fn drop(&mut self) {
        // SAFETY: a handle we opened.
        unsafe {
            midiOutReset(self.handle);
            midiOutClose(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structures_have_the_sdk_layout() {
        assert_eq!(size_of::<InCaps>(), 76);
        assert_eq!(size_of::<OutCaps>(), 84);
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(size_of::<MidiHdr>(), 120);
            assert_eq!(std::mem::offset_of!(MidiHdr, bytes_recorded), 12);
            assert_eq!(std::mem::offset_of!(MidiHdr, flags), 24);
            assert_eq!(std::mem::offset_of!(MidiHdr, next), 32);
            assert_eq!(std::mem::offset_of!(MidiHdr, offset), 48);
        }
    }
}
