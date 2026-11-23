//! EXP-1459: the desktop's sealed session content (design #EXP-1457,
//! `packages/domain-contract/fixtures/sealed-content.json`).
//!
//! * READ: `sync::sealed` opens every row where it enters memory; this
//!   module hands it the device's seal service and re-opens the closed rows
//!   whenever the service's change counter moves (a wrap arrived).
//! * WRITE: person messages and card answers are sealed before
//!   `sessions.send` / `sessions.answer` ([`send_blocking`],
//!   [`answer_blocking`]); mentions ride clear as `mentionUserIds`. #EDE-97
//!   (`sealed-content.json` `clearTree`): NO SILENT PLAINTEXT. Plaintext
//!   goes only into a clear tree (`sessions.clear_tree`) and on a Local
//!   desktop (#EDE-55: no server, no keys); without a service or a key the
//!   send FAILS with `clearTree.noKeyRefusal` (the failed-action toast, the
//!   draft stays).

use std::sync::Arc;
use std::time::Duration;

use api::sessions::{AnswerInput, SealWrap, SendInput, SendOutput, SendSeal};
use coding::sealing::{self, NewRootSeal};
use domain::plan_card::{CardAnswerInput, CardAnswers};
use domain::rows::{MessageRow, SessionRow};
use domain::sealed::SealState;
use gpui::App;
use serde_json::Value;
use steer::enet::InstallNode;
use sync::sealed::{OpenedBody, OpenedCard, Opener, Opens};
use sync::Store;

/// `sync::sealed::Opens` over the process's keyring (`coding::sealing`: the
/// key holder's, else the one preloaded from disk at connect). Cached
/// secrets only: the ingest path never blocks (the key holder pulls
/// `sessionKeys.mine` on start and every minute; a new wrap moves
/// [`sealing::changed`] and the locked rows re-open).
struct KeyringOpens;

fn cached(root_id: &str) -> impl Fn(u32) -> Option<[u8; 32]> + '_ {
    move |epoch| sealing::cached_secret(root_id, epoch)
}

impl Opens for KeyringOpens {
    fn open_body(&self, root_id: &str, body: &str) -> OpenedBody {
        enet_core::sealed::open_body(body, &cached(root_id))
    }
    fn open_card(&self, root_id: &str, card: &Value) -> OpenedCard {
        enet_core::sealed::open_card(card, &cached(root_id))
    }
    fn open_title(&self, row: &SessionRow) -> Option<String> {
        // The keyring learns session → root and each root's current key
        // (what `ensure_key` writes with) from the synced rows.
        sealing::note_sessions(std::slice::from_ref(row));
        let sealed = row.sealed_title.as_deref().filter(|title| !title.is_empty())?;
        let opened = enet_core::sealed::open_body(sealed, &cached(row.root_id_or_self()));
        opened.text.filter(|_| matches!(opened.status, sync::sealed::OpenStatus::Opened | sync::sealed::OpenStatus::Plain))
    }
}

/// The keys `account` already holds, loaded from disk BEFORE its sync store
/// hydrates: the first frame after a launch shows its sealed rows opened
/// (the key holder, which needs the network, takes this same keyring).
pub fn preload(account: &api::Account, data_dir: &std::path::Path) {
    sealing::preload(sealing::keyring_path(data_dir, &account.id, false));
}

/// How often the keyring's change counter is read (an atomic): a key that
/// arrives re-opens its locked rows within this.
const KEYS_POLL: Duration = Duration::from_millis(500);

/// Bootstrap (before the first account connects): the opener reads the
/// keyring, and the closed rows re-open whenever new keys land.
pub fn init(cx: &mut App) {
    Opener::global().set_keys(Arc::new(KeyringOpens));
    cx.spawn(async move |cx| {
        let mut seen = sealing::changed();
        loop {
            cx.background_executor().timer(KEYS_POLL).await;
            let changed = sealing::changed();
            if changed != seen {
                seen = changed;
                cx.update(|cx| Store::global(cx).clone().reopen_sealed(cx));
            }
        }
    })
    .detach();
}

/// The writer's key operations (BLOCKING). [`Live`] = the installed seal
/// service + this account's eNet install; tests fake it.
trait Writer {
    /// `text` sealed for an existing tree (its key is created if missing).
    fn seal_body(&self, root_id: &str, text: &str) -> Result<String, String>;
    fn seal_card(&self, root_id: &str, card: &Value) -> Result<Value, String>;
    /// A fresh tree key for a root `sessions.send` creates, wrapped for
    /// `target_device`'s install too.
    fn new_root(&self, target_device: Option<&str>) -> Result<NewRootSeal, String>;
    fn adopt(&self, root_id: &str, seal: &NewRootSeal);
    /// #FEED-250: `sessions.sendTarget`, the server's word on the caller's
    /// conversation with an agent / app.
    fn send_target(&self, agent_id: Option<&str>, app_id: Option<&str>) -> Result<api::sessions::SendTarget, String>;
    /// Records a root's current key the server named.
    fn note_key(&self, root_id: &str, epoch: u32, public: [u8; 32]);
    /// #EXP-1960: the server's word on a session the store lacks (a push
    /// reply, a deep link before sync): its tree's root, clear flag and
    /// current key.
    fn tree_of(&self, session_id: &str) -> Result<TreeFacts, String> {
        Err(format!("{session_id}: no server to ask"))
    }
}

/// [`Writer::tree_of`]: a tree as the server has it.
#[derive(Clone, Debug, PartialEq)]
struct TreeFacts {
    root_id: String,
    clear: bool,
    /// `None` = keyless (a tree from before #EDE-97).
    key: Option<(u32, [u8; 32])>,
}

struct Live<'a> {
    node: Option<InstallNode>,
    trpc: Option<&'a api::TrpcClient>,
}

impl Writer for Live<'_> {
    fn seal_body(&self, root_id: &str, text: &str) -> Result<String, String> {
        sealing::seal_body(root_id, text).map_err(|err| err.to_string())
    }
    fn seal_card(&self, root_id: &str, card: &Value) -> Result<Value, String> {
        sealing::seal_card(root_id, card).map_err(|err| err.to_string())
    }
    fn new_root(&self, target_device: Option<&str>) -> Result<NewRootSeal, String> {
        self.node.as_ref().ok_or("P2P is not running")?.new_root_seal(target_device)
    }
    fn adopt(&self, root_id: &str, seal: &NewRootSeal) {
        match sealing::service() {
            Some(service) => service.adopt(root_id, seal),
            None => sealing::adopt_new_root(root_id, seal),
        }
    }
    fn send_target(&self, agent_id: Option<&str>, app_id: Option<&str>) -> Result<api::sessions::SendTarget, String> {
        let trpc = self.trpc.ok_or("no tRPC client")?;
        api::sessions::send_target(trpc, agent_id, app_id).map_err(|err| err.to_string())
    }
    fn note_key(&self, root_id: &str, epoch: u32, public: [u8; 32]) {
        if let Some(service) = sealing::service() {
            service.keyring().set_current(root_id, epoch, public);
        }
    }
    /// `sessions get` (any member id resolves to its root, no messages read)
    /// for the root and its clear flag, then `sessionKeys.publicKeys` by the
    /// ROOT id for its key.
    fn tree_of(&self, session_id: &str) -> Result<TreeFacts, String> {
        let trpc = self.trpc.ok_or("no tRPC client")?;
        let tree = api::mcp_tools::sessions_get(trpc, session_id).map_err(|err| format!("sessions get: {err}"))?;
        let root_id = tree.root.id;
        if root_id.is_empty() {
            return Err(format!("{session_id}: the server named no root"));
        }
        if let Some(service) = sealing::service() {
            service.keyring().note_root(session_id, &root_id);
        }
        let clear = tree.root.clear_tree;
        let key = if clear {
            None
        } else {
            let keys = api::session_keys::public_keys(trpc, std::slice::from_ref(&root_id))
                .map_err(|err| format!("sessionKeys.publicKeys: {err}"))?;
            keys.into_iter()
                .find(|key| key.root_id == root_id)
                .map(|key| sealing::public_key_from_b64(&key.public_key).map(|public| (key.epoch, public)))
                .transpose()
                .map_err(|err| err.to_string())?
        };
        Ok(TreeFacts { root_id, clear, key })
    }
}

/// Where a send's text goes, decided on the foreground ([`plan_send`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum SealTarget {
    /// An existing tree: seal for its root (its key is created if missing).
    Root(String),
    /// #FEED-250: a conversation with an agent / app the store does not
    /// have: the server names it (`sessions.sendTarget`) before anything is
    /// sealed; a new root only when it names none.
    Conversation { device_id: Option<String> },
    /// The conversation the server named keyed: sealed to ITS key.
    Keyed { root_id: String, epoch: u32, public: [u8; 32] },
    /// `sessions.send` creates the root: its key rides inline (`seal`),
    /// wrapped for the executing device's install too (`device_id`).
    NewRoot { device_id: Option<String> },
    /// #EXP-1960: an existing session (or parent) the store does not have
    /// yet (a push reply, a deep link before sync, a stale mirror): the
    /// server names its root, key and clear flag before anything is sealed
    /// ([`resolve_unsynced`]); it is NEVER a new chat, never plaintext.
    Unsynced(String),
    /// Nothing to seal (no text, no name). The ONLY plaintext target.
    #[default]
    None,
}

/// A send's sealing facts, read on the foreground before the blocking call.
#[derive(Clone, Default)]
pub struct SendPlan {
    pub target: SealTarget,
    /// #EDE-97: plaintext is this send's legal wire: a clear tree, or a
    /// Local desktop (no server, no keys: its trees are clear by nature).
    pub clear: bool,
    /// This account's eNet install (a new root's key comes from it).
    node: Option<InstallNode>,
    pub mention_user_ids: Vec<String>,
    /// `refIssueIds`: the `#IDENT` refs the text makes, resolved here.
    pub ref_issue_ids: Vec<String>,
}

/// The tree a send writes into and the users its text mentions.
pub fn plan_send(input: &SendInput, cx: &App) -> SendPlan {
    let collections = Store::global(cx).collections();
    let mention_user_ids = input
        .text
        .as_deref()
        .map(|text| {
            let users = collections.users.read(cx);
            let users: Vec<(&str, &str)> =
                users.iter().filter_map(|user| Some((user.id.as_str(), user.email.as_deref()?))).collect();
            domain::sealed::mention_user_ids(text, users.iter().copied())
        })
        .unwrap_or_default();
    let sessions = collections.sessions.read(cx);
    let team_id = input
        .team_id
        .clone()
        .or_else(|| input.session_id.as_ref().or(input.parent_id.as_ref()).and_then(|id| sessions.get(id)?.team_id.clone()));
    let ref_issue_ids = match (input.text.as_deref(), team_id.as_deref()) {
        (Some(text), Some(team_id)) => {
            let issues = collections.issue_refs_in_team(team_id, cx);
            domain::sealed::ref_issue_ids(text, issues.iter().map(|issue| (issue.id.as_str(), issue.identifier.as_str())))
        }
        _ => Vec::new(),
    };
    let root_of = |id: &str| {
        sessions
            .get(id)
            .map(|row| row.root_id_or_self().to_string())
            .or_else(|| Opener::global().root_of(id))
    };
    let target = target_for(input, &root_of, || existing_conversation(input, &sessions.iter().collect::<Vec<_>>(), cx));
    let local = crate::queries::active_account(cx).is_some_and(|account| account.is_local());
    let clear = local
        || matches!(&target, SealTarget::Root(root) if sessions.get(root).is_some_and(|row| row.clear_tree) || sealing::tree_clear(root));
    let node = crate::enet_wiring::active_node(cx);
    SendPlan { target, clear, node, mention_user_ids, ref_issue_ids }
}

/// [`plan_send`]'s tree, given what the store knows: `root_of` a synced
/// session's root, `existing` the member's synced conversation with the
/// send's agent / app.
fn target_for(input: &SendInput, root_of: &dyn Fn(&str) -> Option<String>, existing: impl FnOnce() -> Option<String>) -> SealTarget {
    // #EXP-2067: an action card is content too (sealed like the body).
    let has_content = input.text.is_some() || input.name.is_some() || input.action_card.is_some();
    let known = |id: &String| root_of(id).map_or_else(|| SealTarget::Unsynced(id.clone()), SealTarget::Root);
    if !has_content {
        SealTarget::None
    } else if let Some(session_id) = &input.session_id {
        known(session_id)
    } else if let Some(parent_id) = &input.parent_id {
        known(parent_id)
    } else if let Some(existing) = existing() {
        SealTarget::Root(existing)
    } else if input.team_id.is_some() && is_conversation(input) {
        SealTarget::Conversation { device_id: input.device_id.clone() }
    } else if input.team_id.is_some() {
        SealTarget::NewRoot { device_id: input.device_id.clone() }
    } else {
        SealTarget::None
    }
}

/// An agent / app send without a session id lands in the member's existing
/// conversation root when there is one.
fn existing_conversation(input: &SendInput, sessions: &[&SessionRow], cx: &App) -> Option<String> {
    let me = crate::queries::active_account(cx)?.user_id;
    let mine = |row: &&&SessionRow| row.is_root() && row.user_id.as_deref() == Some(me.as_str());
    if let Some(agent_id) = &input.agent_id {
        return sessions.iter().filter(mine).find(|row| row.agent_id.as_ref() == Some(agent_id)).map(|row| row.id.clone());
    }
    if let Some(app_id) = &input.app_id {
        return sessions.iter().filter(mine).find(|row| row.app_id.as_ref() == Some(app_id)).map(|row| row.id.clone());
    }
    None
}

/// A send to an agent / app that names no session: it lands in the
/// caller's conversation with it.
fn is_conversation(input: &SendInput) -> bool {
    input.session_id.is_none() && input.parent_id.is_none() && (input.agent_id.is_some() || input.app_id.is_some())
}

/// #FEED-250 (`sealed-content.json` `staleTree`): `target` with the server
/// asked where the store cannot know: a conversation it lacks, or (`fresh`:
/// the server refused a key write, the store was wrong once) any agent /
/// app conversation. BLOCKING.
fn resolve(writer: &dyn Writer, input: &SendInput, target: &SealTarget, fresh: bool) -> SealTarget {
    let ask = match target {
        SealTarget::Conversation { .. } => true,
        SealTarget::Root(_) | SealTarget::NewRoot { .. } => fresh && is_conversation(input),
        SealTarget::None | SealTarget::Keyed { .. } | SealTarget::Unsynced(_) => false,
    };
    if !ask {
        return target.clone();
    }
    let device_id = match target {
        SealTarget::Conversation { device_id } | SealTarget::NewRoot { device_id } => device_id.clone(),
        _ => input.device_id.clone(),
    };
    match writer.send_target(input.agent_id.as_deref(), input.app_id.as_deref()) {
        Ok(found) => {
            let key = found.epoch.zip(found.public_key.as_deref().and_then(|key| sealing::public_key_from_b64(key).ok()));
            match (found.root_id, key) {
                // #EDE-97: a clear conversation this app has not synced yet.
                (Some(root_id), _) if found.clear_tree => {
                    sealing::note_clear_tree(&root_id);
                    SealTarget::Root(root_id)
                }
                (Some(root_id), Some((epoch, public))) => {
                    writer.note_key(&root_id, epoch, public);
                    SealTarget::Keyed { root_id, epoch, public }
                }
                // Keyless: keyed by this send's seal (`sessionKeys.create`).
                (Some(root_id), None) => SealTarget::Root(root_id),
                (None, _) => SealTarget::NewRoot { device_id },
            }
        }
        Err(err) => {
            log::warn!("[sealing] sessions.sendTarget failed: {err}");
            match target {
                SealTarget::Conversation { device_id } => SealTarget::NewRoot { device_id: device_id.clone() },
                other => other.clone(),
            }
        }
    }
}

/// Seals `input` for `target` (BLOCKING, resolved by [`resolve`]): the
/// text and name become body envelopes, a new root's key rides inline.
/// Returns the new root's seal to adopt once the server names the root.
/// #EDE-97: any failure is an `Err` (nothing leaves); only a clear tree
/// keeps its content plaintext.
fn seal_send(writer: &dyn Writer, input: &mut SendInput, target: &SealTarget) -> Result<Option<NewRootSeal>, String> {
    // #EXP-1683: the server never appends to a sealed body: the attachment
    // lines go inside it.
    let text = input.text.as_deref().map(|text| {
        domain::sealed::with_attachment_embeds(text, input.attachment_ids.as_deref().unwrap_or_default(), domain::sealed::recorded_upload)
    });
    let failed = |why: String| {
        log::warn!("[sealing] not sent, it could not be encrypted: {why}");
        sealing::NO_KEY_REFUSAL.to_string()
    };
    match target {
        SealTarget::None | SealTarget::Conversation { .. } => Ok(None),
        // Resolved before sealing ([`deliver`]); never leaves unsealed.
        SealTarget::Unsynced(session_id) => Err(failed(format!("{session_id}: its tree is not known yet"))),
        SealTarget::Keyed { root_id, epoch, public } => {
            let body = |text: &str| enet_core::sealed::seal_body(public, *epoch, text).map_err(|err| failed(format!("{root_id}: {err}")));
            let sealed_text = text.as_deref().map(body).transpose()?;
            let sealed_name = input.name.as_deref().map(body).transpose()?;
            if sealed_name.is_some() {
                input.sealed_name = sealed_name;
                input.name = None;
            }
            if sealed_text.is_some() {
                input.text = sealed_text;
            }
            if let Some(card) = input.action_card.as_ref() {
                input.action_card =
                    Some(action_envelope(enet_core::sealed::seal_card(public, *epoch, card).map_err(|err| failed(format!("{root_id}: {err}")))?));
            }
            Ok(None)
        }
        SealTarget::Root(root_id) if sealing::tree_clear(root_id) => Ok(None),
        SealTarget::Root(root_id) => {
            let body = |text: &str| writer.seal_body(root_id, text).map_err(|err| failed(format!("{root_id}: {err}")));
            let sealed_text = text.as_deref().map(body).transpose()?;
            let sealed_name = input.name.as_deref().map(body).transpose()?;
            if sealed_text.is_some() {
                input.text = sealed_text;
            }
            if sealed_name.is_some() {
                input.sealed_name = sealed_name;
                input.name = None;
            }
            if let Some(card) = input.action_card.as_ref() {
                input.action_card = Some(action_envelope(writer.seal_card(root_id, card).map_err(|err| failed(format!("{root_id}: {err}")))?));
            }
            Ok(None)
        }
        SealTarget::NewRoot { device_id } => {
            // The executing device reads the tree from its first message
            // on; any gap is filled by a holder (`pending`).
            let seal = writer.new_root(device_id.as_deref()).map_err(|err| failed(format!("no new tree key: {err}")))?;
            let public = enet_core::sealed::public_key_from_b64(&seal.public_key).map_err(|err| failed(err.to_string()))?;
            let body = |text: &str| enet_core::sealed::seal_body(&public, seal.epoch, text).map_err(|err| failed(err.to_string()));
            let sealed_text = text.as_deref().map(body).transpose()?;
            if let Some(name) = input.name.as_deref() {
                input.sealed_name = Some(body(name)?);
                input.name = None;
            }
            if sealed_text.is_some() {
                input.text = sealed_text;
            }
            if let Some(card) = input.action_card.as_ref() {
                input.action_card = Some(action_envelope(enet_core::sealed::seal_card(&public, seal.epoch, card).map_err(|err| failed(err.to_string()))?));
            }
            input.seal = Some(SendSeal {
                install_id: seal.install_id.clone(),
                public_key: seal.public_key.clone(),
                epoch: seal.epoch,
                wraps: seal
                    .wraps
                    .iter()
                    .map(|wrap| SealWrap { install_id: wrap.install_id.clone(), wrapped: wrap.wrapped.clone() })
                    .collect(),
            });
            Ok(Some(seal))
        }
    }
}

/// #EXP-2067: a sealed action card's wire is its envelope ALONE,
/// `{kind: "action", sealed}` (`sessions.send` `actionCard`): the clear
/// fields a card keeps (`messageId`) live inside it.
fn action_envelope(sealed: Value) -> Value {
    serde_json::json!({"kind": "action", "sealed": sealed.get("sealed").cloned().unwrap_or(Value::Null)})
}

/// #EDE-97: the failed-action error of a send that could not be sealed.
fn no_key(message: String) -> api::ApiError {
    api::ApiError::Http { status: 400, message }
}

/// THE sealed `sessions.send` (BLOCKING, off the UI thread).
pub fn send_blocking(trpc: &api::TrpcClient, input: &SendInput, plan: &SendPlan) -> Result<SendOutput, api::ApiError> {
    let mut input = input.clone();
    if !plan.mention_user_ids.is_empty() {
        input.mention_user_ids = Some(plan.mention_user_ids.clone());
    }
    if !plan.ref_issue_ids.is_empty() {
        input.ref_issue_ids = Some(plan.ref_issue_ids.clone());
    }
    // #EDE-97: plaintext only where it is the legal wire (a clear tree, a
    // Local desktop) or nothing is sealed (an attachment-only send).
    if plan.clear || plan.target == SealTarget::None {
        card_tap_leaves_sealed(&input)?;
        return api::sessions::send(trpc, &input);
    }
    let writer = Live { node: plan.node.clone(), trpc: Some(trpc) };
    // A key holder still coming up is waited for (`clearTree.holderWaitMs`).
    deliver(&writer, &input, &plan.target, sealing::sealer().is_ok(), &mut |input| api::sessions::send(trpc, input))
}

/// #EXP-1958 (`cardStamps` NEVER IN CLEAR): a sealed card's tap hands OPENED
/// text to the writer, so its wire is an `exp-sealed:` envelope with no
/// clear `name` beside it, or it is refused here like a failed action (a
/// `clearTree` target, nothing sealed): no opened word leaves in clear.
fn card_tap_leaves_sealed(wire: &SendInput) -> Result<(), api::ApiError> {
    if wire.card_item.is_none() {
        return Ok(());
    }
    let sealed = wire.text.as_deref().is_some_and(|text| text.starts_with(enet_core::sealed::SEALED_BODY_PREFIX));
    if sealed && wire.name.is_none() {
        return Ok(());
    }
    log::warn!("[sealing] a sealed card's tap not sent: its text could not be sealed");
    Err(no_key(sealing::NO_KEY_REFUSAL.to_string()))
}

/// #EXP-1960: an [`SealTarget::Unsynced`] session's tree as the server has
/// it: `None` = a clear tree (plaintext is its wire); a keyed one is sealed
/// to its key, a keyless one (pre-#EDE-97) keyed by this send's seal when
/// this install may (`ensure_key`; else the NO-KEY refusal). BLOCKING.
fn resolve_unsynced(writer: &dyn Writer, session_id: &str) -> Result<Option<SealTarget>, String> {
    let TreeFacts { root_id, clear, key } = writer.tree_of(session_id)?;
    if clear {
        sealing::note_clear_tree(&root_id);
        return Ok(None);
    }
    Ok(Some(match key {
        Some((epoch, public)) => {
            writer.note_key(&root_id, epoch, public);
            SealTarget::Keyed { root_id, epoch, public }
        }
        None => SealTarget::Root(root_id),
    }))
}

/// [`send_blocking`] past its plaintext wires: an unsynced tree asked of the
/// server first (a clear one goes as given, an unknown one FAILS), then
/// sealed by a running key holder (`holder`) or refused.
fn deliver(
    writer: &dyn Writer,
    input: &SendInput,
    target: &SealTarget,
    holder: bool,
    send: &mut dyn FnMut(&SendInput) -> Result<SendOutput, api::ApiError>,
) -> Result<SendOutput, api::ApiError> {
    // #EXP-1958: whatever wire this takes, a card tap leaves sealed or not at all.
    let send: &mut dyn FnMut(&SendInput) -> Result<SendOutput, api::ApiError> = &mut |wire| {
        card_tap_leaves_sealed(wire)?;
        send(wire)
    };
    let target = match target {
        SealTarget::Unsynced(session_id) => match resolve_unsynced(writer, session_id) {
            Ok(None) => return send(input),
            Ok(Some(target)) => target,
            Err(why) => {
                log::warn!("[sealing] not sent, its tree is unknown: {why}");
                return Err(no_key(sealing::NO_KEY_REFUSAL.to_string()));
            }
        },
        other => other.clone(),
    };
    if !holder {
        log::warn!("[sealing] not sent: no key holder runs");
        return Err(no_key(sealing::NO_KEY_REFUSAL.to_string()));
    }
    send_sealed(writer, input, &target, send)
}

/// #FEED-250 (`staleTree`): resolves, seals and sends; a refused key write
/// (the tree moved on behind this store) is resolved again from the server,
/// re-sealed and re-sent ONCE by itself. A second refusal reads
/// [`domain::sealed::STALE_TREE_REFUSAL`], never a raw epoch.
fn send_sealed(
    writer: &dyn Writer,
    input: &SendInput,
    target: &SealTarget,
    send: &mut dyn FnMut(&SendInput) -> Result<SendOutput, api::ApiError>,
) -> Result<SendOutput, api::ApiError> {
    let mut attempt = |fresh: bool| {
        let mut wire = input.clone();
        let seal = seal_send(writer, &mut wire, &resolve(writer, input, target, fresh)).map_err(no_key)?;
        let out = send(&wire)?;
        if let Some(seal) = seal {
            writer.adopt(&out.session_id, &seal);
        }
        Ok(out)
    };
    let stale = |err: &api::ApiError| matches!(err, api::ApiError::Http { message, .. } if domain::sealed::is_stale_tree_refusal(message));
    match attempt(false) {
        Err(err) if stale(&err) => attempt(true).map_err(|err| match err {
            api::ApiError::Http { status, message } if domain::sealed::is_stale_tree_refusal(&message) => {
                api::ApiError::Http { status, message: domain::sealed::STALE_TREE_REFUSAL.to_string() }
            }
            other => other,
        }),
        other => other,
    }
}

/// An answer's sealing facts: the card's tree and, for a SEALED card, the
/// card as opened (EXP-1486: the stored card alone picks the wire,
/// `sealed-content.json` `answerCases`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnswerPlan {
    pub root_id: Option<String>,
    /// The opened card of a sealed row; `None` = a plaintext card, answered
    /// in clear even inside a keyed tree.
    pub card: Option<Value>,
    /// A sealed card this install cannot open: refused locally.
    pub locked: bool,
}

/// What a sealed card this install cannot answer says (Android's
/// `LOCKED_ANSWER`).
pub const LOCKED_ANSWER: &str = "This card can't be answered on this device yet.";

pub fn plan_answer(message_id: &str, cx: &App) -> AnswerPlan {
    let messages = Store::global(cx).collections().messages.read(cx);
    answer_plan_of(messages.get(message_id))
}

/// #EDE-131: a card the store does not have (not synced yet) is refused
/// locally like a locked one: whether it is sealed is unknown, and a
/// plaintext answer would reach the server before it refused.
fn answer_plan_of(row: Option<&MessageRow>) -> AnswerPlan {
    row.map_or(AnswerPlan { locked: true, ..AnswerPlan::default() }, answer_plan_for)
}

/// A synced row's [`AnswerPlan`]: `seal` is `None` for a plaintext row.
fn answer_plan_for(row: &MessageRow) -> AnswerPlan {
    let root_id = row.root_id.clone();
    match row.seal {
        None => AnswerPlan { root_id, ..AnswerPlan::default() },
        Some(SealState::Opened) => AnswerPlan { root_id, card: row.card.clone(), locked: false },
        Some(_) => AnswerPlan { root_id, card: None, locked: true },
    }
}

/// `input` as a sealed answer: the FULL card re-sealed with the answer, the
/// clear `decision` / `fresh` beside it, no plaintext answer fields.
fn seal_answer(writer: &dyn Writer, input: &AnswerInput, plan: &AnswerPlan) -> Option<AnswerInput> {
    let (root_id, card) = (plan.root_id.as_deref()?, plan.card.as_ref()?);
    let answers = input.answers.as_ref().and_then(|answers| match answers {
        Value::Array(labels) => Some(CardAnswers::Labels(labels.iter().filter_map(|l| l.as_str().map(str::to_owned)).collect())),
        Value::Object(_) => serde_json::from_value(answers.clone()).ok().map(CardAnswers::ByQuestion),
        _ => None,
    });
    let answered = domain::plan_card::answered_card(
        card,
        &CardAnswerInput {
            answer: input.answer.clone(),
            answers,
            decision: input.decision.clone(),
            fresh: input.fresh,
            feedback: input.feedback.clone(),
            surface_data: input.surface_data.clone(),
        },
    )?;
    let sealed = writer.seal_card(root_id, &answered).map_err(|err| log::warn!("[sealing] can't seal the answer: {err}")).ok()?;
    Some(AnswerInput {
        message_id: input.message_id.clone(),
        decision: input.decision.clone(),
        fresh: input.fresh,
        sealed_card: Some(sealed),
        ..AnswerInput::default()
    })
}

/// The answer that leaves: a plaintext card's as given, a sealed card's
/// re-sealed; a sealed card this install can't (re)seal is refused, since a
/// plaintext answer would leak it.
fn answer_wire(writer: Option<&dyn Writer>, input: &AnswerInput, plan: &AnswerPlan) -> Result<AnswerInput, String> {
    // #EXP-1561: a dismiss carries no answer, sealed or not (the server
    // stamps the clear fields beside the envelope).
    if input.dismissed == Some(true) {
        return Ok(AnswerInput { message_id: input.message_id.clone(), dismissed: Some(true), ..AnswerInput::default() });
    }
    if plan.locked {
        return Err(LOCKED_ANSWER.into());
    }
    if plan.card.is_none() {
        return Ok(input.clone());
    }
    writer.and_then(|writer| seal_answer(writer, input, plan)).ok_or_else(|| LOCKED_ANSWER.into())
}

/// THE sealed `sessions.answer` (BLOCKING, off the UI thread).
pub fn answer_blocking(trpc: &api::TrpcClient, input: &AnswerInput, plan: &AnswerPlan) -> Result<api::sessions::TxOutput, api::ApiError> {
    let live = sealing::service().map(|_| Live { node: None, trpc: None });
    let wire = answer_wire(live.as_ref().map(|w| w as &dyn Writer), input, plan)
        .map_err(|message| api::ApiError::Http { status: 400, message })?;
    api::sessions::answer(trpc, &wire)
}

#[cfg(test)]
mod tests {
    use super::*;
    use enet_core::sealed as contract;
    use std::sync::Mutex;

    /// A one-tree writer on a fixed key.
    struct OneTree {
        key: contract::TreeKey,
        devices: Mutex<Vec<Option<String>>>,
    }

    impl OneTree {
        fn open_body(&self, body: &str) -> OpenedBody {
            contract::open_body(body, &|epoch| (epoch == 1).then_some(self.key.secret))
        }
        fn open_card(&self, card: &Value) -> OpenedCard {
            contract::open_card(card, &|epoch| (epoch == 1).then_some(self.key.secret))
        }
    }

    impl Writer for OneTree {
        fn seal_body(&self, _root_id: &str, text: &str) -> Result<String, String> {
            contract::seal_body(&self.key.public, 1, text).map_err(|e| e.to_string())
        }
        fn seal_card(&self, _root_id: &str, card: &Value) -> Result<Value, String> {
            contract::seal_card(&self.key.public, 1, card).map_err(|e| e.to_string())
        }
        fn new_root(&self, target_device: Option<&str>) -> Result<NewRootSeal, String> {
            self.devices.lock().unwrap().push(target_device.map(str::to_string));
            Ok(NewRootSeal {
                install_id: "me".into(),
                public_key: self.key.public_key(),
                epoch: 1,
                wraps: vec![api::session_keys::InstallWrap { install_id: "me".into(), wrapped: "w".into() }],
                secret: self.key.secret,
            })
        }
        fn adopt(&self, _root_id: &str, _seal: &NewRootSeal) {}
        fn send_target(&self, _agent_id: Option<&str>, _app_id: Option<&str>) -> Result<api::sessions::SendTarget, String> {
            Ok(api::sessions::SendTarget::default())
        }
        fn note_key(&self, _root_id: &str, _epoch: u32, _public: [u8; 32]) {}
    }

    fn tree() -> OneTree {
        OneTree { key: contract::generate_tree_key(), devices: Mutex::new(Vec::new()) }
    }

    #[test]
    fn a_message_to_an_existing_tree_seals_text_and_name() {
        let writer = tree();
        let mut input = SendInput::message("s1", "hello @a@b.co", Vec::new());
        input.name = Some("Title".into());
        let plan = SendPlan { target: SealTarget::Root("r1".into()), ..SendPlan::default() };
        assert!(seal_send(&writer, &mut input, &plan.target).unwrap().is_none());
        let text = input.text.as_deref().unwrap();
        assert!(text.starts_with("exp-sealed:v1:1:"));
        assert_eq!(writer.open_body(text).text.as_deref(), Some("hello @a@b.co"));
        assert_eq!(input.name, None);
        assert_eq!(writer.open_body(input.sealed_name.as_deref().unwrap()).text.as_deref(), Some("Title"));
        assert!(input.seal.is_none());
    }

    /// #EXP-2067: a press's action card rides sealed as its envelope alone
    /// (`{kind, sealed}`), the whole card inside; an empty body sends none.
    #[test]
    fn an_action_card_seals_to_its_envelope() {
        let writer = tree();
        let card = serde_json::json!({"kind": "action", "name": "save", "context": {"a": 1}, "surfaceId": "m:1#1", "block": "m:1#1", "messageId": "m1"});
        let mut input = SendInput { session_id: Some("s1".into()), action_card: Some(card.clone()), ..SendInput::default() };
        let plan = SendPlan { target: SealTarget::Root("r1".into()), ..SendPlan::default() };
        assert_eq!(target_for(&input, &|_| Some("r1".into()), || None), SealTarget::Root("r1".into()));
        seal_send(&writer, &mut input, &plan.target).unwrap();
        let wire = input.action_card.clone().unwrap();
        assert_eq!(wire.as_object().unwrap().keys().collect::<Vec<_>>(), vec!["kind", "sealed"]);
        assert_eq!(wire["kind"], "action");
        assert_eq!(writer.open_card(&wire).card, card);
        assert_eq!(input.text, None);
    }

    #[test]
    fn a_new_root_carries_its_key_inline() {
        let writer = tree();
        let mut input = SendInput { team_id: Some("t".into()), device_id: Some("d".into()), text: Some("go".into()), ..SendInput::default() };
        let plan = SendPlan { target: SealTarget::NewRoot { device_id: Some("d".into()) }, ..SendPlan::default() };
        let seal = seal_send(&writer, &mut input, &plan.target).unwrap().unwrap();
        assert_eq!(seal.secret, writer.key.secret);
        assert_eq!(writer.devices.lock().unwrap().clone(), vec![Some("d".to_string())]);
        let wire = input.seal.clone().unwrap();
        assert_eq!((wire.install_id.as_str(), wire.epoch, wire.public_key), ("me", 1, writer.key.public_key()));
        assert_eq!(writer.open_body(input.text.as_deref().unwrap()).text.as_deref(), Some("go"));
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["seal"]["wraps"][0], serde_json::json!({"installId": "me", "wrapped": "w"}));
    }

    #[test]
    fn an_answer_reseals_the_full_card_and_drops_plaintext_fields() {
        let writer = tree();
        let card = serde_json::json!({"kind": "question", "question": "Which?", "options": ["A", "B"]});
        let input = AnswerInput {
            message_id: "m1".into(),
            answer: Some("A".into()),
            answers: Some(serde_json::json!(["A"])),
            ..AnswerInput::default()
        };
        let plan = AnswerPlan { root_id: Some("r1".into()), card: Some(card), locked: false };
        let sealed = seal_answer(&writer, &input, &plan).unwrap();
        assert_eq!((sealed.answer.clone(), sealed.answers.clone(), sealed.feedback.clone()), (None, None, None));
        let wire = serde_json::to_value(&sealed).unwrap();
        assert_eq!(wire["sealedCard"]["kind"], "question");
        assert_eq!(wire["sealedCard"]["answered"], true);
        let opened = writer.open_card(&wire["sealedCard"]).card;
        assert_eq!(opened["answers"], serde_json::json!(["A"]));
        assert_eq!(opened["question"], "Which?");

        let plan_card = AnswerPlan { root_id: Some("r1".into()), card: Some(serde_json::json!({"kind": "plan", "plan": "p"})), locked: false };
        let decide = AnswerInput { message_id: "m2".into(), decision: Some("rejected".into()), feedback: Some("no".into()), ..AnswerInput::default() };
        let sealed = seal_answer(&writer, &decide, &plan_card).unwrap();
        assert_eq!((sealed.decision.as_deref(), sealed.feedback.as_deref()), (Some("rejected"), None));
        let wire = serde_json::to_value(&sealed).unwrap();
        assert_eq!(wire["sealedCard"]["decision"], "rejected");
        assert_eq!(writer.open_card(&wire["sealedCard"]).card["feedback"], "no");
        // An unknown card (not synced, or closed here) never seals.
        assert!(seal_answer(&writer, &decide, &AnswerPlan::default()).is_none());
    }

    /// EXP-1486 (`sealed-content.json` `answerCases`): the STORED card picks
    /// the wire. A plaintext plan inside a keyed tree answers in clear (0.14.88
    /// sealed it and the server refused), a sealed one re-seals, a locked one
    /// never leaves.
    /// #EDE-131: an answer to a card the store lacks never leaves (a
    /// dismiss still does: it carries nothing).
    #[test]
    fn an_answer_to_a_card_the_store_lacks_is_refused_locally() {
        let plan = answer_plan_of(None);
        let input = AnswerInput { message_id: "m-unsynced".into(), answer: Some("Yes".into()), answers: Some(serde_json::json!(["Yes"])), ..AnswerInput::default() };
        assert_eq!(answer_wire(None, &input, &plan), Err(LOCKED_ANSWER.to_string()));
        let dismiss = AnswerInput { message_id: "m-unsynced".into(), dismissed: Some(true), ..AnswerInput::default() };
        assert_eq!(answer_wire(None, &dismiss, &plan).map(|wire| wire.dismissed), Ok(Some(true)));
    }

    #[test]
    fn the_fixture_answer_cases_pick_the_wire_by_the_stored_card() {
        struct FixtureKeys(Option<[u8; 32]>);
        impl Opens for FixtureKeys {
            fn open_body(&self, _root_id: &str, body: &str) -> OpenedBody {
                contract::open_body(body, &|epoch| (epoch == 1).then_some(self.0).flatten())
            }
            fn open_card(&self, _root_id: &str, card: &Value) -> OpenedCard {
                contract::open_card(card, &|epoch| (epoch == 1).then_some(self.0).flatten())
            }
            fn open_title(&self, _row: &SessionRow) -> Option<String> {
                None
            }
        }
        let fixture: Value = serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json")).unwrap();
        let hex = |text: &str| -> [u8; 32] {
            let bytes: Vec<u8> = (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect();
            bytes.try_into().unwrap()
        };
        let secret = hex(fixture["tree"]["secret"].as_str().unwrap());
        let public = contract::public_key_from_b64(fixture["tree"]["publicKey"].as_str().unwrap()).unwrap();
        let writer = OneTree { key: contract::TreeKey { secret, public }, devices: Mutex::new(Vec::new()) };
        let splits = fixture["card"]["splits"].as_array().unwrap();
        let cases = fixture["answerCases"].as_array().unwrap();
        assert!(cases.len() >= 5);
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let stored = match case["stored"]["split"].as_str() {
                Some(split) => splits.iter().find(|s| s["name"] == split).unwrap()["envelope"].clone(),
                None => case["stored"].clone(),
            };
            let with_key = case["keys"] == "tree";
            let mut row = MessageRow { id: "m1".into(), root_id: Some("r1".into()), body: Some(String::new()), card: Some(stored), ..MessageRow::default() };
            Opener::new(Arc::new(FixtureKeys(with_key.then_some(secret)))).open_message(&mut row);
            let plan = answer_plan_for(&row);
            let given = &case["input"];
            let text = |field: &str| given[field].as_str().map(str::to_string);
            let input = AnswerInput {
                message_id: "m1".into(),
                answer: text("answer"),
                decision: text("decision"),
                fresh: given["fresh"].as_bool(),
                feedback: text("feedback"),
                answers: given.get("answers").cloned(),
                ..AnswerInput::default()
            };
            let wire = answer_wire(Some(&writer), &input, &plan);
            match case["wire"].as_str().unwrap() {
                "refused" => assert_eq!(wire, Err(LOCKED_ANSWER.to_string()), "{name}"),
                "clear" => assert_eq!(wire.as_ref(), Ok(&input), "{name}"),
                "sealed" => {
                    let wire = serde_json::to_value(wire.unwrap()).unwrap();
                    let keys: Vec<&String> = wire.as_object().unwrap().keys().filter(|k| *k != "messageId").collect();
                    assert!(keys.iter().all(|k| ["sealedCard", "decision", "fresh"].contains(&k.as_str())), "{name}: {keys:?}");
                    for (field, value) in case["clear"].as_object().unwrap() {
                        assert_eq!(&wire["sealedCard"][field], value, "{name}: clear {field}");
                    }
                    let opened = writer.open_card(&wire["sealedCard"]).card;
                    for (field, value) in case["opened"].as_object().unwrap() {
                        assert_eq!(&opened[field], value, "{name}: opened {field}");
                    }
                }
                other => panic!("{name}: unknown wire {other}"),
            }
        }
    }

    /// #FEED-250 (`sealed-content.json` `staleTree`): a tree as the server
    /// has it, the store's view of it, and every server call the writer makes.
    struct StaleTree {
        key: contract::TreeKey,
        local: String,
        server: String,
        asks: Mutex<Vec<String>>,
    }

    impl StaleTree {
        fn ask(&self, what: &str) {
            self.asks.lock().unwrap().push(what.to_string());
        }
        fn opens(&self, body: &str) -> Option<String> {
            contract::open_body(body, &|epoch| (epoch == 1).then_some(self.key.secret)).text
        }
    }

    impl Writer for StaleTree {
        /// The key holder's `ensure_key`: the key the store synced, else
        /// `publicKeys`, else `sessionKeys.create`.
        fn seal_body(&self, _root_id: &str, text: &str) -> Result<String, String> {
            if self.local != "keyed" {
                self.ask("publicKeys");
                if self.server != "keyed" {
                    self.ask("create");
                }
            }
            contract::seal_body(&self.key.public, 1, text).map_err(|e| e.to_string())
        }
        fn seal_card(&self, _root_id: &str, card: &Value) -> Result<Value, String> {
            contract::seal_card(&self.key.public, 1, card).map_err(|e| e.to_string())
        }
        fn new_root(&self, _target_device: Option<&str>) -> Result<NewRootSeal, String> {
            Ok(NewRootSeal { install_id: "me".into(), public_key: self.key.public_key(), epoch: 1, wraps: Vec::new(), secret: self.key.secret })
        }
        fn adopt(&self, _root_id: &str, _seal: &NewRootSeal) {}
        fn send_target(&self, _agent_id: Option<&str>, _app_id: Option<&str>) -> Result<api::sessions::SendTarget, String> {
            self.ask("sendTarget");
            let root_id = (self.server != "none").then(|| "r1".to_string());
            let keyed = self.server == "keyed";
            Ok(api::sessions::SendTarget {
                root_id,
                epoch: keyed.then_some(1),
                public_key: keyed.then(|| self.key.public_key()),
                clear_tree: false,
                rotation_due: false,
            })
        }
        fn note_key(&self, _root_id: &str, _epoch: u32, _public: [u8; 32]) {}
    }

    fn stale_fixture() -> (Value, contract::TreeKey) {
        let fixture: Value = serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json")).unwrap();
        let hex = fixture["tree"]["secret"].as_str().unwrap();
        let secret: [u8; 32] =
            (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect::<Vec<_>>().try_into().unwrap();
        let public = contract::public_key_from_b64(fixture["tree"]["publicKey"].as_str().unwrap()).unwrap();
        (fixture["staleTree"].clone(), contract::TreeKey { secret, public })
    }

    fn agent_send() -> SendInput {
        SendInput { team_id: Some("t1".into()), agent_id: Some("a1".into()), text: Some("hi".into()), ..SendInput::default() }
    }

    fn refused(message: &str) -> api::ApiError {
        api::ApiError::Http { status: 409, message: message.into() }
    }

    #[test]
    fn the_fixture_stale_tree_cases_ask_the_server_where_the_store_cannot_know() {
        let (stale, key) = stale_fixture();
        let cases = stale["cases"].as_array().unwrap();
        assert!(cases.len() >= 5);
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let text = |field: &str| case[field].as_str().unwrap().to_string();
            let writer = StaleTree { key: key.clone(), local: text("local"), server: text("server"), asks: Mutex::new(Vec::new()) };
            let (input, target) = match (text("send").as_str(), writer.local.as_str()) {
                ("agent", "missing") => (agent_send(), SealTarget::Conversation { device_id: None }),
                ("agent", _) => (agent_send(), SealTarget::Root("r1".into())),
                _ => (SendInput::message("r1", "hi", Vec::new()), SealTarget::Root("r1".into())),
            };
            let mut sent = Vec::new();
            send_sealed(&writer, &input, &target, &mut |wire| {
                sent.push(wire.clone());
                Ok(SendOutput { session_id: "r1".into(), ..SendOutput::default() })
            })
            .unwrap();
            // The desktop's key holder looks a root's key up (`publicKeys`)
            // before it creates one: a refused create never leaves.
            let mut want: Vec<String> = Vec::new();
            for ask in case["asks"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()) {
                match ask {
                    "create" => {
                        want.push("publicKeys".into());
                        if writer.server != "keyed" {
                            want.push("create".into());
                        }
                    }
                    "publicKeys" if want.last().is_some_and(|last| last == "publicKeys") => {}
                    other => want.push(other.into()),
                }
            }
            assert_eq!(*writer.asks.lock().unwrap(), want, "{name}");
            let wire = sent.pop().unwrap();
            match case["wire"].as_str().unwrap() {
                "inline" => assert_eq!(wire.seal.as_ref().map(|seal| seal.epoch), Some(1), "{name}"),
                _ => assert!(wire.seal.is_none(), "{name}"),
            }
            assert_eq!(writer.opens(wire.text.as_deref().unwrap()).as_deref(), Some("hi"), "{name}");
        }
    }

    #[test]
    fn the_fixture_stale_tree_retry_cases_reseal_once_by_themselves() {
        let (stale, key) = stale_fixture();
        let refusal = stale["refusal"].as_str().unwrap();
        assert_eq!(refusal, domain::sealed::STALE_TREE_REFUSAL);
        for case in stale["retryCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            // The store missed the keyed conversation; the server knows it.
            let writer = StaleTree { key: key.clone(), local: "missing".into(), server: "keyed".into(), asks: Mutex::new(Vec::new()) };
            let mut refusals = case["refusals"].as_u64().unwrap();
            let mut sent = Vec::new();
            let out = send_sealed(&writer, &agent_send(), &SealTarget::Conversation { device_id: None }, &mut |wire| {
                sent.push(wire.clone());
                if refusals > 0 {
                    refusals -= 1;
                    return Err(refused(refusal));
                }
                Ok(SendOutput { session_id: "r1".into(), ..SendOutput::default() })
            });
            match case["outcome"].as_str().unwrap() {
                "sent" => assert_eq!(out.unwrap().session_id, "r1", "{name}"),
                _ => assert_eq!(out.unwrap_err().user_message(), refusal, "{name}"),
            }
            assert_eq!(sent.len() as u64, case["sends"].as_u64().unwrap(), "{name}");
            let last = sent.last().unwrap();
            assert!(last.seal.is_none(), "{name}: the re-send is sealed to the server's key");
            assert_eq!(writer.opens(last.text.as_deref().unwrap()).as_deref(), Some("hi"), "{name}");
        }
    }

    #[test]
    fn an_older_servers_epoch_refusal_is_resealed_and_never_shown_raw() {
        let (_, key) = stale_fixture();
        let writer = StaleTree { key, local: "keyed".into(), server: "keyed".into(), asks: Mutex::new(Vec::new()) };
        let mut calls = 0;
        let out = send_sealed(&writer, &agent_send(), &SealTarget::Root("r1".into()), &mut |_| {
            calls += 1;
            Err(refused("The tree is at epoch 3"))
        });
        assert_eq!(calls, 2);
        assert_eq!(out.unwrap_err().user_message(), domain::sealed::STALE_TREE_REFUSAL);
        // The re-send asked the server, not the store.
        assert_eq!(*writer.asks.lock().unwrap(), vec!["sendTarget".to_string()]);
        calls = 0;
        let out = send_sealed(&writer, &agent_send(), &SealTarget::Root("r1".into()), &mut |_| {
            calls += 1;
            Err(refused("Session not found"))
        });
        assert_eq!((calls, out.unwrap_err().user_message().as_str()), (1, "Session not found"));
    }

    /// #EDE-97: no silent plaintext. A send that cannot be sealed (no key,
    /// no new tree key) fails with `clearTree.noKeyRefusal` and leaves the
    /// input as it was; a clear tree keeps its text plaintext.
    #[test]
    fn an_unsealable_send_fails_and_a_clear_tree_stays_plain() {
        struct NoKeys;
        impl Writer for NoKeys {
            fn seal_body(&self, _: &str, _: &str) -> Result<String, String> {
                Err("no key holder".into())
            }
            fn seal_card(&self, _: &str, _: &Value) -> Result<Value, String> {
                Err("no key holder".into())
            }
            fn new_root(&self, _: Option<&str>) -> Result<NewRootSeal, String> {
                Err("P2P is not running".into())
            }
            fn adopt(&self, _: &str, _: &NewRootSeal) {}
            fn send_target(&self, _: Option<&str>, _: Option<&str>) -> Result<api::sessions::SendTarget, String> {
                Err("offline".into())
            }
            fn note_key(&self, _: &str, _: u32, _: [u8; 32]) {}
        }
        let fx: Value = serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json")).unwrap();
        assert_eq!(sealing::NO_KEY_REFUSAL, fx["clearTree"]["noKeyRefusal"].as_str().unwrap());
        for target in [SealTarget::Root("ede97-keyless".into()), SealTarget::NewRoot { device_id: None }] {
            let mut input = SendInput { text: Some("secret".into()), ..SendInput::default() };
            assert_eq!(seal_send(&NoKeys, &mut input, &target), Err(sealing::NO_KEY_REFUSAL.to_string()), "{target:?}");
            assert!(input.seal.is_none(), "{target:?}");
        }
        sealing::note_clear_tree("ede97-clear");
        let mut input = SendInput { text: Some("in clear".into()), ..SendInput::default() };
        assert_eq!(seal_send(&NoKeys, &mut input, &SealTarget::Root("ede97-clear".into())), Ok(None));
        assert_eq!(input.text.as_deref(), Some("in clear"));
    }

    /// #EXP-1960: a server-side tree the store lacks (`tree_of`), whether
    /// this install may key it, and every server call and send it saw.
    struct ServerTree {
        key: contract::TreeKey,
        /// `sessions get`'s answer: `None` = not found / offline.
        facts: Option<TreeFacts>,
        may_key: bool,
        calls: Mutex<Vec<String>>,
    }

    /// Real-shaped ids: a pre-#EDE-97 root and a child of it.
    const ROOT: &str = "6f1c2a4e-3b7d-4c1e-9a52-0d8e7f6b5a41";
    const CHILD: &str = "a83d5c10-7e2f-4b96-8c3a-51f0e9d2b7c4";

    impl ServerTree {
        fn new(clear: bool, keyed: bool, may_key: bool) -> Self {
            let key = contract::generate_tree_key();
            let facts = TreeFacts { root_id: ROOT.into(), clear, key: keyed.then_some((3, key.public)) };
            ServerTree { key, facts: Some(facts), may_key, calls: Mutex::new(Vec::new()) }
        }
        fn call(&self, what: String) {
            self.calls.lock().unwrap().push(what);
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
        fn opens(&self, epoch: u32, body: &str) -> Option<String> {
            contract::open_body(body, &|e| (e == epoch).then_some(self.key.secret)).text
        }
    }

    impl Writer for ServerTree {
        /// `ensure_key` on a keyless tree: `sessionKeys.create` when this
        /// install is a reader, `NotReader` otherwise.
        fn seal_body(&self, root_id: &str, text: &str) -> Result<String, String> {
            if !self.may_key {
                return Err(format!("{root_id}: not a reader"));
            }
            self.call(format!("create {root_id}"));
            contract::seal_body(&self.key.public, 1, text).map_err(|e| e.to_string())
        }
        fn seal_card(&self, _: &str, _: &Value) -> Result<Value, String> {
            unreachable!()
        }
        fn new_root(&self, _: Option<&str>) -> Result<NewRootSeal, String> {
            self.call("new root".into());
            Ok(NewRootSeal { install_id: "me".into(), public_key: self.key.public_key(), epoch: 1, wraps: Vec::new(), secret: self.key.secret })
        }
        fn adopt(&self, _: &str, _: &NewRootSeal) {}
        fn send_target(&self, _: Option<&str>, _: Option<&str>) -> Result<api::sessions::SendTarget, String> {
            unreachable!()
        }
        fn note_key(&self, root_id: &str, epoch: u32, _: [u8; 32]) {
            self.call(format!("key {root_id}@{epoch}"));
        }
        fn tree_of(&self, session_id: &str) -> Result<TreeFacts, String> {
            self.call(format!("get {session_id}"));
            self.facts.clone().ok_or_else(|| "NOT_FOUND".into())
        }
    }

    fn deliver_to(writer: &ServerTree, input: &SendInput, root_of: &dyn Fn(&str) -> Option<String>) -> (Result<SendOutput, api::ApiError>, Vec<SendInput>) {
        let mut sent = Vec::new();
        let target = target_for(input, root_of, || None);
        let out = deliver(writer, input, &target, true, &mut |wire| {
            sent.push(wire.clone());
            Ok(SendOutput { session_id: wire.session_id.clone().unwrap_or_default(), ..SendOutput::default() })
        });
        (out, sent)
    }

    fn unsynced(_: &str) -> Option<String> {
        None
    }

    /// #EXP-1958 (`cardStamps` NEVER IN CLEAR): a sealed card's tap hands
    /// OPENED text to the writer; into a clear tree it is refused before any
    /// request, into a keyed one it leaves as an envelope with its item.
    #[test]
    fn a_sealed_card_tap_never_leaves_in_clear() {
        let tap = SendInput::card_message(CHILD, "0b6f4a8e-51c3-4d27-9e1a-7c2d3f4a5b6c", "c2a9e7d4-18f3-4b65-a0c7-9e4d2b1f6a83", "Yes, ship it");
        // Its own root: a clear tree is noted process-wide (`note_clear_tree`).
        let mut clear = ServerTree::new(true, false, true);
        clear.facts.as_mut().unwrap().root_id = "d41f7b92-6a3e-4c58-b1d0-2e9f8a7c6b35".into();
        let (out, sent) = deliver_to(&clear, &tap, &unsynced);
        assert!(is_no_key_refusal(&out), "{out:?}");
        assert!(sent.is_empty(), "the opened reply left in clear: {sent:?}");
        // The same tree still takes an ordinary message as given.
        let (out, sent) = deliver_to(&clear, &SendInput::message(CHILD, "hi", Vec::new()), &unsynced);
        assert!(out.is_ok());
        assert_eq!(sent.len(), 1);

        let keyed = ServerTree::new(false, true, true);
        let (out, sent) = deliver_to(&keyed, &tap, &unsynced);
        assert!(out.is_ok(), "{out:?}");
        assert_eq!(sent.len(), 1);
        assert!(sent[0].text.as_deref().unwrap().starts_with(contract::SEALED_BODY_PREFIX));
        assert_eq!(sent[0].card_item, tap.card_item);

        // The guard alone: an envelope passes, a clear `name` beside it or a
        // clear text does not; a send that is no card tap is never its business.
        let envelope = SendInput { text: Some(format!("{}x", contract::SEALED_BODY_PREFIX)), ..tap.clone() };
        assert!(card_tap_leaves_sealed(&envelope).is_ok());
        assert!(card_tap_leaves_sealed(&SendInput { name: Some("Add tests".into()), ..envelope.clone() }).is_err());
        assert!(card_tap_leaves_sealed(&tap).is_err());
        assert!(card_tap_leaves_sealed(&SendInput { text: None, ..tap.clone() }).is_err());
        assert!(card_tap_leaves_sealed(&SendInput::message(CHILD, "hi", Vec::new())).is_ok());
    }

    fn is_no_key_refusal(out: &Result<SendOutput, api::ApiError>) -> bool {
        matches!(out, Err(api::ApiError::Http { status: 400, message }) if message == sealing::NO_KEY_REFUSAL)
    }

    #[test]
    fn a_session_the_store_lacks_is_never_a_new_chat_nor_plaintext() {
        let child = SendInput::message(CHILD, "hi", Vec::new());
        assert_eq!(target_for(&child, &unsynced, || None), SealTarget::Unsynced(CHILD.into()));
        let spawn = SendInput::child(CHILD, "dev-1", "go");
        assert_eq!(target_for(&spawn, &unsynced, || None), SealTarget::Unsynced(CHILD.into()));
        // Only a send with nothing to seal stays `None`.
        let bare = SendInput { session_id: Some(CHILD.into()), ..SendInput::default() };
        assert_eq!(target_for(&bare, &unsynced, || None), SealTarget::None);
        // Asked of nobody it is refused, never sent.
        let mut input = child.clone();
        assert_eq!(seal_send(&ServerTree::new(false, true, true), &mut input, &SealTarget::Unsynced(CHILD.into())), Err(sealing::NO_KEY_REFUSAL.to_string()));
    }

    #[test]
    fn a_child_of_a_keyed_root_missing_from_the_store_seals_to_the_roots_key() {
        let writer = ServerTree::new(false, true, false);
        let input = SendInput::message(CHILD, "secret", Vec::new());
        let (out, sent) = deliver_to(&writer, &input, &unsynced);
        assert_eq!(out.unwrap().session_id, CHILD);
        assert_eq!(writer.calls(), vec![format!("get {CHILD}"), format!("key {ROOT}@3")]);
        let [wire] = sent.as_slice() else { panic!("{sent:?}") };
        assert_eq!(wire.session_id.as_deref(), Some(CHILD));
        assert!(wire.seal.is_none(), "an existing tree never takes an inline key");
        assert_eq!(writer.opens(3, wire.text.as_deref().unwrap()).as_deref(), Some("secret"));
    }

    #[test]
    fn a_keyless_pre_ede97_root_missing_from_the_store_is_keyed_or_refused() {
        let writer = ServerTree::new(false, false, true);
        let input = SendInput::message(ROOT, "secret", Vec::new());
        let (out, sent) = deliver_to(&writer, &input, &unsynced);
        assert!(out.is_ok());
        assert_eq!(writer.calls(), vec![format!("get {ROOT}"), format!("create {ROOT}")]);
        let [wire] = sent.as_slice() else { panic!("{sent:?}") };
        assert!(wire.seal.is_none());
        assert_eq!(writer.opens(1, wire.text.as_deref().unwrap()).as_deref(), Some("secret"));

        // This install may not key it: the NO-KEY refusal, nothing leaves.
        let writer = ServerTree::new(false, false, false);
        let (out, sent) = deliver_to(&writer, &SendInput::message(CHILD, "secret", Vec::new()), &unsynced);
        assert!(is_no_key_refusal(&out), "{out:?}");
        assert!(sent.is_empty());
    }

    #[test]
    fn a_clear_tree_missing_from_the_store_goes_plaintext() {
        let writer = ServerTree { key: contract::generate_tree_key(), facts: Some(TreeFacts { root_id: "0b7e4d2a-91c3-4f58-a6e0-3c2d1b9f8e75".into(), clear: true, key: None }), may_key: true, calls: Mutex::new(Vec::new()) };
        let input = SendInput::message(CHILD, "in clear", Vec::new());
        let (out, sent) = deliver_to(&writer, &input, &unsynced);
        assert!(out.is_ok());
        assert_eq!(writer.calls(), vec![format!("get {CHILD}")]);
        assert_eq!(sent, vec![input]);
        assert!(sealing::tree_clear("0b7e4d2a-91c3-4f58-a6e0-3c2d1b9f8e75"));
    }

    #[test]
    fn a_tree_the_server_cannot_name_fails_with_the_no_key_refusal() {
        let writer = ServerTree { facts: None, ..ServerTree::new(false, true, true) };
        let (out, sent) = deliver_to(&writer, &SendInput::message(CHILD, "secret", Vec::new()), &unsynced);
        assert!(is_no_key_refusal(&out), "{out:?}");
        assert!(sent.is_empty());
    }

    #[test]
    fn a_session_the_store_has_seals_as_before_without_asking() {
        let writer = ServerTree::new(false, false, true);
        let synced = |id: &str| (id == CHILD).then(|| ROOT.to_string());
        let input = SendInput::message(CHILD, "secret", Vec::new());
        assert_eq!(target_for(&input, &synced, || None), SealTarget::Root(ROOT.into()));
        let (out, sent) = deliver_to(&writer, &input, &synced);
        assert!(out.is_ok());
        assert_eq!(writer.calls(), vec![format!("create {ROOT}")], "no server lookup");
        assert_eq!(writer.opens(1, sent[0].text.as_deref().unwrap()).as_deref(), Some("secret"));
    }
}
