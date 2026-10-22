//! The KDL-dialect parser for `blueprint.kdl`.
//!
//! Grammar (small, unambiguous):
//!
//! ```text
//! doc       := statement* EOF
//! statement := node
//! node      := IDENT arg? prop* block?
//! arg       := STRING | INT | BOOL | IDENT        // bare IDENT becomes a string arg
//! prop      := IDENT "=" (STRING | INT | BOOL | IDENT)
//! block     := "{" statement* "}"
//! ```
//!
//! Statements are separated by newlines, `}`, or — when unambiguous —
//! by the next node name on the same line: after a node already has
//! its single positional argument or any property, a bare `IDENT`
//! that is not followed by `=` begins a new statement. That makes
//! `intent { summary "s" confirmed "v1" }` and its multi-line form
//! parse identically. Nodes take at most ONE positional argument
//! (every node in the dialect does — enforced loudly).
//!
//! The parser is deliberately two-stage: a generic document parser
//! producing [`KdlNode`] trees, then a mapping stage that produces the
//! typed [`Blueprint`](super::model::Blueprint). Stage one never
//! changes when the model grows; stage two is boring on purpose.

use super::model::*;

// ── errors ───────────────────────────────────────────────────────

/// A parse failure with the line where it happened.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

impl ParseError {
    fn new(line: usize, msg: impl Into<String>) -> Self {
        ParseError {
            line,
            msg: msg.into(),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

impl std::error::Error for ParseError {}

// ── generic document tree ────────────────────────────────────────

/// A parsed value: string, integer, or boolean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KdlValue {
    Str(String),
    Int(i64),
    Bool(bool),
}

impl KdlValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            KdlValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            KdlValue::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            KdlValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

/// One node: a name, positional args, key=value props, child block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KdlNode {
    /// Source line of the node name (1-based). Every mapping error
    /// points here, so the UI and the CLI can say *where*.
    pub line: usize,
    pub name: String,
    pub args: Vec<KdlValue>,
    pub props: Vec<(String, KdlValue)>,
    pub children: Vec<KdlNode>,
}

impl KdlNode {
    /// First child with this name.
    pub fn child(&self, name: &str) -> Option<&KdlNode> {
        self.children.iter().find(|c| c.name == name)
    }

    /// First positional arg as a string.
    pub fn arg_str(&self) -> Option<&str> {
        self.args.first().and_then(|v| v.as_str())
    }

    pub fn arg_int(&self) -> Option<i64> {
        self.args.first().and_then(|v| v.as_int())
    }

    pub fn arg_bool(&self) -> Option<bool> {
        self.args.first().and_then(|v| v.as_bool())
    }

    pub fn prop(&self, key: &str) -> Option<&KdlValue> {
        self.props.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn prop_str(&self, key: &str) -> Option<&str> {
        self.prop(key).and_then(|v| v.as_str())
    }

    pub fn prop_int(&self, key: &str) -> Option<i64> {
        self.prop(key).and_then(|v| v.as_int())
    }

    pub fn prop_bool(&self, key: &str) -> Option<bool> {
        self.prop(key).and_then(|v| v.as_bool())
    }
}

// ── stage one: tokenize + parse the generic tree ─────────────────

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Int(i64),
    Bool(bool),
    LBrace,
    RBrace,
    Eq,
    Newline,
    Eof,
}

fn tokenize(src: &str) -> Result<Vec<(Tok, usize)>, ParseError> {
    let b = src.as_bytes();
    let mut out: Vec<(Tok, usize)> = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;

    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\t' | b'\r' => i += 1,
            b'\n' => {
                out.push((Tok::Newline, line));
                line += 1;
                i += 1;
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' => {
                let (s, next) = lex_string(b, i + 1, line)?;
                out.push((Tok::Str(s), line));
                i = next;
            }
            b'{' => {
                out.push((Tok::LBrace, line));
                i += 1;
            }
            b'}' => {
                out.push((Tok::RBrace, line));
                i += 1;
            }
            b'=' => {
                out.push((Tok::Eq, line));
                i += 1;
            }
            b'0'..=b'9' => {
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                // A decimal literal (prices in config.kdl: in=0.3) is kept
                // as its text: the blueprint has no float fields, so an
                // integer field given "0.3" is still rejected by mapping.
                if i + 1 < b.len() && b[i] == b'.' && b[i + 1].is_ascii_digit() {
                    i += 1;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                    let text = std::str::from_utf8(&b[start..i]).unwrap_or("").to_string();
                    out.push((Tok::Str(text), line));
                    continue;
                }
                let text = std::str::from_utf8(&b[start..i]).unwrap_or("");
                let n: i64 = text
                    .parse()
                    .map_err(|_| ParseError::new(line, format!("integer too large: {text}")))?;
                out.push((Tok::Int(n), line));
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < b.len() {
                    let ch = b[i];
                    if ch.is_ascii_alphanumeric() || ch == b'_' || ch == b'-' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                let word = std::str::from_utf8(&b[start..i]).unwrap_or("").to_string();
                let tok = match word.as_str() {
                    "true" | "on" => Tok::Bool(true),
                    "false" | "off" => Tok::Bool(false),
                    _ => Tok::Ident(word),
                };
                out.push((tok, line));
            }
            other => {
                return Err(ParseError::new(
                    line,
                    format!("unexpected character '{}'", other as char),
                ));
            }
        }
    }
    out.push((Tok::Eof, line));
    Ok(out)
}

/// Lex a quoted string starting just after the opening quote.
/// Returns (value, index after closing quote).
fn lex_string(b: &[u8], mut i: usize, line: usize) -> Result<(String, usize), ParseError> {
    let mut s = String::new();
    while i < b.len() {
        match b[i] {
            b'"' => return Ok((s, i + 1)),
            b'\\' => {
                i += 1;
                let esc = *b.get(i).unwrap_or(&b'\n');
                match esc {
                    b'"' => s.push('"'),
                    b'\\' => s.push('\\'),
                    b'n' => s.push('\n'),
                    b't' => s.push('\t'),
                    b'r' => s.push('\r'),
                    other => {
                        return Err(ParseError::new(
                            line,
                            format!("unknown escape '\\{}' in string", other as char),
                        ));
                    }
                }
                i += 1;
            }
            b'\n' => {
                return Err(ParseError::new(line, "unterminated string"));
            }
            _ => {
                // Copy the raw (UTF-8 safe: we only ever push whole bytes that
                // are part of a valid source string; multi-byte sequences pass
                // through byte-for-byte).
                let ch_len = utf8_len(b[i]);
                let end = (i + ch_len).min(b.len());
                s.push_str(&String::from_utf8_lossy(&b[i..end]));
                i = end;
            }
        }
    }
    Err(ParseError::new(line, "unterminated string (end of file)"))
}

fn utf8_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first >> 5 == 0b110 {
        2
    } else if first >> 4 == 0b1110 {
        3
    } else if first >> 3 == 0b11110 {
        4
    } else {
        1 // invalid lead byte; lossy conversion will handle it
    }
}

struct P {
    toks: Vec<(Tok, usize)>,
    pos: usize,
}

impl P {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos.min(self.toks.len() - 1)].0
    }

    fn line(&self) -> usize {
        self.toks[self.pos.min(self.toks.len() - 1)].1
    }

    fn bump(&mut self) -> Tok {
        let t = self.toks[self.pos.min(self.toks.len() - 1)].0.clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek(), Tok::Eof)
    }

    /// True when the token after the current one is `=`.
    fn next_is_eq(&self) -> bool {
        matches!(self.toks.get(self.pos + 1).map(|t| &t.0), Some(Tok::Eq))
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
    }

    fn parse_doc(&mut self) -> Result<Vec<KdlNode>, ParseError> {
        let mut nodes = Vec::new();
        self.skip_newlines();
        while !self.at_eof() {
            if matches!(self.peek(), Tok::RBrace) {
                return Err(ParseError::new(self.line(), "unexpected '}'"));
            }
            nodes.push(self.parse_node()?);
            self.skip_newlines();
        }
        Ok(nodes)
    }

    fn parse_node(&mut self) -> Result<KdlNode, ParseError> {
        let line = self.line();
        let name = match self.bump() {
            Tok::Ident(s) => s,
            other => {
                return Err(ParseError::new(
                    line,
                    format!("expected a node name, found {other:?}"),
                ));
            }
        };
        let mut node = KdlNode {
            line,
            name,
            args: Vec::new(),
            props: Vec::new(),
            children: Vec::new(),
        };
        let mut seen_prop = false;
        loop {
            let line = self.line();
            match self.peek().clone() {
                Tok::Newline | Tok::RBrace | Tok::Eof => break,
                Tok::LBrace => {
                    self.bump();
                    node.children = self.parse_block()?;
                    break; // a block is always the last part of a node
                }
                Tok::Str(s) => {
                    if seen_prop {
                        return Err(ParseError::new(line, "argument after property"));
                    }
                    if !node.args.is_empty() {
                        return Err(ParseError::new(
                            line,
                            "nodes take at most one positional argument",
                        ));
                    }
                    self.bump();
                    node.args.push(KdlValue::Str(s));
                }
                Tok::Int(n) => {
                    if seen_prop {
                        return Err(ParseError::new(line, "argument after property"));
                    }
                    if !node.args.is_empty() {
                        return Err(ParseError::new(
                            line,
                            "nodes take at most one positional argument",
                        ));
                    }
                    self.bump();
                    node.args.push(KdlValue::Int(n));
                }
                Tok::Bool(v) => {
                    if seen_prop {
                        return Err(ParseError::new(line, "argument after property"));
                    }
                    if !node.args.is_empty() {
                        return Err(ParseError::new(
                            line,
                            "nodes take at most one positional argument",
                        ));
                    }
                    self.bump();
                    node.args.push(KdlValue::Bool(v));
                }
                Tok::Ident(word) => {
                    if self.next_is_eq() {
                        // key=value
                        self.bump(); // ident
                        self.bump(); // '='
                        let value = match self.bump() {
                            Tok::Str(s) => KdlValue::Str(s),
                            Tok::Int(n) => KdlValue::Int(n),
                            Tok::Bool(v) => KdlValue::Bool(v),
                            Tok::Ident(s) => KdlValue::Str(s),
                            other => {
                                return Err(ParseError::new(
                                    line,
                                    format!("expected a value after '=', found {other:?}"),
                                ));
                            }
                        };
                        node.props.push((word, value));
                        seen_prop = true;
                    } else if node.args.is_empty() && !seen_prop {
                        // The node's single positional argument.
                        self.bump();
                        node.args.push(KdlValue::Str(word));
                    } else {
                        // A bare ident after the argument (or any
                        // property) starts the next statement on this
                        // same line. Do not consume it.
                        break;
                    }
                }
                Tok::Eq => {
                    return Err(ParseError::new(line, "unexpected '='"));
                }
            }
        }
        Ok(node)
    }

    fn parse_block(&mut self) -> Result<Vec<KdlNode>, ParseError> {
        let mut nodes = Vec::new();
        self.skip_newlines();
        loop {
            match self.peek() {
                Tok::RBrace => {
                    self.bump();
                    return Ok(nodes);
                }
                Tok::Eof => {
                    return Err(ParseError::new(
                        self.line(),
                        "unterminated block (missing '}')",
                    ));
                }
                _ => {
                    nodes.push(self.parse_node()?);
                    self.skip_newlines();
                }
            }
        }
    }
}

/// Parse source text into a generic KDL document tree.
pub fn parse_doc(src: &str) -> Result<Vec<KdlNode>, ParseError> {
    let mut p = P {
        toks: tokenize(src)?,
        pos: 0,
    };
    p.parse_doc()
}

// ── stage two: map the tree onto the typed model ─────────────────

/// Parse source text into a typed, defaulted [`Blueprint`].
///
/// This succeeds for structurally valid blueprints; run
/// [`validate`](super::validate::validate) before compiling.
pub fn blueprint(src: &str) -> Result<Blueprint, ParseError> {
    let doc = parse_doc(src)?;
    map_blueprint(&doc)
}

fn map_blueprint(doc: &[KdlNode]) -> Result<Blueprint, ParseError> {
    let mut bp = Blueprint::default();
    let mut seen_meta = false;
    let mut seen_intent = false;

    for node in doc {
        match node.name.as_str() {
            "meta" => {
                if seen_meta {
                    return Err(section_dup(node.line, "meta"));
                }
                seen_meta = true;
                map_meta(node, &mut bp.meta)?;
            }
            "intent" => {
                if seen_intent {
                    return Err(section_dup(node.line, "intent"));
                }
                seen_intent = true;
                map_intent(node, &mut bp.intent)?;
            }
            "agent" => bp.agents.push(map_agent(node)?),
            "edge" => bp.edges.push(map_edge(node)?),
            "chips" => map_chips(node, &mut bp.chips)?,
            "guardrails" => map_guardrails(node, &mut bp.guardrails)?,
            "deliverable" => bp.deliverables.push(map_deliverable(node)?),
            "models" => map_models(node, &mut bp.models)?,
            "project" => map_project(node, &mut bp.project)?,
            other => {
                return Err(ParseError::new(
                    node.line,
                    format!("unknown section '{other}'"),
                ));
            }
        }
    }

    if !seen_meta {
        return Err(ParseError::new(0, "blueprint is missing a 'meta' section"));
    }
    if !seen_intent {
        return Err(ParseError::new(
            0,
            "blueprint is missing an 'intent' section",
        ));
    }
    Ok(bp)
}

fn section_dup(line: usize, name: &str) -> ParseError {
    ParseError::new(line, format!("duplicate section '{name}'"))
}

fn want_str(node: &KdlNode, field: &str) -> Result<String, ParseError> {
    node.arg_str()
        .map(|s| s.to_string())
        .ok_or_else(|| ParseError::new(node.line, format!("'{field}' needs a string argument")))
}

fn map_meta(node: &KdlNode, meta: &mut Meta) -> Result<(), ParseError> {
    for child in &node.children {
        match child.name.as_str() {
            "name" => meta.name = want_str(child, "meta.name")?,
            "format" => {
                meta.format = child.arg_int().ok_or_else(|| {
                    ParseError::new(child.line, "'format' needs an integer argument")
                })? as u32;
            }
            "min-app" => meta.min_app = want_str(child, "meta.min-app")?,
            "created" => meta.created = want_str(child, "meta.created")?,
            "lineage" => meta.lineage = want_str(child, "meta.lineage")?,
            other => {
                return Err(ParseError::new(
                    child.line,
                    format!("unknown field '{other}' in 'meta'"),
                ));
            }
        }
    }
    Ok(())
}

fn map_intent(node: &KdlNode, intent: &mut Intent) -> Result<(), ParseError> {
    for child in &node.children {
        match child.name.as_str() {
            "summary" => intent.summary = want_str(child, "intent.summary")?,
            "confirmed" => intent.confirmed = want_str(child, "intent.confirmed")?,
            "evidence" => intent.evidence = want_str(child, "intent.evidence")?,
            other => {
                return Err(ParseError::new(
                    child.line,
                    format!("unknown field '{other}' in 'intent'"),
                ));
            }
        }
    }
    Ok(())
}

fn map_agent(node: &KdlNode) -> Result<AgentDef, ParseError> {
    let name = node.arg_str().map(|s| s.to_string()).ok_or_else(|| {
        ParseError::new(node.line, "agent needs a name: agent \"Fetcher\" { ... }")
    })?;
    let mut explicit_retries = false;
    let mut a = AgentDef {
        name,
        ..AgentDef::default()
    };

    for child in &node.children {
        match child.name.as_str() {
            "goal" => a.goal = want_str(child, "agent.goal")?,
            "model" => {
                let tier = child.prop_str("tier").ok_or_else(|| {
                    ParseError::new(child.line, "'model' needs tier=\"cheap|quality|auto\"")
                })?;
                a.model_tier = ModelTier::parse(tier).ok_or_else(|| {
                    ParseError::new(child.line, format!("unknown model tier '{tier}'"))
                })?;
            }
            "tools" => {
                let t = want_str(child, "agent.tools")?;
                a.tools.push(t);
            }
            "memory" => {
                let scope = child.prop_str("scope").ok_or_else(|| {
                    ParseError::new(child.line, "'memory' needs scope=\"none|firing|project\"")
                })?;
                a.memory = MemoryScope::parse(scope).ok_or_else(|| {
                    ParseError::new(child.line, format!("unknown memory scope '{scope}'"))
                })?;
            }
            "in" => a.inputs.push(want_str(child, "agent.in")?),
            "out" => a.outputs.push(want_str(child, "agent.out")?),
            "approval" => {
                let pol = want_str(child, "agent.approval")?;
                a.approval = ApprovalPolicy::parse(&pol).ok_or_else(|| {
                    ParseError::new(child.line, format!("unknown approval policy '{pol}'"))
                })?;
            }
            "retries" => {
                let n = child.arg_int().ok_or_else(|| {
                    ParseError::new(child.line, "'retries' needs an integer argument")
                })?;
                if !(0..=255).contains(&n) {
                    return Err(ParseError::new(child.line, "'retries' must be 0..=255"));
                }
                a.retries = n as u8;
                explicit_retries = true;
            }
            "permissions" => {
                for (key, value) in &child.props {
                    let on = value.as_bool().ok_or_else(|| {
                        ParseError::new(
                            child.line,
                            format!("permission '{key}' must be true or false"),
                        )
                    })?;
                    if on {
                        a.permissions = a.permissions.with(match key.as_str() {
                            "files-read" => PermissionSet::READ_FILES,
                            "files-write" => PermissionSet::WRITE_FILES,
                            "network" => PermissionSet::NETWORK,
                            "shell" => PermissionSet::SHELL,
                            "spawn" => PermissionSet::SPAWN,
                            other => {
                                return Err(ParseError::new(
                                    child.line,
                                    format!("unknown permission '{other}'"),
                                ));
                            }
                        });
                    }
                }
            }
            "worker" => {
                a.worker = Some(want_str(child, "agent.worker")?);
            }
            "worker-files" => a.worker_files.push(want_str(child, "agent.worker-files")?),
            "secret" => a.secrets.push(want_str(child, "agent.secret")?),
            "worker-timeout" => {
                a.worker_timeout_secs = child
                    .arg_int()
                    .ok_or_else(|| {
                        ParseError::new(child.line, "'worker-timeout' needs an integer argument")
                    })?
                    .max(1) as u64;
            }
            "mcp" => {
                a.mcp_server = Some(
                    child
                        .prop_str("server")
                        .ok_or_else(|| {
                            ParseError::new(
                                child.line,
                                "'mcp' needs server=\"name\" (as named in config.kdl)",
                            )
                        })?
                        .to_string(),
                );
                a.mcp_allow = child
                    .prop_str("allow")
                    .unwrap_or("")
                    .split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
            }
            other => {
                return Err(ParseError::new(
                    child.line,
                    format!("unknown field '{other}' in agent '{}'", a.name),
                ));
            }
        }
    }
    // A worker step runs through the `worker` tool; an MCP card through
    // `mcp-call`. Adding the tool HERE — not in the compiler — keeps one
    // source of truth: validation, the legs, the gate and the canonical
    // form all see the same allowlist. Idempotent, so round-trips.
    if a.worker.is_some() && !a.tools.iter().any(|t| t == "worker") {
        a.tools.push("worker".into());
    }
    // A worker allows at most one retry (validation says why); a card
    // that never mentioned retries gets that, not the general default of
    // two — which validation would refuse, for a line nobody wrote.
    if a.worker.is_some() && !explicit_retries {
        a.retries = a.retries.min(1);
    }
    if a.mcp_server.is_some() && !a.tools.iter().any(|t| t == "mcp-call") {
        a.tools.push("mcp-call".into());
    }
    Ok(a)
}

fn map_edge(node: &KdlNode) -> Result<Edge, ParseError> {
    let from = node
        .prop_str("from")
        .ok_or_else(|| ParseError::new(node.line, "edge needs from=\"...\""))?
        .to_string();
    let to = node
        .prop_str("to")
        .ok_or_else(|| ParseError::new(node.line, "edge needs to=\"...\""))?
        .to_string();
    let out = node.prop_str("out").map(String::from);
    let input = node.prop_str("in").map(String::from);
    let schema = match node.prop_str("schema") {
        Some(s) => Some(crate::schema::Shape::parse(s).ok_or_else(|| {
            ParseError::new(
                node.line,
                format!("unknown edge schema '{s}' (any | text | json-object | json-array | csv)"),
            )
        })?),
        None => None,
    };
    Ok(Edge {
        from,
        to,
        out,
        input,
        schema,
    })
}

fn map_chips(node: &KdlNode, chips: &mut Chips) -> Result<(), ParseError> {
    for child in &node.children {
        match child.name.as_str() {
            "scheduling" => {
                let cron = child
                    .prop_str("cron")
                    .ok_or_else(|| ParseError::new(child.line, "'scheduling' needs cron=\"...\""))?
                    .to_string();
                let tz = child.prop_str("tz").unwrap_or("UTC").to_string();
                chips.scheduling = Some(Schedule { cron, tz });
            }
            "secrets" => {
                for (key, value) in &child.props {
                    let reference = value.as_str().ok_or_else(|| {
                        ParseError::new(child.line, "secret values must be strings")
                    })?;
                    if !reference.starts_with("keychain:") {
                        return Err(ParseError::new(child.line,
                            format!(
                                "secret '{}' must use the keychain: scheme (references, never values)",
                                key
                            ),
                        ));
                    }
                    chips.secrets.push(SecretRef {
                        name: key.clone(),
                        reference: reference.to_string(),
                    });
                }
            }
            "logging" => chips.logging = bool_arg(child, "logging")?,
            "tests" => chips.tests = bool_arg(child, "tests")?,
            "ci" => chips.ci = bool_arg(child, "ci")?,
            "webhooks" => chips.webhooks = bool_arg(child, "webhooks")?,
            other => {
                return Err(ParseError::new(
                    child.line,
                    format!("unknown chip '{other}'"),
                ));
            }
        }
    }
    Ok(())
}

fn bool_arg(node: &KdlNode, field: &str) -> Result<bool, ParseError> {
    node.arg_bool()
        .ok_or_else(|| ParseError::new(node.line, format!("'{field}' needs 'on' or 'off'")))
}

fn map_guardrails(node: &KdlNode, g: &mut Guardrails) -> Result<(), ParseError> {
    for child in &node.children {
        match child.name.as_str() {
            "budget" => {
                if let Some(v) = child.prop_int("per-firing") {
                    g.per_firing_micro_usd = v.max(0) as u64;
                }
                if let Some(v) = child.prop_int("daily") {
                    g.daily_micro_usd = v.max(0) as u64;
                }
            }
            "wall-clock" => {
                g.wall_clock_secs = child
                    .arg_int()
                    .ok_or_else(|| {
                        ParseError::new(child.line, "'wall-clock' needs an integer argument")
                    })?
                    .max(0) as u64;
            }
            "max-turns" => {
                g.max_turns = child
                    .arg_int()
                    .ok_or_else(|| {
                        ParseError::new(child.line, "'max-turns' needs an integer argument")
                    })?
                    .max(0) as u32;
            }
            other => {
                return Err(ParseError::new(
                    child.line,
                    format!("unknown field '{other}' in 'guardrails'"),
                ));
            }
        }
    }
    Ok(())
}

fn map_deliverable(node: &KdlNode) -> Result<Deliverable, ParseError> {
    let kind = node.prop_str("kind").unwrap_or("artifact").to_string();
    let check = node
        .child("check")
        .and_then(|c| c.arg_str())
        .ok_or_else(|| {
            ParseError::new(node.line, "deliverable needs a 'check' child with a string")
        })?
        .to_string();
    let mut accept = Vec::new();
    for child in &node.children {
        if child.name == "accept" {
            accept.push(want_str(child, "deliverable.accept")?);
        }
    }
    Ok(Deliverable {
        kind,
        check,
        accept,
    })
}

fn map_models(node: &KdlNode, models: &mut ModelTiers) -> Result<(), ParseError> {
    for child in &node.children {
        if child.name != "tier" {
            return Err(ParseError::new(
                child.line,
                format!("unknown field '{}' in 'models'", child.name),
            ));
        }
        let tier_name = child.arg_str().ok_or_else(|| {
            ParseError::new(child.line, "'tier' needs a name: tier cheap prefer=...")
        })?;
        let pref = TierPref {
            prefer: child.prop_str("prefer").unwrap_or("").to_string(),
            fallback: child.prop_str("fallback").unwrap_or("").to_string(),
        };
        match tier_name {
            "cheap" => models.cheap = pref,
            "quality" => models.quality = pref,
            other => {
                return Err(ParseError::new(
                    child.line,
                    format!("unknown tier '{other}' (cheap | quality)"),
                ));
            }
        }
    }
    Ok(())
}

fn map_project(node: &KdlNode, p: &mut ProjectSpec) -> Result<(), ParseError> {
    for child in &node.children {
        match child.name.as_str() {
            "language" => {
                let lang = want_str(child, "project.language")?;
                if !crate::convention::LANGUAGES.contains(&lang.as_str()) {
                    return Err(ParseError::new(
                        child.line,
                        format!(
                            "unknown project language '{lang}' (one of: {})",
                            crate::convention::LANGUAGES.join(", ")
                        ),
                    ));
                }
                p.language = lang;
            }
            "test" => p.test_cmd = want_str(child, "project.test")?,
            "run" => p.run_cmd = want_str(child, "project.run")?,
            other => {
                return Err(ParseError::new(
                    child.line,
                    format!("unknown field '{other}' in 'project'"),
                ));
            }
        }
    }
    Ok(())
}

// ── tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A compact but complete example exercising every mapping branch.
    const SRC: &str = r#"
// a comment line
meta {
    name "HN Daily Digest"
    format 1
    min-app "0.1.0"
    created "2026-10-20T11:14:09+05:30"
    lineage "recipe://hn-digest@2.1.0"
}

intent {
    summary "Daily digest, emailed at 8am."
    confirmed "v7"
}

agent Fetcher {
    goal "Fetch top 30 HN stories"
    model tier="cheap"
    tools "http-read"
    out "stories.json"
    permissions network=true
}

agent Summarizer {
    goal "Say \"punchy\" things"
    model tier="quality"
    memory scope="firing"
    in "stories.json"
    out "summaries.json"
}

agent Mailer {
    goal "Send the digest"
    model tier="cheap"
    tools "http-post"
    approval "before-send"
    retries 0
    permissions network=true
}

edge from="Fetcher" to="Summarizer"
edge from="Summarizer" to="Mailer"

chips {
    scheduling cron="0 8 * * *" tz="Asia/Kolkata"
    secrets smtp="keychain:wright/hn-digest/smtp"
    logging on
    tests on
}

guardrails {
    budget per-firing=500000 daily=3000000
    wall-clock 300
    max-turns 40
}

deliverable kind="email" {
    check "Mail received 08:00-08:05 local"
}

models {
    tier cheap prefer="deepseek-v4-flash" fallback="glm-5.3-flash"
    tier quality prefer="claude-opus-4.6" fallback="gpt-5.2"
}
"#;

    #[test]
    fn parses_the_full_example() {
        let bp = blueprint(SRC).expect("example must parse");
        assert_eq!(bp.meta.name, "HN Daily Digest");
        assert_eq!(bp.meta.format, 1);
        assert_eq!(bp.intent.confirmed, "v7");
        assert_eq!(bp.agents.len(), 3);
        assert_eq!(bp.edges.len(), 2);

        let fetcher = bp.agent("Fetcher").unwrap();
        assert_eq!(fetcher.model_tier, ModelTier::Cheap);
        assert_eq!(fetcher.tools, vec!["http-read"]);
        assert!(fetcher
            .permissions
            .allows(crate::palette::Capability::Network));
        assert!(!fetcher
            .permissions
            .allows(crate::palette::Capability::Shell));

        let mailer = bp.agent("Mailer").unwrap();
        assert_eq!(mailer.approval, ApprovalPolicy::BeforeSend);
        assert_eq!(mailer.retries, 0);

        let summarizer = bp.agent("Summarizer").unwrap();
        assert_eq!(summarizer.memory, MemoryScope::Firing);
        // escaped quote survived
        assert!(summarizer.goal.contains('"'));

        let sched = bp.chips.scheduling.as_ref().unwrap();
        assert_eq!(sched.cron, "0 8 * * *");
        assert_eq!(sched.tz, "Asia/Kolkata");
        assert_eq!(bp.chips.secrets.len(), 1);
        assert_eq!(
            bp.chips.secrets[0].reference,
            "keychain:wright/hn-digest/smtp"
        );
        assert!(bp.chips.logging && bp.chips.tests);

        assert_eq!(bp.guardrails.per_firing_micro_usd, 500_000);
        assert_eq!(bp.guardrails.daily_micro_usd, 3_000_000);
        assert_eq!(bp.guardrails.wall_clock_secs, 300);
        assert_eq!(bp.guardrails.max_turns, 40);

        assert_eq!(bp.deliverables.len(), 1);
        assert_eq!(bp.deliverables[0].kind, "email");
        assert_eq!(bp.models.cheap.prefer, "deepseek-v4-flash");
        assert_eq!(bp.models.quality.prefer, "claude-opus-4.6");
    }

    #[test]
    fn missing_meta_is_an_error() {
        let err = blueprint("intent { summary \"x\" confirmed \"v1\" }").unwrap_err();
        assert!(err.msg.contains("meta"));
    }

    #[test]
    fn unknown_section_is_an_error() {
        let src = "meta { name \"x\" }\nintent { summary \"y\" confirmed \"v1\" }\nwat on\n";
        let err = blueprint(src).unwrap_err();
        assert!(err.msg.contains("unknown section 'wat'"));
    }

    #[test]
    fn secret_without_keychain_scheme_is_rejected() {
        let src = "meta { name \"x\" }\nintent { summary \"y\" confirmed \"v1\" }\nchips { secrets smtp=\"plaintext-doom\" }\n";
        let err = blueprint(src).unwrap_err();
        assert!(err.msg.contains("keychain"));
    }

    #[test]
    fn agent_needs_a_name() {
        let src = "meta { name \"x\" }\nintent { summary \"y\" confirmed \"v1\" }\nagent { goal \"g\" }\n";
        let err = blueprint(src).unwrap_err();
        assert!(err.msg.contains("agent needs a name"));
    }

    #[test]
    fn unterminated_string_reports_line() {
        let err = blueprint("meta { name \"oops\n}\n").unwrap_err();
        assert!(err.msg.contains("unterminated string"));
        assert_eq!(err.line, 1);
    }

    #[test]
    fn multiple_tools_stack() {
        let src = "meta { name \"x\" }\nintent { summary \"y\" confirmed \"v1\" }\nagent A { goal \"g\" tools \"http-read\" tools \"browser\" }\n";
        let bp = blueprint(src).unwrap();
        assert_eq!(bp.agent("A").unwrap().tools, vec!["http-read", "browser"]);
    }

    #[test]
    fn mapping_errors_point_at_the_right_line() {
        let src = "meta { name \"x\" }\nintent { summary \"y\" confirmed \"v1\" }\nagent A {\n    goal \"g\"\n    wat \"?\"\n}\n";
        let err = blueprint(src).unwrap_err();
        assert_eq!(err.line, 5, "{err}");
    }

    #[test]
    fn project_section_parses() {
        let src = "meta { name \"x\" }\nintent { summary \"y\" confirmed \"v1\" }\nproject {\n language \"python\"\n test \"python -m unittest\"\n}\n";
        let bp = blueprint(src).unwrap();
        assert_eq!(bp.project.language, "python");
        assert_eq!(bp.project.test_cmd, "python -m unittest");
        let bad = "meta { name \"x\" }\nintent { summary \"y\" confirmed \"v1\" }\nproject { language \"cobol\" }\n";
        assert!(blueprint(bad).unwrap_err().msg.contains("cobol"));
    }

    #[test]
    fn decimals_lex_as_text_and_integer_fields_still_reject_them() {
        let doc = parse_doc("price model=\"m\" in=0.3 out=12\n").unwrap();
        assert_eq!(doc[0].prop_str("in"), Some("0.3"));
        assert_eq!(doc[0].prop_int("out"), Some(12));
        let src = "meta { name \"x\" }\nintent { summary \"y\" confirmed \"v1\" }\nguardrails { wall-clock 1.5 }\n";
        assert!(blueprint(src).is_err());
    }
}
