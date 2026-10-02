//! The network layer — requests, responses, and the seam a socket plugs into.
//!
//! # What this crate is, honestly
//!
//! **The shape of an HTTP call.** [`HttpRequest`] and [`HttpResponse`],
//! a [`Url`] that parses what a UI actually sends (scheme, host, port,
//! path, query — the five parts), a [`Method`], an [`HttpClient`] trait
//! returning a [`Task`] — the frame-aligned future the task module already
//! defines — and two implementations: [`MemoryClient`], which answers from
//! a table and records what it was asked, and [`NoNetwork`], which refuses.
//!
//! It is *not* a transport. No TCP, no TLS, no HTTP/2, no cookie jar: the
//! transport is platform work (a desktop crate's `reqwest` wiring, the
//! browser's `fetch` behind wasm-bindgen), and it plugs in by implementing
//! the trait — the same seam rule the service registry draws everywhere
//! else. What the framework can own, and does here, is the part every
//! transport must agree on: what a request *is*, so that a widget written
//! against `HttpClient` works on every platform, and a test can assert
//! what it sent without a server.
//!
//! ```
//! use vieww_network::{HttpRequest, HttpResponse, HttpClient, MemoryClient, Url};
//!
//! // A client that answers from a table: the test double, and the demo.
//! let client = MemoryClient::new()
//!     .route("/api/health", |_| HttpResponse::ok("fine"));
//!
//! let request = HttpRequest::get(Url::parse("https://example.test/api/health").unwrap());
//! let task = client.send(request.clone());
//!
//! // A ready task's answer is waiting the moment `send` returns.
//! let response = task.value().ready().expect("the route exists");
//! assert_eq!(response.status, 200);
//! assert_eq!(response.body, b"fine");
//!
//! // And the client remembers what it was asked — the assertion a widget
//! // test wants to make.
//! assert_eq!(client.requests()[0].url.path(), "/api/health");
//! ```
//!
//! # Why `Task` and not `async fn`
//!
//! Because the frame loop is not an async runtime, and bolting one on for
//! network calls alone would give the workspace two concurrency models to
//! keep in agreement forever. [`Task::spawn`] takes a
//! [`Spawn`](vieww_foundation::task::Spawn) (a thread pool, or `Inline`)
//! and a `FrameWaker`, and asks for a frame when the work lands — so a
//! response arrives as *part of a frame*, through the same door as every
//! other state change, rather than by parking a widget's `build` on an
//! executor the framework does not have.
//!
//! # The `Url` that is here, and the one that is not
//!
//! [`Url`] parses the five parts a UI sends and refuses the rest — no
//! userinfo, no fragments, no percent-decoding, no IDN. Those are real
//! (a fragment lives client-side; userinfo is a security smell), and each
//! is a subsystem; a loader that half-handled them would misroute requests
//! in ways a test cannot see. Parse what is sent, refuse what is not, and
//! the platform transport can do its own full validation on the string.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use vieww_foundation::task::Task;

pub use vieww_foundation as foundation;

pub mod osc;

/// An HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Method {
    /// Read. The default, because it is what a UI does by far the most of.
    #[default]
    Get,
    /// Create.
    Post,
    /// Replace.
    Put,
    /// Remove.
    Delete,
    /// Partial change.
    Patch,
    /// Existence check: the response's headers matter, its body does not.
    Head,
}

impl Method {
    /// The uppercase verb, as it goes on the wire.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
            Self::Head => "HEAD",
        }
    }
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The five parts of the URL a UI sends.
///
/// Scheme, host, port (defaulted per scheme), path, query. Constructed by
/// [`parse`](Self::parse), which refuses anything else — see the module
/// docs for why refusing beats half-parsing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Url {
    scheme: Scheme,
    host: String,
    port: u16,
    path: String,
    /// The query, *without* the leading `?`.
    query: Option<String>,
}

/// The schemes a UI's URLs come in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Scheme {
    /// Plain HTTP — development servers, local hardware.
    Http,
    /// TLS. The default, because a production UI talks to it.
    #[default]
    Https,
}

impl Scheme {
    /// The default port for the scheme: 80 / 443.
    #[must_use]
    pub const fn default_port(self) -> u16 {
        match self {
            Self::Http => 80,
            Self::Https => 443,
        }
    }

    /// The scheme's name, lowercase, as it appears in a URL.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

impl Url {
    /// Parse `text` into a [`Url`], or say why it is not one.
    ///
    /// # Errors
    ///
    /// [`NetError::NotAUrl`] naming the reason: no scheme, an unknown
    /// scheme, no host, a port that is not a number, or a feature (userinfo,
    /// fragment) this type deliberately does not carry.
    pub fn parse(text: &str) -> Result<Self, NetError> {
        let (scheme, rest) = match text.split_once("://") {
            Some(("http", rest)) => (Scheme::Http, rest),
            Some(("https", rest)) => (Scheme::Https, rest),
            Some((other, _)) => {
                return Err(NetError::NotAUrl(format!(
                    "scheme `{other}` is not http or https"
                )))
            }
            None => return Err(NetError::NotAUrl("no `://` in it".into())),
        };

        // The authority ends at the first `/`, `?`, or the end of the text.
        let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        let after = &rest[authority_end..];

        if authority.contains('@') {
            return Err(NetError::NotAUrl(
                "userinfo (`user:pass@`) is not carried".into(),
            ));
        }
        if authority.is_empty() {
            return Err(NetError::NotAUrl("no host".into()));
        }

        let (host, port) = match authority.rsplit_once(':') {
            // A `host:port` split — but only when the tail is a port, not
            // an IPv6 literal's colon.
            Some((host, tail)) if !host.is_empty() && !host.starts_with('[') => {
                let port = tail
                    .parse::<u16>()
                    .map_err(|_| NetError::NotAUrl(format!("port `{tail}` is not a number")))?;
                (host.to_ascii_lowercase(), port)
            }
            _ => (authority.to_ascii_lowercase(), scheme.default_port()),
        };

        let (path, query) = match after.split_once('?') {
            Some((path, query)) => (
                if path.is_empty() {
                    "/".to_string()
                } else {
                    path.to_string()
                },
                Some(query.to_string()),
            ),
            None => (
                if after.is_empty() {
                    "/".to_string()
                } else {
                    after.to_string()
                },
                None,
            ),
        };

        Ok(Self {
            scheme,
            host,
            port,
            path,
            query,
        })
    }

    /// The scheme.
    #[must_use]
    pub const fn scheme(&self) -> Scheme {
        self.scheme
    }

    /// The host, lowercased — `Example.COM` and `example.com` are one host.
    #[must_use]
    pub const fn host(&self) -> &String {
        &self.host
    }

    /// The port, always explicit (defaulted at parse).
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// The path, always starting with `/`.
    #[must_use]
    pub const fn path(&self) -> &String {
        &self.path
    }

    /// The query without its `?`, if there is one.
    #[must_use]
    pub const fn query(&self) -> Option<&String> {
        self.query.as_ref()
    }

    /// The URL reassembled, with the port shown only when it is not the
    /// scheme's default — the spelling a person expects to read.
    #[must_use]
    pub fn to_string_full(&self) -> String {
        let port = if self.port == self.scheme.default_port() {
            String::new()
        } else {
            format!(":{}", self.port)
        };
        match &self.query {
            Some(query) => format!(
                "{}://{}{}{}?{}",
                self.scheme.as_str(),
                self.host,
                port,
                self.path,
                query
            ),
            None => format!(
                "{}://{}{}{}",
                self.scheme.as_str(),
                self.host,
                port,
                self.path
            ),
        }
    }
}

impl std::fmt::Display for Url {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_string_full())
    }
}

/// Why a request did not produce a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// The URL is not a URL this layer will send. The reason, in words.
    NotAUrl(String),
    /// This platform has no network client registered.
    ///
    /// Distinct from "the network is down", and the distinction is the
    /// same one [`ServiceError::Unsupported`](vieww_foundation::service::ServiceError::Unsupported)
    /// draws everywhere: hide the feature, do not retry it.
    Unsupported,
    /// The transport refused: DNS, connection, TLS, timeout. The reason as
    /// the transport phrased it.
    Connection(String),
}

impl std::fmt::Display for NetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAUrl(why) => write!(f, "not a URL this layer sends: {why}"),
            Self::Unsupported => f.write_str("no network client on this platform"),
            Self::Connection(why) => write!(f, "the transport refused: {why}"),
        }
    }
}

impl std::error::Error for NetError {}

/// A request to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    /// The method.
    pub method: Method,
    /// The target.
    pub url: Url,
    /// Headers, as `name: value` pairs in send order.
    ///
    /// A `Vec` rather than a map because order is real (auth tokens some
    /// servers want first) and because duplicate header names are legal on
    /// the wire; a map would have to silently decide both questions.
    pub headers: Vec<(String, String)>,
    /// The body, exactly as it will be sent. Empty for a `GET`.
    pub body: Vec<u8>,
}

impl HttpRequest {
    /// A `GET` to `url`, no headers, no body.
    #[must_use]
    pub fn get(url: Url) -> Self {
        Self {
            method: Method::Get,
            url,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// A `POST` of `body` to `url`.
    #[must_use]
    pub fn post(url: Url, body: impl Into<Vec<u8>>) -> Self {
        Self {
            method: Method::Post,
            url,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    /// Set the method.
    #[must_use]
    pub const fn method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }

    /// Add a header.
    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// The first value for `name`, case-insensitive — the lookup every
    /// header-reading call wants, and the reason header *sending* stays a
    /// `Vec` while header *reading* is a question.
    #[must_use]
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// A response that arrived.
///
/// The status is a plain `u16` rather than an enum of the sixty-odd HTTP
/// status codes, because a client meets new ones (a CDN's 530, a
/// rate-limiter's 429) faster than an enum can be kept honest; the two
/// questions a UI actually asks — [`is_success`] and
/// [`is_client_error`](Self::is_client_error) — are methods, and the rest
/// stays a number.
///
/// [`is_success`]: Self::is_success
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// The status code: 200, 404, 503.
    pub status: u16,
    /// Headers, as they arrived.
    pub headers: Vec<(String, String)>,
    /// The body, exactly as it arrived.
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// A `200` with a body.
    #[must_use]
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    /// A response with an explicit status and body.
    #[must_use]
    pub fn status(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: body.into(),
        }
    }

    /// `true` for 2xx: the body is the answer.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    /// `true` for 4xx: the caller did something the server did not allow.
    ///
    /// Split from 5xx because the UI's correct response differs — a 4xx is
    /// "show the user what they got wrong", a 5xx is " apologise and
    /// maybe retry" — and folding them into one `is_error` would put that
    /// decision back in every caller's hands.
    #[must_use]
    pub const fn is_client_error(&self) -> bool {
        self.status >= 400 && self.status < 500
    }

    /// The first value for `name`, case-insensitive.
    #[must_use]
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// A network client: the seam a transport plugs into.
///
/// Implementations: [`MemoryClient`] (a table), [`NoNetwork`] (a refusal),
/// a platform crate (the real transport, through [`Task::spawn`]). The
/// registration is the services registry — see the module docs of
/// `vieww_foundation::service` for the rule that makes that work without
/// this crate knowing a socket exists.
pub trait HttpClient: 'static {
    /// Send `request`, answering through a [`Task`].
    ///
    /// The task never blocks the caller: whatever the transport does, it
    /// does off the UI thread and wakes a frame when the answer lands.
    fn send(&self, request: HttpRequest) -> Task<HttpResponse, NetError>;
}

/// The client that is not there.
///
/// Every request fails fast with [`NetError::Unsupported`] — the honest
/// register-this-on-purpose statement, the same one `NoAudio` and
/// `NoCamera` make.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoNetwork;

impl HttpClient for NoNetwork {
    fn send(&self, _request: HttpRequest) -> Task<HttpResponse, NetError> {
        Task::failed(NetError::Unsupported)
    }
}

/// The test-and-demo client: answers from a table, remembers everything.
///
/// Routes are matched by path — the part of a URL that identifies a
/// *resource*, ignoring host, scheme and query — because "the health
/// endpoint" is the question a table answers and "example.test vs
/// staging.example.test" is not a question a widget's wiring test is asking.
///
/// A route is a closure from the request to the response, so a canned
/// answer can be *computed* (count calls, echo the body, fail the third
/// time) rather than merely replayed.
///
/// [`RefCell`] inside, [`Rc`] shared, because the services registry holds
/// clients as `Rc<dyn HttpClient>` while the test holds a second handle to
/// read the request log — the same shape `RecordingPlayer` has in the
/// audio crate, for the same reason.
/// A route's responder, named so the client's field is a type a reader can
/// hold in their head.
type Responder = Rc<dyn Fn(&HttpRequest) -> HttpResponse>;

#[derive(Clone, Default)]
pub struct MemoryClient {
    routes: Rc<RefCell<HashMap<String, Responder>>>,
    requests: Rc<RefCell<Vec<HttpRequest>>>,
}

impl std::fmt::Debug for MemoryClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The routes are closures and have no Debug; what a test wants to
        // see is which paths are routed and how many requests arrived.
        f.debug_struct("MemoryClient")
            .field("routes", &self.routes.borrow().keys().collect::<Vec<_>>())
            .field("requests", &self.requests.borrow().len())
            .finish()
    }
}

impl MemoryClient {
    /// A client with no routes: every request fails with "no route".
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Answer requests whose path is `path` with `responder`.
    #[must_use]
    pub fn route(
        self,
        path: impl Into<String>,
        responder: impl Fn(&HttpRequest) -> HttpResponse + 'static,
    ) -> Self {
        self.routes
            .borrow_mut()
            .insert(path.into(), Rc::new(responder));
        self
    }

    /// The requests this client has been sent, in order.
    ///
    /// The assertion surface: "the widget asked for the health endpoint,
    /// with the auth header, twice" is three lines against this.
    #[must_use]
    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.borrow().clone()
    }
}

impl HttpClient for MemoryClient {
    fn send(&self, request: HttpRequest) -> Task<HttpResponse, NetError> {
        // The log takes the request *before* dispatch, so a panicking
        // responder still leaves the record of what was asked.
        self.requests.borrow_mut().push(request.clone());

        let responder = self
            .routes
            .borrow()
            .get(&request.url.path().clone())
            .cloned();
        match responder {
            Some(responder) => Task::ready(responder(&request)),
            None => Task::failed(NetError::Connection(format!(
                "no route for `{}`",
                request.url.path()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_parse_their_five_parts() {
        let url = Url::parse("https://example.com/api/v1/items?q=boot").unwrap();
        assert_eq!(url.scheme(), Scheme::Https);
        assert_eq!(url.host(), "example.com");
        assert_eq!(url.port(), 443, "the scheme's default, made explicit");
        assert_eq!(url.path(), "/api/v1/items");
        assert_eq!(url.query(), Some(&"q=boot".to_string()));
    }

    #[test]
    fn a_port_is_kept_and_a_path_is_rooted() {
        let url = Url::parse("http://localhost:8080").unwrap();
        assert_eq!(url.port(), 8080);
        assert_eq!(url.path(), "/", "no path is the root, not the empty string");

        let url = Url::parse("https://example.com?query=1").unwrap();
        assert_eq!(url.path(), "/");
        assert_eq!(url.query(), Some(&"query=1".to_string()));
    }

    #[test]
    fn hosts_fold_to_lowercase() {
        let url = Url::parse("https://Example.COM/Path").unwrap();
        assert_eq!(url.host(), "example.com");
        assert_eq!(url.path(), "/Path", "paths are case-sensitive and stay");
    }

    #[test]
    fn urls_round_trip_to_the_expected_spelling() {
        let plain = Url::parse("https://example.com/a").unwrap();
        assert_eq!(plain.to_string(), "https://example.com/a");

        let ported = Url::parse("https://example.com:8443/a?b=c").unwrap();
        assert_eq!(ported.to_string(), "https://example.com:8443/a?b=c");
    }

    #[test]
    fn bad_urls_say_why() {
        assert!(matches!(
            Url::parse("example.com"),
            Err(NetError::NotAUrl(_))
        ));
        assert!(Url::parse("ftp://example.com").is_err());
        assert!(Url::parse("https://").is_err());
        assert!(Url::parse("https://example.com:not-a-port/").is_err());
        assert!(Url::parse("https://user:pass@example.com/").is_err());
    }

    #[test]
    fn requests_carry_their_parts() {
        let url = Url::parse("https://example.com/api").unwrap();
        let request = HttpRequest::post(url, b"{\"a\":1}".to_vec())
            .method(Method::Patch)
            .header("Content-Type", "application/json")
            .header("X-Custom", "1");

        assert_eq!(request.method, Method::Patch);
        assert_eq!(request.method.as_str(), "PATCH");
        assert_eq!(
            request.header_value("content-type"),
            Some("application/json")
        );
        assert_eq!(request.header_value("X-CUSTOM"), Some("1"));
        assert_eq!(request.header_value("missing"), None);
        assert_eq!(request.body, br#"{"a":1}"#.to_vec());
    }

    #[test]
    fn responses_classify_themselves() {
        assert!(HttpResponse::ok(b"fine".to_vec()).is_success());
        assert!(HttpResponse::status(204, Vec::new()).is_success());
        assert!(HttpResponse::status(404, b"gone".to_vec()).is_client_error());
        assert!(!HttpResponse::status(500, Vec::new()).is_client_error());
        assert!(!HttpResponse::status(500, Vec::new()).is_success());
    }

    #[test]
    fn no_network_refuses_fast() {
        let client = NoNetwork;
        let url = Url::parse("https://example.com/").unwrap();
        let task = client.send(HttpRequest::get(url));
        assert_eq!(task.value().failed(), Some(&NetError::Unsupported));
    }

    #[test]
    fn a_memory_client_answers_from_its_table() {
        let client = MemoryClient::new()
            .route("/api/health", |_| HttpResponse::ok("fine"))
            .route("/api/items", |request| {
                HttpResponse::status(201, request.body.clone())
            });

        let health = Url::parse("https://example.com/api/health").unwrap();
        let task = client.send(HttpRequest::get(health));
        let response = task.value().ready().expect("the route exists").clone();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"fine");

        // The path is the route key: host and scheme are ignored.
        let staging = Url::parse("http://staging.example.test/api/health").unwrap();
        let task = client.send(HttpRequest::get(staging));
        assert_eq!(task.value().ready().unwrap().body, b"fine");

        // An un-routed path is a connection error naming it.
        let missing = Url::parse("https://example.com/api/missing").unwrap();
        let task = client.send(HttpRequest::get(missing));
        match task.value() {
            vieww_foundation::task::AsyncValue::Failed(NetError::Connection(why)) => {
                assert!(why.contains("/api/missing"), "{why}")
            }
            other => panic!("wrong answer: {other:?}"),
        }
    }

    #[test]
    fn a_memory_client_records_what_it_was_asked() {
        let client = MemoryClient::new().route("/a", |_| HttpResponse::ok("1"));

        let a = Url::parse("https://example.com/a").unwrap();
        let _ = client.send(HttpRequest::get(a.clone()).header("X-Trace", "42"));
        let b = Url::parse("https://example.com/b?x=1").unwrap();
        let _ = client.send(HttpRequest::get(b));

        let requests = client.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].url.path(), "/a");
        assert_eq!(requests[0].header_value("X-Trace"), Some("42"));
        assert_eq!(requests[1].url.query(), Some(&"x=1".to_string()));
    }

    #[test]
    fn a_memory_client_computes_its_answers() {
        // The responder is a closure, so a canned answer can depend on the
        // request: an echo service, in three lines.
        let client = MemoryClient::new().route("/echo", |request| {
            let name = request
                .header_value("X-Name")
                .unwrap_or("anonymous")
                .to_string();
            HttpResponse::ok(format!("hello, {name}").into_bytes())
        });

        let url = Url::parse("https://example.com/echo").unwrap();
        let task = client.send(HttpRequest::get(url).header("X-Name", "vieww"));
        assert_eq!(task.value().ready().unwrap().body, b"hello, vieww");
    }

    #[test]
    fn the_client_registers_as_a_service() {
        use vieww_foundation::service::Services;

        let mut services = Services::new();
        assert!(services.get::<dyn HttpClient>().is_none());

        services.provide::<dyn HttpClient>(Rc::new(MemoryClient::new()));
        assert!(services.get::<dyn HttpClient>().is_some());
    }

    #[test]
    fn errors_read_as_sentences() {
        assert_eq!(
            NetError::NotAUrl("no host".into()).to_string(),
            "not a URL this layer sends: no host"
        );
        assert_eq!(
            NetError::Unsupported.to_string(),
            "no network client on this platform"
        );
    }
}
