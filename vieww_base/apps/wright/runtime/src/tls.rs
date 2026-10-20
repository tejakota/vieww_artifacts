//! The TLS boundary. Doctrine line one: crypto is never hand-rolled.
//!
//! TLS reaches the OS-native implementation on each platform:
//!
//! | Platform | Backend                 | Binding             | Status (v0.2)        |
//! |----------|-------------------------|---------------------|----------------------|
//! | Linux    | system OpenSSL 1.1 / 3  | `dlopen` at runtime | real, tested live    |
//! | macOS    | Security.framework      | direct FFI          | refuses loudly (W-6) |
//! | Windows  | SChannel                | direct FFI          | refuses loudly (W-6) |
//!
//! Binding the operating system is not "adding a dependency" — it is
//! using the platform we already ship on. OpenSSL is loaded with
//! `dlopen` (not linked), so the binary builds anywhere and the library
//! the user's OS keeps patched is the one that runs.
//!
//! Verification is never optional: peer certificates are verified
//! against the system trust store (OpenSSL's default paths, which honor
//! `SSL_CERT_FILE` / `SSL_CERT_DIR`), the hostname is checked with
//! `SSL_set1_host`, SNI is sent, and TLS 1.2 is the floor. There is no
//! "insecure" switch, by design. A failure to load OpenSSL is an error,
//! never a downgrade to plaintext.
//!
//! This is one of the files AGENTS.md allows `unsafe` in. Every block
//! carries a SAFETY comment.

use std::io::{Read, Write};
use std::net::TcpStream;

/// A connected TLS stream over TCP.
pub struct TlsStream {
    #[cfg(target_os = "linux")]
    inner: linux::SslConn,
    #[cfg(not(target_os = "linux"))]
    _never: std::convert::Infallible,
}

impl TlsStream {
    /// Handshake over an already-connected TCP stream (which may be a
    /// CONNECT tunnel through a proxy). `host` is the name to verify.
    pub fn connect(tcp: TcpStream, host: &str) -> Result<TlsStream, String> {
        #[cfg(target_os = "linux")]
        {
            linux::SslConn::connect(tcp, host).map(|inner| TlsStream { inner })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (tcp, host);
            Err(format!(
                "TLS on {} is not wired yet (Spike W-6: {}); refusing to downgrade to plaintext",
                std::env::consts::OS,
                if cfg!(target_os = "macos") { "Security.framework" } else { "SChannel" }
            ))
        }
    }

    /// Which backend is in use, for `wright doctor`.
    pub fn backend() -> Result<String, String> {
        #[cfg(target_os = "linux")]
        {
            linux::lib().map(|l| l.version.clone())
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err("not wired on this OS yet (Spike W-6)".into())
        }
    }
}

impl Read for TlsStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        #[cfg(target_os = "linux")]
        {
            self.inner.read(buf)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = buf;
            match self._never {}
        }
    }
}

impl Write for TlsStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        #[cfg(target_os = "linux")]
        {
            self.inner.write(buf)
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = buf;
            match self._never {}
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::{c_char, c_int, c_long, c_ulong, c_void, CStr, CString};
    use std::io;
    use std::net::TcpStream;
    use std::os::fd::AsRawFd;
    use std::sync::OnceLock;

    const RTLD_NOW: c_int = 2;
    const SSL_VERIFY_PEER: c_int = 1;
    const SSL_CTRL_SET_TLSEXT_HOSTNAME: c_int = 55;
    const TLSEXT_NAMETYPE_HOST_NAME: c_long = 0;
    const SSL_CTRL_SET_MIN_PROTO_VERSION: c_int = 123;
    const TLS1_2_VERSION: c_long = 0x0303;
    const SSL_ERROR_WANT_READ: c_int = 2;
    const SSL_ERROR_WANT_WRITE: c_int = 3;
    const SSL_ERROR_SYSCALL: c_int = 5;
    const SSL_ERROR_ZERO_RETURN: c_int = 6;
    const X509_V_OK: c_long = 0;

    #[link(name = "dl")]
    extern "C" {
        fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }

    type FnMethod = unsafe extern "C" fn() -> *const c_void;
    type FnCtxNew = unsafe extern "C" fn(*const c_void) -> *mut c_void;
    type FnCtxFree = unsafe extern "C" fn(*mut c_void);
    type FnCtxInt = unsafe extern "C" fn(*mut c_void) -> c_int;
    type FnCtxSetVerify = unsafe extern "C" fn(*mut c_void, c_int, *const c_void);
    type FnCtrl = unsafe extern "C" fn(*mut c_void, c_int, c_long, *mut c_void) -> c_long;
    type FnSslNew = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
    type FnSslFree = unsafe extern "C" fn(*mut c_void);
    type FnSetFd = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
    type FnSet1Host = unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int;
    type FnSslInt = unsafe extern "C" fn(*mut c_void) -> c_int;
    type FnRw = unsafe extern "C" fn(*mut c_void, *mut c_void, c_int) -> c_int;
    type FnGetError = unsafe extern "C" fn(*const c_void, c_int) -> c_int;
    type FnVerifyResult = unsafe extern "C" fn(*const c_void) -> c_long;
    type FnErrGet = unsafe extern "C" fn() -> c_ulong;
    type FnErrString = unsafe extern "C" fn(c_ulong, *mut c_char, usize);
    type FnVersion = unsafe extern "C" fn(c_int) -> *const c_char;
    type FnAlpn = unsafe extern "C" fn(*mut c_void, *const u8, u32) -> c_int;

    pub struct Lib {
        pub version: String,
        tls_client_method: FnMethod,
        ctx_new: FnCtxNew,
        ctx_free: FnCtxFree,
        ctx_default_verify_paths: FnCtxInt,
        ctx_set_verify: FnCtxSetVerify,
        ctx_ctrl: FnCtrl,
        ssl_new: FnSslNew,
        ssl_free: FnSslFree,
        set_fd: FnSetFd,
        set1_host: FnSet1Host,
        ssl_ctrl: FnCtrl,
        connect: FnSslInt,
        shutdown: FnSslInt,
        read: FnRw,
        write: FnRw,
        get_error: FnGetError,
        verify_result: FnVerifyResult,
        err_get: FnErrGet,
        err_string: FnErrString,
        set_alpn: Option<FnAlpn>,
    }

    // SAFETY: the struct holds only function pointers into a library that
    // is never unloaded; OpenSSL >= 1.1 is thread-safe for these calls.
    unsafe impl Send for Lib {}
    unsafe impl Sync for Lib {}

    static LIB: OnceLock<Result<Lib, String>> = OnceLock::new();

    pub fn lib() -> Result<&'static Lib, String> {
        LIB.get_or_init(load).as_ref().map_err(|e| e.clone())
    }

    fn load() -> Result<Lib, String> {
        let names = ["libssl.so.3", "libssl.so.1.1", "libssl.so"];
        let mut handle = std::ptr::null_mut();
        for n in names {
            let c = CString::new(n).expect("static name");
            // SAFETY: dlopen with a valid NUL-terminated name; a null
            // return is handled. The handle is intentionally never closed.
            handle = unsafe { dlopen(c.as_ptr(), RTLD_NOW) };
            if !handle.is_null() {
                break;
            }
        }
        if handle.is_null() {
            return Err("the system OpenSSL library (libssl.so.3 or libssl.so.1.1) was not found; install OpenSSL to use HTTPS".into());
        }
        let sym = |name: &str| -> Result<*mut c_void, String> {
            let c = CString::new(name).expect("static name");
            // SAFETY: valid handle from dlopen, valid C string.
            let p = unsafe { dlsym(handle, c.as_ptr()) };
            if p.is_null() {
                Err(format!("OpenSSL is too old: missing {name} (need 1.1.1 or newer)"))
            } else {
                Ok(p)
            }
        };
        // SAFETY (all transmutes below): each symbol is the documented
        // OpenSSL function with exactly the signature of its type alias
        // (see openssl/ssl.h, openssl/err.h for 1.1.1 and 3.x).
        unsafe {
            let version_fn: FnVersion = std::mem::transmute::<*mut c_void, FnVersion>(sym("OpenSSL_version")?);
            let version = CStr::from_ptr(version_fn(0)).to_string_lossy().into_owned();
            Ok(Lib {
                version,
                tls_client_method: std::mem::transmute::<*mut c_void, FnMethod>(sym("TLS_client_method")?),
                ctx_new: std::mem::transmute::<*mut c_void, FnCtxNew>(sym("SSL_CTX_new")?),
                ctx_free: std::mem::transmute::<*mut c_void, FnCtxFree>(sym("SSL_CTX_free")?),
                ctx_default_verify_paths: std::mem::transmute::<*mut c_void, FnCtxInt>(sym("SSL_CTX_set_default_verify_paths")?),
                ctx_set_verify: std::mem::transmute::<*mut c_void, FnCtxSetVerify>(sym("SSL_CTX_set_verify")?),
                ctx_ctrl: std::mem::transmute::<*mut c_void, FnCtrl>(sym("SSL_CTX_ctrl")?),
                ssl_new: std::mem::transmute::<*mut c_void, FnSslNew>(sym("SSL_new")?),
                ssl_free: std::mem::transmute::<*mut c_void, FnSslFree>(sym("SSL_free")?),
                set_fd: std::mem::transmute::<*mut c_void, FnSetFd>(sym("SSL_set_fd")?),
                set1_host: std::mem::transmute::<*mut c_void, FnSet1Host>(sym("SSL_set1_host")?),
                ssl_ctrl: std::mem::transmute::<*mut c_void, FnCtrl>(sym("SSL_ctrl")?),
                connect: std::mem::transmute::<*mut c_void, FnSslInt>(sym("SSL_connect")?),
                shutdown: std::mem::transmute::<*mut c_void, FnSslInt>(sym("SSL_shutdown")?),
                read: std::mem::transmute::<*mut c_void, FnRw>(sym("SSL_read")?),
                write: std::mem::transmute::<*mut c_void, FnRw>(sym("SSL_write")?),
                get_error: std::mem::transmute::<*mut c_void, FnGetError>(sym("SSL_get_error")?),
                verify_result: std::mem::transmute::<*mut c_void, FnVerifyResult>(sym("SSL_get_verify_result")?),
                err_get: std::mem::transmute::<*mut c_void, FnErrGet>(sym("ERR_get_error")?),
                err_string: std::mem::transmute::<*mut c_void, FnErrString>(sym("ERR_error_string_n")?),
                set_alpn: sym("SSL_set_alpn_protos").ok().map(|p| std::mem::transmute::<*mut c_void, FnAlpn>(p)),
            })
        }
    }

    impl Lib {
        fn last_error(&self) -> String {
            let mut msgs = Vec::new();
            loop {
                // SAFETY: ERR_get_error has no preconditions.
                let e = unsafe { (self.err_get)() };
                if e == 0 {
                    break;
                }
                let mut buf = [0 as c_char; 256];
                // SAFETY: buffer and its exact length are passed.
                unsafe { (self.err_string)(e, buf.as_mut_ptr(), buf.len()) };
                // SAFETY: ERR_error_string_n always NUL-terminates.
                msgs.push(unsafe { CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().into_owned());
            }
            if msgs.is_empty() { "unknown OpenSSL error".into() } else { msgs.join("; ") }
        }
    }

    /// One TLS connection. Owns the TCP stream (and so the fd) and the
    /// OpenSSL objects; Drop frees them in the right order.
    pub struct SslConn {
        lib: &'static Lib,
        ctx: *mut c_void,
        ssl: *mut c_void,
        tcp: TcpStream,
        closed: bool,
    }

    // SAFETY: an SslConn is used by one thread at a time (it is moved,
    // never shared); OpenSSL permits an SSL object to move between threads.
    unsafe impl Send for SslConn {}

    impl SslConn {
        pub fn connect(tcp: TcpStream, host: &str) -> Result<SslConn, String> {
            let lib = lib()?;
            let chost = CString::new(host).map_err(|_| "host name contains NUL".to_string())?;
            // SAFETY: TLS_client_method has no preconditions; SSL_CTX_new
            // returns null on failure, which we check.
            let ctx = unsafe { (lib.ctx_new)((lib.tls_client_method)()) };
            if ctx.is_null() {
                return Err(format!("TLS context: {}", lib.last_error()));
            }
            // From here on, the guard frees what we allocated on early return.
            let mut conn = SslConn { lib, ctx, ssl: std::ptr::null_mut(), tcp, closed: false };
            // SAFETY: ctx is a valid SSL_CTX for every call below.
            unsafe {
                if (lib.ctx_default_verify_paths)(ctx) != 1 {
                    return Err(format!("loading the system trust store: {}", lib.last_error()));
                }
                (lib.ctx_set_verify)(ctx, SSL_VERIFY_PEER, std::ptr::null());
                (lib.ctx_ctrl)(ctx, SSL_CTRL_SET_MIN_PROTO_VERSION, TLS1_2_VERSION, std::ptr::null_mut());
                conn.ssl = (lib.ssl_new)(ctx);
            }
            if conn.ssl.is_null() {
                return Err(format!("TLS session: {}", lib.last_error()));
            }
            let ssl = conn.ssl;
            // SAFETY: ssl is valid; the fd belongs to conn.tcp, which lives
            // as long as ssl (both owned by conn); chost outlives the calls
            // (OpenSSL copies the name in both SNI and set1_host).
            unsafe {
                if (lib.set_fd)(ssl, conn.tcp.as_raw_fd()) != 1 {
                    return Err(format!("TLS fd: {}", lib.last_error()));
                }
                (lib.ssl_ctrl)(ssl, SSL_CTRL_SET_TLSEXT_HOSTNAME, TLSEXT_NAMETYPE_HOST_NAME, chost.as_ptr() as *mut c_void);
                if (lib.set1_host)(ssl, chost.as_ptr()) != 1 {
                    return Err(format!("TLS hostname check setup: {}", lib.last_error()));
                }
                if let Some(alpn) = lib.set_alpn {
                    // ALPN wire format: length-prefixed protocol names.
                    let protos = b"\x08http/1.1";
                    alpn(ssl, protos.as_ptr(), protos.len() as u32);
                }
                let rc = (lib.connect)(ssl);
                if rc != 1 {
                    let verify = (lib.verify_result)(ssl);
                    let detail = lib.last_error();
                    return Err(if verify != X509_V_OK {
                        format!("TLS handshake with {host} failed: the certificate could not be verified (X509 error {verify}) — {detail}")
                    } else {
                        format!("TLS handshake with {host} failed: {detail}")
                    });
                }
                if (lib.verify_result)(ssl) != X509_V_OK {
                    return Err(format!("TLS: certificate for {host} did not verify"));
                }
            }
            Ok(conn)
        }

        fn io_error(&self, rc: c_int, what: &str) -> io::Error {
            // SAFETY: ssl is valid while self lives.
            let code = unsafe { (self.lib.get_error)(self.ssl, rc) };
            match code {
                SSL_ERROR_WANT_READ | SSL_ERROR_WANT_WRITE => io::Error::new(io::ErrorKind::WouldBlock, format!("TLS {what} timed out")),
                SSL_ERROR_SYSCALL => {
                    let os = io::Error::last_os_error();
                    if os.raw_os_error().unwrap_or(0) == 0 {
                        io::Error::new(io::ErrorKind::UnexpectedEof, format!("TLS {what}: connection closed"))
                    } else {
                        os
                    }
                }
                _ => io::Error::other(format!("TLS {what}: {}", self.lib.last_error())),
            }
        }

        pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.closed || buf.is_empty() {
                return Ok(0);
            }
            let len = buf.len().min(c_int::MAX as usize) as c_int;
            // SAFETY: ssl valid; buf is valid for len bytes.
            let rc = unsafe { (self.lib.read)(self.ssl, buf.as_mut_ptr() as *mut c_void, len) };
            if rc > 0 {
                return Ok(rc as usize);
            }
            // SAFETY: as above.
            let code = unsafe { (self.lib.get_error)(self.ssl, rc) };
            if code == SSL_ERROR_ZERO_RETURN {
                self.closed = true;
                return Ok(0);
            }
            let e = self.io_error(rc, "read");
            if e.kind() == io::ErrorKind::UnexpectedEof {
                // Many servers close without close_notify after
                // Connection: close; framing (Content-Length/chunked)
                // already guards against truncation one layer up.
                self.closed = true;
                return Ok(0);
            }
            Err(e)
        }

        pub fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if buf.is_empty() {
                return Ok(0);
            }
            let len = buf.len().min(c_int::MAX as usize) as c_int;
            // SAFETY: ssl valid; buf is valid for len bytes.
            let rc = unsafe { (self.lib.write)(self.ssl, buf.as_ptr() as *mut c_void, len) };
            if rc > 0 {
                Ok(rc as usize)
            } else {
                Err(self.io_error(rc, "write"))
            }
        }
    }

    impl Drop for SslConn {
        fn drop(&mut self) {
            // SAFETY: pointers are either null or valid and owned by us;
            // SSL is freed before its CTX, and both before the fd closes
            // (self.tcp drops after this function returns).
            unsafe {
                if !self.ssl.is_null() {
                    (self.lib.shutdown)(self.ssl);
                    (self.lib.ssl_free)(self.ssl);
                }
                if !self.ctx.is_null() {
                    (self.lib.ctx_free)(self.ctx);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn openssl_loads_from_the_system() {
        let v = TlsStream::backend().expect("system OpenSSL should be present on CI and dev machines");
        assert!(v.starts_with("OpenSSL"), "{v}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn handshake_against_a_non_tls_server_fails_cleanly() {
        // A local plaintext server: the handshake must fail with an
        // error, never hang and never fall back to plaintext.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let t = std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let _ = s.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n");
            }
        });
        let tcp = TcpStream::connect(addr).unwrap();
        tcp.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
        let err = match TlsStream::connect(tcp, "localhost") {
            Err(e) => e,
            Ok(_) => panic!("a plaintext server must not complete a TLS handshake"),
        };
        assert!(err.contains("TLS handshake"), "{err}");
        t.join().unwrap();
    }
}
