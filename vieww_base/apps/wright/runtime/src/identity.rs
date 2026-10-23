//! Task-scoped agent identities: every agent in a firing acts under its
//! own identity, for its task only, revoked when the task ends.
//!
//! The finding this answers: when agents share one set of credentials, a
//! compromised one acts with everybody's reach, and afterwards nobody can
//! say which agent did what. In Wright:
//!
//! * **Issued per task.** When a task starts, it gets a fresh Ed25519 key
//!   (OS randomness) — a `did:key` nobody else holds — and a *delegation*:
//!   the machine's signing key signs the identity's claims (agent, run,
//!   tools, permissions, the secrets it may use, issued/expires). The
//!   claims are exactly the card's grant; nothing is added.
//! * **Checked on every call.** The permission gate refuses any tool call
//!   by an agent without a live identity — before its task, after it,
//!   after expiry (the firing's wall clock), after revocation. Each permit
//!   carries the identity, and each tool call is recorded against it.
//! * **Secrets are per identity.** An agent resolves only the secrets its
//!   card names (`secret "smtp"`) — never the whole blueprint's set.
//! * **Revoked at the end.** When the task finishes, the identity signs an
//!   attestation over the digest of every call it made and its key is
//!   dropped. The receipt lists every identity with its delegation and
//!   attestation, so a verifier can check that each action in the log
//!   was taken by an identity the machine actually issued, within its
//!   scope.

use std::collections::BTreeMap;
use std::sync::Mutex;

use wright_core::events::now_ms;
use wright_core::wire::json::Value;
use wright_core::{ed25519, hash, jcs};

use crate::signing::SigningKey;

/// What an identity is allowed, as delegated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claims {
    pub agent: String,
    pub run: u64,
    pub tools: Vec<String>,
    pub permissions: String,
    pub secrets: Vec<String>,
    pub issued_ms: u64,
    pub expires_ms: u64,
}

impl Claims {
    fn to_value(&self, subject_did: &str, issuer_did: &str) -> Value {
        Value::obj(vec![
            ("type", Value::str("wright.agent-identity/1")),
            ("id", Value::str(subject_did)),
            ("issuer", Value::str(issuer_did)),
            ("agent", Value::str(self.agent.clone())),
            ("run", Value::int(self.run as i64)),
            (
                "tools",
                Value::Arr(self.tools.iter().map(|t| Value::str(t.clone())).collect()),
            ),
            ("permissions", Value::str(self.permissions.clone())),
            (
                "secrets",
                Value::Arr(self.secrets.iter().map(|t| Value::str(t.clone())).collect()),
            ),
            ("issued_ms", Value::int(self.issued_ms as i64)),
            ("expires_ms", Value::int(self.expires_ms as i64)),
        ])
    }
}

/// A live identity (private: its key never leaves this module).
struct Live {
    did: String,
    seed: [u8; 32],
    claims: Claims,
    /// The delegation: the claims, as the machine signed them.
    claims_value: Value,
    delegation: String,
    /// Running BLAKE3 chain over the calls made, and their count.
    chain: String,
    calls: u64,
}

impl Drop for Live {
    fn drop(&mut self) {
        // The key dies with the task.
        self.seed = [0u8; 32];
    }
}

/// A finished identity, as the receipt records it.
#[derive(Debug, Clone, PartialEq)]
pub struct Closed {
    pub did: String,
    pub claims: Value,
    /// Hex Ed25519 signature by the machine key over JCS(claims).
    pub delegation: String,
    pub calls: u64,
    pub calls_digest: String,
    pub revoked_ms: u64,
    /// Hex Ed25519 signature by the identity's own key over
    /// JCS({did, calls, calls_digest, revoked_ms}).
    pub attestation: String,
}

impl Closed {
    pub fn to_value(&self) -> Value {
        Value::obj(vec![
            ("did", Value::str(self.did.clone())),
            ("claims", self.claims.clone()),
            ("delegation", Value::str(self.delegation.clone())),
            ("calls", Value::int(self.calls as i64)),
            ("calls_digest", Value::str(self.calls_digest.clone())),
            ("revoked_ms", Value::int(self.revoked_ms as i64)),
            ("attestation", Value::str(self.attestation.clone())),
        ])
    }
}

fn attestation_body(did: &str, calls: u64, digest: &str, revoked_ms: u64) -> Value {
    Value::obj(vec![
        ("did", Value::str(did)),
        ("calls", Value::int(calls as i64)),
        ("calls_digest", Value::str(digest)),
        ("revoked_ms", Value::int(revoked_ms as i64)),
    ])
}

/// Every identity of one firing.
pub struct Registry {
    machine: SigningKey,
    issuer_did: String,
    live: Mutex<BTreeMap<String, Live>>,
    closed: Mutex<Vec<Closed>>,
}

impl Registry {
    pub fn new(machine: SigningKey) -> Registry {
        let issuer_did = jcs::did_key(&ed25519::public_key(&machine.seed_for_signing()));
        Registry {
            machine,
            issuer_did,
            live: Mutex::new(BTreeMap::new()),
            closed: Mutex::new(Vec::new()),
        }
    }

    /// Issue an identity for one task. Replaces nothing: a task that
    /// already holds a live identity keeps it.
    pub fn issue(&self, claims: Claims) -> Result<String, String> {
        let mut live = self.live.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(l) = live.get(&claims.agent) {
            return Ok(l.did.clone());
        }
        let seed = crate::signing::random_seed()?;
        let did = jcs::did_key(&ed25519::public_key(&seed));
        let claims_value = claims.to_value(&did, &self.issuer_did);
        let delegation = to_hex64(
            &self
                .machine
                .sign(jcs::canonicalize(&claims_value)?.as_bytes()),
        );
        let agent = claims.agent.clone();
        live.insert(
            agent,
            Live {
                did: did.clone(),
                seed,
                claims,
                claims_value,
                delegation,
                chain: hash::blake3_hex(did.as_bytes()),
                calls: 0,
            },
        );
        Ok(did)
    }

    /// The live identity of an agent, or why there is none.
    pub fn check(&self, agent: &str) -> Result<String, String> {
        let live = self.live.lock().unwrap_or_else(|p| p.into_inner());
        match live.get(agent) {
            Some(l) if now_ms() <= l.claims.expires_ms => Ok(l.did.clone()),
            Some(_) => Err(format!("'{agent}''s identity expired with the firing's clock")),
            None => Err(format!("'{agent}' holds no live identity: its task is not running, or its identity was revoked")),
        }
    }

    /// May this agent's identity use the secret `name`?
    pub fn may_use_secret(&self, agent: &str, name: &str) -> bool {
        let live = self.live.lock().unwrap_or_else(|p| p.into_inner());
        live.get(agent)
            .is_some_and(|l| l.claims.secrets.iter().any(|s| s == name))
    }

    /// Record one call against the agent's identity (the chain covers
    /// the tool, the input's hash and the outcome, in order).
    pub fn record(&self, agent: &str, tool: &str, input_hash: &str, ok: bool) {
        let mut live = self.live.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(l) = live.get_mut(agent) {
            l.calls += 1;
            l.chain = hash::blake3_hex(format!("{}|{tool}|{input_hash}|{ok}", l.chain).as_bytes());
        }
    }

    /// End the task: the identity attests to its calls, then is revoked.
    pub fn revoke(&self, agent: &str) {
        let Some(l) = self
            .live
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(agent)
        else {
            return;
        };
        let revoked_ms = now_ms();
        let body = attestation_body(&l.did, l.calls, &l.chain, revoked_ms);
        let attestation = jcs::canonicalize(&body)
            .map(|b| to_hex64(&ed25519::sign(&l.seed, b.as_bytes())))
            .unwrap_or_default();
        self.closed
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(Closed {
                did: l.did.clone(),
                claims: l.claims_value.clone(),
                delegation: l.delegation.clone(),
                calls: l.calls,
                calls_digest: l.chain.clone(),
                revoked_ms,
                attestation,
            });
    }

    /// Revoke everything still live (the end of a firing) and return all.
    pub fn close_all(&self) -> Vec<Closed> {
        let agents: Vec<String> = self
            .live
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .keys()
            .cloned()
            .collect();
        for a in agents {
            self.revoke(&a);
        }
        self.closed
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

fn to_hex64(sig: &[u8; 64]) -> String {
    sig.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex<const N: usize>(h: &str) -> Option<[u8; N]> {
    if h.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(h.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// Check one recorded identity: the machine (`issuer_hex`) delegated it,
/// and the identity itself attested to its calls. Ok(agent name).
pub fn verify_closed(v: &Value, issuer_hex: &str) -> Result<String, String> {
    let did = v.str_of("did").ok_or("an identity without a did")?;
    let claims = v.get("claims").ok_or("an identity without claims")?;
    let agent = claims.str_of("agent").unwrap_or("?").to_string();
    if claims.str_of("id") != Some(did) {
        return Err(format!("{agent}: the claims name another identity"));
    }
    let issuer = unhex::<32>(issuer_hex).ok_or("the receipt's signer key is malformed")?;
    if claims.str_of("issuer") != Some(jcs::did_key(&issuer).as_str()) {
        return Err(format!(
            "{agent}: delegated by someone other than the receipt's signer"
        ));
    }
    let deleg = unhex::<64>(v.str_of("delegation").unwrap_or(""))
        .ok_or_else(|| format!("{agent}: malformed delegation"))?;
    if !ed25519::verify(&issuer, jcs::canonicalize(claims)?.as_bytes(), &deleg) {
        return Err(format!("{agent}: the delegation does not match its claims"));
    }
    let pk = jcs::did_key_public(did)?;
    let n = |k: &str| v.get(k).and_then(|x| x.as_u64()).unwrap_or(0);
    let body = attestation_body(
        did,
        n("calls"),
        v.str_of("calls_digest").unwrap_or(""),
        n("revoked_ms"),
    );
    let att = unhex::<64>(v.str_of("attestation").unwrap_or(""))
        .ok_or_else(|| format!("{agent}: malformed attestation"))?;
    if !ed25519::verify(&pk, jcs::canonicalize(&body)?.as_bytes(), &att) {
        return Err(format!(
            "{agent}: the identity's attestation does not match its calls"
        ));
    }
    Ok(agent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claims(agent: &str, secrets: &[&str]) -> Claims {
        Claims {
            agent: agent.into(),
            run: 1,
            tools: vec!["http-post".into()],
            permissions: "network".into(),
            secrets: secrets.iter().map(|s| s.to_string()).collect(),
            issued_ms: now_ms(),
            expires_ms: now_ms() + 60_000,
        }
    }

    #[test]
    fn issued_checked_scoped_revoked_and_verifiable() {
        let machine = SigningKey::for_tests([4u8; 32]);
        let issuer = machine.public_hex();
        let r = Registry::new(machine);
        assert!(r.check("Mailer").is_err(), "no identity before the task");
        let did = r.issue(claims("Mailer", &["smtp"])).unwrap();
        assert!(did.starts_with("did:key:z6Mk"));
        assert_eq!(r.check("Mailer").unwrap(), did);
        r.issue(claims("Fetcher", &[])).unwrap();
        assert_ne!(
            r.check("Fetcher").unwrap(),
            did,
            "every task its own identity"
        );
        assert!(r.may_use_secret("Mailer", "smtp"));
        assert!(
            !r.may_use_secret("Fetcher", "smtp"),
            "no shared credentials"
        );
        r.record("Mailer", "http-post", "abc", true);
        r.revoke("Mailer");
        assert!(r.check("Mailer").is_err(), "revoked at the end of its task");
        assert!(!r.may_use_secret("Mailer", "smtp"));
        let closed = r.close_all();
        assert_eq!(closed.len(), 2);
        let mailer = closed.iter().find(|c| c.did == did).unwrap();
        assert_eq!(mailer.calls, 1);
        assert_eq!(
            verify_closed(&mailer.to_value(), &issuer).unwrap(),
            "Mailer"
        );
        // A widened scope breaks the delegation...
        let mut forged = mailer.to_value();
        if let Some(Value::Obj(c)) = forged.get("claims").cloned().map(|mut c| {
            c.set("tools", Value::Arr(vec![Value::str("shell")]));
            c
        }) {
            forged.set("claims", Value::Obj(c));
        }
        assert!(verify_closed(&forged, &issuer)
            .unwrap_err()
            .contains("delegation"));
        // ...and a hidden call breaks the attestation.
        let mut hidden = mailer.to_value();
        hidden.set("calls", Value::int(0));
        assert!(verify_closed(&hidden, &issuer)
            .unwrap_err()
            .contains("attestation"));
        // Another machine's identities are not this receipt's.
        assert!(verify_closed(&mailer.to_value(), &"00".repeat(32)).is_err());
    }

    #[test]
    fn an_expired_identity_is_refused() {
        let r = Registry::new(SigningKey::for_tests([5u8; 32]));
        let mut c = claims("Slow", &[]);
        c.expires_ms = 1;
        r.issue(c).unwrap();
        assert!(r.check("Slow").unwrap_err().contains("expired"));
    }
}
