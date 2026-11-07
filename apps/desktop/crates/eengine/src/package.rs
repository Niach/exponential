//! VAPP-261: loading a package file: the signature check (the Ed25519 the
//! genUI host leaves to its host, over `eui_core::host::package_file_signed_bytes`;
//! the twin of TS `verifyPackageFile`, `fixtures/host-package.json`
//! `signatures`) and the package's validation.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use eui_core::host::{package_file_signed_bytes, PackageIssue, AUTHOR_KEY_PREFIX, AUTHOR_NAME_MAX, PACKAGE_FILE_SIGNATURE_ALG};

/// A package file's signature status.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Verification {
    Unsigned,
    Valid { author: Author },
    Invalid { issues: Vec<Issue> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    pub key: String,
}

/// A JSON-pointer issue (genUI's `PackageIssue`, serialisable).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

impl From<PackageIssue> for Issue {
    fn from(i: PackageIssue) -> Self {
        Issue { path: i.path, message: i.message }
    }
}

/// Unpadded base64url (`[A-Za-z0-9_-]*`, a length that is not 1 mod 4) → bytes.
pub fn base64url_decode(text: &str) -> Option<Vec<u8>> {
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in text.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The 32-byte public key of `author.key` (`ed25519:` + base64url).
pub fn author_public_key(key: Option<&Value>) -> Option<Vec<u8>> {
    let key = key?.as_str()?.strip_prefix(AUTHOR_KEY_PREFIX)?;
    base64url_decode(key).filter(|b| b.len() == 32)
}

fn issue(path: &str, message: &str) -> Issue {
    Issue { path: path.into(), message: message.into() }
}

/// Verify a package file's signature (TS `verifyPackageFile`).
pub fn verify_package_file(file: &Value) -> Verification {
    let Some(f) = file.as_object() else { return Verification::Invalid { issues: vec![issue("", "a package file is an object")] } };
    let Some(sig) = f.get("signature") else { return Verification::Unsigned };
    let mut issues = Vec::new();
    let mut value = None;
    match sig.as_object() {
        None => issues.push(issue("/signature", "signature is {alg: \"ed25519\", value}")),
        Some(sig) => {
            if sig.get("alg").and_then(Value::as_str) != Some(PACKAGE_FILE_SIGNATURE_ALG) {
                issues.push(issue("/signature/alg", "alg is ed25519"));
            }
            value = sig.get("value").and_then(Value::as_str).and_then(base64url_decode).filter(|v| v.len() == 64);
            if value.is_none() {
                issues.push(issue("/signature/value", "value is 64 bytes, base64url"));
            }
        }
    }
    let mut key = None;
    match f.get("author").and_then(Value::as_object) {
        None => issues.push(issue("/author", "a signed file names its author {name, key}")),
        Some(author) => {
            let name_ok = author.get("name").and_then(Value::as_str).is_some_and(|n| !n.trim().is_empty() && n.chars().count() <= AUTHOR_NAME_MAX);
            if !name_ok {
                issues.push(issue("/author/name", &format!("name is 1 to {AUTHOR_NAME_MAX} characters")));
            }
            key = author_public_key(author.get("key"));
            if key.is_none() {
                issues.push(issue("/author/key", "key is ed25519: and 32 bytes, base64url"));
            }
        }
    }
    if !issues.is_empty() {
        return Verification::Invalid { issues };
    }
    let (Some(value), Some(key)) = (value, key) else { return Verification::Invalid { issues } };
    let signed = package_file_signed_bytes(f);
    let ok = ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, &key).verify(&signed, &value).is_ok();
    if !ok {
        return Verification::Invalid { issues: vec![issue("/signature", "does not verify against /author/key")] };
    }
    let author = &f["author"];
    Verification::Valid { author: Author { name: author["name"].as_str().unwrap_or_default().into(), key: author["key"].as_str().unwrap_or_default().into() } }
}

/// The package a file carries (`package`), or the value itself when it is a bare package.
pub fn package_of(file: &Value) -> &Value {
    file.get("package").filter(|p| p.is_object()).unwrap_or(file)
}

/// A package's issues against the core catalogs (genUI's `validatePackage`).
pub fn validate(pkg: &Value) -> Vec<Issue> {
    // Round 9 §14.1: format 5 is read through the bridge until the flag day.
    let upgraded = eui_core::catalog::ACCEPT_V09.then(|| eui_core::upgrade09::upgrade_package_body09(pkg));
    eui_core::host::validate_package(upgraded.as_ref().unwrap_or(pkg), &eui_core::host::supported_catalog_ids::<&str>(&[])).into_iter().map(Issue::from).collect()
}

/// The signer of a verified file (its author key), what grants are kept for.
pub fn signer_of(verification: &Verification) -> Option<&str> {
    match verification {
        Verification::Valid { author } => Some(&author.key),
        _ => None,
    }
}
