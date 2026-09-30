//! Open Sound Control — the wire protocol TouchDesigner's OSC In/Out,
//! openFrameworks' `ofxOsc`, Processing's `oscP5`, Max, Ableton and every
//! show-control rig speak (§2.10–§2.12).
//!
//! OSC 1.0 with the common 1.1 types: [`Packet`] is a [`Message`] (address
//! pattern + typed [`Arg`]s) or a [`Bundle`] (an NTP timetag and nested
//! packets). [`Packet::encode`]/[`Packet::decode`] are byte-exact to the
//! spec (4-byte alignment, big-endian, `,iffs`-style type tags);
//! [`matches`] implements address-pattern matching (`?`, `*`, `[a-z]`,
//! `[!…]`, `{foo,bar}`), and [`Router`] dispatches to handlers by pattern.
//! [`OscSocket`] is the UDP transport.

use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

/// An argument.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Int(i32),
    Float(f32),
    Str(String),
    Blob(Vec<u8>),
    Long(i64),
    Double(f64),
    /// NTP timetag.
    Time(u64),
    Bool(bool),
    Nil,
    Impulse,
    Char(char),
    /// RGBA.
    Color([u8; 4]),
    Midi([u8; 4]),
    Array(Vec<Arg>),
}

impl Arg {
    #[must_use]
    pub fn as_f32(&self) -> Option<f32> {
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        match self {
            Self::Int(v) => Some(*v as f32),
            Self::Float(v) => Some(*v),
            Self::Long(v) => Some(*v as f32),
            Self::Double(v) => Some(*v as f32),
            Self::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(s) => Some(s),
            _ => None,
        }
    }
}

/// A message.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub address: String,
    pub args: Vec<Arg>,
}

impl Message {
    #[must_use]
    pub fn new(address: &str, args: Vec<Arg>) -> Self {
        Self {
            address: address.to_owned(),
            args,
        }
    }
}

/// A bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    /// NTP timetag; 1 means "immediately".
    pub time: u64,
    pub content: Vec<Packet>,
}

/// An OSC packet.
#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    Message(Message),
    Bundle(Bundle),
}

/// Timetag "now / immediately".
pub const IMMEDIATELY: u64 = 1;

/// Seconds since the Unix epoch → NTP timetag.
#[must_use]
pub fn timetag_from_unix(secs: f64) -> u64 {
    let ntp = secs + 2_208_988_800.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = ntp.floor() as u64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frac = ((ntp - ntp.floor()) * 4_294_967_296.0) as u64;
    whole << 32 | frac
}

/// NTP timetag → seconds since the Unix epoch.
#[must_use]
pub fn timetag_to_unix(t: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let s = (t >> 32) as f64 + (t & 0xffff_ffff) as f64 / 4_294_967_296.0;
    s - 2_208_988_800.0
}

fn pad(out: &mut Vec<u8>) {
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend(s.as_bytes());
    out.push(0);
    pad(out);
}

fn tag_and_body(a: &Arg, tags: &mut String, body: &mut Vec<u8>) {
    match a {
        Arg::Int(v) => {
            tags.push('i');
            body.extend(v.to_be_bytes());
        }
        Arg::Float(v) => {
            tags.push('f');
            body.extend(v.to_be_bytes());
        }
        Arg::Str(s) => {
            tags.push('s');
            put_str(body, s);
        }
        Arg::Blob(b) => {
            tags.push('b');
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            body.extend((b.len() as i32).to_be_bytes());
            body.extend(b);
            pad(body);
        }
        Arg::Long(v) => {
            tags.push('h');
            body.extend(v.to_be_bytes());
        }
        Arg::Double(v) => {
            tags.push('d');
            body.extend(v.to_be_bytes());
        }
        Arg::Time(v) => {
            tags.push('t');
            body.extend(v.to_be_bytes());
        }
        Arg::Bool(b) => tags.push(if *b { 'T' } else { 'F' }),
        Arg::Nil => tags.push('N'),
        Arg::Impulse => tags.push('I'),
        Arg::Char(c) => {
            tags.push('c');
            body.extend((*c as u32).to_be_bytes());
        }
        Arg::Color(c) => {
            tags.push('r');
            body.extend(c);
        }
        Arg::Midi(m) => {
            tags.push('m');
            body.extend(m);
        }
        Arg::Array(v) => {
            tags.push('[');
            for x in v {
                tag_and_body(x, tags, body);
            }
            tags.push(']');
        }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let s = self.b.get(self.i..self.i + n).ok_or("truncated packet")?;
        self.i += n;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32, String> {
        let s = self.take(4)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn u64(&mut self) -> Result<u64, String> {
        let s = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        Ok(u64::from_be_bytes(a))
    }

    fn string(&mut self) -> Result<String, String> {
        let rest = &self.b[self.i..];
        let end = rest.iter().position(|&c| c == 0).ok_or("unterminated string")?;
        let s = String::from_utf8_lossy(&rest[..end]).into_owned();
        self.i += (end + 4) & !3;
        if self.i > self.b.len() {
            return Err("truncated string".into());
        }
        Ok(s)
    }
}

impl Packet {
    /// Encode to wire bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Self::Message(m) => {
                put_str(&mut out, &m.address);
                let mut tags = String::from(",");
                let mut body = Vec::new();
                for a in &m.args {
                    tag_and_body(a, &mut tags, &mut body);
                }
                put_str(&mut out, &tags);
                out.extend(body);
            }
            Self::Bundle(b) => {
                put_str(&mut out, "#bundle");
                out.extend(b.time.to_be_bytes());
                for p in &b.content {
                    let e = p.encode();
                    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                    out.extend((e.len() as i32).to_be_bytes());
                    out.extend(e);
                }
            }
        }
        out
    }

    /// Decode wire bytes.
    ///
    /// # Errors
    /// On malformed packets.
    pub fn decode(b: &[u8]) -> Result<Self, String> {
        if b.len() % 4 != 0 {
            return Err("OSC packets are 4-byte aligned".into());
        }
        let mut r = Reader { b, i: 0 };
        let head = r.string()?;
        if head == "#bundle" {
            let time = r.u64()?;
            let mut content = Vec::new();
            while r.i < b.len() {
                let n = r.u32()? as usize;
                content.push(Self::decode(r.take(n)?)?);
            }
            return Ok(Self::Bundle(Bundle { time, content }));
        }
        if !head.starts_with('/') {
            return Err(format!("bad address {head:?}"));
        }
        let tags = if r.i < b.len() { r.string()? } else { ",".into() };
        let mut stack: Vec<Vec<Arg>> = vec![Vec::new()];
        for t in tags.chars().skip(1) {
            let a = match t {
                'i' => Arg::Int(r.u32()?.cast_signed()),
                'f' => Arg::Float(f32::from_bits(r.u32()?)),
                's' | 'S' => Arg::Str(r.string()?),
                'b' => {
                    let n = r.u32()? as usize;
                    let d = r.take(n)?.to_vec();
                    r.i += (4 - n % 4) % 4;
                    Arg::Blob(d)
                }
                'h' => Arg::Long(r.u64()?.cast_signed()),
                'd' => Arg::Double(f64::from_bits(r.u64()?)),
                't' => Arg::Time(r.u64()?),
                'T' => Arg::Bool(true),
                'F' => Arg::Bool(false),
                'N' => Arg::Nil,
                'I' => Arg::Impulse,
                'c' => Arg::Char(char::from_u32(r.u32()?).unwrap_or('\u{fffd}')),
                'r' => {
                    let s = r.take(4)?;
                    Arg::Color([s[0], s[1], s[2], s[3]])
                }
                'm' => {
                    let s = r.take(4)?;
                    Arg::Midi([s[0], s[1], s[2], s[3]])
                }
                '[' => {
                    stack.push(Vec::new());
                    continue;
                }
                ']' => {
                    let v = stack.pop().ok_or("unbalanced ]")?;
                    Arg::Array(v)
                }
                other => return Err(format!("unknown type tag {other:?}")),
            };
            stack.last_mut().ok_or("unbalanced ]")?.push(a);
        }
        if stack.len() != 1 {
            return Err("unbalanced [".into());
        }
        Ok(Self::Message(Message {
            address: head,
            args: stack.pop().unwrap_or_default(),
        }))
    }

    /// All messages, bundles flattened (with their timetags).
    #[must_use]
    pub fn messages(&self) -> Vec<(u64, &Message)> {
        let mut out = Vec::new();
        fn walk<'a>(p: &'a Packet, t: u64, out: &mut Vec<(u64, &'a Message)>) {
            match p {
                Packet::Message(m) => out.push((t, m)),
                Packet::Bundle(b) => b.content.iter().for_each(|c| walk(c, b.time, out)),
            }
        }
        walk(self, IMMEDIATELY, &mut out);
        out
    }
}

/// OSC address-pattern match of `pattern` against a concrete `address`.
#[must_use]
pub fn matches(pattern: &str, address: &str) -> bool {
    let (p, a): (Vec<&str>, Vec<&str>) = (pattern.split('/').collect(), address.split('/').collect());
    // OSC 1.1 `//` wildcard: an empty part after the root matches any
    // number of address parts.
    fn parts(p: &[&str], a: &[&str]) -> bool {
        match (p.first(), a.first()) {
            (None, None) => true,
            (Some(&""), _) => (0..=a.len()).any(|k| parts(&p[1..], &a[k..])),
            (Some(x), Some(y)) => part(x.as_bytes(), y.as_bytes()) && parts(&p[1..], &a[1..]),
            _ => false,
        }
    }
    p.first() == Some(&"") && a.first() == Some(&"") && parts(&p[1..], &a[1..])
}

fn part(p: &[u8], s: &[u8]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some(b'*') => (0..=s.len()).any(|k| part(&p[1..], &s[k..])),
        Some(b'?') => !s.is_empty() && part(&p[1..], &s[1..]),
        Some(b'[') => {
            let Some(end) = p.iter().position(|&c| c == b']') else { return false };
            let Some(&c) = s.first() else { return false };
            let mut set = &p[1..end];
            let neg = set.first() == Some(&b'!');
            if neg {
                set = &set[1..];
            }
            let mut hit = false;
            let mut i = 0;
            while i < set.len() {
                if i + 2 < set.len() && set[i + 1] == b'-' {
                    hit |= (set[i]..=set[i + 2]).contains(&c);
                    i += 3;
                } else {
                    hit |= set[i] == c;
                    i += 1;
                }
            }
            hit != neg && part(&p[end + 1..], &s[1..])
        }
        Some(b'{') => {
            let Some(end) = p.iter().position(|&c| c == b'}') else { return false };
            p[1..end].split(|&c| c == b',').any(|alt| s.starts_with(alt) && part(&p[end + 1..], &s[alt.len()..]))
        }
        Some(&c) => s.first() == Some(&c) && part(&p[1..], &s[1..]),
    }
}

type Handler = Box<dyn FnMut(&Message)>;

/// Pattern → handler dispatch.
#[derive(Default)]
pub struct Router {
    routes: Vec<(String, Handler)>,
}

impl std::fmt::Debug for Router {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.routes.iter().map(|r| &r.0)).finish()
    }
}

impl Router {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Handle messages whose address matches `pattern`.
    pub fn on(&mut self, pattern: &str, f: impl FnMut(&Message) + 'static) -> &mut Self {
        self.routes.push((pattern.to_owned(), Box::new(f)));
        self
    }

    /// Dispatch every message in `packet`; returns how many handlers ran.
    pub fn dispatch(&mut self, packet: &Packet) -> usize {
        let mut n = 0;
        for (_, m) in packet.messages() {
            for (pat, h) in &mut self.routes {
                if matches(pat, &m.address) {
                    h(m);
                    n += 1;
                }
            }
        }
        n
    }
}

/// A UDP OSC endpoint.
#[derive(Debug)]
pub struct OscSocket {
    socket: UdpSocket,
}

impl OscSocket {
    /// Bind to `addr` (e.g. `"0.0.0.0:9000"`).
    ///
    /// # Errors
    /// If the bind fails.
    pub fn bind(addr: impl ToSocketAddrs) -> io::Result<Self> {
        Ok(Self { socket: UdpSocket::bind(addr)? })
    }

    /// The bound address.
    ///
    /// # Errors
    /// From the OS.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    /// Send a packet.
    ///
    /// # Errors
    /// From the OS.
    pub fn send_to(&self, packet: &Packet, to: impl ToSocketAddrs) -> io::Result<usize> {
        self.socket.send_to(&packet.encode(), to)
    }

    /// Receive one packet (blocking, or with the timeout set by
    /// [`set_timeout`](Self::set_timeout)).
    ///
    /// # Errors
    /// From the OS, or `InvalidData` for a malformed packet.
    pub fn recv(&self) -> io::Result<(Packet, SocketAddr)> {
        let mut buf = vec![0u8; 65_536];
        let (n, from) = self.socket.recv_from(&mut buf)?;
        let p = Packet::decode(&buf[..n]).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok((p, from))
    }

    /// Set a receive timeout.
    ///
    /// # Errors
    /// From the OS.
    pub fn set_timeout(&self, d: Option<std::time::Duration>) -> io::Result<()> {
        self.socket.set_read_timeout(d)
    }

    /// Non-blocking drain of everything waiting.
    ///
    /// # Errors
    /// From the OS.
    pub fn poll(&self) -> io::Result<Vec<Packet>> {
        self.socket.set_nonblocking(true)?;
        let mut out = Vec::new();
        let mut buf = vec![0u8; 65_536];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((n, _)) => {
                    if let Ok(p) = Packet::decode(&buf[..n]) {
                        out.push(p);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    self.socket.set_nonblocking(false)?;
                    return Err(e);
                }
            }
        }
        self.socket.set_nonblocking(false)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_example_bytes() {
        // The OSC 1.0 spec's "/oscillator/4/frequency" ,f 440.0 example.
        let p = Packet::Message(Message::new("/oscillator/4/frequency", vec![Arg::Float(440.0)]));
        let b = p.encode();
        let mut expect = b"/oscillator/4/frequency\0,f\0\0".to_vec();
        expect.extend([0x43, 0xdc, 0x00, 0x00]);
        assert_eq!(b, expect);
        assert_eq!(Packet::decode(&b).unwrap(), p);
    }

    #[test]
    fn every_type_round_trips_in_a_bundle() {
        let m = Message::new(
            "/all",
            vec![
                Arg::Int(-7),
                Arg::Float(1.5),
                Arg::Str("hello".into()),
                Arg::Blob(vec![1, 2, 3, 4, 5]),
                Arg::Long(-1 << 40),
                Arg::Double(std::f64::consts::PI),
                Arg::Time(42),
                Arg::Bool(true),
                Arg::Bool(false),
                Arg::Nil,
                Arg::Impulse,
                Arg::Char('x'),
                Arg::Color([1, 2, 3, 4]),
                Arg::Midi([0, 0x90, 60, 100]),
                Arg::Array(vec![Arg::Int(1), Arg::Array(vec![Arg::Str("n".into())])]),
            ],
        );
        let p = Packet::Bundle(Bundle {
            time: timetag_from_unix(1_700_000_000.25),
            content: vec![Packet::Message(m), Packet::Bundle(Bundle { time: IMMEDIATELY, content: vec![Packet::Message(Message::new("/x", vec![]))] })],
        });
        let b = p.encode();
        assert_eq!(b.len() % 4, 0);
        let back = Packet::decode(&b).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.messages().len(), 2);
        let Packet::Bundle(bb) = back else { panic!() };
        assert!((timetag_to_unix(bb.time) - 1_700_000_000.25).abs() < 1e-6);
    }

    #[test]
    fn address_patterns() {
        assert!(matches("/synth/*/freq", "/synth/3/freq"));
        assert!(matches("/synth/?/freq", "/synth/3/freq"));
        assert!(!matches("/synth/?/freq", "/synth/33/freq"));
        assert!(matches("/layer[1-3]/opacity", "/layer2/opacity"));
        assert!(!matches("/layer[!1-3]/opacity", "/layer2/opacity"));
        assert!(matches("/{kick,snare}/hit", "/snare/hit"));
        assert!(!matches("/{kick,snare}/hit", "/hat/hit"));
        assert!(matches("//hit", "/a/b/c/hit"));
        assert!(!matches("/a/*", "/a/b/c"));
    }

    #[test]
    fn router_dispatches_by_pattern() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let got = Rc::new(RefCell::new(Vec::new()));
        let mut r = Router::new();
        let g = got.clone();
        r.on("/fader/*", move |m| g.borrow_mut().push((m.address.clone(), m.args[0].as_f32().unwrap())));
        let p = Packet::Bundle(Bundle {
            time: IMMEDIATELY,
            content: vec![
                Packet::Message(Message::new("/fader/1", vec![Arg::Float(0.5)])),
                Packet::Message(Message::new("/button/1", vec![Arg::Int(1)])),
                Packet::Message(Message::new("/fader/2", vec![Arg::Int(1)])),
            ],
        });
        assert_eq!(r.dispatch(&p), 2);
        assert_eq!(*got.borrow(), vec![("/fader/1".into(), 0.5), ("/fader/2".into(), 1.0)]);
    }

    #[test]
    fn udp_loopback() {
        let a = OscSocket::bind("127.0.0.1:0").unwrap();
        let b = OscSocket::bind("127.0.0.1:0").unwrap();
        b.set_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        let p = Packet::Message(Message::new("/ping", vec![Arg::Int(7)]));
        a.send_to(&p, b.local_addr().unwrap()).unwrap();
        let (got, from) = b.recv().unwrap();
        assert_eq!(got, p);
        assert_eq!(from, a.local_addr().unwrap());
    }

    #[test]
    fn malformed_input_is_an_error_not_a_panic() {
        for bad in [&b"/a\0"[..], b"/a\0\0,i\0\0", b"nope", b"/a\0\0,[\0\0", b"#bundle\0\0\0\0\0"] {
            assert!(Packet::decode(bad).is_err(), "{bad:?}");
        }
    }
}
