//! MCP behind permits (ADR-029): a hash-pinned server, an allow-listed
//! tool, nothing else.
//!
//! MCP servers are other people's programs; the memo's rule for them is
//! the same as its rule for recipes: provenance or nothing. So:
//!
//! 1. **The server is pinned by hash.** config.kdl names the executable
//!    and the BLAKE3 of its bytes (`hash="blake3:..."`). Before the very
//!    first call, the file is hashed again; a binary that changed since
//!    the pin was set is REFUSED, loudly. (Compute the pin with
//!    `wright mcp-hash <command>`.)
//! 2. **The card allow-lists tools.** `mcp server="weather"
//!    allow="get_forecast"` — a tool not on the card does not exist.
//!    The server is asked for its list first, and an allow-listed name
//!    that the server does not offer is an error, not a guess.
//! 3. **The answers are outside data.** The palette marks `mcp-call`
//!    untrusted, so taint tracking treats whatever a server returns the
//!    way it treats a web page: data with a mark on it.
//!
//! The wire protocol is MCP's stdio transport: JSON-RPC 2.0, one message
//! per line, over the server's stdin/stdout — small enough to speak with
//! std alone, which is the point.
//!
//! What this client deliberately does NOT do: it never registers
//! Wright-side tools automatically, never keeps a server alive between
//! calls (a fresh process per call — slow, stateless, and impossible to
//! hold a channel open in the background), and never passes more of the
//! world to a server than the card allows.

use std::io::{BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use wright_core::hash;
use wright_core::wire::json::{self, Value};
use wright_runtime::config::McpEntry;
use wright_runtime::gate::Permit;

use crate::{ToolCtx, ToolError, ToolOutput};

/// The MCP protocol version this client speaks (MCP 2024-11-05 stdio).
const PROTOCOL_VERSION: &str = "2024-11-05";

/// Run one `mcp-call`. The input is `{"server", "tool", "args"}`.
pub fn run(permit: &Permit, input: &Value, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let agent = permit.agent();
    let server_name = input
        .str_of("server")
        .ok_or_else(|| ToolError::BadInput("mcp-call needs a 'server' string".into()))?;
    let tool = input
        .str_of("tool")
        .ok_or_else(|| ToolError::BadInput("mcp-call needs a 'tool' string".into()))?
        .to_string();
    let args = input
        .get("args")
        .cloned()
        .unwrap_or_else(|| Value::obj(Vec::<(String, Value)>::new()));

    // The card is the law: server name must match, tool must be listed.
    let Some(entry) = &ctx.mcp_server else {
        return Err(ToolError::Failed(format!(
            "'{agent}' has no MCP server on its card, but tried to call '{server_name}'."
        )));
    };
    if entry.name != server_name {
        return Err(ToolError::Failed(format!(
            "'{agent}' tried the MCP server '{server_name}', but its card names only '{}'",
            entry.name
        )));
    }
    if !ctx.mcp_allow.iter().any(|t| t == &tool) {
        return Err(ToolError::Failed(format!(
            "'{agent}' tried the MCP tool '{tool}', which is not on its card (allowed: {}). A tool not on the card does not exist (ADR-029).",
            if ctx.mcp_allow.is_empty() { "none".to_string() } else { ctx.mcp_allow.join(", ") }
        )));
    }

    // 1. The pin: the executable's bytes must still hash to the config's
    //    pin. A server that updated itself out from under the pin is a
    //    different program than the one approved.
    let exe = find_executable(&entry.command).map_err(ToolError::Failed)?;
    let pinned = pin_of(&exe).map_err(ToolError::Failed)?;
    if pinned != entry.hash {
        return Err(ToolError::Failed(format!(
            "the MCP server '{}' changed since it was pinned: its command hashes to {}, but config.kdl pins {}. \
             A server that is not the program that was approved is refused (ADR-029). Re-pin with `wright mcp-hash {}`.",
            entry.name, pinned, entry.hash, entry.command
        )));
    }

    // 2. Fresh process, handshake, allow-list check, the call, teardown.
    let timeout = Duration::from_secs(entry.timeout_secs.max(1));
    let mut child = spawn_server(entry, timeout).map_err(ToolError::Failed)?;
    let result = (|| -> Result<String, String> {
        let mut stdin = child
            .stdin
            .take()
            .ok_or("the server closed its stdin before saying hello")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("the server closed its stdout before saying hello")?;
        let mut reader = BufReader::new(stdout);

        // initialize -> (response) -> initialized (notification)
        let init = jsonrpc_request(
            &mut stdin,
            &mut reader,
            1,
            "initialize",
            &Value::obj(vec![
                ("protocolVersion", Value::str(PROTOCOL_VERSION)),
                ("capabilities", Value::obj(Vec::<(String, Value)>::new())),
                (
                    "clientInfo",
                    Value::obj(vec![
                        ("name", Value::str("wright")),
                        ("version", Value::str(env!("CARGO_PKG_VERSION"))),
                    ]),
                ),
            ]),
            timeout,
        )?;
        if init.get("error").is_some() {
            return Err(format!(
                "the server refused the handshake: {}",
                json::to_string(&init)
            ));
        }
        jsonrpc_notify(&mut stdin, "notifications/initialized")?;

        // tools/list: the allow-listed tool must be offered by the server.
        let list = jsonrpc_request(
            &mut stdin,
            &mut reader,
            2,
            "tools/list",
            &Value::obj(Vec::<(String, Value)>::new()),
            timeout,
        )?;
        let offered: Vec<String> = list
            .get("result")
            .and_then(|r| r.get("tools"))
            .and_then(|t| t.as_arr().map(|a| a.to_vec()))
            .unwrap_or_default()
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect();
        if !offered.iter().any(|t| t == &tool) {
            return Err(format!(
                "the card allows the MCP tool '{tool}', but the server does not offer it (it offers {}). Two sides of the same contract must agree.",
                if offered.is_empty() { "nothing".to_string() } else { offered.join(", ") }
            ));
        }

        // tools/call
        let call = jsonrpc_request(
            &mut stdin,
            &mut reader,
            3,
            "tools/call",
            &Value::obj(vec![
                ("name", Value::str(tool.clone())),
                ("arguments", args),
            ]),
            timeout,
        )?;
        if let Some(err) = call.get("error") {
            return Err(format!(
                "the server's tool failed: {}",
                json::to_string(err)
            ));
        }
        let result = call.get("result").cloned().unwrap_or(Value::Null);
        // MCP results carry content blocks; text is the useful one.
        let text: Vec<String> = result
            .get("content")
            .and_then(|c| c.as_arr().map(|a| a.to_vec()))
            .unwrap_or_default()
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()).map(String::from))
            .collect();
        if !text.is_empty() {
            Ok(text.join("\n"))
        } else if !matches!(result, Value::Null) {
            Ok(json::to_string(&result))
        } else {
            Ok("(the server returned no content)".into())
        }
    })();
    // Teardown: an exit notification, then the wall (the watchdog may
    // have already swung if the call's clock ran out).
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/exit\"}\n");
        let _ = stdin.flush();
    }
    let _ = child.kill();
    let _ = child.wait();

    let text = result.map_err(ToolError::Failed)?;
    Ok(ToolOutput {
        text: format!(
            "{} answered via the pinned MCP server '{}':\n{}",
            agent, entry.name, text
        ),
        wrote: Vec::new(),
    })
}

// ── the tiny JSON-RPC layer ─────────────────────────────────────

fn jsonrpc_request(
    stdin: &mut std::process::ChildStdin,
    reader: &mut BufReader<std::process::ChildStdout>,
    id: u64,
    method: &str,
    params: &Value,
    timeout: Duration,
) -> Result<Value, String> {
    let msg = Value::obj(vec![
        ("jsonrpc", Value::str("2.0")),
        ("id", Value::int(id as i64)),
        ("method", Value::str(method)),
        ("params", params.clone()),
    ]);
    write_msg(stdin, &msg)?;
    // Read lines until a response with OUR id arrives; notifications and
    // requests from the server are skipped (a client that never answers
    // sampling requests is honest: we don't sample).
    for _ in 0..1000 {
        let mut line = String::new();
        let n = read_line(reader, &mut line)?;
        if n == 0 {
            return Err(format!(
                "the MCP server closed its stdout before answering '{method}' (or its {}s clock ran out)",
                timeout.as_secs()
            ));
        }
        let Ok(v) = json::parse(line.trim()) else {
            continue;
        };
        if v.get("id").and_then(|i| i.as_u64()) == Some(id) {
            return Ok(v);
        }
    }
    Err(format!(
        "the MCP server talked past a thousand lines without answering '{method}'"
    ))
}

fn jsonrpc_notify(stdin: &mut std::process::ChildStdin, method: &str) -> Result<(), String> {
    let msg = Value::obj(vec![
        ("jsonrpc", Value::str("2.0")),
        ("method", Value::str(method)),
    ]);
    write_msg(stdin, &msg)
}

fn write_msg(stdin: &mut std::process::ChildStdin, msg: &Value) -> Result<(), String> {
    let line = json::to_string(msg);
    stdin
        .write_all(line.as_bytes())
        .and_then(|_| stdin.write_all(b"\n"))
        .and_then(|_| stdin.flush())
        .map_err(|e| format!("write to the MCP server: {e}"))
}

/// One line from the server. The watchdog (not this read) enforces the
/// clock: when the server's time runs out, the watchdog kills it, the
/// pipe closes, and this returns — a hung server becomes a closed pipe.
fn read_line(
    reader: &mut BufReader<std::process::ChildStdout>,
    out: &mut String,
) -> Result<usize, String> {
    out.clear();
    use std::io::BufRead;
    reader
        .read_line(out)
        .map_err(|e| format!("read from the MCP server: {e}"))
}

// ── process + pin ──────────────────────────────────────────────

fn spawn_server(entry: &McpEntry, timeout: Duration) -> Result<std::process::Child, String> {
    let mut cmd = Command::new(&entry.command);
    cmd.args(&entry.args)
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let _ = timeout;
    cmd.spawn()
        .map_err(|e| format!("start the MCP server '{}': {e}", entry.command))
}

/// `command` resolves through PATH the way a shell would.
fn find_executable(command: &str) -> Result<std::path::PathBuf, String> {
    if command.contains('/') {
        let p = std::path::PathBuf::from(command);
        if p.is_file() {
            return Ok(p);
        }
        return Err(format!("the MCP command '{}' is not a file", command));
    }
    let path = std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
    for dir in path.split(':') {
        let candidate = std::path::Path::new(dir).join(command);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!("the MCP command '{}' is not on PATH", command))
}

/// The BLAKE3 pin of an executable's bytes, in config's format.
pub fn pin_of(exe: &std::path::Path) -> Result<String, String> {
    let bytes =
        std::fs::read(exe).map_err(|e| format!("read '{}' to pin it: {e}", exe.display()))?;
    Ok(format!("blake3:{}", hash::blake3_hex(&bytes)))
}

/// What `wright mcp-hash <command>` prints, so a person can pin a
/// server without reading source code.
pub fn pin_for_command(command: &str) -> Result<String, String> {
    let exe = find_executable(command)?;
    pin_of(&exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolCtx;
    use wright_core::palette;
    use wright_runtime::config::McpEntry;
    use wright_runtime::gate::Permit;

    /// A whole stub MCP server in one python script: initialize,
    /// tools/list, tools/call — the stdio transport, answered line by
    /// line. Pinned by hash like a real one.
    const SERVER_SCRIPT: &str = r#"#!/usr/bin/env python3
import sys, json
for line in sys.stdin:
    line = line.strip()
    if not line: continue
    try: m = json.loads(line)
    except: continue
    method = m.get("method", "")
    if method == "initialize":
        print(json.dumps({"jsonrpc":"2.0","id":m["id"],"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}}}}), flush=True)
    elif method == "tools/list":
        print(json.dumps({"jsonrpc":"2.0","id":m["id"],"result":{"tools":[{"name":"get_forecast","description":"tomorrow"},{"name":"dangerous_wipe","description":"no"}]}}), flush=True)
    elif method == "tools/call":
        name = m["params"]["name"]
        if name == "get_forecast":
            print(json.dumps({"jsonrpc":"2.0","id":m["id"],"result":{"content":[{"type":"text","text":"sunny, 22C"}]}}), flush=True)
        else:
            print(json.dumps({"jsonrpc":"2.0","id":m["id"],"result":{"content":[{"type":"text","text":"done"}]}}), flush=True)
"#;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("wright-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn stub_server(dir: &std::path::Path) -> McpEntry {
        let script = dir.join("server.py");
        std::fs::write(&script, SERVER_SCRIPT).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let pin = pin_of(&script).unwrap();
        McpEntry {
            name: "weather".into(),
            command: script.to_string_lossy().to_string(),
            args: Vec::new(),
            hash: pin,
            timeout_secs: 30,
        }
    }

    fn ctx_with(entry: McpEntry, allow: &[&str]) -> ToolCtx {
        let project = tmp("ctx");
        let mut ctx = ToolCtx::new(&project).unwrap();
        ctx.mcp_server = Some(entry);
        ctx.mcp_allow = allow.iter().map(|s| s.to_string()).collect();
        ctx
    }

    fn permit(agent: &str) -> Permit {
        let spec = palette::find("mcp-call").unwrap();
        let grants = wright_core::blueprint::PermissionSet::none()
            .with(wright_core::blueprint::PermissionSet::SPAWN)
            .with(wright_core::blueprint::PermissionSet::NETWORK);
        Permit::for_tests(agent, spec, grants)
    }

    fn call(tool: &str, args_json: &str) -> Value {
        json::parse(&format!(
            r#"{{"server":"weather","tool":"{tool}","args":{args_json}}}"#
        ))
        .unwrap()
    }

    #[test]
    fn an_allowed_call_on_a_pinned_server_answers() {
        let dir = tmp("ok");
        let entry = stub_server(&dir);
        let ctx = ctx_with(entry.clone(), &["get_forecast"]);
        let out = run(
            &permit("Looker"),
            &call("get_forecast", r#"{"city":"Oslo"}"#),
            &ctx,
        )
        .unwrap();
        assert!(
            out.text.contains("sunny, 22C"),
            "the answer came back: {}",
            out.text
        );
        assert!(
            out.text.contains("pinned MCP server 'weather'"),
            "the report names the server: {}",
            out.text
        );
        assert!(
            out.wrote.is_empty(),
            "MCP calls write nothing to the project"
        );
    }

    #[test]
    fn a_tool_not_on_the_card_does_not_exist() {
        let dir = tmp("allow");
        let entry = stub_server(&dir);
        let ctx = ctx_with(entry, &["get_forecast"]);
        let err = run(&permit("Looker"), &call("dangerous_wipe", "{}"), &ctx).unwrap_err();
        assert!(err.to_string().contains("not on its card"), "{}", err);
        assert!(err.to_string().contains("does not exist"), "{}", err);
    }

    #[test]
    fn an_allowed_tool_the_server_does_not_offer_is_an_error() {
        let dir = tmp("mismatch");
        let entry = stub_server(&dir);
        let ctx = ctx_with(entry, &["get_forecast", "not_offered"]);
        let err = run(&permit("Looker"), &call("not_offered", "{}"), &ctx).unwrap_err();
        assert!(err.to_string().contains("does not offer it"), "{}", err);
    }

    #[test]
    fn a_tampered_server_is_refused_by_the_pin() {
        let dir = tmp("tamper");
        let entry = stub_server(&dir);
        // The binary changes after the pin was set (a "helpful" update).
        let script = std::path::Path::new(&entry.command);
        {
            use std::io::Write as _;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(script)
                .unwrap();
            f.write_all(b"\n# an update nobody approved\n").unwrap();
        }
        let ctx = ctx_with(entry, &["get_forecast"]);
        let err = run(&permit("Looker"), &call("get_forecast", "{}"), &ctx).unwrap_err();
        assert!(
            err.to_string().contains("changed since it was pinned"),
            "{}",
            err
        );
        assert!(err.to_string().contains("refused"), "{}", err);
        // and nothing ran: the error came before any server process.
    }

    #[test]
    fn a_wrong_server_name_is_refused_by_the_card() {
        let dir = tmp("wrongname");
        let entry = stub_server(&dir);
        let ctx = ctx_with(entry, &["get_forecast"]);
        let bad =
            json::parse(r#"{"server":"not-weather","tool":"get_forecast","args":{}}"#).unwrap();
        let err = run(&permit("Looker"), &bad, &ctx).unwrap_err();
        assert!(err.to_string().contains("its card names only"), "{}", err);
    }

    #[test]
    fn pinning_helpers_reach_the_file_through_path_or_slash() {
        let dir = tmp("pin");
        let entry = stub_server(&dir);
        let pin = pin_for_command(&entry.command).unwrap();
        assert!(pin.starts_with("blake3:"));
        assert_eq!(pin.len(), 7 + 64);
        assert!(pin_of(std::path::Path::new(&entry.command)).unwrap() == pin);
        assert!(pin_for_command("definitely-not-on-path-xyz").is_err());
    }
}
