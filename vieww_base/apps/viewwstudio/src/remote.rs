//! The out-of-process preview: the compiled screen runs in a child process,
//! and a panic or an abort in it costs a frame, not the studio.
//!
//! # Why
//!
//! The in-process preview (`loaded`) `dlopen`s the guest into the studio and
//! relies on a shared `libstd` to catch its panics. MSVC ships no dynamic
//! `std`, so on Windows a panic in a later `build()` is a foreign exception and
//! takes the window and every unsaved buffer with it (`PENDING.md` §3.3). And
//! on every platform a guest can do worse than panic: `abort`, overflow its
//! stack, dereference null, call `exit`. No `catch_unwind` reaches any of
//! those. A process boundary reaches all of them.
//!
//! # Shape
//!
//! ```text
//!   studio (host)                         viewwstudio-preview (child)
//!   RemotePreview::spawn ── spawns ─────▶ serve(): reads stdin, writes stdout
//!   load(library)        ── Load ───────▶ loaded::Preview::load, mount in a
//!                        ◀─ Loaded ─────   FrameDriver under ui::preview::guest_tree
//!   frame(env, events)   ── Frame ──────▶ pointer events in, draw_frame_at,
//!                        ◀─ Frame ──────   NativeRenderer → RGBA, wants_frame
//!   (child died)         ◀─ EOF ────────  RemoteError::Crashed { status, stderr }
//! ```
//!
//! The child mounts the guest inside [`crate::ui::preview::guest_tree`] —
//! the function the in-process pane uses — under a root that reads the
//! environment (platform, dark mode, metrics, accessibility) from a signal, so
//! a change to any of them rebuilds the wrapper and keeps the guest's own
//! state, exactly as the in-process pane does.
//!
//! # The wire
//!
//! Frames on stdin/stdout: a little-endian `u32` length, a tag byte, the
//! payload. Hand-written rather than serde'd — this workspace does not use
//! serde (`PENDING.md`'s non-goals), and five message shapes do not need it.
//! The child's stderr is collected by the host and returned with a crash, so
//! the panic message the guest printed is what the studio shows.
//!
//! # What is not here yet
//!
//! Keyboard and text input, scroll and hover are not forwarded — pointer
//! press, move and release are, which is what a tap and a drag need. The
//! studio's state capture across Renders (`capture_preview_state`) reads the
//! in-process element tree and has no remote counterpart.

use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use vieww_foundation::{
    Accessibility, Color, EdgeInsets, Offset, PointerEvent, PointerId, Size, ViewMetrics,
};

use crate::state::Platform;

/// The child binary's file name, next to the studio's own executable.
pub const CHILD_BINARY: &str = if cfg!(windows) {
    "viewwstudio-preview.exe"
} else {
    "viewwstudio-preview"
};

/// Everything around the guest that the pane decides: the simulated device.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Environment {
    pub platform: Platform,
    pub dark: bool,
    pub metrics: ViewMetrics,
    pub accessibility: Accessibility,
}

impl Environment {
    /// The device's own metrics and default accessibility, light.
    #[must_use]
    pub fn of(platform: Platform) -> Self {
        Self {
            platform,
            dark: false,
            metrics: platform.view_metrics(),
            accessibility: Accessibility::default(),
        }
    }
}

/// A pointer event, in the guest's logical coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pointer {
    Down(Offset),
    Move(Offset),
    Up(Offset),
}

/// One rendered frame of the guest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Straight-alpha RGBA8, row-major.
    pub rgba: Vec<u8>,
    /// The guest is animating (or a gesture wants a tick): ask again.
    pub wants_frame: bool,
}

/// What went wrong talking to the child.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteError {
    /// The child binary could not be started.
    Spawn(String),
    /// The child exited — a panic it could not catch, an abort, a signal.
    /// `stderr` is the end of what it printed, which is where the guest's
    /// panic message is.
    Crashed { status: String, stderr: String },
    /// `loaded::Preview::load` refused the library (the same refusals the
    /// in-process path has: an ABI mismatch, no entry point, a panicking
    /// `screen()`).
    Load(String),
    /// The preview was compiled against a different build of `vieww` than
    /// the child links — the studio's cue to try its next candidate rlib, as
    /// it does for `loaded::LoadError::AbiMismatch` in process.
    AbiMismatch(String),
    /// The child answered with something this host does not understand.
    Protocol(String),
}

impl std::fmt::Display for RemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "could not start the preview process: {e}"),
            Self::Crashed { status, stderr } => {
                write!(f, "the preview process stopped ({status})")?;
                if !stderr.trim().is_empty() {
                    write!(f, ":\n{}", stderr.trim_end())?;
                }
                Ok(())
            }
            Self::Load(e) | Self::AbiMismatch(e) => write!(f, "{e}"),
            Self::Protocol(e) => write!(f, "preview protocol: {e}"),
        }
    }
}

impl std::error::Error for RemoteError {}

// ── the wire ────────────────────────────────────────────────────────────────

const LOAD: u8 = 1;
const FRAME: u8 = 2;
const SHUTDOWN: u8 = 3;

const LOADED: u8 = 1;
const LOAD_FAILED: u8 = 2;
const FRAME_DONE: u8 = 3;

/// `LOAD_FAILED`'s first byte: why.
const FAILED_OTHER: u8 = 0;
const FAILED_ABI: u8 = 1;

/// The largest message either side will accept: a 4096² RGBA frame and change.
const MAX_MESSAGE: u32 = 4096 * 4096 * 4 + 64;

#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, v: &[u8]) {
        self.u32(u32::try_from(v.len()).unwrap_or(u32::MAX));
        self.0.extend_from_slice(v);
    }
    fn insets(&mut self, e: EdgeInsets) {
        for v in [e.left, e.top, e.right, e.bottom] {
            self.f32(v);
        }
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> io::Result<&[u8]> {
        if self.0.len() < n {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "message truncated",
            ));
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Ok(head)
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> io::Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f32(&mut self) -> io::Result<f32> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn bytes(&mut self) -> io::Result<Vec<u8>> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }
    fn insets(&mut self) -> io::Result<EdgeInsets> {
        Ok(EdgeInsets {
            left: self.f32()?,
            top: self.f32()?,
            right: self.f32()?,
            bottom: self.f32()?,
        })
    }
}

fn send(out: &mut impl Write, tag: u8, payload: &[u8]) -> io::Result<()> {
    let len = u32::try_from(payload.len() + 1)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "message too large"))?;
    out.write_all(&len.to_le_bytes())?;
    out.write_all(&[tag])?;
    out.write_all(payload)?;
    out.flush()
}

/// `Ok(None)` at a clean end of stream.
fn receive(input: &mut impl Read) -> io::Result<Option<(u8, Vec<u8>)>> {
    let mut len = [0u8; 4];
    match input.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_le_bytes(len);
    if len == 0 || len > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("message of {len} bytes"),
        ));
    }
    let mut body = vec![0u8; len as usize];
    input.read_exact(&mut body)?;
    let tag = body[0];
    body.remove(0);
    Ok(Some((tag, body)))
}

const fn platform_byte(p: Platform) -> u8 {
    match p {
        Platform::Ios => 0,
        Platform::Android => 1,
        Platform::Desktop => 2,
    }
}

fn platform_of(b: u8) -> io::Result<Platform> {
    match b {
        0 => Ok(Platform::Ios),
        1 => Ok(Platform::Android),
        2 => Ok(Platform::Desktop),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unknown platform",
        )),
    }
}

fn write_frame_request(
    env: &Environment,
    scale: f32,
    at: Duration,
    base: Color,
    events: &[Pointer],
) -> Vec<u8> {
    let mut w = Writer::default();
    w.u8(platform_byte(env.platform));
    w.u8(u8::from(env.dark));
    w.f32(env.metrics.size.width);
    w.f32(env.metrics.size.height);
    w.f32(env.metrics.device_pixel_ratio);
    w.insets(env.metrics.safe_area);
    w.insets(env.metrics.view_insets);
    w.u8(u8::from(env.accessibility.reduce_motion));
    w.u8(u8::from(env.accessibility.high_contrast));
    w.f32(env.accessibility.text_scale);
    w.f32(scale);
    w.u32(u32::try_from(at.as_millis()).unwrap_or(u32::MAX));
    for c in [base.r, base.g, base.b, base.a] {
        w.u8(c);
    }
    w.u32(u32::try_from(events.len()).unwrap_or(0));
    for e in events {
        let (kind, at) = match *e {
            Pointer::Down(p) => (0, p),
            Pointer::Move(p) => (1, p),
            Pointer::Up(p) => (2, p),
        };
        w.u8(kind);
        w.f32(at.dx);
        w.f32(at.dy);
    }
    w.0
}

struct FrameRequest {
    env: Environment,
    scale: f32,
    at: Duration,
    base: Color,
    events: Vec<Pointer>,
}

fn read_frame_request(body: &[u8]) -> io::Result<FrameRequest> {
    let mut r = Reader(body);
    let platform = platform_of(r.u8()?)?;
    let dark = r.u8()? != 0;
    let size = Size::new(r.f32()?, r.f32()?);
    let device_pixel_ratio = r.f32()?;
    let safe_area = r.insets()?;
    let view_insets = r.insets()?;
    let accessibility = Accessibility {
        reduce_motion: r.u8()? != 0,
        high_contrast: r.u8()? != 0,
        text_scale: r.f32()?,
    };
    let scale = r.f32()?;
    let at = Duration::from_millis(u64::from(r.u32()?));
    let base = Color::rgba(r.u8()?, r.u8()?, r.u8()?, r.u8()?);
    let n = r.u32()? as usize;
    let mut events = Vec::with_capacity(n.min(1024));
    for _ in 0..n {
        let kind = r.u8()?;
        let at = Offset::new(r.f32()?, r.f32()?);
        events.push(match kind {
            0 => Pointer::Down(at),
            1 => Pointer::Move(at),
            _ => Pointer::Up(at),
        });
    }
    Ok(FrameRequest {
        env: Environment {
            platform,
            dark,
            metrics: ViewMetrics {
                size,
                device_pixel_ratio,
                safe_area,
                view_insets,
            },
            accessibility,
        },
        scale,
        at,
        base,
        events,
    })
}

// ── the host ────────────────────────────────────────────────────────────────

/// A running preview process. Dropping it ends the process.
pub struct RemotePreview {
    child: Child,
    input: BufWriter<ChildStdin>,
    output: BufReader<ChildStdout>,
    stderr: Arc<Mutex<Vec<u8>>>,
    dead: Option<RemoteError>,
    frames: u64,
}

impl std::fmt::Debug for RemotePreview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemotePreview")
            .field("pid", &self.child.id())
            .field("dead", &self.dead)
            .finish_non_exhaustive()
    }
}

/// How much of the child's stderr a crash report keeps — the end, where the
/// panic message and the abort reason are.
const STDERR_TAIL: usize = 8 * 1024;

impl RemotePreview {
    /// The child binary next to the running executable, if it is there.
    #[must_use]
    pub fn sibling_binary() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        // `cargo test` puts test binaries one level down, in `deps/`.
        [dir.join(CHILD_BINARY), dir.parent()?.join(CHILD_BINARY)]
            .into_iter()
            .find(|p| p.is_file())
    }

    /// Start `binary` (normally [`Self::sibling_binary`]).
    ///
    /// # Errors
    ///
    /// [`RemoteError::Spawn`] when the process cannot be started.
    pub fn spawn(binary: &Path) -> Result<Self, RemoteError> {
        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RemoteError::Spawn(format!("{}: {e}", binary.display())))?;
        let input = BufWriter::new(
            child
                .stdin
                .take()
                .ok_or_else(|| RemoteError::Spawn("no stdin".into()))?,
        );
        let output = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| RemoteError::Spawn("no stdout".into()))?,
        );
        let stderr = Arc::new(Mutex::new(Vec::new()));
        if let Some(mut pipe) = child.stderr.take() {
            let sink = Arc::clone(&stderr);
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = pipe.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    if let Ok(mut all) = sink.lock() {
                        all.extend_from_slice(&buf[..n]);
                        if all.len() > STDERR_TAIL * 2 {
                            let cut = all.len() - STDERR_TAIL;
                            all.drain(..cut);
                        }
                    }
                }
            });
        }
        Ok(Self {
            child,
            input,
            output,
            stderr,
            dead: None,
            frames: 0,
        })
    }

    /// Whether the process has died; a dead preview answers every request
    /// with the same [`RemoteError::Crashed`].
    #[must_use]
    pub const fn crashed(&self) -> Option<&RemoteError> {
        self.dead.as_ref()
    }

    /// Frames the child has drawn for this host.
    #[must_use]
    pub const fn frames(&self) -> u64 {
        self.frames
    }

    /// The process id, for a test that wants to watch it die.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    fn crash(&mut self) -> RemoteError {
        if let Some(dead) = &self.dead {
            return dead.clone();
        }
        let status = match self.child.wait() {
            Ok(status) => status.to_string(),
            Err(e) => format!("unknown: {e}"),
        };
        // The stderr thread may still be draining the pipe; give it a moment.
        std::thread::sleep(Duration::from_millis(50));
        let bytes = self.stderr.lock().map(|b| b.clone()).unwrap_or_default();
        let start = bytes.len().saturating_sub(STDERR_TAIL);
        let error = RemoteError::Crashed {
            status,
            stderr: String::from_utf8_lossy(&bytes[start..]).into_owned(),
        };
        self.dead = Some(error.clone());
        error
    }

    fn request(&mut self, tag: u8, payload: &[u8]) -> Result<(u8, Vec<u8>), RemoteError> {
        if let Some(dead) = &self.dead {
            return Err(dead.clone());
        }
        if send(&mut self.input, tag, payload).is_err() {
            return Err(self.crash());
        }
        match receive(&mut self.output) {
            Ok(Some(reply)) => Ok(reply),
            Ok(None) | Err(_) => Err(self.crash()),
        }
    }

    /// Load and mount a compiled preview. `sysroot_lib` is the toolchain's
    /// `lib` directory, for `loaded::prime_libstd` in the child.
    ///
    /// # Errors
    ///
    /// [`RemoteError::Load`] for the in-process path's refusals,
    /// [`RemoteError::Crashed`] if loading killed the child.
    pub fn load(&mut self, library: &Path, sysroot_lib: Option<&Path>) -> Result<(), RemoteError> {
        let mut w = Writer::default();
        w.bytes(library.to_string_lossy().as_bytes());
        w.bytes(
            sysroot_lib
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
                .as_bytes(),
        );
        match self.request(LOAD, &w.0)? {
            (LOADED, _) => Ok(()),
            (LOAD_FAILED, body) => {
                let mut r = Reader(&body);
                let kind = r.u8().unwrap_or(0);
                let message = r
                    .bytes()
                    .map(|b| String::from_utf8_lossy(&b).into_owned())
                    .unwrap_or_default();
                Err(if kind == FAILED_ABI {
                    RemoteError::AbiMismatch(message)
                } else {
                    RemoteError::Load(message)
                })
            }
            (tag, _) => Err(RemoteError::Protocol(format!(
                "unexpected reply {tag} to Load"
            ))),
        }
    }

    /// Feed `events`, draw the frame for time `at`, and return its pixels at
    /// the device size times `scale`, over `base`.
    ///
    /// # Errors
    ///
    /// [`RemoteError::Crashed`] when the guest killed the child — this frame
    /// or any earlier one.
    pub fn frame(
        &mut self,
        env: &Environment,
        scale: f32,
        at: Duration,
        base: Color,
        events: &[Pointer],
    ) -> Result<Frame, RemoteError> {
        let payload = write_frame_request(env, scale, at, base, events);
        match self.request(FRAME, &payload)? {
            (FRAME_DONE, body) => {
                let mut r = Reader(&body);
                let mut parse = || -> io::Result<Frame> {
                    let width = r.u32()?;
                    let height = r.u32()?;
                    let wants_frame = r.u8()? != 0;
                    let rgba = r.bytes()?;
                    Ok(Frame {
                        width,
                        height,
                        rgba,
                        wants_frame,
                    })
                };
                let frame = parse().map_err(|e| RemoteError::Protocol(e.to_string()))?;
                self.frames += 1;
                Ok(frame)
            }
            (LOAD_FAILED, _) => Err(RemoteError::Load("no preview is loaded".into())),
            (tag, _) => Err(RemoteError::Protocol(format!(
                "unexpected reply {tag} to Frame"
            ))),
        }
    }
}

impl Drop for RemotePreview {
    fn drop(&mut self) {
        if self.dead.is_none() {
            let _ = send(&mut self.input, SHUTDOWN, &[]);
            // A guest stuck in a loop never reads the shutdown.
            for _ in 0..20 {
                if matches!(self.child.try_wait(), Ok(Some(_))) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A [`RemotePreview`] the studio's state and its pane share. Cloning shares
/// the one process; equality is identity, which is what a `Signal` needs to
/// know whether the preview changed.
#[derive(Clone)]
pub struct RemoteHandle(std::rc::Rc<std::cell::RefCell<RemotePreview>>);

impl RemoteHandle {
    #[must_use]
    pub fn new(remote: RemotePreview) -> Self {
        Self(std::rc::Rc::new(std::cell::RefCell::new(remote)))
    }

    /// The process, for one request at a time.
    #[must_use]
    pub fn borrow_mut(&self) -> std::cell::RefMut<'_, RemotePreview> {
        self.0.borrow_mut()
    }
}

impl PartialEq for RemoteHandle {
    fn eq(&self, other: &Self) -> bool {
        std::rc::Rc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for RemoteHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RemoteHandle")
            .field(&self.0.borrow())
            .finish()
    }
}

// ── the child ───────────────────────────────────────────────────────────────

/// The guest mounted under the environment signal, so an environment change
/// rebuilds the wrapper and keeps the guest's state.
struct Hosted {
    env: vieww_element::Signal<Environment>,
    guest: vieww_widget::WidgetNode,
}

impl std::fmt::Debug for Hosted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Hosted")
    }
}

impl vieww_widget::Widget for Hosted {
    fn debug_name(&self) -> &'static str {
        "Hosted"
    }
    fn kind(&self) -> vieww_widget::WidgetKind<'_> {
        vieww_widget::WidgetKind::Composed
    }
    fn build(&self, _ctx: &vieww_widget::BuildContext) -> vieww_widget::WidgetNode {
        let env = self.env.get();
        crate::ui::preview::guest_tree(
            self.guest.clone(),
            env.metrics,
            env.accessibility,
            env.platform,
            env.dark,
        )
    }
}

vieww_widget::widget_node_from!(Hosted);

struct Mounted {
    driver: vieww_render::FrameDriver,
    env: vieww_element::Signal<Environment>,
    // Kept alive: the library image stays mapped for the process's life.
    _preview: crate::loaded::Preview,
}

/// The child's loop: answer requests on `input` until it ends or says
/// `Shutdown`. A guest panic the element tree catches is reported by the tree
/// and the loop goes on; anything worse ends the process, which is the point.
///
/// # Errors
///
/// An I/O error on either pipe.
pub fn serve(input: &mut impl Read, output: &mut impl Write) -> io::Result<()> {
    let mut mounted: Option<Mounted> = None;
    let mut renderer = vieww_paint::native::NativeRenderer::new();
    let mut next_pointer = 1u64;
    let mut active: Option<PointerId> = None;
    let mut last = Offset::ZERO;
    while let Some((tag, body)) = receive(input)? {
        match tag {
            LOAD => {
                let mut r = Reader(&body);
                let library = PathBuf::from(String::from_utf8_lossy(&r.bytes()?).into_owned());
                let sysroot = String::from_utf8_lossy(&r.bytes()?).into_owned();
                if !sysroot.is_empty() {
                    crate::loaded::prime_libstd(Path::new(&sysroot));
                }
                match crate::loaded::Preview::load(&library) {
                    Ok(preview) => {
                        let env0 = Environment::of(Platform::Ios);
                        let mut driver = vieww_render::FrameDriver::new(env0.metrics.size);
                        // The studio's own error policy, as in process: a guest
                        // panic the tree catches becomes the error placeholder.
                        crate::install(&mut driver);
                        let env = driver.elements().runtime().signal(env0);
                        driver.set_root(Hosted {
                            env: env.clone(),
                            guest: preview.node(),
                        });
                        mounted = Some(Mounted {
                            driver,
                            env,
                            _preview: preview,
                        });
                        send(output, LOADED, &[])?;
                    }
                    Err(error) => {
                        let mut w = Writer::default();
                        w.u8(
                            if matches!(error, crate::loaded::LoadError::AbiMismatch { .. }) {
                                FAILED_ABI
                            } else {
                                FAILED_OTHER
                            },
                        );
                        w.bytes(error.to_string().as_bytes());
                        send(output, LOAD_FAILED, &w.0)?;
                    }
                }
            }
            FRAME => {
                let request = read_frame_request(&body)?;
                let Some(m) = mounted.as_mut() else {
                    let mut w = Writer::default();
                    w.u8(FAILED_OTHER);
                    w.bytes(b"no preview is loaded");
                    send(output, LOAD_FAILED, &w.0)?;
                    continue;
                };
                if m.env.get() != request.env {
                    m.env.set(request.env);
                }
                let logical = request.env.metrics.size;
                m.driver.resize(logical);
                for event in &request.events {
                    let pointer = match event {
                        Pointer::Down(p) => {
                            let id = PointerId(next_pointer);
                            next_pointer += 1;
                            active = Some(id);
                            last = *p;
                            PointerEvent::down(id, *p, request.at)
                        }
                        Pointer::Move(p) => {
                            let Some(id) = active else { continue };
                            let previous = last;
                            last = *p;
                            PointerEvent::moved(id, previous, *p, request.at)
                        }
                        Pointer::Up(p) => {
                            let Some(id) = active.take() else { continue };
                            PointerEvent::up(id, *p, request.at)
                        }
                    };
                    m.driver.handle_pointer(&pointer);
                }
                m.driver.draw_frame_at(request.at);
                let scale = if request.scale.is_finite() && request.scale > 0.0 {
                    request.scale
                } else {
                    1.0
                };
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "a device size times a small scale"
                )]
                let (width, height) = (
                    (logical.width * scale).round().max(1.0) as u32,
                    (logical.height * scale).round().max(1.0) as u32,
                );
                let scene = m.driver.scene().scaled(scale);
                let (pixels, _) = renderer
                    .render_to_pixels(&scene, width, height, request.base)
                    .map_err(|e| io::Error::other(e.to_string()))?;
                let mut w = Writer::default();
                w.u32(width);
                w.u32(height);
                w.u8(u8::from(m.driver.needs_frame()));
                w.bytes(pixels.data());
                send(output, FRAME_DONE, &w.0)?;
            }
            SHUTDOWN => return Ok(()),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown request {tag}"),
                ));
            }
        }
    }
    Ok(())
}

/// [`serve`] on this process's stdin and stdout — the child binary's `main`.
///
/// # Errors
///
/// As [`serve`].
pub fn serve_stdio() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    serve(&mut stdin.lock(), &mut BufWriter::new(stdout.lock()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_request_round_trips() {
        let env = Environment {
            platform: Platform::Android,
            dark: true,
            metrics: Platform::Android.view_metrics(),
            accessibility: Accessibility {
                reduce_motion: true,
                high_contrast: false,
                text_scale: 1.3,
            },
        };
        let events = [
            Pointer::Down(Offset::new(10.0, 20.0)),
            Pointer::Move(Offset::new(12.5, 21.0)),
            Pointer::Up(Offset::new(12.5, 21.0)),
        ];
        let bytes = write_frame_request(
            &env,
            2.0,
            Duration::from_millis(1234),
            Color::WHITE,
            &events,
        );
        let back = read_frame_request(&bytes).unwrap();
        assert_eq!(back.env, env);
        assert!((back.scale - 2.0).abs() < f32::EPSILON);
        assert_eq!(back.at, Duration::from_millis(1234));
        assert_eq!(back.base, Color::WHITE);
        assert_eq!(back.events, events);
    }

    #[test]
    fn a_truncated_request_is_an_error_not_a_panic() {
        let env = Environment::of(Platform::Ios);
        let bytes = write_frame_request(&env, 1.0, Duration::ZERO, Color::BLACK, &[]);
        for cut in 0..bytes.len() {
            assert!(read_frame_request(&bytes[..cut]).is_err(), "cut at {cut}");
        }
    }

    #[test]
    fn framing_round_trips_and_rejects_nonsense_lengths() {
        let mut wire = Vec::new();
        send(&mut wire, FRAME, b"abc").unwrap();
        let (tag, body) = receive(&mut wire.as_slice()).unwrap().unwrap();
        assert_eq!((tag, body.as_slice()), (FRAME, &b"abc"[..]));
        assert!(receive(&mut [].as_slice()).unwrap().is_none());
        let huge = (MAX_MESSAGE + 1).to_le_bytes();
        assert!(receive(&mut huge.as_slice()).is_err());
    }
}
