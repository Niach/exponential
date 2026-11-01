//! The vault: encryption at rest for every secret eMCP touches
//! (`packages/emcp/fixtures/vault.json`; its `$comment` is the spec, its
//! vectors the proof). `ev1.<kid>.<iv>.<ciphertext‖tag>`, AES-256-GCM, the
//! AAD binding the value to its row and the key.

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use rand::RngCore;

use super::fixture;

pub const VERSION: &str = "ev1";
const KEY_BYTES: usize = 32;
const IV_BYTES: usize = 12;
const TAG_BYTES: usize = 16;

/// One key: its id and the 32 key bytes (never printed).
#[derive(Clone, PartialEq, Eq)]
pub struct VaultKey {
    pub kid: String,
    key: [u8; KEY_BYTES],
}

impl std::fmt::Debug for VaultKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VaultKey({}, <redacted>)", self.kid)
    }
}

/// The current key and the retired ones (open only).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keyring {
    pub current: VaultKey,
    pub retired: Vec<VaultKey>,
}

/// Where a value lives: its AAD names the row and column.
#[derive(Clone, Copy, Debug)]
pub struct Slot<'a> {
    pub table: &'a str,
    pub column: &'a str,
    pub row_id: &'a str,
}

/// `kidPattern`: `^[a-z0-9]{1,16}$`.
fn kid_ok(kid: &str) -> bool {
    (1..=16).contains(&kid.len()) && kid.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

fn standard_base64(text: &str) -> bool {
    let body = text.trim_end_matches('=');
    !body.is_empty()
        && text.len() - body.len() <= 2
        && body.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
}

/// `<kid>:<base64 of 32 bytes>` → the key; `None` for anything else.
pub fn parse_key(spec: Option<&str>) -> Option<VaultKey> {
    let spec = spec?;
    let (kid, text) = spec.split_once(':')?;
    let text = text.trim();
    if !kid_ok(kid) || !standard_base64(text) {
        return None;
    }
    let bytes = STANDARD.decode(text).ok()?;
    let key: [u8; KEY_BYTES] = bytes.try_into().ok()?;
    Some(VaultKey { kid: kid.to_string(), key })
}

/// `VAULT_KEY` + `VAULT_KEYS_RETIRED` → the keyring; `None` when the
/// current key is missing or malformed, a retired entry is malformed, or a
/// kid repeats.
pub fn parse_keyring(current: Option<&str>, retired: Option<&str>) -> Option<Keyring> {
    let current = parse_key(current)?;
    let mut old = Vec::new();
    for part in retired.unwrap_or_default().split(',').map(str::trim).filter(|part| !part.is_empty()) {
        old.push(parse_key(Some(part))?);
    }
    let mut kids: Vec<&str> = std::iter::once(current.kid.as_str()).chain(old.iter().map(|k| k.kid.as_str())).collect();
    let count = kids.len();
    kids.sort_unstable();
    kids.dedup();
    (kids.len() == count).then_some(Keyring { current, retired: old })
}

/// `ev1|<kid>|<table>.<column>|<row id>`.
pub fn aad(kid: &str, slot: Slot<'_>) -> String {
    format!("{VERSION}|{kid}|{}.{}|{}", slot.table, slot.column, slot.row_id)
}

/// Seal with an explicit iv (the vectors); [`seal`] draws one.
pub fn seal_with(key: &VaultKey, iv: &[u8; IV_BYTES], plaintext: &str, slot: Slot<'_>) -> String {
    let cipher = Aes256Gcm::new_from_slice(&key.key).expect("32-byte key");
    let aad = aad(&key.kid, slot);
    let body = cipher
        .encrypt(Nonce::from_slice(iv), Payload { msg: plaintext.as_bytes(), aad: aad.as_bytes() })
        .expect("AES-GCM seals any length this side of 64 GiB");
    format!("{VERSION}.{}.{}.{}", key.kid, URL_SAFE_NO_PAD.encode(iv), URL_SAFE_NO_PAD.encode(body))
}

pub fn seal(keyring: &Keyring, plaintext: &str, slot: Slot<'_>) -> String {
    let mut iv = [0u8; IV_BYTES];
    rand::thread_rng().fill_bytes(&mut iv);
    seal_with(&keyring.current, &iv, plaintext, slot)
}

/// The kid a value was sealed with; `None` for anything not a vault value.
pub fn kid_of(value: &str) -> Option<&str> {
    let parts: Vec<&str> = value.split('.').collect();
    (parts.len() == 4 && parts[0] == VERSION && kid_ok(parts[1])).then(|| parts[1])
}

/// The plaintext, or `None` for anything that does not open (another
/// version, an unknown kid, another slot, a tampered or malformed value).
pub fn open(keyring: &Keyring, value: &str, slot: Slot<'_>) -> Option<String> {
    let kid = kid_of(value)?;
    let key = std::iter::once(&keyring.current).chain(keyring.retired.iter()).find(|key| key.kid == kid)?;
    let mut parts = value.split('.').skip(2);
    let iv = URL_SAFE_NO_PAD.decode(parts.next()?).ok()?;
    let body = URL_SAFE_NO_PAD.decode(parts.next()?).ok()?;
    if iv.len() != IV_BYTES || body.len() < TAG_BYTES {
        return None;
    }
    let cipher = Aes256Gcm::new_from_slice(&key.key).ok()?;
    let aad = aad(kid, slot);
    let plain = cipher.decrypt(Nonce::from_slice(&iv), Payload { msg: &body, aad: aad.as_bytes() }).ok()?;
    String::from_utf8(plain).ok()
}

/// A value sealed with a retired key: the rotation sweep re-seals it.
pub fn needs_rotation(keyring: &Keyring, value: &str) -> bool {
    kid_of(value).is_some_and(|kid| kid != keyring.current.kid)
}

/// The fixture's sizes, checked against this module's by the replay.
pub fn fixture_constants() -> (usize, usize, usize) {
    let f = fixture::vault();
    let n = |key: &str| f[key].as_u64().unwrap_or_default() as usize;
    assert_eq!((n("keyBytes"), n("ivBytes"), n("tagBytes")), (KEY_BYTES, IV_BYTES, TAG_BYTES), "vault.json sizes");
    (KEY_BYTES, IV_BYTES, TAG_BYTES)
}
