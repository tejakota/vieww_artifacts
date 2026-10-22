//! Signed recipes (ADR-028): a blueprint you can hand to a stranger.
//!
//! The lesson of the poisoned marketplaces is written in the memo:
//! open galleries without provenance get poisoned, so Wright's recipes
//! start curated and SIGNED, and an unsigned recipe cannot be installed
//! — not "warned about", refused. A recipe is one file, the same KDL
//! dialect as everything else:
//!
//! ```kdl
//! recipe "hn-digest" {
//!   version "1.0"
//!   publisher "Wright gallery"
//!   description "Every morning at 8, the top HN stories in your inbox."
//!   blueprint "<the canonical blueprint.kdl text, one escaped string>"
//!   publisher-key "<32-byte Ed25519 public key, hex>"
//! }
//! signature "<64-byte Ed25519 signature over the canonical recipe text, hex>"
//! ```
//!
//! The signature covers [`canon_bytes`] — a deterministic serialization
//! of the recipe WITHOUT the signature line — so two byte-different
//! files that mean the same recipe verify identically, and one edited
//! character does not. `wright recipe sign` signs with the publishing
//! key (`keychain:wright/recipe-publishing`, or the same env override
//! the receipt key uses); `verify` checks the signature against the
//! key EMBEDDED in the recipe — trust in a publisher is established by
//! pinning their key, which is a setting, not a guess.

use crate::blueprint::canon::escape_string;
use crate::blueprint::parse::parse_doc;
use crate::ed25519;

/// The one format id a signed recipe may carry.
pub const FORMAT: &str = "wright.recipe/1";

/// A parsed recipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub id: String,
    pub version: String,
    pub publisher: String,
    pub description: String,
    /// The full `blueprint.kdl` text, verbatim.
    pub blueprint: String,
    /// The publisher's Ed25519 public key, hex (64 chars).
    pub publisher_key: String,
    /// The Ed25519 signature over [`canon_bytes`], hex (128 chars); None
    /// until the recipe is signed.
    pub signature: Option<String>,
}

impl Recipe {
    /// Parse recipe text. Structural errors are plain sentences.
    pub fn parse(src: &str) -> Result<Recipe, String> {
        let doc = parse_doc(src).map_err(|e| format!("recipe: {e}"))?;
        let mut recipe: Option<Recipe> = None;
        let mut signature = None;
        for node in &doc {
            match node.name.as_str() {
                "recipe" => {
                    if recipe.is_some() {
                        return Err(
                            "a recipe file has exactly one recipe{} block".replace("{}", "")
                        );
                    }
                    let id = node
                        .arg_str()
                        .ok_or("recipe needs an id: recipe \"hn-digest\" { ... }")?
                        .to_string();
                    let mut r = Recipe {
                        id,
                        version: String::new(),
                        publisher: String::new(),
                        description: String::new(),
                        blueprint: String::new(),
                        publisher_key: String::new(),
                        signature: None,
                    };
                    for child in &node.children {
                        let v = |k: &str| child.prop_str(k).map(String::from);
                        match child.name.as_str() {
                            "version" => {
                                r.version = child.arg_str().unwrap_or_default().to_string()
                            }
                            "publisher" => {
                                r.publisher = child.arg_str().unwrap_or_default().to_string()
                            }
                            "description" => {
                                r.description = child.arg_str().unwrap_or_default().to_string()
                            }
                            "blueprint" => {
                                r.blueprint = child.arg_str().unwrap_or_default().to_string()
                            }
                            "publisher-key" => {
                                r.publisher_key = child.arg_str().unwrap_or_default().to_string()
                            }
                            other => {
                                let _ = v(other);
                                return Err(format!("unknown field '{other}' in the recipe block"));
                            }
                        }
                    }
                    if r.blueprint.trim().is_empty() {
                        return Err(format!("recipe '{}' embeds no blueprint", r.id));
                    }
                    if !r.publisher_key.is_empty() && r.publisher_key.len() != 64 {
                        return Err(format!(
                            "recipe '{}': publisher-key must be 32 bytes of hex",
                            r.id
                        ));
                    }
                    recipe = Some(r);
                }
                "signature" => {
                    let sig = node
                        .arg_str()
                        .ok_or("signature needs a hex string argument")?;
                    if sig.len() != 128 || !sig.bytes().all(|b| b.is_ascii_hexdigit()) {
                        return Err("signature must be 64 bytes of hex".into());
                    }
                    signature = Some(sig.to_string());
                }
                other => return Err(format!("unknown node '{other}' in a recipe file")),
            }
        }
        let mut r = recipe.ok_or("the file has no recipe{} block")?;
        r.signature = signature;
        Ok(r)
    }

    /// The bytes a signature covers: the recipe, deterministic and
    /// signature-free. Signing the canonical form (not the file bytes)
    /// is what lets a publisher reformat the file without re-signing,
    /// while an edited blueprint still fails verification.
    pub fn canon_bytes(&self) -> Vec<u8> {
        let mut s = String::new();
        s.push_str(&format!("format {}\n", escape_string(FORMAT)));
        s.push_str(&format!("recipe {}\n", escape_string(&self.id)));
        s.push_str(&format!("version {}\n", escape_string(&self.version)));
        s.push_str(&format!("publisher {}\n", escape_string(&self.publisher)));
        s.push_str(&format!(
            "description {}\n",
            escape_string(&self.description)
        ));
        s.push_str(&format!("blueprint {}\n", escape_string(&self.blueprint)));
        s.push_str(&format!(
            "publisher-key {}\n",
            escape_string(&self.publisher_key)
        ));
        s.into_bytes()
    }

    /// Render the recipe as KDL text, signature line included when set.
    pub fn to_kdl(&self) -> String {
        let mut s = String::new();
        s.push_str("// wright.recipe/1 — verify with: wright recipe verify <file>\n");
        s.push_str(&format!("recipe \"{}\" {{\n", escape_string(&self.id)));
        s.push_str(&format!("  version \"{}\"\n", escape_string(&self.version)));
        s.push_str(&format!(
            "  publisher \"{}\"\n",
            escape_string(&self.publisher)
        ));
        s.push_str(&format!(
            "  description \"{}\"\n",
            escape_string(&self.description)
        ));
        s.push_str(&format!(
            "  blueprint \"{}\"\n",
            escape_string(&self.blueprint)
        ));
        s.push_str(&format!(
            "  publisher-key \"{}\"\n",
            escape_string(&self.publisher_key)
        ));
        s.push_str("}\n");
        if let Some(sig) = &self.signature {
            s.push_str(&format!("signature \"{}\"\n", escape_string(sig)));
        }
        s
    }

    /// Check the embedded signature. Unsigned recipes fail with the
    /// sentence that stops the install (ADR-028).
    pub fn verify(&self) -> Result<(), String> {
        let Some(sig) = &self.signature else {
            return Err(format!(
                "recipe '{}' carries no signature. Unsigned recipes are not installed — \
                 the gallery's lesson is that an open, unsigned marketplace gets poisoned. \
                 Ask {} to sign it, or verify the blueprint by hand.",
                self.id,
                if self.publisher.is_empty() {
                    "its publisher"
                } else {
                    &self.publisher
                }
            ));
        };
        if self.publisher_key.len() != 64 {
            return Err(format!("recipe '{}' has no publisher key pinned", self.id));
        }
        let unhex = |h: &str, n: usize| -> Result<Vec<u8>, String> {
            if h.len() != n * 2 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("bad hex".into());
            }
            (0..n)
                .map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string()))
                .collect()
        };
        let key = unhex(&self.publisher_key, 32).map_err(|_| {
            format!(
                "recipe '{}': the publisher key is not 32 bytes of hex",
                self.id
            )
        })?;
        let sig = unhex(sig, 64)
            .map_err(|_| format!("recipe '{}': the signature is not 64 bytes of hex", self.id))?;
        let mut key_arr = [0u8; 32];
        key_arr.copy_from_slice(&key);
        let mut sig_arr = [0u8; 64];
        sig_arr.copy_from_slice(&sig);
        if ed25519::verify(&key_arr, &self.canon_bytes(), &sig_arr) {
            Ok(())
        } else {
            Err(format!(
                "recipe '{}' failed verification: its content no longer matches {}'s signature. \
                 Get a fresh copy from the publisher.",
                self.id,
                if self.publisher.is_empty() {
                    "the publisher"
                } else {
                    &self.publisher
                }
            ))
        }
    }

    /// Sign with a raw 32-byte seed. Returns the signature hex.
    pub fn sign_with(&mut self, seed: &[u8; 32]) -> String {
        self.publisher_key = hex_key(&ed25519::public_key(seed));
        let sig = ed25519::sign(seed, &self.canon_bytes());
        let hex_sig = hex_bytes(&sig);
        self.signature = Some(hex_sig.clone());
        hex_sig
    }
}

/// Sign a recipe file in place: read, sign, verify, write back.
pub fn sign_file(path: &std::path::Path, seed: &[u8; 32]) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut r = Recipe::parse(&text)?;
    r.sign_with(seed);
    r.verify()?;
    crate::blueprint::parse::blueprint(&r.blueprint)
        .map_err(|e| format!("the embedded blueprint does not parse: {e}"))?;
    std::fs::write(path, r.to_kdl()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(())
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hex_key(k: &[u8; 32]) -> String {
    hex_bytes(k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blueprint::{self, parse};

    const BLUEPRINT: &str = "meta { name \"Weekly report\" }\nintent { summary \"weekly\" confirmed \"v1\" }\nagent Reporter { goal \"report\" out \"report.md\" }\ndeliverable kind=\"report\" { check \"it is one page\" }\n";

    fn recipe_text() -> String {
        Recipe {
            id: "weekly-report".into(),
            version: "1.0".into(),
            publisher: "Wright gallery".into(),
            description: "A one-page summary of the CSVs in your sales folder, every Monday."
                .into(),
            blueprint: BLUEPRINT.into(),
            publisher_key: String::new(),
            signature: None,
        }
        .to_kdl()
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("wright-recipe-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn round_trips_through_the_dialect() {
        let src = recipe_text();
        let r = Recipe::parse(&src).unwrap();
        assert_eq!(r.id, "weekly-report");
        assert_eq!(r.blueprint, BLUEPRINT);
        assert_eq!(r.signature, None);
        assert_eq!(Recipe::parse(&r.to_kdl()).unwrap(), r);
    }

    #[test]
    fn unsigned_recipes_are_refused_in_plain_words() {
        let r = Recipe::parse(&recipe_text()).unwrap();
        let err = r.verify().unwrap_err();
        assert!(err.contains("no signature"), "{err}");
        assert!(err.contains("Unsigned recipes are not installed"), "{err}");
    }

    #[test]
    fn signing_then_verifying_and_tamper_detection() {
        let mut r = Recipe::parse(&recipe_text()).unwrap();
        let seed = [7u8; 32];
        r.sign_with(&seed);
        r.verify().expect("freshly signed recipe verifies");
        assert!(r.signature.is_some());
        // A different publisher's key cannot verify it.
        let mut other = r.clone();
        other.publisher_key = hex_key(&ed25519::public_key(&[8u8; 32]));
        assert!(other.verify().is_err());
        // One edited character in the blueprint breaks it.
        let mut tampered = r.clone();
        tampered.blueprint = tampered.blueprint.replace("report", "REDACTED");
        let err = tampered.verify().unwrap_err();
        assert!(err.contains("failed verification"), "{err}");
        // The signature covers the canon bytes, not the file layout:
        // a reformatted file verifies if the meaning is unchanged.
        let reformatted = Recipe {
            id: r.id.clone(),
            version: r.version.clone(),
            publisher: r.publisher.clone(),
            description: r.description.clone(),
            blueprint: r.blueprint.clone(),
            publisher_key: r.publisher_key.clone(),
            signature: r.signature.clone(),
        };
        assert_eq!(reformatted.canon_bytes(), r.canon_bytes());
        reformatted.verify().unwrap();
    }

    #[test]
    fn canon_bytes_change_when_anything_meaningful_changes() {
        let mut r = Recipe::parse(&recipe_text()).unwrap();
        let before = r.canon_bytes();
        r.description.push('X');
        assert_ne!(r.canon_bytes(), before);
    }

    #[test]
    fn embedded_blueprint_must_be_a_real_blueprint() {
        // Build a recipe whose embedded text is NOT a valid blueprint
        // (meta only, no intent): structure is fine, meaning is not.
        let r = Recipe::parse(&recipe_text()).unwrap();
        let mut broken = r.clone();
        broken.blueprint = "meta { name \"no agents\" }".into();
        let dir = tmp("bad-embed");
        let p = dir.join("bad.recipe.kdl");
        std::fs::write(&p, broken.to_kdl()).unwrap();
        // the embedded text does not parse as a blueprint
        let parsed = Recipe::parse(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert!(parse::blueprint(&parsed.blueprint).is_err());
        // so signing must refuse to sign it
        assert!(
            sign_file(&p, &[9u8; 32]).is_err(),
            "signing must refuse an unparseable embedded blueprint"
        );
        // while the good one signs fine
        let q = dir.join("good.recipe.kdl");
        std::fs::write(&q, recipe_text()).unwrap();
        sign_file(&q, &[9u8; 32]).unwrap();
    }

    #[test]
    fn sign_file_signs_and_the_file_verifies_after() {
        let dir = tmp("sign");
        let p = dir.join("weekly-report.recipe.kdl");
        std::fs::write(&p, recipe_text()).unwrap();
        sign_file(&p, &[1u8; 32]).unwrap();
        let r = Recipe::parse(&std::fs::read_to_string(&p).unwrap()).unwrap();
        r.verify().unwrap();
        // and the file parses back into the same recipe, now signed
        assert!(r.signature.is_some());
        // tampering on disk is caught on the next verify
        let mut text = std::fs::read_to_string(&p).unwrap();
        text = text.replace("one-page", "two-page");
        std::fs::write(&p, text).unwrap();
        let r2 = Recipe::parse(&std::fs::read_to_string(&p).unwrap()).unwrap();
        assert!(r2.verify().is_err());
    }

    #[test]
    fn blueprint_validation_still_applies_to_installed_content() {
        // The recipe machinery signs bytes; the blueprint machinery
        // decides if the PLAN is sound. Both must pass to install.
        let r = Recipe::parse(&recipe_text()).unwrap();
        let bp = blueprint::parse::blueprint(&r.blueprint).unwrap();
        assert!(
            blueprint::validate::validate(&bp).is_ok(),
            "{:?}",
            blueprint::validate::validate(&bp)
        );
    }
}
