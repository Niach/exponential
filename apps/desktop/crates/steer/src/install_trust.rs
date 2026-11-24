//! #EXP-1950 (`packages/domain-contract/fixtures/install-trust.json`): this
//! install's side of install approval and the four rules, per account, around
//! the pure `enet_core::install_trust`. The desktop app and the daemon are two
//! installs: two identities, two pin stores (`install-pins` /
//! `cli-install-pins`, a 0600 file beside the keyring `seal-keys` /
//! `cli-seal-keys`).
//!
//! - It follows `installs.trust` into the pin store ([`TrustKeeper::sync`],
//!   every holder pass): claims its account's root when there is no chain
//!   (with the approvals of the installs that existed with it, decision e),
//!   3 rounds (a CONFLICT = another install claimed first: the next round
//!   finds it on that chain), asks ONCE to be approved while it waits.
//! - It APPROVES or REMOVES an install only on a person's press
//!   ([`TrustKeeper::approve`], [`TrustKeeper::remove`]); by itself only the
//!   installs that existed with it, once (`approveExisting`).
//! - It answers for the key holder whether a wrap is kept (R1), a key sealed
//!   to (R2, R3), a target wrapped for (R4) and a call keyed. Release 1 is
//!   RECORD-ONLY ([`TRUST_MODE`], a build constant, never a server flag): it
//!   refuses only what the fixture refuses in every mode (a failing
//!   signature, a rollback, a swapped key on a pinned endpoint, an unapproved
//!   own install as a target, a waiting install wrapping for others).
//! - A server without `installs.trust` (NOT_FOUND) = `Unavailable`: every
//!   target is wrapped as before and recorded `legacy`.
//!
//! EXISTING (decision e) is decided here alone: no store file next to an
//! identity an earlier build had made = `existing`. A build that mints the
//! identity writes the store first ([`mark_new`]), before it registers. A
//! store that cannot be read is never taken for a missing one: the keeper
//! then neither syncs nor writes.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use api::error::ApiError;
use api::installs::RootClaimApproval;
pub use enet_core::install_trust::{
    copy, install_code, KeyRecord, RowKind, TrustRow, TrustStore, TrustStatus, TRUST_MODE, WAITING_ICON,
};
use enet_core::install_trust::{
    call_key, keep_wrap, reconcile_trust, seal_to, sign_approval, wrap_target, ApprovalKind, ApprovalSubject, CallKeyVerdict,
    KeepVerdict, PinVia, SealVerdict, ServedInstall, TargetVerdict, TrustAnswer, TrustMode, Vouch,
};
use enet_core::wrap_sig::{endpoint_id_hex, ServedWrap};
use enet_core::Identity;

/// The `installs.*` calls the keeper makes: tRPC in the apps, a double in
/// the tests. BLOCKING.
pub trait TrustApi: Send + Sync + 'static {
    fn trust(&self, install_id: &str) -> Result<TrustAnswer, ApiError>;
    fn request_approval(&self, install_id: &str) -> Result<(), ApiError>;
    fn claim_root(&self, install_id: &str, signature: &str, approvals: &[RootClaimApproval]) -> Result<(), ApiError>;
    fn approve(&self, install_id: &str, subject_install_id: &str, kind: ApprovalKind, signature: &str) -> Result<(), ApiError>;
    fn revoke(&self, install_id: &str) -> Result<(), ApiError>;
}

impl TrustApi for api::TrpcClient {
    fn trust(&self, install_id: &str) -> Result<TrustAnswer, ApiError> {
        api::installs::trust(self, install_id)
    }
    fn request_approval(&self, install_id: &str) -> Result<(), ApiError> {
        api::installs::request_approval(self, install_id)
    }
    fn claim_root(&self, install_id: &str, signature: &str, approvals: &[RootClaimApproval]) -> Result<(), ApiError> {
        api::installs::claim_root(self, install_id, signature, approvals)
    }
    fn approve(&self, install_id: &str, subject_install_id: &str, kind: ApprovalKind, signature: &str) -> Result<(), ApiError> {
        api::installs::approve(self, install_id, subject_install_id, kind.as_str(), signature)
    }
    fn revoke(&self, install_id: &str) -> Result<(), ApiError> {
        api::installs::revoke(self, install_id)
    }
}

/// The pin store's file, beside the keyring.
pub fn store_path(keyring_path: &Path) -> PathBuf {
    let name = keyring_path.file_name().and_then(|n| n.to_str()).unwrap_or("seal-keys");
    keyring_path.with_file_name(if name.starts_with("cli-") { "cli-install-pins" } else { "install-pins" })
}

/// `Trusted` / `Waiting` / `ClaimRoot` ([`TrustStatus`]), `Unknown` before
/// the first answer, `Unavailable` on a server without `installs.trust`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustState {
    Unknown,
    Unavailable,
    Trusted,
    Waiting,
    ClaimRoot,
}

impl From<TrustStatus> for TrustState {
    fn from(status: TrustStatus) -> TrustState {
        match status {
            TrustStatus::Trusted => TrustState::Trusted,
            TrustStatus::Waiting => TrustState::Waiting,
            TrustStatus::ClaimRoot => TrustState::ClaimRoot,
        }
    }
}

/// What the UI draws: the rows (Needs you) and this install's own waiting
/// state with its code (derived here from its own endpoint id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustSnapshot {
    pub state: TrustState,
    pub rows: Vec<TrustRow>,
    pub code: Option<String>,
}

impl TrustSnapshot {
    /// Waiting for approval: the waiting EmptyState instead of the rows.
    pub fn waiting(&self) -> bool {
        self.state == TrustState::Waiting && self.code.is_some()
    }
}

/// One recorded decision (the newest last, bounded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustDecision {
    /// `R1` | `R3` | `R4` | `call`.
    pub rule: &'static str,
    pub subject: String,
    pub verdict: String,
}

const DECISION_LOG: usize = 200;
const ROUNDS: usize = 3;

struct State {
    store: TrustStore,
    /// The kept store could not be read: never synced, never written.
    unreadable: bool,
    trust: TrustState,
    rows: Vec<TrustRow>,
    served: HashMap<String, ServedInstall>,
    roots: HashSet<String>,
    asked: bool,
    decisions: VecDeque<TrustDecision>,
}

/// One install's pin store and its rules, for one account. Cheap to share.
pub struct TrustKeeper {
    identity: Identity,
    endpoint_id: String,
    install_id: String,
    path: PathBuf,
    api: Arc<dyn TrustApi>,
    mode: TrustMode,
    state: Mutex<State>,
    generation: AtomicU64,
    /// Serializes sync rounds (a press and a holder pass).
    syncing: Mutex<()>,
}

/// Reads the account's store at `path`. `Ok(None)` = no store (decision e:
/// an existing install); `Err` = it exists and cannot be read.
fn read_store(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// Sibling + rename, 0600, the directory 0700 (the eNet identity posture).
fn write_store(path: &Path, store: &TrustStore) -> std::io::Result<()> {
    use std::io::Write as _;
    let text = serde_json::to_string(store).map_err(std::io::Error::other)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

/// Decision (e): writes the account's empty store (`existing` false) BEFORE
/// this install registers: an identity this build minted (a new install, or
/// a newly signed-in account, whose identity is per account). A store this
/// endpoint already keeps stays; one that cannot be read is left alone.
pub fn mark_new(path: &Path, identity: &Identity, user_id: &str) -> std::io::Result<()> {
    let endpoint = endpoint_id_hex(identity);
    if let Some(text) = read_store(path)? {
        if serde_json::from_str::<TrustStore>(&text).is_ok_and(|store| store.self_endpoint_id == endpoint) {
            return Ok(());
        }
    }
    write_store(path, &TrustStore::empty(&endpoint, user_id, false))
}

impl TrustKeeper {
    /// The keeper of the store at `path` (its own endpoint's; one kept for
    /// another endpoint or another user, or that does not decode, is a new
    /// install's). No file = decision (e): this install EXISTED.
    pub fn open(path: PathBuf, identity: &Identity, user_id: &str, install_id: String, api: Arc<dyn TrustApi>, mode: TrustMode) -> TrustKeeper {
        let endpoint_id = endpoint_id_hex(identity);
        let (store, unreadable) = match read_store(&path) {
            Ok(None) => (TrustStore::empty(&endpoint_id, user_id, true), false),
            Ok(Some(text)) => match serde_json::from_str::<TrustStore>(&text) {
                Ok(kept) if kept.self_endpoint_id == endpoint_id && (kept.user_id == user_id || kept.user_id.is_empty()) => {
                    let mut kept = kept;
                    kept.user_id = user_id.to_string();
                    (kept, false)
                }
                _ => (TrustStore::empty(&endpoint_id, user_id, false), false),
            },
            Err(err) => {
                log::error!("install-trust: the pin store {} could not be read: {err}", path.display());
                (TrustStore::empty(&endpoint_id, user_id, false), true)
            }
        };
        TrustKeeper {
            identity: identity.clone(),
            endpoint_id,
            install_id,
            path,
            api,
            mode,
            state: Mutex::new(State {
                store,
                unreadable,
                trust: TrustState::Unknown,
                rows: Vec::new(),
                served: HashMap::new(),
                roots: HashSet::new(),
                asked: false,
                decisions: VecDeque::new(),
            }),
            generation: AtomicU64::new(0),
            syncing: Mutex::new(()),
        }
    }

    pub fn install_id(&self) -> &str {
        &self.install_id
    }

    pub fn mode(&self) -> TrustMode {
        self.mode
    }

    /// Bumps on every change of the state or the rows (the UI polls it).
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn snapshot(&self) -> TrustSnapshot {
        let state = lock(&self.state);
        TrustSnapshot { state: state.trust, rows: state.rows.clone(), code: install_code(&self.endpoint_id) }
    }

    /// The pin store (tests, diagnostics).
    pub fn store(&self) -> TrustStore {
        lock(&self.state).store.clone()
    }

    pub fn unreadable(&self) -> bool {
        lock(&self.state).unreadable
    }

    pub fn decisions(&self) -> Vec<TrustDecision> {
        lock(&self.state).decisions.iter().cloned().collect()
    }

    fn persist(&self, state: &State) {
        if state.unreadable {
            return;
        }
        if let Err(err) = write_store(&self.path, &state.store) {
            log::warn!("install-trust: writing the pin store failed: {err}");
        }
    }

    fn set(&self, trust: TrustState, rows: Option<Vec<TrustRow>>) {
        let changed = {
            let mut state = lock(&self.state);
            let changed = state.trust != trust || rows.as_ref().is_some_and(|rows| *rows != state.rows);
            state.trust = trust;
            if let Some(rows) = rows {
                state.rows = rows;
            }
            changed
        };
        if !changed {
            return;
        }
        self.generation.fetch_add(1, Ordering::SeqCst);
        let code = install_code(&self.endpoint_id).unwrap_or_default();
        if trust == TrustState::Waiting {
            // Where the daemon reports its state: the log and its doctor row.
            log::info!("install-trust: {} · {code}", copy::WAITING);
            coding::sealing::set_install_waiting(Some(code));
        } else {
            coding::sealing::set_install_waiting(None);
        }
    }

    // ── following the server ─────────────────────────────────────────────

    /// Follows `installs.trust` once: claims the root when the account has
    /// no chain (with the existing installs' approvals), approves the
    /// existing ones its chain does not reach yet, asks once while it
    /// waits. BLOCKING.
    pub fn sync(&self) {
        let _syncing = lock(&self.syncing);
        if lock(&self.state).unreadable {
            return;
        }
        for _ in 0..ROUNDS {
            let answer = match self.api.trust(&self.install_id) {
                Ok(answer) => answer,
                Err(ApiError::Http { status: 404, .. }) => {
                    self.set(TrustState::Unavailable, Some(Vec::new()));
                    return;
                }
                Err(err) => {
                    log::debug!("install-trust: installs.trust failed: {err}");
                    return;
                }
            };
            let (status, me, approve_existing) = {
                let mut state = lock(&self.state);
                let view = reconcile_trust(&state.store, &answer);
                state.store = view.store;
                state.served = answer.installs.iter().map(|i| (i.endpoint_id.clone(), i.clone())).collect();
                state.roots = answer
                    .approvals
                    .iter()
                    .filter(|a| a.kind == ApprovalKind::Root && a.approver_endpoint_id == a.subject_endpoint_id)
                    .map(|a| a.subject_endpoint_id.clone())
                    .collect();
                self.persist(&state);
                let me = state.served.get(&self.endpoint_id).cloned();
                let rows = view.rows;
                let status = view.status;
                drop(state);
                self.set(status.into(), Some(rows));
                (status, me, view.approve_existing)
            };
            match status {
                TrustStatus::ClaimRoot => {
                    let Some(me) = me else { return };
                    let Some(signature) = sign_approval(&self.identity, ApprovalKind::Root, &subject_of(&me)) else { return };
                    let approvals = self.existing_approvals(&approve_existing);
                    match self.api.claim_root(&self.install_id, &signature, &approvals) {
                        Ok(()) => log::info!("install-trust: claimed the root of this account's install chain"),
                        // A CONFLICT (another install claimed first) is read next round.
                        Err(err) => log::debug!("install-trust: installs.claimRoot: {err}"),
                    }
                    continue;
                }
                TrustStatus::Trusted => {
                    for approval in self.existing_approvals(&approve_existing) {
                        if let Err(err) = self.api.approve(&self.install_id, &approval.subject_install_id, ApprovalKind::Approve, &approval.signature) {
                            log::debug!("install-trust: approving an existing install failed: {err}");
                        }
                    }
                }
                TrustStatus::Waiting => {
                    let asked = lock(&self.state).asked;
                    if !asked && !me.is_some_and(|me| me.approval_requested) {
                        let ok = self.api.request_approval(&self.install_id).is_ok();
                        lock(&self.state).asked = ok;
                    }
                }
            }
            return;
        }
    }

    /// The `approve`s of the installs that existed with this one (decision
    /// e), over the key it pinned them with.
    fn existing_approvals(&self, endpoint_ids: &[String]) -> Vec<RootClaimApproval> {
        let state = lock(&self.state);
        endpoint_ids
            .iter()
            .filter_map(|endpoint_id| {
                let row = state.served.get(endpoint_id)?;
                let pin = state.store.pins.get(endpoint_id).filter(|pin| pin.via == PinVia::Existing)?;
                let subject = ApprovalSubject { user_id: &pin.user_id, endpoint_id, x25519_public: &pin.x25519_public };
                let signature = sign_approval(&self.identity, ApprovalKind::Approve, &subject)?;
                Some(RootClaimApproval { subject_install_id: row.install_id.clone(), signature })
            })
            .collect()
    }

    /// The served row a press names: a row THIS install yields (never one
    /// the UI was handed from elsewhere), with its served install.
    fn pressed(&self, row: &TrustRow) -> Result<ServedInstall, String> {
        let state = lock(&self.state);
        if state.trust != TrustState::Trusted {
            return Err("This install is not trusted yet".into());
        }
        let ours = state.rows.iter().any(|r| r.install_id == row.install_id && r.endpoint_id == row.endpoint_id && r.kind == row.kind);
        match state.served.get(&row.endpoint_id) {
            Some(subject) if ours && subject.install_id == row.install_id => Ok(subject.clone()),
            _ => Err("This install is not waiting for approval any more".into()),
        }
    }

    /// A person pressed Approve on a row. BLOCKING.
    pub fn approve(&self, row: &TrustRow) -> Result<(), String> {
        let subject = self.pressed(row)?;
        let kind = if row.kind == RowKind::Reset { ApprovalKind::Reset } else { ApprovalKind::Approve };
        let signature = sign_approval(&self.identity, kind, &subject_of(&subject)).ok_or("malformed install")?;
        self.api.approve(&self.install_id, &row.install_id, kind, &signature).map_err(|err| err.to_string())?;
        self.sync();
        Ok(())
    }

    /// A person pressed Remove: an own install is revoked (its wraps go, its
    /// trees rotate); a team-mate's reset is declined. BLOCKING.
    pub fn remove(&self, row: &TrustRow) -> Result<(), String> {
        let subject = self.pressed(row)?;
        match row.kind {
            RowKind::NewInstall => self.api.revoke(&row.install_id).map_err(|err| err.to_string())?,
            RowKind::Reset => {
                let signature = sign_approval(&self.identity, ApprovalKind::Decline, &subject_of(&subject)).ok_or("malformed install")?;
                self.api.approve(&self.install_id, &row.install_id, ApprovalKind::Decline, &signature).map_err(|err| err.to_string())?;
            }
        }
        self.sync();
        Ok(())
    }

    // ── the rules, for the key holder ────────────────────────────────────

    fn log(&self, state: &mut State, rule: &'static str, subject: String, verdict: String) {
        log::debug!("install-trust: {rule} {subject}: {verdict}");
        state.decisions.push_back(TrustDecision { rule, subject, verdict });
        while state.decisions.len() > DECISION_LOG {
            state.decisions.pop_front();
        }
    }

    /// R1: whether a served wrap is kept; recorded in the store.
    pub fn keep(&self, wrap: &ServedWrap, install_secret: &[u8; 32]) -> KeepVerdict {
        let mut state = lock(&self.state);
        let verdict = keep_wrap(wrap, &self.identity, install_secret, &state.store, self.mode);
        let subject = format!("{}#{}", wrap.root_id, wrap.epoch);
        match &verdict {
            KeepVerdict::Keep { record, .. } => {
                self.log(&mut state, "R1", subject, record.as_str().into());
                if !state.unreadable && state.store.record(wrap.root_id, wrap.epoch) != Some(*record) {
                    state.store.record_key(wrap.root_id, wrap.epoch, *record);
                    self.persist(&state);
                }
            }
            KeepVerdict::Refuse(reason) => {
                log::warn!("install-trust: the wrap of {subject} is refused: {}", reason.as_str());
                self.log(&mut state, "R1", subject, format!("refused:{}", reason.as_str()));
            }
        }
        verdict
    }

    /// Records a key (its own new keys are `verified`: it made them).
    pub fn record(&self, root_id: &str, epoch: u32, record: KeyRecord) {
        let mut state = lock(&self.state);
        if state.unreadable || state.store.record(root_id, epoch) == Some(record) {
            return;
        }
        state.store.record_key(root_id, epoch, record);
        self.persist(&state);
    }

    /// Decision (c): every key held before the store knew it stays, `legacy`.
    pub fn record_legacy(&self, held: &[(String, u32)]) {
        let mut state = lock(&self.state);
        if state.unreadable {
            return;
        }
        let mut changed = false;
        for (root_id, epoch) in held {
            if state.store.record(root_id, *epoch).is_none() {
                state.store.record_key(root_id, *epoch, KeyRecord::Legacy);
                changed = true;
            }
        }
        if changed {
            self.persist(&state);
        }
    }

    /// What this install recorded about a key it holds (none = `legacy`).
    pub fn record_of(&self, root_id: &str, epoch: u32) -> KeyRecord {
        lock(&self.state).store.record(root_id, epoch).unwrap_or(KeyRecord::Legacy)
    }

    /// R2 + R3: whether this install seals to (root, epoch, key).
    pub fn seal(&self, root_id: &str, epoch: u32, public_key: &str, held: Option<[u8; 32]>, vouch: Option<&Vouch>) -> SealVerdict {
        let mut state = lock(&self.state);
        let held = held.map(|secret| (secret, state.store.record(root_id, epoch).unwrap_or(KeyRecord::Legacy)));
        let verdict = seal_to(root_id, epoch, public_key, held, vouch, &state.store, self.mode);
        let text = match verdict {
            SealVerdict::Seal(record) => record.as_str().to_string(),
            SealVerdict::Refuse(reason) => format!("refused:{}", reason.as_str()),
        };
        self.log(&mut state, "R3", format!("{root_id}#{epoch}"), text);
        verdict
    }

    /// R4: whether this install wraps a key for a target the server names.
    pub fn target(&self, install_id: &str, x25519_public: &str, endpoint_id: Option<&str>, user_id: Option<&str>) -> TargetVerdict {
        let mut state = lock(&self.state);
        if install_id == self.install_id {
            return TargetVerdict::Wrap(KeyRecord::Verified);
        }
        let verdict = if state.trust == TrustState::Unavailable {
            // An older server: as before, recorded `legacy`.
            TargetVerdict::Wrap(KeyRecord::Legacy)
        } else {
            let known = endpoint_id
                .and_then(|id| state.served.get(id))
                .or_else(|| state.served.values().find(|row| row.install_id == install_id))
                .cloned();
            let served = ServedInstall {
                install_id: install_id.to_string(),
                user_id: user_id.map(str::to_string).or_else(|| known.as_ref().map(|k| k.user_id.clone())).unwrap_or_default(),
                endpoint_id: endpoint_id.map(str::to_string).or_else(|| known.as_ref().map(|k| k.endpoint_id.clone())).unwrap_or_default(),
                x25519_public: x25519_public.to_string(),
                registration_signature: known.as_ref().and_then(|k| k.registration_signature.clone()),
                registration_signed_at: known.as_ref().and_then(|k| k.registration_signed_at.clone()),
                approval_requested: known.as_ref().is_some_and(|k| k.approval_requested),
                label: None,
            };
            // The registration binds the key `installs.trust` lists; a target
            // served elsewhere with another key is checked against it.
            match &known {
                Some(known) if known.x25519_public != x25519_public && !state.store.pins.contains_key(&served.endpoint_id) => {
                    TargetVerdict::Refuse(enet_core::install_trust::TargetRefusal::KeyMismatch)
                }
                // `installs.trust` answered and does not list it: no pin, no
                // registration to check, no account to place it in. Not
                // wrapped until an answer lists it (the next pass).
                None if matches!(state.trust, TrustState::Trusted | TrustState::Waiting | TrustState::ClaimRoot) => {
                    TargetVerdict::Refuse(enet_core::install_trust::TargetRefusal::Unapproved)
                }
                _ => wrap_target(&served, &state.store, self.mode, &state.roots),
            }
        };
        let text = match verdict {
            TargetVerdict::Wrap(record) => record.as_str().to_string(),
            TargetVerdict::Refuse(reason) => format!("refused:{}", reason.as_str()),
        };
        if matches!(verdict, TargetVerdict::Refuse(_)) {
            log::info!("install-trust: no wrap for install {install_id}: {text}");
        }
        self.log(&mut state, "R4", install_id.to_string(), text);
        verdict
    }

    /// A call's key: the keyring's secret of (root, epoch), by the rule.
    pub fn call(&self, root_id: &str, epoch: u32, held: Option<[u8; 32]>) -> CallKeyVerdict {
        let mut state = lock(&self.state);
        let held = held.map(|secret| (secret, state.store.record(root_id, epoch).unwrap_or(KeyRecord::Legacy)));
        let verdict = call_key(root_id, epoch, held, &state.store, self.mode);
        let text = match &verdict {
            CallKeyVerdict::Key { record, .. } => record.as_str().to_string(),
            CallKeyVerdict::NoKey(reason) => reason.as_str().to_string(),
        };
        self.log(&mut state, "call", format!("{root_id}#{epoch}"), text);
        verdict
    }

    /// Stops reporting (sign-out).
    pub fn stop(&self) {
        coding::sealing::set_install_waiting(None);
    }
}

fn subject_of(install: &ServedInstall) -> ApprovalSubject<'_> {
    ApprovalSubject { user_id: &install.user_id, endpoint_id: &install.endpoint_id, x25519_public: &install.x25519_public }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

#[cfg(test)]
#[path = "install_trust_tests.rs"]
mod tests;
