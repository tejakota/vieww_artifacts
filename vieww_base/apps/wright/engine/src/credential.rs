//! A receipt anyone can verify without Wright: a W3C Verifiable Credential
//! (VC Data Model 2.0) with a Data Integrity proof in the `eddsa-jcs-2022`
//! cryptosuite — RFC 8785 canonical JSON, SHA-256, Ed25519 — issued by the
//! signer's `did:key`.
//!
//! Wright's own `receipt.json` stays the operational record (`wright
//! verify` checks it against the store: log, artifacts, inputs, plan).
//! This file is the portable proof of the same facts: an auditor, an
//! insurer or a procurement tool checks it with any standard Data
//! Integrity library, no Wright installed. Every run writes one next to
//! its receipt (`receipt.vc.json`); `wright receipt vc` exports it.
//!
//! A standard format does not change the rule of ADR-031: a valid proof
//! says the bytes are the issuer's; WHO the issuer is comes from a key
//! the verifier pinned. `verify` here checks both, and `wright verify
//! <file.vc.json>` refuses an issuer nobody pinned.
//!
//! The algorithm (Data Integrity EdDSA Cryptosuites v1.0, §3.3):
//!
//! ```text
//!   proof.@context = the document's @context (kept in the proof, as the
//!                    spec and its reference implementation do)
//!   proofConfig = proof without proofValue
//!   hashData    = SHA-256(JCS(proofConfig)) ‖ SHA-256(JCS(document − proof))
//!   proofValue  = "z" + base58btc(Ed25519.sign(key, hashData))
//! ```

use wright_core::jcs;
use wright_core::schedule::rfc3339_utc;
use wright_core::sha256;
use wright_core::wire::json::{self, Value};
use wright_runtime::signing::SigningKey;

const CONTEXT: &str = "https://www.w3.org/ns/credentials/v2";
pub const CRYPTOSUITE: &str = "eddsa-jcs-2022";

/// Turn a signed `wright.run-receipt/2` into a signed credential.
pub fn issue(receipt: &Value, key: &SigningKey) -> Result<Value, String> {
    let Value::Obj(fields) = receipt else {
        return Err("a receipt is a JSON object".into());
    };
    let pk = key_bytes(&key.public_hex())?;
    let did = jcs::did_key(&pk);
    let method = format!("{did}#{}", jcs::multibase_key(&pk));
    let finished = receipt
        .get("finished_ms")
        .and_then(|n| n.as_u64())
        .unwrap_or(0);
    let run = receipt.get("run").and_then(|n| n.as_u64()).unwrap_or(0);
    // The subject is the receipt itself, minus Wright's own signature
    // block (this credential's proof replaces it).
    let mut subject: Vec<(String, Value)> = vec![(
        "id".into(),
        Value::str(format!(
            "urn:wright:run:{}:{run}",
            receipt.str_of("blueprint").unwrap_or("unknown")
        )),
    )];
    subject.extend(fields.iter().filter(|(k, _)| k != "signature").cloned());
    let mut doc = Value::obj(vec![
        ("@context", Value::Arr(vec![Value::str(CONTEXT)])),
        (
            "type",
            Value::Arr(vec![
                Value::str("VerifiableCredential"),
                Value::str("WrightRunReceipt"),
            ]),
        ),
        ("issuer", Value::str(did)),
        (
            "validFrom",
            Value::str(rfc3339_utc((finished / 1000) as i64)),
        ),
        ("credentialSubject", Value::Obj(subject)),
    ]);
    let mut proof = Value::obj(vec![
        ("type", Value::str("DataIntegrityProof")),
        ("cryptosuite", Value::str(CRYPTOSUITE)),
        ("created", Value::str(rfc3339_utc((finished / 1000) as i64))),
        ("verificationMethod", Value::str(method)),
        ("proofPurpose", Value::str("assertionMethod")),
        ("@context", Value::Arr(vec![Value::str(CONTEXT)])),
    ]);
    let data = hash_data(&doc, &proof)?;
    let sig = key.sign(&data);
    proof.set(
        "proofValue",
        Value::str(format!("z{}", jcs::base58_encode(&sig))),
    );
    doc.set("proof", proof);
    Ok(doc)
}

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// The issuer's did:key.
    pub issuer: String,
    /// Who that is, as this machine knows it (pinned name, own key, --key).
    pub who: String,
    pub run: u64,
}

/// Verify a credential: the proof over the exact canonical bytes, the
/// issuer matching the key that signed, and the issuer trusted here.
pub fn verify(doc: &Value, pins: &[String]) -> Result<Checked, String> {
    let proof = doc
        .get("proof")
        .ok_or("the credential carries no proof")?
        .clone();
    if proof.str_of("cryptosuite") != Some(CRYPTOSUITE)
        || proof.str_of("type") != Some("DataIntegrityProof")
    {
        return Err(format!(
            "the proof is not a DataIntegrityProof using {CRYPTOSUITE}"
        ));
    }
    let method = proof
        .str_of("verificationMethod")
        .ok_or("the proof names no verificationMethod")?;
    let issuer = doc
        .str_of("issuer")
        .ok_or("the credential names no issuer")?;
    if method.split('#').next() != Some(issuer) {
        return Err(format!(
            "the proof was made by {method}, which is not the issuer {issuer}"
        ));
    }
    let pk = jcs::did_key_public(method)?;
    let value = proof
        .str_of("proofValue")
        .ok_or("the proof has no proofValue")?;
    let sig_bytes = jcs::base58_decode(
        value
            .strip_prefix('z')
            .ok_or("proofValue is not base58btc multibase")?,
    )?;
    let sig: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| "the signature is not 64 bytes".to_string())?;
    let mut config = proof.clone();
    if let Value::Obj(f) = &mut config {
        f.retain(|(k, _)| k != "proofValue");
    }
    let mut unsigned = doc.clone();
    if let Value::Obj(f) = &mut unsigned {
        f.retain(|(k, _)| k != "proof");
    }
    // The proof's @context must be the start of the document's.
    if let Some(Value::Arr(pc)) = config.get("@context") {
        let dc = match doc.get("@context") {
            Some(Value::Arr(d)) => d.clone(),
            Some(other) => vec![other.clone()],
            None => Vec::new(),
        };
        if dc.len() < pc.len() || dc[..pc.len()] != pc[..] {
            return Err("the document's @context does not start with the proof's".into());
        }
        unsigned.set("@context", Value::Arr(pc.clone()));
    }
    let data = hash_data(&unsigned, &config)?;
    if !wright_core::ed25519::verify(&pk, &data, &sig) {
        return Err(
            "the proof does NOT match the credential — it was changed after it was signed".into(),
        );
    }
    let hex: String = pk.iter().map(|b| format!("{b:02x}")).collect();
    let who = wright_runtime::signing::signer(&hex, pins)?;
    Ok(Checked {
        issuer: issuer.to_string(),
        who,
        run: doc
            .get("credentialSubject")
            .and_then(|s| s.get("run"))
            .and_then(|n| n.as_u64())
            .unwrap_or(0),
    })
}

/// `SHA-256(JCS(proofConfig)) ‖ SHA-256(JCS(document))`.
fn hash_data(doc: &Value, proof_config: &Value) -> Result<Vec<u8>, String> {
    let mut config = proof_config.clone();
    if let Value::Obj(f) = &mut config {
        f.retain(|(k, _)| k != "proofValue");
    }
    let mut out = sha256::digest(jcs::canonicalize(&config)?.as_bytes()).to_vec();
    out.extend_from_slice(&sha256::digest(jcs::canonicalize(doc)?.as_bytes()));
    Ok(out)
}

fn key_bytes(hex: &str) -> Result<[u8; 32], String> {
    if hex.len() != 64 {
        return Err("a public key is 64 hex characters".into());
    }
    let mut k = [0u8; 32];
    for (i, b) in k.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string())?;
    }
    Ok(k)
}

/// Read a credential file and verify it.
pub fn verify_file(path: &std::path::Path, pins: &[String]) -> Result<Checked, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc = json::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    verify(&doc, pins)
}
