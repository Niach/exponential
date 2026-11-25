//! Which installs an install TRUSTS, and what it does with a key the server
//! hands it (#EXP-1950; the spec is
//! `packages/domain-contract/fixtures/install-trust.json`, its TS reference
//! `packages/domain-contract/src/install-trust.ts`; the wrap signature it
//! builds on is [`crate::wrap_sig`]).
//!
//! - THE CODE an install shows while it waits for approval, and the row of a
//!   trusted install shows beside it: [`install_code`].
//! - THE APPROVALS, signed by an install's eNet identity: [`approval_message`].
//! - THE PIN STORE of one install (per account, beside its tree keys) and how
//!   it follows the server's answers: [`reconcile_trust`]. The server only
//!   ever SHRINKS the pins; it grows them only with a signature this install
//!   checks itself.
//! - THE FOUR RULES, record-only in release 1 ([`TRUST_MODE`]): [`keep_wrap`]
//!   (R1), [`seal_epoch_check`] (R2), [`seal_to`] (R3), [`wrap_target`] (R4)
//!   and [`call_key`]. Each answers what a build does NOW, and what it does
//!   once signing is mandatory (decision d).
//!
//! Pure: no I/O. The desktop app's and the daemon's holder
//! (`steer::install_trust`) keep the store in a 0600 file per account.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use data_encoding::HEXLOWER;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::identity::{verify_endpoint_signature, Identity};
use crate::push::PushKeypair;
use crate::sealed::{b64url_decode, b64url_encode, TreeKey};
use crate::wrap_sig::{accept_vouch, accept_wrap, ServedWrap, WrapRefusal, WrapStatement, WrapVerdict};

/// The approval rows' and the waiting state's copy (`copy`), the same ×4.
pub use crate::install_trust_copy as copy;
pub use crate::key_maker::{
    is_key_maker_refusal, key_maker_step, key_maker_waits, stranded_rotation_epoch, KeyMakerStep, KeyWrite, KEY_MAKER_REFUSAL,
    KEY_MAKER_REFUSAL_CODE,
};

/// The waiting state's glyph (`icon`).
pub const WAITING_ICON: &str = "ui-private";

// ── the code ──────────────────────────────────────────────────────────────

pub const INSTALL_CODE_LABEL: &str = "exp/enet/install-code/1";
const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// The 32 bytes of an endpoint id (64 lowercase hex only).
fn endpoint_bytes(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    HEXLOWER.decode(text.as_bytes()).ok()?.try_into().ok()
}

/// The 10 characters (two groups of five) an install shows to be approved:
/// the first 50 bits of SHA-256("exp/enet/install-code/1" ‖ 0x00 ‖
/// endpointId[32]) in Crockford base32. Both ends derive it from the key;
/// never read from the server. None for a malformed endpoint id.
pub fn install_code(endpoint_id: &str) -> Option<String> {
    let endpoint = endpoint_bytes(endpoint_id)?;
    let mut hasher = Sha256::new();
    hasher.update(INSTALL_CODE_LABEL.as_bytes());
    hasher.update([0u8]);
    hasher.update(endpoint);
    let digest = hasher.finalize();
    let mut bits: u64 = 0;
    for byte in &digest[..7] {
        bits = (bits << 8) | u64::from(*byte);
    }
    bits >>= 6; // 56 bits read, the first 50 kept
    let out: String = (0..10).rev().map(|i| CROCKFORD[((bits >> (5 * i)) & 31) as usize] as char).collect();
    Some(format!("{} {}", &out[..5], &out[5..]))
}

// ── identities, registrations, approvals ──────────────────────────────────

/// An install's X25519 public key (base64url) from its identity bytes.
pub fn install_x25519_of(identity: &[u8; 32]) -> String {
    b64url_encode(&PushKeypair::derive_from_identity_bytes(identity).public())
}

/// `installs.register`'s message (install-register.json).
pub fn registration_message(user_id: &str, endpoint_id: &str, x25519_public: &str, signed_at: &str) -> String {
    format!("exp-install-v1\n{user_id}\n{endpoint_id}\n{x25519_public}\n{signed_at}")
}

/// The identity's registration signature (base64url).
pub fn sign_registration(identity: &Identity, user_id: &str, signed_at: &str) -> String {
    let endpoint_id = HEXLOWER.encode(identity.endpoint_id().as_bytes());
    let x25519 = install_x25519_of(&identity.to_bytes());
    b64url_encode(&identity.sign(registration_message(user_id, &endpoint_id, &x25519, signed_at).as_bytes()))
}

pub const APPROVAL_LABEL: &str = "exp/enet/install-approval/1";
pub const APPROVAL_MESSAGE_BYTES: usize = 125;

/// What an approval says. `Root` = an install starts its account's chain
/// (signed by itself: the first install, or a reset); `Approve` = a trusted
/// install of the SAME account approves it; `Reset` = a team-mate's trusted
/// install confirms a member's new chain; `Decline` = a team-mate's install
/// turns that reset down (the row's Remove).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalKind {
    Root,
    Approve,
    Reset,
    Decline,
}

impl ApprovalKind {
    /// The message's kind byte (`approval.kinds`).
    pub fn byte(self) -> u8 {
        match self {
            ApprovalKind::Root => 1,
            ApprovalKind::Approve => 2,
            ApprovalKind::Reset => 3,
            ApprovalKind::Decline => 4,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            ApprovalKind::Root => "root",
            ApprovalKind::Approve => "approve",
            ApprovalKind::Reset => "reset",
            ApprovalKind::Decline => "decline",
        }
    }
}

/// The install an approval is about, as its registration binds it.
#[derive(Debug, Clone, Copy)]
pub struct ApprovalSubject<'a> {
    pub user_id: &'a str,
    pub endpoint_id: &'a str,
    pub x25519_public: &'a str,
}

/// "exp/enet/install-approval/1" ‖ 0x00 ‖ kind u8 ‖ SHA-256(userId utf8)[32]
/// ‖ endpointId[32] ‖ x25519Public[32] (125 bytes); None when malformed.
pub fn approval_message(kind: ApprovalKind, subject: &ApprovalSubject) -> Option<[u8; APPROVAL_MESSAGE_BYTES]> {
    let endpoint = endpoint_bytes(subject.endpoint_id)?;
    let x: [u8; 32] = b64url_decode(subject.x25519_public)?.try_into().ok()?;
    if subject.user_id.is_empty() {
        return None;
    }
    let label = APPROVAL_LABEL.as_bytes();
    let mut out = [0u8; APPROVAL_MESSAGE_BYTES];
    out[..label.len()].copy_from_slice(label);
    out[label.len() + 1] = kind.byte();
    out[label.len() + 2..label.len() + 34].copy_from_slice(&Sha256::digest(subject.user_id.as_bytes()));
    out[label.len() + 34..label.len() + 66].copy_from_slice(&endpoint);
    out[label.len() + 66..].copy_from_slice(&x);
    Some(out)
}

/// The identity's approval (base64url, 86 chars); None for a malformed subject.
pub fn sign_approval(identity: &Identity, kind: ApprovalKind, subject: &ApprovalSubject) -> Option<String> {
    Some(b64url_encode(&identity.sign(&approval_message(kind, subject)?)))
}

/// Strict ed25519 over base64url: 64 bytes, canonical encoding.
fn verify_b64(signature: &str, message: &[u8], endpoint_id: &str) -> bool {
    let Some(bytes) = b64url_decode(signature) else { return false };
    bytes.len() == 64 && endpoint_bytes(endpoint_id).is_some() && verify_endpoint_signature(endpoint_id, message, &bytes)
}

pub fn verify_approval(kind: ApprovalKind, subject: &ApprovalSubject, signature: &str, approver_endpoint_id: &str) -> bool {
    approval_message(kind, subject).is_some_and(|message| verify_b64(signature, &message, approver_endpoint_id))
}

// ── what the server serves (`installs.trust`) ─────────────────────────────

/// A live install of the viewer's account or of a team-mate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServedInstall {
    pub install_id: String,
    pub user_id: String,
    pub endpoint_id: String,
    pub x25519_public: String,
    #[serde(default)]
    pub registration_signature: Option<String>,
    #[serde(default)]
    pub registration_signed_at: Option<String>,
    /// A build that follows the rule asked to be approved (`installs.requestApproval`).
    #[serde(default)]
    pub approval_requested: bool,
    #[serde(default)]
    pub label: Option<String>,
}

impl ServedInstall {
    fn subject(&self) -> ApprovalSubject<'_> {
        ApprovalSubject { user_id: &self.user_id, endpoint_id: &self.endpoint_id, x25519_public: &self.x25519_public }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServedApproval {
    pub kind: ApprovalKind,
    pub subject_endpoint_id: String,
    pub approver_endpoint_id: String,
    pub signature: String,
}

/// `installs.trust`'s answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustAnswer {
    #[serde(default)]
    pub installs: Vec<ServedInstall>,
    #[serde(default)]
    pub approvals: Vec<ServedApproval>,
}

/// Whether the registration binds the row's user, endpoint id and X25519 key.
pub fn registration_valid(install: &ServedInstall) -> bool {
    let (Some(signature), Some(signed_at)) = (&install.registration_signature, &install.registration_signed_at) else {
        return false;
    };
    let message = registration_message(&install.user_id, &install.endpoint_id, &install.x25519_public, signed_at);
    verify_b64(signature, message.as_bytes(), &install.endpoint_id)
}

// ── the pin store ─────────────────────────────────────────────────────────

/// Why an install is pinned (install-trust.ts `PinVia`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PinVia {
    #[serde(rename = "self")]
    Own,
    Chain,
    Approved,
    First,
    Reset,
    Existing,
}

impl PinVia {
    pub fn as_str(self) -> &'static str {
        match self {
            PinVia::Own => "self",
            PinVia::Chain => "chain",
            PinVia::Approved => "approved",
            PinVia::First => "first",
            PinVia::Reset => "reset",
            PinVia::Existing => "existing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pin {
    pub user_id: String,
    pub x25519_public: String,
    pub via: PinVia,
    /// The approver (`approved`, `reset`); None otherwise.
    #[serde(default)]
    pub by: Option<String>,
}

/// What this install recorded about a key it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyRecord {
    Verified,
    Legacy,
    Unsigned,
    RefusedWouldBe,
}

impl KeyRecord {
    pub fn as_str(self) -> &'static str {
        match self {
            KeyRecord::Verified => "verified",
            KeyRecord::Legacy => "legacy",
            KeyRecord::Unsigned => "unsigned",
            KeyRecord::RefusedWouldBe => "refused-would-be",
        }
    }
    pub fn parse(text: &str) -> Option<KeyRecord> {
        Some(match text {
            "verified" => KeyRecord::Verified,
            "legacy" => KeyRecord::Legacy,
            "unsigned" => KeyRecord::Unsigned,
            "refused-would-be" => KeyRecord::RefusedWouldBe,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootRecord {
    pub verified_epoch: u32,
    pub epochs: BTreeMap<String, KeyRecord>,
}

/// One install's pin store for one account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustStore {
    /// This install's endpoint id.
    #[serde(rename = "self")]
    pub self_endpoint_id: String,
    pub user_id: String,
    /// Placed in its account's chain (a root, approved, or existing).
    #[serde(default)]
    pub joined: bool,
    /// Decision (e): this install existed before approval shipped; consumed
    /// (`false`) by the first reconcile that serves this install.
    #[serde(default)]
    pub existing: bool,
    #[serde(default)]
    pub pins: BTreeMap<String, Pin>,
    /// Every team-mate ever pinned: a new root of one is a Reset.
    #[serde(default)]
    pub members: Vec<String>,
    /// Endpoint ids this install's account turned down (a Reset row's Remove).
    #[serde(default)]
    pub declined: Vec<String>,
    /// Per root (lowercase): the highest VERIFIED epoch and what each held epoch is.
    #[serde(default)]
    pub roots: BTreeMap<String, RootRecord>,
}

impl TrustStore {
    pub fn empty(self_endpoint_id: &str, user_id: &str, existing: bool) -> TrustStore {
        TrustStore {
            self_endpoint_id: self_endpoint_id.to_string(),
            user_id: user_id.to_string(),
            joined: false,
            existing,
            pins: BTreeMap::new(),
            members: Vec::new(),
            declined: Vec::new(),
            roots: BTreeMap::new(),
        }
    }

    /// What this install recorded about (root, epoch); None = never recorded.
    pub fn record(&self, root_id: &str, epoch: u32) -> Option<KeyRecord> {
        self.roots.get(&root_id.to_ascii_lowercase())?.epochs.get(&epoch.to_string()).copied()
    }

    /// Records a kept key (`legacy` = held before the store existed, decision c).
    pub fn record_key(&mut self, root_id: &str, epoch: u32, record: KeyRecord) {
        let root = self.roots.entry(root_id.to_ascii_lowercase()).or_default();
        if record == KeyRecord::Verified {
            root.verified_epoch = root.verified_epoch.max(epoch);
        }
        root.epochs.insert(epoch.to_string(), record);
    }

    fn verified_epoch(&self, root_id: &str) -> u32 {
        self.roots.get(&root_id.to_ascii_lowercase()).map_or(0, |root| root.verified_epoch)
    }

    fn pinned(&self, endpoint_id: &str) -> bool {
        self.pins.contains_key(endpoint_id)
    }
}

/// A row of Needs you: approve or remove an install.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RowKind {
    NewInstall,
    Reset,
}

impl RowKind {
    /// The Badge's text.
    pub fn badge(self) -> &'static str {
        match self {
            RowKind::NewInstall => copy::NEW_INSTALL,
            RowKind::Reset => copy::RESET,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            RowKind::NewInstall => "newInstall",
            RowKind::Reset => "reset",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustRow {
    pub kind: RowKind,
    pub install_id: String,
    pub endpoint_id: String,
    pub user_id: String,
    pub label: Option<String>,
    /// Derived HERE from `endpoint_id` ([`install_code`]), never served.
    pub code: String,
}

/// `Trusted` = joined; `Waiting` = shows "Waiting for approval" and its
/// code; `ClaimRoot` = its account has no chain: it signs `root` itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrustStatus {
    Trusted,
    Waiting,
    ClaimRoot,
}

impl TrustStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TrustStatus::Trusted => "trusted",
            TrustStatus::Waiting => "waiting",
            TrustStatus::ClaimRoot => "claimRoot",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustView {
    pub store: TrustStore,
    pub status: TrustStatus,
    pub rows: Vec<TrustRow>,
    /// The `existing` pins this install signs `approve` for (with its root
    /// claim, or once it is on the chain). Sorted by endpoint id.
    pub approve_existing: Vec<String>,
}

/// Follows the server's answer (`installs.trust`). The server may only
/// SHRINK the pins; a pin is only ever ADDED for a signature this install
/// verified, by an install already pinned (or, once, at joining, at an
/// existing install's first reconcile and at first contact).
pub fn reconcile_trust(before: &TrustStore, served: &TrustAnswer) -> TrustView {
    let mut store = before.clone();
    // `valid` = the registration verifies; `counted` adds rows that store
    // none (registered before #3394). Insertion order = the answer's.
    // (A Map per endpoint id, the later row wins, as the TS reference.)
    let mut valid: Vec<&ServedInstall> = Vec::new();
    let mut counted_order: Vec<&ServedInstall> = Vec::new();
    fn put<'a>(list: &mut Vec<&'a ServedInstall>, install: &'a ServedInstall) {
        match list.iter_mut().find(|row| row.endpoint_id == install.endpoint_id) {
            Some(slot) => *slot = install,
            None => list.push(install),
        }
    }
    for install in &served.installs {
        if registration_valid(install) {
            put(&mut valid, install);
        }
        if valid.iter().any(|row| row.endpoint_id == install.endpoint_id) || install.registration_signature.is_none() {
            put(&mut counted_order, install);
        }
    }
    let counted: HashMap<&str, &ServedInstall> = counted_order.iter().map(|row| (row.endpoint_id.as_str(), *row)).collect();
    let self_id = store.self_endpoint_id.clone();
    let me = counted.get(self_id.as_str()).copied();

    // Shrink: what the server no longer lists (or binds) is no longer pinned.
    let keys: Vec<String> = store.pins.keys().cloned().collect();
    for endpoint_id in keys {
        if endpoint_id == self_id {
            continue;
        }
        let pin = &store.pins[&endpoint_id];
        let keep = counted.get(endpoint_id.as_str()).is_some_and(|row| row.user_id == pin.user_id && row.x25519_public == pin.x25519_public);
        if !keep {
            store.pins.remove(&endpoint_id);
        }
    }
    if let Some(me) = me {
        store.pins.insert(
            self_id.clone(),
            Pin { user_id: store.user_id.clone(), x25519_public: me.x25519_public.clone(), via: PinVia::Own, by: None },
        );
    }

    // The approvals that verify, against the subject as its registration binds it.
    let approvals: Vec<&ServedApproval> = served
        .approvals
        .iter()
        .filter(|approval| {
            let Some(subject) = counted.get(approval.subject_endpoint_id.as_str()) else { return false };
            if approval.kind == ApprovalKind::Root && approval.approver_endpoint_id != approval.subject_endpoint_id {
                return false;
            }
            verify_approval(approval.kind, &subject.subject(), &approval.signature, &approval.approver_endpoint_id)
        })
        .collect();
    let roots: HashSet<&str> =
        approvals.iter().filter(|a| a.kind == ApprovalKind::Root).map(|a| a.subject_endpoint_id.as_str()).collect();
    let is_root = |endpoint_id: &str| roots.contains(endpoint_id);
    fn user_in<'a>(counted: &HashMap<&str, &'a ServedInstall>, endpoint_id: &str) -> Option<&'a str> {
        counted.get(endpoint_id).map(|row| row.user_id.as_str())
    }
    let user_of = |endpoint_id: &str| user_in(&counted, endpoint_id).map(str::to_string);
    let own_chain = approvals.iter().any(|a| {
        matches!(a.kind, ApprovalKind::Root | ApprovalKind::Approve) && user_of(&a.subject_endpoint_id).as_deref() == Some(store.user_id.as_str())
    });

    // Joining: its own root claim, or the chain from its account's root to
    // it, or (decision e) an existing install of an account with no chain.
    if !store.joined && me.is_some() {
        if is_root(&self_id) {
            store.joined = true;
        } else if let Some(path) = chain_to(&self_id, &store.user_id, &approvals, &is_root, &user_of) {
            for endpoint_id in path {
                if endpoint_id == self_id {
                    continue;
                }
                let row = counted[endpoint_id.as_str()];
                store.pins.insert(
                    endpoint_id,
                    Pin { user_id: row.user_id.clone(), x25519_public: row.x25519_public.clone(), via: PinVia::Chain, by: None },
                );
            }
            store.joined = true;
        } else if store.existing && !own_chain {
            for install in &counted_order {
                if install.user_id != store.user_id || install.endpoint_id == self_id || install.approval_requested {
                    continue;
                }
                store.pins.insert(
                    install.endpoint_id.clone(),
                    Pin { user_id: install.user_id.clone(), x25519_public: install.x25519_public.clone(), via: PinVia::Existing, by: None },
                );
            }
            store.joined = true;
        }
    }
    // Only the FIRST reconcile that serves this install adopts.
    if me.is_some() {
        store.existing = false;
    }

    // Declines by this account's pinned installs.
    for approval in &approvals {
        if approval.kind != ApprovalKind::Decline {
            continue;
        }
        let ours = store.pins.get(&approval.approver_endpoint_id).is_some_and(|pin| pin.user_id == store.user_id);
        if ours && !store.declined.contains(&approval.subject_endpoint_id) {
            store.declined.push(approval.subject_endpoint_id.clone());
        }
    }

    if store.joined {
        // First contact with a team-mate: their root(s) as served, once.
        for install in &counted_order {
            if install.user_id == store.user_id || store.members.contains(&install.user_id) {
                continue;
            }
            if store.pins.values().any(|pin| pin.user_id == install.user_id) {
                continue;
            }
            for root in &counted_order {
                if root.user_id == install.user_id && is_root(&root.endpoint_id) && !store.declined.contains(&root.endpoint_id) {
                    store.pins.insert(
                        root.endpoint_id.clone(),
                        Pin { user_id: root.user_id.clone(), x25519_public: root.x25519_public.clone(), via: PinVia::First, by: None },
                    );
                }
            }
        }
        // Grow by signatures of pinned installs, to a fixpoint.
        let mut changed = true;
        while changed {
            changed = false;
            for approval in &approvals {
                let subject = counted[approval.subject_endpoint_id.as_str()];
                if store.pinned(&subject.endpoint_id) || store.declined.contains(&subject.endpoint_id) {
                    continue;
                }
                let Some(approver) = store.pins.get(&approval.approver_endpoint_id) else { continue };
                let pinned = (approval.kind == ApprovalKind::Approve && approver.user_id == subject.user_id)
                    || (approval.kind == ApprovalKind::Reset
                        && approver.user_id != subject.user_id
                        && subject.user_id != store.user_id
                        && is_root(&subject.endpoint_id));
                if !pinned {
                    continue;
                }
                store.pins.insert(
                    subject.endpoint_id.clone(),
                    Pin {
                        user_id: subject.user_id.clone(),
                        x25519_public: subject.x25519_public.clone(),
                        via: if approval.kind == ApprovalKind::Approve { PinVia::Approved } else { PinVia::Reset },
                        by: Some(approval.approver_endpoint_id.clone()),
                    },
                );
                changed = true;
            }
        }
        let new_members: BTreeSet<String> = store
            .pins
            .values()
            .filter(|pin| pin.user_id != store.user_id && !store.members.contains(&pin.user_id))
            .map(|pin| pin.user_id.clone())
            .collect();
        store.members.extend(new_members);
    }

    let mut rows: Vec<TrustRow> = Vec::new();
    if store.joined {
        for install in &valid {
            if store.pinned(&install.endpoint_id) || store.declined.contains(&install.endpoint_id) {
                continue;
            }
            // Own: every install nobody pinned is new (decision e), any
            // build. A team-mate's: only a new root of a member pinned before.
            let kind = if install.user_id == store.user_id {
                Some(RowKind::NewInstall)
            } else if is_root(&install.endpoint_id) && store.members.contains(&install.user_id) {
                Some(RowKind::Reset)
            } else {
                None
            };
            let (Some(kind), Some(code)) = (kind, install_code(&install.endpoint_id)) else { continue };
            rows.push(TrustRow {
                kind,
                install_id: install.install_id.clone(),
                endpoint_id: install.endpoint_id.clone(),
                user_id: install.user_id.clone(),
                label: install.label.clone(),
                code,
            });
        }
        rows.sort_by(|a, b| {
            (a.kind != RowKind::NewInstall, &a.endpoint_id).cmp(&(b.kind != RowKind::NewInstall, &b.endpoint_id))
        });
    }

    // A new install claims a root only ALONE (decision e).
    let alone = !served.installs.iter().any(|install| install.user_id == store.user_id && install.endpoint_id != self_id);
    let status = if store.joined {
        if own_chain {
            TrustStatus::Trusted
        } else {
            TrustStatus::ClaimRoot
        }
    } else if own_chain || !alone {
        TrustStatus::Waiting
    } else {
        TrustStatus::ClaimRoot
    };

    // The existing pins its account's chain does not reach yet.
    let approved_by_pin = |endpoint_id: &str| {
        approvals.iter().any(|a| a.kind == ApprovalKind::Approve && a.subject_endpoint_id == endpoint_id && store.pinned(&a.approver_endpoint_id))
    };
    let on_chain = is_root(&self_id) || approved_by_pin(&self_id);
    let approve_existing = if store.joined && (status == TrustStatus::ClaimRoot || on_chain) {
        store
            .pins
            .iter()
            .filter(|(endpoint_id, pin)| pin.via == PinVia::Existing && !is_root(endpoint_id) && !approved_by_pin(endpoint_id))
            .map(|(endpoint_id, _)| endpoint_id.clone())
            .collect()
    } else {
        Vec::new()
    };
    TrustView { store, status, rows, approve_existing }
}

/// The installs from a root of `user_id` down to `target` (`approve` only,
/// every approver of `user_id`), shortest first; None when none.
fn chain_to(
    target: &str,
    user_id: &str,
    approvals: &[&ServedApproval],
    is_root: &dyn Fn(&str) -> bool,
    user_of: &dyn Fn(&str) -> Option<String>,
) -> Option<Vec<String>> {
    let roots: BTreeSet<&str> = approvals
        .iter()
        .filter(|a| a.kind == ApprovalKind::Root && user_of(&a.subject_endpoint_id).as_deref() == Some(user_id))
        .map(|a| a.subject_endpoint_id.as_str())
        .collect();
    let mut parents: HashMap<&str, &str> = HashMap::new();
    let mut seen: HashSet<&str> = roots.iter().copied().collect();
    let mut queue: VecDeque<&str> = roots.into_iter().collect();
    while let Some(at) = queue.pop_front() {
        if at == target {
            let mut path = vec![at.to_string()];
            let mut cursor = at;
            while let Some(parent) = parents.get(cursor) {
                path.insert(0, parent.to_string());
                cursor = parent;
            }
            return Some(path);
        }
        for approval in approvals {
            if approval.kind != ApprovalKind::Approve || approval.approver_endpoint_id != at || seen.contains(approval.subject_endpoint_id.as_str()) {
                continue;
            }
            if user_of(&approval.subject_endpoint_id).as_deref() != Some(user_id) || is_root(&approval.subject_endpoint_id) {
                continue;
            }
            seen.insert(&approval.subject_endpoint_id);
            parents.insert(&approval.subject_endpoint_id, at);
            queue.push_back(&approval.subject_endpoint_id);
        }
    }
    None
}

// ── the four rules ────────────────────────────────────────────────────────

/// Release 1 = `Record`: nothing a released build makes is refused;
/// `Mandatory` = release 2 of decision (d), a BUILD constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustMode {
    Record,
    Mandatory,
}

impl TrustMode {
    pub fn as_str(self) -> &'static str {
        match self {
            TrustMode::Record => "record",
            TrustMode::Mandatory => "mandatory",
        }
    }
}

/// THE build constant (`mode.release1`). Never a server flag.
pub const TRUST_MODE: TrustMode = TrustMode::Record;

#[derive(Clone, PartialEq, Eq)]
pub enum KeepVerdict {
    Keep { record: KeyRecord, secret: [u8; 32], tree_public_key: String },
    Refuse(WrapRefusal),
}

impl std::fmt::Debug for KeepVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeepVerdict::Keep { record, tree_public_key, .. } => write!(f, "Keep({}, {tree_public_key})", record.as_str()),
            KeepVerdict::Refuse(reason) => write!(f, "Refuse({})", reason.as_str()),
        }
    }
}

/// R1: whether this install keeps a served wrap (of ANY epoch). A signature
/// that fails, a malformed wrap and one that does not open are refused in
/// every mode. `Record` keeps an unsigned wrap as `unsigned` and one signed
/// by an UNPINNED install that verifies as `refused-would-be`; `Mandatory`
/// refuses both.
pub fn keep_wrap(wrap: &ServedWrap, identity: &Identity, install_secret: &[u8; 32], store: &TrustStore, mode: TrustMode) -> KeepVerdict {
    let pinned = |id: &str| store.pinned(id);
    match accept_wrap(wrap, identity, install_secret, &pinned, mode == TrustMode::Mandatory) {
        WrapVerdict::Verified { secret, tree_public_key, .. } => KeepVerdict::Keep { record: KeyRecord::Verified, secret, tree_public_key },
        WrapVerdict::Unsigned { secret, tree_public_key } => KeepVerdict::Keep { record: KeyRecord::Unsigned, secret, tree_public_key },
        WrapVerdict::Refused(WrapRefusal::Unpinned) if mode == TrustMode::Record => {
            // Is it at least its signer's? Then it is kept, and recorded.
            let signer = wrap.signer_endpoint_id.unwrap_or_default().to_string();
            let own = |id: &str| id == signer;
            match accept_wrap(wrap, identity, install_secret, &own, false) {
                WrapVerdict::Verified { secret, tree_public_key, .. } => {
                    KeepVerdict::Keep { record: KeyRecord::RefusedWouldBe, secret, tree_public_key }
                }
                WrapVerdict::Refused(reason) => KeepVerdict::Refuse(reason),
                WrapVerdict::Unsigned { .. } => KeepVerdict::Refuse(WrapRefusal::Signature),
            }
        }
        WrapVerdict::Refused(reason) => KeepVerdict::Refuse(reason),
    }
}

/// R2: `false` (= `rollback`, every mode) when this install verified a
/// higher epoch of `root_id` than the one a server answer names.
pub fn seal_epoch_check(store: &TrustStore, root_id: &str, epoch: u32) -> bool {
    epoch >= store.verified_epoch(root_id)
}

/// Why R3 refuses a seal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SealRefusal {
    Wrap(WrapRefusal),
    Rollback,
    KeyMismatch,
}

impl SealRefusal {
    pub fn as_str(self) -> &'static str {
        match self {
            SealRefusal::Wrap(reason) => reason.as_str(),
            SealRefusal::Rollback => "rollback",
            SealRefusal::KeyMismatch => "keyMismatch",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SealVerdict {
    Seal(KeyRecord),
    Refuse(SealRefusal),
}

/// A vouch: a signed wrap of (root, epoch) made for another install, which
/// names the key (`publicKeys` / `sendTarget`'s `vouch`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vouch {
    pub recipient_endpoint_id: String,
    pub wrapped: String,
    pub signature: String,
    pub signer_endpoint_id: String,
}

/// R3: whether this install seals to (root, epoch, public key) as the
/// server names it. With a held secret of that epoch: only to ITS public key.
/// Without: a vouch by a pinned install = `verified`; none = `unsigned` and
/// one by an unpinned install that verifies = `refused-would-be` (sealed in
/// `Record`, refused once `Mandatory`); a vouch that fails is refused in
/// every mode.
pub fn seal_to(
    root_id: &str,
    epoch: u32,
    public_key: &str,
    held: Option<([u8; 32], KeyRecord)>,
    vouch: Option<&Vouch>,
    store: &TrustStore,
    mode: TrustMode,
) -> SealVerdict {
    if !seal_epoch_check(store, root_id, epoch) {
        return SealVerdict::Refuse(SealRefusal::Rollback);
    }
    if let Some((secret, record)) = held {
        if TreeKey::from_secret(secret).public_key() != public_key {
            return SealVerdict::Refuse(SealRefusal::KeyMismatch);
        }
        return match (mode, record) {
            (TrustMode::Mandatory, KeyRecord::Unsigned) => SealVerdict::Refuse(SealRefusal::Wrap(WrapRefusal::Unsigned)),
            (TrustMode::Mandatory, KeyRecord::RefusedWouldBe) => SealVerdict::Refuse(SealRefusal::Wrap(WrapRefusal::Unpinned)),
            _ => SealVerdict::Seal(record),
        };
    }
    let Some(vouch) = vouch else {
        return match mode {
            TrustMode::Record => SealVerdict::Seal(KeyRecord::Unsigned),
            TrustMode::Mandatory => SealVerdict::Refuse(SealRefusal::Wrap(WrapRefusal::Unsigned)),
        };
    };
    let statement = WrapStatement {
        root_id,
        epoch,
        tree_public_key: public_key,
        recipient_endpoint_id: &vouch.recipient_endpoint_id,
        wrapped: &vouch.wrapped,
    };
    let pinned = |id: &str| store.pinned(id);
    match accept_vouch(&statement, Some(&vouch.signature), Some(&vouch.signer_endpoint_id), &pinned) {
        Ok(()) => SealVerdict::Seal(KeyRecord::Verified),
        Err(WrapRefusal::Unsigned) => match mode {
            TrustMode::Record => SealVerdict::Seal(KeyRecord::Unsigned),
            TrustMode::Mandatory => SealVerdict::Refuse(SealRefusal::Wrap(WrapRefusal::Unsigned)),
        },
        Err(WrapRefusal::Unpinned) => {
            let signer = vouch.signer_endpoint_id.clone();
            let own = |id: &str| id == signer;
            if let Err(reason) = accept_vouch(&statement, Some(&vouch.signature), Some(&vouch.signer_endpoint_id), &own) {
                return SealVerdict::Refuse(SealRefusal::Wrap(reason));
            }
            match mode {
                TrustMode::Record => SealVerdict::Seal(KeyRecord::RefusedWouldBe),
                TrustMode::Mandatory => SealVerdict::Refuse(SealRefusal::Wrap(WrapRefusal::Unpinned)),
            }
        }
        Err(reason) => SealVerdict::Refuse(SealRefusal::Wrap(reason)),
    }
}

/// Why R4 refuses a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetRefusal {
    Unapproved,
    KeyMismatch,
    Registration,
    NotTrusted,
}

impl TargetRefusal {
    pub fn as_str(self) -> &'static str {
        match self {
            TargetRefusal::Unapproved => "unapproved",
            TargetRefusal::KeyMismatch => "keyMismatch",
            TargetRefusal::Registration => "registration",
            TargetRefusal::NotTrusted => "notTrusted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetVerdict {
    /// `Verified` or `Legacy`.
    Wrap(KeyRecord),
    Refuse(TargetRefusal),
}

/// R4: whether this install wraps a key for a target the server names
/// (`pending`, a new key's readers, a rotation, a new root's seal). Only a
/// joined install wraps for others. `roots` = the targets that claim a
/// verified root.
pub fn wrap_target(target: &ServedInstall, store: &TrustStore, mode: TrustMode, roots: &HashSet<String>) -> TargetVerdict {
    if target.endpoint_id == store.self_endpoint_id {
        return TargetVerdict::Wrap(KeyRecord::Verified);
    }
    if !store.joined {
        return TargetVerdict::Refuse(TargetRefusal::NotTrusted);
    }
    if let Some(pin) = store.pins.get(&target.endpoint_id) {
        return if pin.x25519_public == target.x25519_public && pin.user_id == target.user_id {
            TargetVerdict::Wrap(KeyRecord::Verified)
        } else {
            TargetVerdict::Refuse(TargetRefusal::KeyMismatch)
        };
    }
    // An older build that registered before #EXP-1950 kept no signature: legacy too.
    if target.registration_signature.is_some() && !registration_valid(target) {
        return TargetVerdict::Refuse(TargetRefusal::Registration);
    }
    // Decision (e): every own install that existed is pinned; one nobody
    // pinned is a New install row, any build.
    if target.user_id == store.user_id || target.approval_requested || roots.contains(&target.endpoint_id) {
        return TargetVerdict::Refuse(TargetRefusal::Unapproved);
    }
    match mode {
        TrustMode::Record => TargetVerdict::Wrap(KeyRecord::Legacy),
        TrustMode::Mandatory => TargetVerdict::Refuse(TargetRefusal::Unapproved),
    }
}

/// Why a call has no key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoKey {
    NoKey,
    Rollback,
}

impl NoKey {
    pub fn as_str(self) -> &'static str {
        match self {
            NoKey::NoKey => "noKey",
            NoKey::Rollback => "rollback",
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub enum CallKeyVerdict {
    Key { secret: [u8; 32], record: KeyRecord },
    NoKey(NoKey),
}

impl std::fmt::Debug for CallKeyVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallKeyVerdict::Key { record, .. } => write!(f, "Key({})", record.as_str()),
            CallKeyVerdict::NoKey(reason) => write!(f, "NoKey({})", reason.as_str()),
        }
    }
}

/// A call's key (attack 4): the secret of the (root, epoch) `sendTarget`
/// names, from this install's KEYRING only. `Record` uses any held key;
/// `Mandatory` only `verified` and `legacy` ones; else the call ends `no_key`.
pub fn call_key(root_id: &str, epoch: u32, held: Option<([u8; 32], KeyRecord)>, store: &TrustStore, mode: TrustMode) -> CallKeyVerdict {
    if !seal_epoch_check(store, root_id, epoch) {
        return CallKeyVerdict::NoKey(NoKey::Rollback);
    }
    let Some((secret, record)) = held else { return CallKeyVerdict::NoKey(NoKey::NoKey) };
    if mode == TrustMode::Mandatory && !matches!(record, KeyRecord::Verified | KeyRecord::Legacy) {
        return CallKeyVerdict::NoKey(NoKey::NoKey);
    }
    CallKeyVerdict::Key { secret, record }
}

#[cfg(test)]
#[path = "install_trust_tests.rs"]
mod tests;
