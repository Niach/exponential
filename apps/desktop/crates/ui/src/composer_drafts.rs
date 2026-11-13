//! #FEED-219 (fixture `composer-draft.json`): the desktop's writer for
//! composer drafts, web `lib/composer-drafts.ts` + `hooks/use-composer-draft.ts`'s
//! twin. Every composer's field mirrors into the caller's synced
//! `composer_drafts` row for its target key, so a tab switch, a restart or
//! another device shows the text again.
//!
//! The state is pure [`domain::composer_draft::DraftWriter`] in an app
//! global: an edit is written `debounceMs` after the last keystroke, at once
//! when the composer goes away ([`flush`]) or a send settles ([`write_now`]).
//! A failed write is silent. [`DraftField`] is the per-composer half: it
//! decides what the field shows ([`domain::composer_draft::field`]) until
//! the person edits it.

use std::time::Instant;

use domain::composer_draft::{self, DraftWriter, Target, Write, WriteJob};
use gpui::{App, Global, Task};

use sync::Store;

#[derive(Default)]
struct ComposerDrafts {
    writer: DraftWriter,
    /// The pending debounce timer and when it fires.
    timer: Option<(Instant, Task<()>)>,
}

impl Global for ComposerDrafts {}

/// The synced row's text for `key` (None = no row).
pub(crate) fn synced(key: &str, cx: &App) -> Option<String> {
    let store = Store::try_global(cx)?;
    store
        .collections()
        .composer_drafts
        .read(cx)
        .iter()
        .find(|row| row.key.as_deref() == Some(key))
        .and_then(|row| row.text.clone())
}

/// This client's own write for `key` not yet echoed back.
pub(crate) fn local(key: &str, cx: &App) -> Option<String> {
    cx.try_global::<ComposerDrafts>()?.writer.local(key).map(str::to_string)
}

/// What an UNTOUCHED field for `key` shows (local, else synced, else empty).
pub(crate) fn shown(key: &str, cx: &App) -> String {
    composer_draft::field("", false, local(key, cx).as_deref(), synced(key, cx).as_deref())
}

/// Record the field's text; written after the debounce.
pub(crate) fn edit(key: &str, team_id: &str, text: &str, cx: &mut App) {
    cx.default_global::<ComposerDrafts>().writer.edit(key, team_id, text, Instant::now());
    schedule(cx);
}

/// Record the field's text and write it at once (a send settled, a discard).
pub(crate) fn write_now(key: &str, team_id: &str, text: &str, cx: &mut App) {
    cx.default_global::<ComposerDrafts>().writer.edit(key, team_id, text, Instant::now());
    flush(key, cx);
}

/// Write the key's pending text now (the composer goes away).
pub(crate) fn flush(key: &str, cx: &mut App) {
    if cx.try_global::<ComposerDrafts>().is_none() {
        return;
    }
    if let Some(job) = cx.global_mut::<ComposerDrafts>().writer.flush(key) {
        run(job, cx);
    }
    schedule(cx);
}

/// Every pending write now (the window goes away).
pub(crate) fn flush_all(cx: &mut App) {
    if cx.try_global::<ComposerDrafts>().is_none() {
        return;
    }
    let jobs = cx.global_mut::<ComposerDrafts>().writer.flush_all();
    for job in jobs {
        run(job, cx);
    }
}

/// Drop the local copy of `key` once the shape echoed it (called when the
/// composer_drafts collection changes).
pub(crate) fn reconcile(key: &str, cx: &mut App) {
    if cx.try_global::<ComposerDrafts>().is_none() {
        return;
    }
    let synced = synced(key, cx);
    cx.global_mut::<ComposerDrafts>().writer.observe_synced(key, synced.as_deref());
}

fn schedule(cx: &mut App) {
    let Some(next) = cx.global::<ComposerDrafts>().writer.next_due() else {
        cx.global_mut::<ComposerDrafts>().timer = None;
        return;
    };
    if cx.global::<ComposerDrafts>().timer.as_ref().is_some_and(|(at, _)| *at == next) {
        return;
    }
    let task = cx.spawn(async move |cx| {
        let wait = next.saturating_duration_since(Instant::now());
        cx.background_executor().timer(wait).await;
        cx.update(|cx| {
            let drafts = cx.global_mut::<ComposerDrafts>();
            drafts.timer = None;
            let jobs = drafts.writer.due(Instant::now());
            for job in jobs {
                run(job, cx);
            }
            schedule(cx);
        });
    });
    cx.global_mut::<ComposerDrafts>().timer = Some((next, task));
}

/// One write over tRPC (blocking; run off the foreground).
fn call_api(trpc: &api::TrpcClient, job: &WriteJob) -> Result<(), api::ApiError> {
    match &job.write {
        Write::Upsert(text) => {
            api::composer_drafts::composer_drafts_upsert(trpc, &job.team_id, &job.key, text).map(|_| ())
        }
        Write::Delete => api::composer_drafts::composer_drafts_delete(trpc, &job.key).map(|_| ()),
    }
}

fn run(job: WriteJob, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        // Signed out: the text stays local; the next write retries.
        cx.global_mut::<ComposerDrafts>().writer.settled(&job, false);
        return;
    };
    cx.spawn(async move |cx| {
        let call = job.clone();
        let result = cx
            .background_executor()
            .spawn(async move { call_api(&trpc, &call) })
            .await;
        if let Err(err) = &result {
            log::info!("[composer-drafts] write {} failed: {err}", job.key);
        }
        cx.update(|cx| {
            let next = cx.global_mut::<ComposerDrafts>().writer.settled(&job, result.is_ok());
            if let Some(next) = next {
                run(next, cx);
            }
            reconcile(&job.key, cx);
        });
    })
    .detach();
}

/// A send landed for `key` while its composer was gone or showing another
/// target: apply composer-send.json to the key's draft and write what it
/// leaves (the sent text must not stay a draft).
pub(crate) fn settle_elsewhere(key: &str, team_id: &str, sent: &str, cx: &mut App) {
    let current = shown(key, cx);
    let after = domain::composer_send::text_after_send(sent, &current);
    if after != current {
        write_now(key, team_id, &after, cx);
    }
}

/// One composer's field state: its key, its team and whether the person has
/// edited it since it opened (the fixture's `edited`). The key is known
/// before the team (an issue whose board has not synced yet): the field
/// already follows the draft, writes wait for the team.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DraftField {
    key: Option<String>,
    team_id: Option<String>,
    edited: bool,
    /// The field's last text, written once the team resolves.
    text: Option<String>,
}

impl DraftField {
    /// A field bound to `target` (None = a plain field).
    pub(crate) fn new(target: Option<Target>, team_id: Option<String>) -> Self {
        Self { key: target.map(|t| t.key()), team_id, edited: false, text: None }
    }

    pub(crate) fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    pub(crate) fn team_id(&self) -> Option<&str> {
        self.team_id.as_deref()
    }

    /// Re-point at another target: the old key's pending text is written
    /// now, the new field starts untouched. Returns whether it changed.
    pub(crate) fn retarget(&mut self, target: Option<Target>, team_id: Option<String>, cx: &mut App) -> bool {
        let key = target.map(|t| t.key());
        if key == self.key {
            self.set_team(team_id, cx);
            return false;
        }
        self.leave(cx);
        *self = Self { key, team_id, edited: false, text: None };
        true
    }

    /// The target's team resolved (or changed): a text typed before it is
    /// written now.
    pub(crate) fn set_team(&mut self, team_id: Option<String>, cx: &mut App) {
        if team_id == self.team_id {
            return;
        }
        self.team_id = team_id;
        if let (Some(key), Some(team), Some(text)) = (&self.key, &self.team_id, &self.text) {
            edit(key, team, text, cx);
        }
    }

    /// The text an untouched field shows; None = the field is the person's
    /// (or unbound) and must not be overwritten.
    pub(crate) fn follow(&self, cx: &App) -> Option<String> {
        if self.edited {
            return None;
        }
        self.key.as_deref().map(|key| shown(key, cx))
    }

    /// The person changed the field's text.
    pub(crate) fn edited(&mut self, text: &str, cx: &mut App) {
        self.edited = true;
        self.text = Some(text.to_string());
        if let (Some(key), Some(team)) = (&self.key, &self.team_id) {
            edit(key, team, text, cx);
        }
    }

    /// Write `text` at once: a send settled (composer-send.json already
    /// applied) or a discard (`""`).
    pub(crate) fn write_now(&mut self, text: &str, cx: &mut App) {
        self.edited = true;
        self.text = Some(text.to_string());
        if let (Some(key), Some(team)) = (&self.key, &self.team_id) {
            write_now(key, team, text, cx);
        }
    }

    /// The composer goes away: write what is pending now.
    pub(crate) fn leave(&self, cx: &mut App) {
        if let Some(key) = &self.key {
            flush(key, cx);
        }
    }

    /// The shape changed: drop an echoed local copy.
    pub(crate) fn reconcile(&self, cx: &mut App) {
        if let Some(key) = &self.key {
            reconcile(key, cx);
        }
    }
}

/// Show an untouched field's draft in `input` (no `InputEvent::Change`:
/// `set_value` emits none, so the field stays untouched). Returns whether the
/// text changed.
pub(crate) fn apply_follow(
    field: &DraftField,
    input: &gpui::Entity<gpui_component::input::TextareaState>,
    window: &mut gpui::Window,
    cx: &mut App,
) -> bool {
    let Some(text) = field.follow(cx) else {
        return false;
    };
    if input.read(cx).value().as_ref() == text.as_str() {
        return false;
    }
    input.update(cx, |input, cx| input.set_value(text, window, cx));
    true
}

/// The app quits: every pending write goes out on the quit's own deadline
/// (the regular spawned writes would die with the foreground).
pub(crate) fn on_quit(cx: &mut App) -> Task<()> {
    let jobs = if cx.try_global::<ComposerDrafts>().is_some() {
        cx.global_mut::<ComposerDrafts>().writer.flush_all()
    } else {
        Vec::new()
    };
    let trpc = crate::queries::trpc_client(cx);
    cx.background_executor().spawn(async move {
        let Some(trpc) = trpc else { return };
        for job in jobs {
            if let Err(err) = call_api(&trpc, &job) {
                log::info!("[composer-drafts] quit write {} failed: {err}", job.key);
            }
        }
    })
}
