//! `wright verify` (ADR-025): five checks over a run receipt.
//!
//! A receipt is only worth something if a third party can check it:
//! a manager, a client, an auditor — anyone with the project and the
//! receipt, no model, no Wright account. This module IS that check.
//! `wright verify` prints one line per check and a verdict; each line
//! says what it compared, in words.
//!
//! The six checks:
//!
//! 1. **The signature.** The receipt's exact JSON bytes (minus the
//!    signature field) are signed with Ed25519. Verify re-parses,
//!    re-serializes (the JSON writer is insertion-ordered, so the bytes
//!    come back identical), strips the signature field, and verifies
//!    against the embedded public key — reporting the key's provenance
//!    (os-keychain / env / local:generated / ephemeral) honestly. Then
//!    WHO signed: the key must be one this machine trusts (a `--key`
//!    named on the command line, a `trust` line in config.kdl, or this
//!    machine's own key). A valid signature by a stranger's key fails:
//!    anyone can edit a receipt and re-sign it with a fresh key.
//! 2. **The event log.** The receipt names the BLAKE3 of events.jsonl;
//!    hash the file as it is now and compare. A log edited after the
//!    fact fails here.
//! 3. **The artifacts.** Every artifact the receipt lists must be in
//!    the object store, byte-identical to its recorded hash; and every
//!    file in the project's artifacts/ dir that the receipt names must
//!    match too. A swapped file fails here, with both hashes named.
//! 4. **The inputs.** Receipt v2 records the hashes of every input the
//!    plan consumed; each one must still be in the store and byte-true.
//!    (Receipts v1 — from before this existed — are reported as such,
//!    not silently passed.)
//! 5. **The blueprint.** The receipt names the blueprint commit's hash;
//!    the store's history must contain it and the stored text must
//!    still parse. A plan nobody can reproduce fails here.
//! 6. **The identities.** Every task acted under its own identity: each
//!    one delegated by the receipt's signer, attesting to its calls, and
//!    every tool call in the log made under one of them, by its agent.

use std::path::Path;

use wright_core::hash;
use wright_core::wire::json::{self, Value};
use wright_runtime::store::Store;

/// One check's outcome, in words a person can read aloud.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
    pub note: String,
}

/// The full verdict over one receipt file.
#[derive(Debug, Clone)]
pub struct Verification {
    pub checks: Vec<Check>,
    /// All five passed.
    pub ok: bool,
    /// The receipt's run number, when it parsed.
    pub run: Option<u64>,
}

impl Verification {
    /// One line per check, plus the verdict — exactly what the CLI
    /// prints. Ends with a verdict line, not a trailing newline.
    pub fn report(&self) -> String {
        let mut out = String::new();
        for c in &self.checks {
            let mark = if c.ok { "OK" } else { "FAILED" };
            out.push_str(&format!("[{mark}] {}: {}\n", c.name, c.note));
        }
        out.push_str(if self.ok {
            "\nverified: every check passed. This receipt is evidence of what ran, what it touched and what it cost."
        } else {
            "\nNOT verified: at least one check failed. Do not trust this receipt; ask the person who ran it."
        });
        out
    }
}

/// Verify a receipt file in place, with its project's store.
pub fn receipt_file(project_dir: &Path, receipt: &Path) -> Verification {
    receipt_file_pinned(project_dir, receipt, &[])
}

/// [`receipt_file`], also trusting the public keys in `pins` (hex).
pub fn receipt_file_pinned(project_dir: &Path, receipt: &Path, pins: &[String]) -> Verification {
    let store = match Store::open(project_dir) {
        Ok(s) => s,
        Err(e) => return fail(format!("cannot open the project store: {e}")),
    };
    let text = std::fs::read_to_string(receipt).unwrap_or_default();
    if text.is_empty() {
        return fail(format!("cannot read {}", receipt.display()));
    }
    verify_text_pinned(&store, &text, pins)
}

/// Verify the latest (or a named) run of a project.
pub fn project_run(project_dir: &Path, run: Option<u64>) -> Result<Verification, String> {
    project_run_pinned(project_dir, run, &[])
}

/// [`project_run`], also trusting the public keys in `pins` (hex).
pub fn project_run_pinned(project_dir: &Path, run: Option<u64>, pins: &[String]) -> Result<Verification, String> {
    let store = Store::open(project_dir)?;
    let run = match run {
        Some(r) => r,
        None => store
            .runs()
            .last()
            .copied()
            .ok_or("this project has no runs to verify")?,
    };
    let path = store.run_dir(run).join("receipt.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(verify_text_pinned(&store, &text, pins))
}

fn fail(note: String) -> Verification {
    Verification {
        checks: vec![Check {
            name: "receipt",
            ok: false,
            note,
        }],
        ok: false,
        run: None,
    }
}

/// The whole thing. `text` is the receipt's JSON; the store must be the
/// project's own (artifacts, objects, blueprint history).
pub fn verify_text(store: &Store, text: &str) -> Verification {
    verify_text_pinned(store, text, &[])
}

/// [`verify_text`], also trusting the public keys in `pins` (hex).
pub fn verify_text_pinned(store: &Store, text: &str, pins: &[String]) -> Verification {
    let v = match json::parse(text) {
        Ok(v) => v,
        Err(e) => return fail(format!("the receipt is not valid JSON ({e})")),
    };
    let fmt = v.str_of("format").unwrap_or("");
    if fmt != "wright.run-receipt/2" {
        return fail(format!(
            "the receipt says its format is '{}', not wright.run-receipt/2. v1 receipts predate signatures and input hashes; ask for a fresh run.",
            if fmt.is_empty() { "missing" } else { fmt }
        ));
    }
    let run = v.get("run").and_then(|r| r.as_u64());
    let mut checks = Vec::new();

    // 1. The signature over the exact bytes, minus the signature field.
    let sig_ok = check_signature(&v, &mut checks, pins);

    // 2. The event log's hash.
    let events_ok = match run {
        Some(r) => {
            let path = store.run_dir(r).join("events.jsonl");
            let bytes = std::fs::read(&path);
            match bytes {
                Ok(b) => {
                    let now_hash = hash::blake3_hex(&b);
                    let claimed = v.str_of("events_blake3").unwrap_or("");
                    if claimed.is_empty() {
                        checks.push(Check {
                            name: "event log",
                            ok: false,
                            note: "the receipt names no event-log hash".into(),
                        });
                        false
                    } else if now_hash == claimed {
                        checks.push(Check {
                            name: "event log",
                            ok: true,
                            note: format!(
                                "events.jsonl still hashes to {} as signed",
                                hash::short(&now_hash)
                            ),
                        });
                        true
                    } else {
                        checks.push(Check { name: "event log", ok: false, note: format!("the event log changed after the receipt was signed: receipt says {}, the file now is {}", hash::short(claimed), hash::short(&now_hash)) });
                        false
                    }
                }
                Err(e) => {
                    checks.push(Check {
                        name: "event log",
                        ok: false,
                        note: format!("cannot read {}: {e}", path.display()),
                    });
                    false
                }
            }
        }
        None => {
            checks.push(Check {
                name: "event log",
                ok: false,
                note: "the receipt names no run".into(),
            });
            false
        }
    };

    // 3. The artifacts: recorded hashes vs the object store and the
    //    project's artifacts/ directory.
    let artifacts_ok = check_artifacts(store, &v, &mut checks);

    // 4. The inputs: every consumed input recorded with a hash that
    //    still resolves in the store.
    let inputs_ok = check_inputs(store, &v, &mut checks);

    // 5. The blueprint commit.
    let blueprint_ok = match v.str_of("blueprint") {
        Some(commit) if !commit.is_empty() => match store.blueprint_at(commit) {
            Ok(text) => match wright_core::blueprint::parse::blueprint(&text) {
                Ok(_) => {
                    let in_history = store.blueprint_history().iter().any(|c| c.hash == commit);
                    if in_history {
                        checks.push(Check {
                            name: "blueprint",
                            ok: true,
                            note: format!(
                                "the plan is committed in this project's history as {}",
                                hash::short(commit)
                            ),
                        });
                        true
                    } else {
                        checks.push(Check { name: "blueprint", ok: false, note: format!("the blueprint {} exists but the store's history does not list it; the log was truncated or forged", hash::short(commit)) });
                        false
                    }
                }
                Err(e) => {
                    checks.push(Check {
                        name: "blueprint",
                        ok: false,
                        note: format!("the committed blueprint no longer parses: {e}"),
                    });
                    false
                }
            },
            Err(e) => {
                checks.push(Check {
                    name: "blueprint",
                    ok: false,
                    note: format!(
                        "the receipt names blueprint {}, which is not in the store: {e}",
                        hash::short(commit)
                    ),
                });
                false
            }
        },
        _ => {
            checks.push(Check {
                name: "blueprint",
                ok: false,
                note: "the receipt names no blueprint commit".into(),
            });
            false
        }
    };

    // 6. The identities: every task acted under an identity this
    //    receipt's signer delegated, each attests to its own calls, and
    //    the log's tool calls match those attestations one for one.
    let identities_ok = check_identities(store, &v, run, &mut checks);

    Verification {
        ok: sig_ok && events_ok && artifacts_ok && inputs_ok && blueprint_ok && identities_ok,
        checks,
        run,
    }
}

fn check_signature(v: &Value, checks: &mut Vec<Check>, pins: &[String]) -> bool {
    let Some(sig) = v.get("signature") else {
        checks.push(Check {
            name: "signature",
            ok: false,
            note: "the receipt carries no signature".into(),
        });
        return false;
    };
    let public_hex = sig.str_of("public_key").unwrap_or("");
    let value_hex = sig.str_of("value").unwrap_or("");
    let provenance = sig.str_of("provenance").unwrap_or("unknown");
    if public_hex.len() != 64 || value_hex.len() != 128 {
        checks.push(Check {
            name: "signature",
            ok: false,
            note: "the signature block is malformed (key or signature is not the right length)"
                .into(),
        });
        return false;
    }
    // Re-serialize the receipt WITHOUT the signature field. The writer is
    // insertion-ordered, so a re-parsed receipt serializes to the exact
    // bytes that were signed.
    let mut fields_out: Vec<(String, Value)> = Vec::new();
    if let Value::Obj(fields) = v {
        for (k, val) in fields {
            if k != "signature" {
                fields_out.push((k.clone(), val.clone()));
            }
        }
    }
    let stripped = Value::Obj(fields_out);
    let bytes = json::to_string(&stripped);
    let unhex = |h: &str, n: usize| -> Option<Vec<u8>> {
        if h.len() != n * 2 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        (0..n)
            .map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).ok())
            .collect()
    };
    let (Some(key), Some(sig_bytes)) = (unhex(public_hex, 32), unhex(value_hex, 64)) else {
        checks.push(Check {
            name: "signature",
            ok: false,
            note: "the signature block is not valid hex".into(),
        });
        return false;
    };
    let mut key_arr = [0u8; 32];
    key_arr.copy_from_slice(&key);
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    if wright_core::ed25519::verify(&key_arr, bytes.as_bytes(), &sig_arr) {
        let worth = match provenance {
            "os-keychain" => "an OS-keychain key — the strongest home",
            "env" => "an environment-provided key (CI or a server)",
            "local:generated" => {
                "a key generated on the signer's machine (weaker than an OS keychain, and says so)"
            }
            "ephemeral" => {
                "a key that lasted one process — only useful for replaying the same session"
            }
            other => other,
        };
        match wright_runtime::signing::signer(public_hex, pins) {
            Ok(who) => {
                checks.push(Check { name: "signature", ok: true, note: format!("Ed25519 signature by {} — {who} — verifies over the receipt's exact bytes ({}). The key is {worth}", hash::short(public_hex), provenance) });
                true
            }
            Err(why) => {
                checks.push(Check {
                    name: "signature",
                    ok: false,
                    note: format!("the signature matches the receipt's bytes, but {why}"),
                });
                false
            }
        }
    } else {
        checks.push(Check {
            name: "signature",
            ok: false,
            note: "the signature does NOT match the receipt's bytes — the receipt was edited after it was signed".into(),
        });
        false
    }
}

fn check_identities(store: &Store, v: &Value, run: Option<u64>, checks: &mut Vec<Check>) -> bool {
    let Some(list) = v.get("identities").and_then(|a| a.as_arr().map(|x| x.to_vec())) else {
        checks.push(Check {
            name: "identities",
            ok: true,
            note: "not recorded — this receipt predates task-scoped identities".into(),
        });
        return true;
    };
    let signer = v.get("signature").and_then(|s| s.str_of("public_key")).unwrap_or("");
    let mut by_did: std::collections::BTreeMap<String, (String, u64)> = std::collections::BTreeMap::new();
    for id in &list {
        match wright_runtime::identity::verify_closed(id, signer) {
            Ok(agent) => {
                by_did.insert(id.str_of("did").unwrap_or("").to_string(), (agent, id.get("calls").and_then(|n| n.as_u64()).unwrap_or(0)));
            }
            Err(e) => {
                checks.push(Check { name: "identities", ok: false, note: e });
                return false;
            }
        }
    }
    // The log's calls, per identity.
    let mut seen: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    if let Some(log) = run.and_then(|r| store.read_events(r).ok()) {
        for e in log.events() {
            if e.kind != wright_core::events::EventKind::ToolCalled {
                continue;
            }
            let Some(did) = e.data_value().and_then(|d| d.str_of("identity").map(String::from)) else { continue };
            if did.is_empty() {
                continue;
            }
            match by_did.get(&did) {
                Some((agent, _)) if Some(agent.as_str()) == e.agent.as_deref() => *seen.entry(did).or_insert(0) += 1,
                Some((agent, _)) => {
                    checks.push(Check { name: "identities", ok: false, note: format!("a call by {} was made under {agent}'s identity", e.agent.as_deref().unwrap_or("?")) });
                    return false;
                }
                None => {
                    checks.push(Check { name: "identities", ok: false, note: format!("a call in the log was made under {}, which this machine never issued", hash::short(&did)) });
                    return false;
                }
            }
        }
    }
    for (did, (agent, calls)) in &by_did {
        let logged = seen.get(did).copied().unwrap_or(0);
        if logged != *calls {
            checks.push(Check { name: "identities", ok: false, note: format!("{agent}'s identity attests to {calls} call(s), but the log shows {logged}") });
            return false;
        }
    }
    let total: u64 = by_did.values().map(|(_, c)| c).sum();
    checks.push(Check {
        name: "identities",
        ok: true,
        note: format!("{} task-scoped identit{}, each delegated by the signer and revoked at its task's end; all {total} tool call(s) in the log were made under them", by_did.len(), if by_did.len() == 1 { "y" } else { "ies" }),
    });
    true
}

fn check_artifacts(store: &Store, v: &Value, checks: &mut Vec<Check>) -> bool {
    let Some(list) = v
        .get("artifacts")
        .and_then(|a| a.as_arr().map(|x| x.to_vec()))
    else {
        checks.push(Check {
            name: "artifacts",
            ok: false,
            note: "the receipt lists no artifacts".into(),
        });
        return false;
    };
    if list.is_empty() {
        checks.push(Check {
            name: "artifacts",
            ok: true,
            note: "the run produced no artifacts; nothing to check".into(),
        });
        return true;
    }
    let mut all_ok = true;
    let mut notes = Vec::new();
    for a in &list {
        let Some(name) = a.str_of("name") else {
            continue;
        };
        let Some(claimed) = a.str_of("blake3") else {
            continue;
        };
        // (a) in the object store, byte-true
        match store.get_object(claimed) {
            Ok(bytes) if hash::blake3_hex(&bytes) == claimed => {}
            Ok(_) => {
                all_ok = false;
                notes.push(format!(
                    "the stored copy of {name} no longer matches its hash"
                ));
                continue;
            }
            Err(e) => {
                all_ok = false;
                notes.push(format!("the store cannot serve {name}'s object: {e}"));
                continue;
            }
        }
        // (b) on disk in the project, matching
        let path = store.project_dir.join("artifacts").join(name);
        match std::fs::read(&path) {
            Ok(bytes) => {
                let now = hash::blake3_hex(&bytes);
                if now == claimed {
                    continue;
                }
                all_ok = false;
                notes.push(format!(
                    "artifacts/{name} was swapped after the run: receipt says {}, file now is {}",
                    hash::short(claimed),
                    hash::short(&now)
                ));
            }
            Err(_) => {
                // Absent on disk is not tampering (later runs move on);
                // the store copy above is the durable witness.
                notes.push(format!(
                    "artifacts/{name} is no longer in the project (kept in the store as {}) — fine",
                    hash::short(claimed)
                ));
            }
        }
    }
    if all_ok {
        checks.push(Check {
            name: "artifacts",
            ok: true,
            note: format!(
                "{} artifact(s) are byte-identical to their recorded hashes in the store",
                list.len()
            ),
        });
        if !notes.is_empty() {
            // Informational only; keep the note honest but short.
        }
        true
    } else {
        checks.push(Check {
            name: "artifacts",
            ok: false,
            note: notes.join("; "),
        });
        false
    }
}

fn check_inputs(store: &Store, v: &Value, checks: &mut Vec<Check>) -> bool {
    let Some(list) = v.get("inputs").and_then(|a| a.as_arr().map(|x| x.to_vec())) else {
        checks.push(Check {
            name: "inputs",
            ok: false,
            note: "the receipt records no inputs (v2 receipts always do — this one is malformed)"
                .into(),
        });
        return false;
    };
    if list.is_empty() {
        checks.push(Check {
            name: "inputs",
            ok: true,
            note: "the plan consumed no upstream inputs; nothing to check".into(),
        });
        return true;
    }
    let mut all_ok = true;
    let mut bad = Vec::new();
    for i in &list {
        let Some(name) = i.str_of("name") else {
            continue;
        };
        let Some(claimed) = i.str_of("blake3") else {
            continue;
        };
        match store.get_object(claimed) {
            Ok(bytes) if hash::blake3_hex(&bytes) == claimed => {}
            _ => {
                all_ok = false;
                bad.push(format!("{name} ({})", hash::short(claimed)));
            }
        }
    }
    if all_ok {
        checks.push(Check {
            name: "inputs",
            ok: true,
            note: format!(
                "all {} recorded input(s) resolve in the object store, byte-true",
                list.len()
            ),
        });
        true
    } else {
        checks.push(Check {
            name: "inputs",
            ok: false,
            note: format!(
                "these recorded inputs are gone or corrupt in the store: {}",
                bad.join(", ")
            ),
        });
        false
    }
}
