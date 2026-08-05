//! Full-page action detail (EXP-277) — the Actions tool-window rows' center
//! screen, built on the ISSUE DETAIL's shape (EXP-417): a fixed header
//! (title, one-line description, the Run/repository/icon controls and the
//! typed input definitions) over the scrolling markdown prompt.
//!
//! Data model: the synced `actions` shape carries the body-less row (name,
//! description, icon, inputs, repo binding); the prompt body is tRPC-only
//! (`actions.get` — EXP-268), fetched per [`ActionDetailView::set_action`]
//! and refetched when the synced row's `updated_at` moves (covers remote/MCP
//! edits).
//!
//! **EXP-282 — this screen IS the action editor.** The raw editor dialog is
//! gone; owners edit everything in place, each field mutating on its own
//! through `actions.update` (the issue-detail contract — no submit button, no
//! dirty dialog): name and description are borderless inputs saved on
//! blur/Enter, and the icon picker, the Repository picker and the input
//! definition rows save immediately. Non-owners get the same screen read-only.
//!
//! **EXP-298 — the prompt uses the SAME editor as an issue description** (the
//! vendored WYSIWYG, [`crate::wysiwyg`]), inline and always live for owners;
//! the Edit/Done source-field toggle is gone. Two consequences of that choice,
//! both accepted deliberately:
//!   * the vendored serializer normalizes render-equivalent markdown on save
//!     (setext→ATX, `_i_`→`*i*`, `1)`→`1.`, intra-word `*` escaping, and
//!     defensive escapes like `## 0. Scope` → `## 0\. Scope`), so an edited
//!     prompt can come back a few bytes different. Everything in the GFM
//!     contract — tables, fences, task lists included — round-trips byte-exact
//!     (`markdown/wysiwyg_parity.rs` is the gate), so the agent still receives
//!     the same program. A no-edit visit never writes: the save path is gated
//!     on the editor's revision counter, not on a byte diff.
//!   * an action owns no attachments, so the editor runs in staging mode
//!     (`upload_issue: None`) and a pasted IMAGE is stripped by
//!     `markdown_for_save` instead of being uploaded. Prompts are agent-facing
//!     text; images have no meaning in them.
//!
//! **EXP-298 — builtins open here too, fully read-only.** The two virtual
//! builtins are not DB rows (`actions.get/update/delete` reject their ids), so
//! this view constructs them locally like every other client surface does and
//! renders `coding::action_prompt::builtin_prompt_preview` as the prompt — the
//! REAL shipped prompt with placeholder tokens where the runner substitutes
//! run-time values, so the screen can never drift from what a run sends.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, AppContext as _, ClickEvent, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{self, Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    popover::Popover,
    skeleton::Skeleton,
    text::TextView,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;

use crate::action_run::{fetch_repositories, ActionRepoRow};
use crate::icons::{registry, ExpIcon};
use crate::issue_detail::{
    centered_column, centered_scroll_column, DETAIL_GUTTER, WYSIWYG_BLOCK_PADDING_X,
};
use crate::issue_header::group_label;
use crate::navigation::{active_team_id, nav_for_window, Navigation};
use crate::pickers::chip_button;
use crate::queries;
use crate::surface::glass_chip;
use crate::wysiwyg::WysiwygDescription;

/// The server's `MAX_ACTION_INPUTS` cap (`@exp/db-schema/domain`). The label
/// fields are a fixed pool of this size: [`ActionDetailView::set_action`] has
/// no `Window`, so per-row `InputState`s can't be built on demand — building
/// them all once in `new` is the honest way to keep the seam window-free.
const MAX_ACTION_INPUTS: usize = 10;

/// One editable input DEFINITION row (EXP-282). The label lives in the
/// matching pool field; `key` is the row's stable server key — empty on a
/// freshly added row, derived from the label at the first save and then kept.
#[derive(Clone)]
struct InputDraft {
    key: String,
    input_type: String,
    required: bool,
    /// Round-tripped untouched — the desktop has no placeholder field, but
    /// dropping one an MCP/web author set would be a silent data loss.
    placeholder: Option<String>,
}

pub struct ActionDetailView {
    /// The window's navigation — the team scope the two builtins are
    /// constructed against (they carry no synced row to read it from).
    nav: Entity<Navigation>,
    action_id: Option<String>,
    /// The prompt body (`None` = in flight; a builtin's is the local preview).
    body: Option<SharedString>,
    body_error: Option<SharedString>,
    /// The synced row's `updated_at` at fetch time — a moved value on the
    /// collection observer triggers a refetch.
    fetched_updated_at: Option<String>,
    /// The team's repo rows (the Repository chip's full name + the picker).
    repos: Vec<ActionRepoRow>,
    /// The repo fetch landed. Until it does the picker stays a read-only
    /// chip — an empty list must never let an owner clear the binding.
    repos_loaded: bool,
    body_scroll: gpui::ScrollHandle,

    // -- EXP-282 inline editing ------------------------------------------
    name_input: Entity<InputState>,
    description_input: Entity<InputState>,
    /// What was last PUSHED into each field from the row (`None` = not seeded
    /// yet for this action). The echo guard: re-seeding only when the server
    /// value moves is what keeps a remote echo from clobbering the characters
    /// the owner is typing.
    seeded_name: Option<String>,
    seeded_description: Option<String>,
    /// EXP-298: the WYSIWYG prompt editor, built once per EDITABLE action
    /// (`None` for read-only viewers and builtins — they render the prompt as
    /// markdown). Mirrors `IssueDetailView`'s editor/editor_issue pair.
    body_editor: Option<Entity<WysiwygDescription>>,
    editor_action: Option<String>,
    /// Last prompt bytes we saved or synced — dedupes echoes (the issue
    /// detail's `last_saved_description`).
    last_saved_body: Rc<RefCell<String>>,
    /// Label fields for the input definitions — index-aligned with
    /// [`Self::input_drafts`], sized to [`MAX_ACTION_INPUTS`].
    input_labels: Vec<Entity<InputState>>,
    input_drafts: Vec<InputDraft>,
    /// The definitions the drafts were seeded from — a moved value reseeds.
    /// Set to what we SEND on every save, so our own echo is a no-op and a
    /// half-typed new row survives it.
    drafts_source: Option<Vec<api::actions::ActionInput>>,
    /// Last write failure (duplicate name, empty body, offline…).
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl ActionDetailView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Action name")
                .auto_grow(1, 3)
                .submit_on_enter(true)
        });
        // EXP-298: auto-grow so a long one-liner WRAPS — it used to clip at
        // the column edge mid-sentence.
        let description_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Add a one-line description…")
                .auto_grow(1, 3)
                .submit_on_enter(true)
        });
        let input_labels: Vec<Entity<InputState>> = (0..MAX_ACTION_INPUTS)
            .map(|_| cx.new(|cx| InputState::new(window, cx).placeholder("Label")))
            .collect();

        let nav = nav_for_window(window, cx);
        let mut subscriptions = Vec::new();
        subscriptions.push(cx.subscribe_in(
            &name_input,
            window,
            |this, _, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    this.save_name(cx);
                }
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &description_input,
            window,
            |this, _, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    this.save_description(cx);
                }
            },
        ));
        for label in &input_labels {
            subscriptions.push(cx.subscribe_in(
                label,
                window,
                |this, _, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                        this.save_inputs(cx);
                    }
                },
            ));
        }
        if let Some(store) = sync::Store::try_global(cx) {
            let actions = store.collections().actions.clone();
            subscriptions.push(cx.observe(&actions, |this, _, cx| {
                this.refetch_if_stale(cx);
                cx.notify();
            }));
        }
        // A team switch re-scopes the locally constructed builtins.
        subscriptions.push(cx.observe(&nav, |_, _, cx| cx.notify()));

        Self {
            nav,
            action_id: None,
            body: None,
            body_error: None,
            fetched_updated_at: None,
            repos: Vec::new(),
            repos_loaded: false,
            body_scroll: gpui::ScrollHandle::new(),
            name_input,
            description_input,
            seeded_name: None,
            seeded_description: None,
            body_editor: None,
            editor_action: None,
            last_saved_body: Rc::new(RefCell::new(String::new())),
            input_labels,
            input_drafts: Vec::new(),
            drafts_source: None,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    /// Point the view at an action (the screens panel calls this on
    /// navigation, never mid-render). Same-id re-points keep state.
    pub fn set_action(&mut self, action_id: String, cx: &mut gpui::Context<Self>) {
        if self.action_id.as_deref() == Some(action_id.as_str()) {
            return;
        }
        // Commit in-flight edits to the OUTGOING action first (issue detail's
        // EXP-68 lesson): re-pointing the view never blurs the focused field,
        // so the characters would just be dropped when the seeds reset below.
        self.save_name(cx);
        self.save_description(cx);
        self.flush_body(cx);
        self.save_inputs(cx);
        self.action_id = Some(action_id.clone());
        // A builtin has no fetchable body — its prompt is the shipped preview.
        self.body = coding::action_prompt::builtin_prompt_preview(&action_id).map(SharedString::from);
        self.body_error = None;
        self.fetched_updated_at = None;
        self.repos = Vec::new();
        self.repos_loaded = false;
        // Every editor belongs to the PREVIOUS action — drop the seeds so the
        // next render refills the fields from the new row (this entry point
        // has no `Window`, so the actual `set_value`s happen in `render`).
        self.seeded_name = None;
        self.seeded_description = None;
        self.body_editor = None;
        self.editor_action = None;
        *self.last_saved_body.borrow_mut() = String::new();
        self.input_drafts = Vec::new();
        self.drafts_source = None;
        self.error = None;
        // The scroll offset belongs to the previous action (the shared-
        // instance rule — issue detail's EXP-67 lesson).
        self.body_scroll
            .set_offset(gpui::point(gpui::px(0.), gpui::px(0.)));
        self.fetch_body(cx);
        self.fetch_repos(cx);
        cx.notify();
    }

    /// The action this screen shows. Real actions come from the synced shape;
    /// the two builtins are constructed locally against the window's team
    /// (EXP-298 — they are not DB rows, exactly like every other client's
    /// action list does it).
    fn action(&self, cx: &App) -> Option<api::actions::Action> {
        let action_id = self.action_id.as_deref()?;
        if api::actions::is_builtin_action_id(action_id) {
            let team_id = active_team_id(&self.nav, cx)?;
            return Some(if action_id == api::actions::BUILTIN_FIX_CONFLICTS_ID {
                api::actions::builtin_fix_conflicts_action(&team_id)
            } else {
                api::actions::builtin_create_action(&team_id)
            });
        }
        Store::global(cx)
            .collections()
            .actions
            .read(cx)
            .get(action_id)
            .map(api::actions::from_row)
    }

    /// The action if the current viewer may WRITE it: a real synced row in a
    /// team they own. EVERY save path goes through this, so a read-only
    /// screen (member, or a product-shipped builtin) cannot mutate anything
    /// even through a stale field blur.
    fn editable_action(&self, cx: &App) -> Option<api::actions::Action> {
        let action = self.action(cx)?;
        if action.builtin {
            return None;
        }
        crate::settings::is_owner(cx, &action.team_id).then_some(action)
    }

    /// Refetch the body when the synced row's `updated_at` moved past the
    /// fetched one (our own prompt save, remote/MCP edit).
    fn refetch_if_stale(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(action) = self.action(cx) else {
            return;
        };
        if self.fetched_updated_at.is_some() && action.updated_at != self.fetched_updated_at {
            self.fetch_body(cx);
        }
    }

    /// One guarded `actions.get`: the landing closure re-checks the id so a
    /// stale response for a previous action never repaints the current one.
    fn fetch_body(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(action_id) = self.action_id.clone() else {
            return;
        };
        if api::actions::is_builtin_action_id(&action_id) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.body_error = None;
        self.fetched_updated_at = self.action(cx).and_then(|action| action.updated_at.clone());
        let fetch_id = action_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::actions::get(&trpc, &fetch_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.action_id.as_deref() != Some(action_id.as_str()) {
                    return;
                }
                match result {
                    Ok(action) => {
                        this.body = Some(SharedString::from(action.body));
                        this.fetched_updated_at = action.updated_at;
                    }
                    Err(err) => {
                        this.body_error =
                            Some(SharedString::from(format!("Could not load the prompt: {err}")));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The team repo rows for the Repository chip/picker. Failure degrades to
    /// the read-only chip — never blocks the screen, and never lets the
    /// picker offer an empty list that could clear the binding.
    fn fetch_repos(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.action(cx).map(|action| action.team_id) else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let rows = cx
                .background_executor()
                .spawn(async move { fetch_repositories(&trpc, &team_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Ok(rows) = rows {
                    this.repos = rows;
                    this.repos_loaded = true;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    // -- writes ------------------------------------------------------------

    /// One un-gated `actions.update` (the issue-detail contract: mutate now,
    /// let the Electric echo re-render). Failures surface under the header.
    fn spawn_update(&mut self, input: api::actions::ActionUpdate, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };
        self.error = None;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::actions::update(&trpc, &input).map(|_| ()) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(err) = result {
                    this.error = Some(SharedString::from(format!("{err}")));
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn save_name(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(action) = self.editable_action(cx) else {
            return;
        };
        // A name is one logical line; pasted newlines collapse (the issue
        // title's EXP-230 rule).
        let name = self
            .name_input
            .read(cx)
            .value()
            .replace(['\r', '\n'], " ")
            .trim()
            .to_string();
        if name.is_empty() || name == action.name {
            return;
        }
        self.seeded_name = Some(name.clone());
        let mut input = api::actions::ActionUpdate::new(action.id);
        input.name = Some(name);
        self.spawn_update(input, cx);
    }

    fn save_description(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(action) = self.editable_action(cx) else {
            return;
        };
        let description = self
            .description_input
            .read(cx)
            .value()
            .replace(['\r', '\n'], " ")
            .trim()
            .to_string();
        if description == action.description.clone().unwrap_or_default() {
            return;
        }
        self.seeded_description = Some(description.clone());
        let mut input = api::actions::ActionUpdate::new(action.id);
        input.description = Some(description);
        self.spawn_update(input, cx);
    }

    /// EXP-273/EXP-298: the curated glyph, saved the moment it is picked.
    fn save_icon(&mut self, icon: &str, cx: &mut gpui::Context<Self>) {
        let Some(action) = self.editable_action(cx) else {
            return;
        };
        if action.icon.as_deref() == Some(icon) {
            return;
        }
        let mut input = api::actions::ActionUpdate::new(action.id);
        input.icon = Some(icon.to_string());
        self.spawn_update(input, cx);
    }

    /// The prompt editor's save hook (blur, or an explicit editor save).
    /// A blank body is refused locally (the server's `bodySchema` rejects it)
    /// instead of round-tripping a 400 the owner can't act on.
    ///
    /// Deliberately NOT trimmed — leading/trailing markdown whitespace is
    /// prompt content, and the server takes the same stance.
    fn save_body(&mut self, body: String, cx: &mut gpui::Context<Self>) {
        let Some(action) = self.editable_action(cx) else {
            return;
        };
        if body == *self.last_saved_body.borrow() {
            return;
        }
        if body.trim().is_empty() {
            self.error = Some("The prompt must not be empty.".into());
            cx.notify();
            return;
        }
        *self.last_saved_body.borrow_mut() = body.clone();
        self.body = Some(SharedString::from(body.clone()));
        let mut input = api::actions::ActionUpdate::new(action.id);
        input.body = Some(body);
        self.spawn_update(input, cx);
    }

    /// Flush a pending (un-blurred) prompt edit (the issue detail's EXP-68
    /// `flush_description`): the editor saves on blur, but a tab close / view
    /// re-point / team switch tears its element out of the tree without a blur
    /// ever firing. Every such path routes through here first; a clean editor
    /// is a no-op.
    pub(crate) fn flush_body(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(editor) = self.body_editor.clone() else {
            return;
        };
        // EXP-261: no user edit → nothing to flush. The serializer normalizes
        // render-equivalent markdown, so a byte diff alone proves nothing.
        if !editor.read(cx).is_dirty(cx) {
            return;
        }
        // EXP-261: a paste still uploading when the tab closes must not
        // persist its `draft://` placeholder.
        let body =
            crate::markdown::image_paste::markdown_for_save(editor.read(cx).markdown(cx));
        editor.update(cx, |editor, cx| editor.mark_clean(cx));
        self.save_body(body, cx);
    }

    fn save_repository(&mut self, repository_id: Option<String>, cx: &mut gpui::Context<Self>) {
        let Some(action) = self.editable_action(cx) else {
            return;
        };
        if repository_id == action.repository_id {
            return;
        }
        let mut input = api::actions::ActionUpdate::new(action.id);
        input.repository_id = api::Patch::set_or_null(repository_id);
        self.spawn_update(input, cx);
    }

    /// The current definition rows as the server wants them. Blank-label rows
    /// are skipped (a freshly added row is not a definition until it is
    /// named); the returned draft indices let the caller pin the derived keys
    /// back onto the drafts so they stay stable across later edits.
    fn collect_inputs(&self, cx: &App) -> Vec<(usize, api::actions::ActionInput)> {
        let mut used: HashSet<String> = HashSet::new();
        let mut out = Vec::new();
        for (ix, draft) in self.input_drafts.iter().enumerate() {
            let Some(field) = self.input_labels.get(ix) else {
                break;
            };
            let label = field.read(cx).value().trim().to_string();
            if label.is_empty() {
                continue;
            }
            let base = if draft.key.is_empty() {
                slug_key(&label)
            } else {
                draft.key.clone()
            };
            let mut key = base.clone();
            let mut suffix = 2;
            while !used.insert(key.clone()) {
                key = format!("{base}_{suffix}");
                suffix += 1;
            }
            out.push((
                ix,
                api::actions::ActionInput {
                    key,
                    label,
                    input_type: draft.input_type.clone(),
                    required: draft.required,
                    placeholder: draft.placeholder.clone(),
                },
            ));
        }
        out
    }

    /// Whole-array replace (EXP-282 — the server patches nothing here).
    fn save_inputs(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(action) = self.editable_action(cx) else {
            return;
        };
        let pairs = self.collect_inputs(cx);
        let inputs: Vec<api::actions::ActionInput> =
            pairs.iter().map(|(_, input)| input.clone()).collect();
        if inputs == action.inputs {
            return;
        }
        for (ix, input) in &pairs {
            self.input_drafts[*ix].key = input.key.clone();
        }
        // Our own echo must not reseed the drafts — a half-typed new row
        // would vanish under the owner's cursor.
        self.drafts_source = Some(inputs.clone());
        let mut update = api::actions::ActionUpdate::new(action.id);
        update.inputs = Some(inputs);
        self.spawn_update(update, cx);
    }

    /// Rewrite the label pool from `labels` and blank the tail.
    fn write_label_fields(&self, labels: &[String], window: &mut Window, cx: &mut App) {
        for (ix, field) in self.input_labels.iter().enumerate() {
            let value = labels.get(ix).cloned().unwrap_or_default();
            field.update(cx, |state, cx| state.set_value(value, window, cx));
        }
    }

    fn add_input(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.input_drafts.len() >= MAX_ACTION_INPUTS {
            return;
        }
        let mut labels: Vec<String> = self.current_labels(cx);
        labels.push(String::new());
        self.input_drafts.push(InputDraft {
            key: String::new(),
            input_type: "text".to_string(),
            required: false,
            placeholder: None,
        });
        self.write_label_fields(&labels, window, cx);
        self.input_labels[self.input_drafts.len() - 1]
            .update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    fn remove_input(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if ix >= self.input_drafts.len() {
            return;
        }
        let mut labels = self.current_labels(cx);
        labels.remove(ix);
        self.input_drafts.remove(ix);
        self.write_label_fields(&labels, window, cx);
        self.save_inputs(cx);
        cx.notify();
    }

    fn current_labels(&self, cx: &App) -> Vec<String> {
        (0..self.input_drafts.len())
            .filter_map(|ix| self.input_labels.get(ix))
            .map(|field| field.read(cx).value().to_string())
            .collect()
    }

    /// Push the row's server-side values into the inline editors. Runs from
    /// `render` because it is the only place with a `Window` (the screens
    /// panel's `set_action` has none), and it is a no-op on every frame where
    /// nothing moved — the seed guards make it converge in one pass.
    fn sync_editors(
        &mut self,
        action: &api::actions::Action,
        editable: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.seeded_name.as_deref() != Some(action.name.as_str()) {
            self.seeded_name = Some(action.name.clone());
            let value = action.name.clone();
            self.name_input
                .update(cx, |state, cx| state.set_value(value, window, cx));
        }
        let description = action.description.clone().unwrap_or_default();
        if self.seeded_description.as_deref() != Some(description.as_str()) {
            self.seeded_description = Some(description.clone());
            self.description_input
                .update(cx, |state, cx| state.set_value(description, window, cx));
        }
        if self.drafts_source.as_ref() != Some(&action.inputs) {
            self.drafts_source = Some(action.inputs.clone());
            self.input_drafts = action
                .inputs
                .iter()
                .take(MAX_ACTION_INPUTS)
                .map(|input| InputDraft {
                    key: input.key.clone(),
                    input_type: input.input_type.clone(),
                    required: input.required,
                    placeholder: input.placeholder.clone(),
                })
                .collect();
            let labels: Vec<String> = action
                .inputs
                .iter()
                .take(MAX_ACTION_INPUTS)
                .map(|input| input.label.clone())
                .collect();
            self.write_label_fields(&labels, window, cx);
        }
        self.sync_body_editor(action, editable, window, cx);
    }

    /// Build the prompt editor for an editable action once its body landed,
    /// and forward later remote echoes into it (the issue detail's
    /// `ensure_editor` + `sync_from_issue` pair, fused: this is the only place
    /// with a `Window`).
    fn sync_body_editor(
        &mut self,
        action: &api::actions::Action,
        editable: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !editable {
            // Read-only viewers render markdown; drop an editor left over
            // from a previous action so nothing can save through it.
            self.body_editor = None;
            self.editor_action = None;
            return;
        }
        let Some(body) = self.body.as_ref().map(|body| body.to_string()) else {
            return;
        };
        if self.editor_action.as_deref() != Some(action.id.as_str()) {
            *self.last_saved_body.borrow_mut() = body.clone();
            let view = cx.entity().downgrade();
            let on_save: crate::wysiwyg::OnSave =
                Rc::new(move |markdown: String, _window, cx: &mut App| {
                    if let Some(view) = view.upgrade() {
                        view.update(cx, |this, cx| this.save_body(markdown, cx));
                    }
                });
            let editor = crate::description_editor::build_wysiwyg_editor(
                Some(action.team_id.clone()),
                // No attachment owner for an action — staging mode (see the
                // module docs on pasted images).
                None,
                "The markdown prompt this action runs…",
                &body,
                Some(on_save),
                window,
                cx,
            );
            // EXP-288: the detail body's scroll container follows the caret.
            let scroll = self.body_scroll.clone();
            editor.update(cx, |editor, cx| {
                editor.set_scroll_handle(scroll, cx);
                // EXP-417: the toolbar is pinned in the fixed header instead
                // of riding the scrolling prompt.
                editor.use_external_toolbar();
            });
            self.body_editor = Some(editor);
            self.editor_action = Some(action.id.clone());
            return;
        }
        // A refetch landed (our own save's echo, or a remote/MCP edit): push
        // it in only while the owner is not typing in the editor.
        if body != *self.last_saved_body.borrow() {
            let Some(editor) = self.body_editor.clone() else {
                return;
            };
            if editor.read(cx).is_focused(window, cx) {
                return;
            }
            *self.last_saved_body.borrow_mut() = body.clone();
            editor.update(cx, |editor, cx| editor.set_markdown(&body, window, cx));
        }
    }

    fn not_found(&self, cx: &gpui::Context<Self>) -> gpui::AnyElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Action not found in this team."),
            )
            .into_any_element()
    }

    // -- center column ------------------------------------------------------

    /// The prompt: the WYSIWYG editor for owners, rendered markdown for
    /// everyone else (and for the builtins' shipped preview).
    fn render_prompt(&self, editable: bool, cx: &gpui::Context<Self>) -> gpui::AnyElement {
        if let Some(error) = self.body_error.clone() {
            return div()
                .px(px(DETAIL_GUTTER))
                .text_xs()
                .text_color(cx.theme().danger)
                .child(error)
                .into_any_element();
        }
        if editable {
            if let Some(editor) = self.body_editor.clone() {
                // Issue-detail parity (EXP-282/EXP-285): the vendored editor
                // pads every block by its own 12px, so the slot contributes
                // only the REMAINDER of the shared gutter; the 96px floor
                // keeps a short prompt reading as a text area, and the flex
                // column lets the editor's filler strip absorb the leftover
                // (clicking below the text places the caret at the end).
                // `flex_1` lets the slot take the scroll column's leftover
                // height (centered_scroll_column's single-hop chain), so the
                // filler genuinely reaches the pane bottom.
                return div()
                    .px(px(DETAIL_GUTTER - WYSIWYG_BLOCK_PADDING_X))
                    .min_h(px(96.))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(editor.clone())
                    .into_any_element();
            }
        }
        let Some(body) = self.body.clone() else {
            return div()
                .px(px(DETAIL_GUTTER))
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Loading prompt…")
                .into_any_element();
        };
        div()
            .px(px(DETAIL_GUTTER))
            .text_sm()
            .child(
                TextView::markdown("action-body", body)
                    .style(crate::surface::markdown_style())
                    .selectable(true),
            )
            .into_any_element()
    }

    /// Inputs: editable definition rows for the owner (label field + type
    /// picker + required toggle + remove, plus "Add input"), read-only
    /// `label · type · required` lines otherwise. EXP-298: full-width rows —
    /// the old 192px sidebar could not fit one on a line.
    fn render_inputs(
        &self,
        action: &api::actions::Action,
        editable: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let muted = cx.theme().muted_foreground;
        if !editable {
            if action.inputs.is_empty() {
                return None;
            }
            let mut lines = v_flex().w_full().gap_1();
            for input in &action.inputs {
                lines = lines.child(
                    h_flex()
                        .w_full()
                        .gap_1()
                        .items_center()
                        .text_sm()
                        // No `truncate` on the label: a gpui ellipsis needs a
                        // DEFINITE width all the way down (EXP-175), and in
                        // this content-sized row it collapsed the label to a
                        // bare "…". Labels are short; a long one wraps.
                        .child(div().child(SharedString::from(input.label.clone())))
                        .child(
                            div().flex_shrink_0().text_xs().text_color(muted).child(
                                SharedString::from(format!(
                                    "· {}{}",
                                    input.input_type,
                                    if input.required { " · required" } else { "" }
                                )),
                            ),
                        )
                        // Soaks the remaining width so the pair stays left.
                        .child(div().flex_1()),
                );
            }
            return Some(lines.into_any_element());
        }

        // EXP-313: a card-shaped table — muted header row + separator-lined
        // rows with aligned columns — instead of the loose per-row flex that
        // read as disconnected controls.
        let border = cx.theme().border;
        // EXP-426: one density step tighter than the EXP-313 original —
        // the table reads as a compact manifest, not a form.
        let mut column = v_flex().w_full().gap_1();
        if !self.input_drafts.is_empty() {
            let mut table = v_flex()
                .w_full()
                .border_1()
                .border_color(border)
                .rounded(cx.theme().radius)
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_1p5()
                        .px_2()
                        .py_0p5()
                        .text_xs()
                        .text_color(muted)
                        .child(div().flex_1().min_w_0().child("Label"))
                        .child(div().w(px(84.)).flex_shrink_0().child("Type"))
                        .child(div().w(px(52.)).flex_shrink_0().child("Required"))
                        .child(div().w(px(20.)).flex_shrink_0()),
                );
            for (ix, draft) in self.input_drafts.iter().enumerate() {
                let Some(field) = self.input_labels.get(ix) else {
                    break;
                };
                let type_label: SharedString = draft.input_type.clone().into();
                let required = draft.required;
                table = table.child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap_1p5()
                        .px_2()
                        .py_1()
                        .border_t_1()
                        .border_color(border)
                        .child(Input::new(field).xsmall().flex_1().min_w_0())
                        .child(
                            Button::new(("action-input-type", ix))
                                .outline()
                                .xsmall()
                                .w(px(84.))
                                .child(
                                    h_flex()
                                        .w_full()
                                        .justify_between()
                                        .items_center()
                                        .child(div().text_xs().child(type_label))
                                        .child(
                                            Icon::new(registry::UI_CHEVRON_DOWN)
                                                .size_3()
                                                .text_color(muted),
                                        ),
                                )
                                .dropdown_menu({
                                    let view = cx.entity().downgrade();
                                    move |mut menu, _window, _cx| {
                                        for value in domain::contract::ACTION_INPUT_TYPE_VALUES {
                                            let view = view.clone();
                                            menu = menu.item(PopupMenuItem::new(*value).on_click(
                                                move |_, _, cx| {
                                                    let Some(view) = view.upgrade() else {
                                                        return;
                                                    };
                                                    view.update(cx, |view, cx| {
                                                        if let Some(draft) =
                                                            view.input_drafts.get_mut(ix)
                                                        {
                                                            draft.input_type = value.to_string();
                                                        }
                                                        view.save_inputs(cx);
                                                        cx.notify();
                                                    });
                                                },
                                            ));
                                        }
                                        menu
                                    }
                                }),
                        )
                        .child(
                            // The header names the column, so the checkbox
                            // drops its inline label.
                            div().w(px(52.)).flex_shrink_0().child(
                                Checkbox::new(("action-input-required", ix))
                                    .checked(required)
                                    .on_click(cx.listener(move |this, on: &bool, _, cx| {
                                        if let Some(draft) = this.input_drafts.get_mut(ix) {
                                            draft.required = *on;
                                        }
                                        this.save_inputs(cx);
                                        cx.notify();
                                    })),
                            ),
                        )
                        .child(
                            div().w(px(20.)).flex_shrink_0().child(
                                Button::new(("action-input-remove", ix))
                                    .ghost()
                                    .xsmall()
                                    .icon(Icon::new(registry::UI_CLOSE).text_color(muted))
                                    .tooltip("Remove input")
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, window, cx| {
                                            this.remove_input(ix, window, cx);
                                        },
                                    )),
                            ),
                        ),
                );
            }
            column = column.child(table);
        }
        if self.input_drafts.len() < MAX_ACTION_INPUTS {
            column = column.child(
                h_flex().w_full().child(
                    Button::new("action-input-add")
                        .ghost()
                        .xsmall()
                        .icon(registry::UI_ADD)
                        .label("Add input")
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.add_input(window, cx);
                        })),
                ),
            );
        }
        Some(column.into_any_element())
    }

    // -- header controls ----------------------------------------------------

    /// Icon: a chip picker over the curated grid for owners (the label-color
    /// swatch popover pattern), a static glyph chip otherwise. EXP-298: a
    /// PROPERTY, not a title ornament — the title has to keep the one shared
    /// left edge the description, the prompt and the section labels resolve to.
    fn render_icon(
        &self,
        action: &api::actions::Action,
        editable: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let glyph = crate::icons::action_icon(action.icon.as_deref());
        let name: SharedString = action
            .icon
            .clone()
            .unwrap_or_else(|| "Default".to_string())
            .into();
        if !editable {
            return glass_chip()
                .child(glyph.xsmall().text_color(cx.theme().muted_foreground))
                .child(name)
                .into_any_element();
        }
        let selected = action.icon.clone().unwrap_or_default();
        let view = cx.entity().downgrade();
        Popover::new("action-detail-icon")
            .trigger(
                chip_button("action-detail-icon-trigger", cx)
                    .icon(glyph.xsmall())
                    .child(crate::pickers::chip_label(name, false, cx)),
            )
            .content(move |_, _, cx| {
                let popover = cx.entity();
                let view = view.clone();
                // The grid is a `flex_wrap` row: it only wraps inside a
                // DEFINITE width, and a popover's content box is unconstrained
                // — without this it paints all 60 glyphs as one clipped strip
                // across the window. 8 × 28px cells + 7 × 6px gaps.
                div().w(px(266.)).p_1().child(
                    crate::board_form::icon_swatch_grid(
                        "action-detail",
                        &selected,
                        move |name, window, cx| {
                            if let Some(view) = view.upgrade() {
                                view.update(cx, |view, cx| view.save_icon(name, cx));
                            }
                            popover.update(cx, |state, cx| state.dismiss(window, cx));
                        },
                        cx,
                    ),
                )
            })
            .into_any_element()
    }

    /// Repository: an owner picker over the team's repos (plus the repo-less
    /// scratch option) once the fetch landed; a read-only chip otherwise. A
    /// builtin resolves its own context per run (the creator's `repo` input,
    /// the PR's repo for fix-conflicts), so it says so instead of claiming a
    /// scratch run.
    fn render_repository(
        &self,
        action: &api::actions::Action,
        editable: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        if action.builtin {
            return glass_chip()
                .text_color(muted)
                .child("Repository chosen when you run it")
                .into_any_element();
        }
        let current = action.repository_id.as_deref().map(|repo_id| {
            self.repos
                .iter()
                .find(|row| row.id == repo_id)
                .map(|row| row.full_name.clone())
                .unwrap_or_else(|| "Repository".to_string())
        });

        if !editable || !self.repos_loaded {
            return match current {
                Some(name) => glass_chip()
                    .child(Icon::from(ExpIcon::GitMerge).xsmall().text_color(muted))
                    .child(crate::pickers::chip_label(name, false, cx))
                    .into_any_element(),
                None => glass_chip()
                    .text_color(muted)
                    .child("No repository (scratch run)")
                    .into_any_element(),
            };
        }

        // EXP-417: a content-sized chip in the header's controls row (the
        // issue-header chip parity).
        let bound = current.is_some();
        let label: SharedString = match current {
            Some(name) => name.into(),
            None => "No repository (scratch run)".into(),
        };
        let repos = self.repos.clone();
        let view = cx.entity().downgrade();
        let trigger = chip_button("action-detail-repo", cx)
            .when(bound, |button| {
                button.icon(Icon::from(ExpIcon::GitMerge).xsmall().text_color(muted))
            })
            .child(crate::pickers::chip_label(label, !bound, cx));
        trigger.dropdown_menu(move |mut menu, _window, _cx| {
            let none_view = view.clone();
            menu = menu.item(
                PopupMenuItem::new("No repository (scratch run)").on_click(move |_, _, cx| {
                    if let Some(view) = none_view.upgrade() {
                        view.update(cx, |view, cx| view.save_repository(None, cx));
                    }
                }),
            );
            for repo in &repos {
                let view = view.clone();
                let repo_id = repo.id.clone();
                menu = menu.item(
                    PopupMenuItem::new(SharedString::from(repo.full_name.clone())).on_click(
                        move |_, _, cx| {
                            if let Some(view) = view.upgrade() {
                                let repo_id = repo_id.clone();
                                view.update(cx, |view, cx| {
                                    view.save_repository(Some(repo_id), cx)
                                });
                            }
                        },
                    ),
                );
            }
            menu
        })
        .into_any_element()
    }
}

/// Derive a server-legal input key (`^[a-z][a-z0-9_]{0,31}$`) from a label.
fn slug_key(label: &str) -> String {
    let mut key = String::new();
    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            key.push(ch.to_ascii_lowercase());
        } else if !key.ends_with('_') && !key.is_empty() {
            key.push('_');
        }
    }
    let key = key.trim_end_matches('_').to_string();
    // Must START with a letter; leave room for the de-duplication suffix.
    let key: String = key
        .chars()
        .skip_while(|ch| !ch.is_ascii_lowercase())
        .take(28)
        .collect();
    if key.is_empty() {
        "input".to_string()
    } else {
        key
    }
}

impl Render for ActionDetailView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let collections = Store::global(cx).collections();
        // The builtins are constructed against the window's TEAM, so their
        // resolution waits on the teams shape, not just on actions.
        let ready = collections.actions.read(cx).is_ready() && collections.teams.read(cx).is_ready();
        let Some(action) = self.action(cx) else {
            if !ready {
                // §4.1: never render "not found" off an unsynced snapshot.
                return v_flex()
                    .size_full()
                    .p_4()
                    .gap_2()
                    .child(Skeleton::new().h_4().w_48())
                    .child(Skeleton::new().h_4().w_64())
                    .child(Skeleton::new().h_4().w_56())
                    .into_any_element();
            }
            return self.not_found(cx);
        };

        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        // Owners edit real rows in place; members and the product-shipped
        // builtins read the same screen.
        let editable = !action.builtin && crate::settings::is_owner(cx, &action.team_id);
        self.sync_editors(&action, editable, window, cx);

        // ---- center column: icon + name, description, prompt, inputs -------
        // [`DETAIL_GUTTER`] is the ONE left edge every block resolves to
        // (issue-detail §8.3): the title row, the description, the section
        // labels and the prompt slot all align on it.
        let name: gpui::AnyElement = if editable {
            // Borderless title input (issue-detail parity): looks like the
            // heading, saves on blur/Enter. `flex_1` goes ON the Input — its
            // root sizes itself with a percent width, which collapses to
            // content width inside a flex-basis-0 wrapper.
            Input::new(&self.name_input)
                .appearance(false)
                .text_2xl()
                .font_weight(FontWeight::SEMIBOLD)
                .line_height(gpui::rems(2.))
                .px_0()
                .h_auto()
                .into_any_element()
        } else {
            div()
                .text_2xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(action.name.clone()))
                .into_any_element()
        };

        let title = div()
            .px(px(DETAIL_GUTTER))
            .pt_3()
            .pb_1()
            // Tab jumps from the name into the prompt editor (issue-detail /
            // web EXP-10 parity). Capture runs before the InputState's own Tab
            // handling; Shift+Tab is a different action and keeps its default.
            .capture_action(cx.listener(
                |this, _: &input::IndentInline, window, cx: &mut gpui::Context<Self>| {
                    if let Some(editor) = this.body_editor.clone() {
                        cx.stop_propagation();
                        editor.update(cx, |editor, cx| editor.focus(window, cx));
                    }
                },
            ))
            // Shift+Enter would insert a newline in the auto-grow input
            // (`submit_on_enter` only intercepts plain Enter) — swallow it so
            // no keyboard path can put a newline in a name (EXP-230).
            .capture_action(cx.listener(
                |_, action: &input::Enter, _window, cx: &mut gpui::Context<Self>| {
                    if action.shift {
                        cx.stop_propagation();
                    }
                },
            ))
            .child(name);

        let description: Option<gpui::AnyElement> = if editable {
            Some(
                div()
                    .px(px(DETAIL_GUTTER))
                    .child(
                        Input::new(&self.description_input)
                            .appearance(false)
                            .text_sm()
                            .text_color(muted)
                            .px_0()
                            .h_auto(),
                    )
                    .into_any_element(),
            )
        } else {
            action
                .description
                .clone()
                .filter(|text| !text.trim().is_empty())
                .map(|text| {
                    div()
                        .px(px(DETAIL_GUTTER))
                        .text_sm()
                        .text_color(muted)
                        .child(SharedString::from(text))
                        .into_any_element()
                })
        };

        // ---- controls row (EXP-417: was the right sidebar) ------------------
        let run_id = action.id.clone();
        let run_team = action.team_id.clone();
        // EXP-367: no agent CLI → disabled with the reason, never hidden.
        let no_agent = crate::coding_flow::no_agent_reason(cx);
        let mut controls = h_flex()
            .w_full()
            .flex_wrap()
            .gap_2()
            .items_center()
            .px(px(DETAIL_GUTTER))
            .pt_2()
            .child(
                Button::new("action-detail-run")
                    .primary()
                    .small()
                    .icon(Icon::from(ExpIcon::Play))
                    .label("Run on this device")
                    .when_some(no_agent.clone(), |button, reason| button.tooltip(reason))
                    .disabled(no_agent.is_some())
                    .on_click(move |_: &ClickEvent, window, cx| {
                        crate::start_coding_dialog::open_for_action(
                            window,
                            cx,
                            run_team.clone(),
                            run_id.clone(),
                        );
                    }),
            )
            .child(self.render_repository(&action, editable, cx));
        // A builtin's glyph is product-shipped like the rest of it.
        if !action.builtin {
            controls = controls.child(self.render_icon(&action, editable, cx));
        }
        controls = controls.child(div().flex_1());
        // EXP-426: the visible trash button + the issue detail's two-step
        // "Confirm delete" popup replaced the `…` menu (and the old alert).
        if editable {
            let delete_id = action.id.clone();
            controls = controls.child(
                Button::new("action-detail-delete")
                    .ghost()
                    .xsmall()
                    .icon(Icon::new(registry::UI_DELETE).text_color(muted))
                    .tooltip("Delete action")
                    .dropdown_menu(move |menu, _window, _cx| {
                        let id = delete_id.clone();
                        menu.item(
                            PopupMenuItem::new("Confirm delete")
                                .icon(Icon::new(registry::UI_DELETE))
                                .on_click(move |_, window, cx| {
                                    crate::actions_panel::spawn_action_delete(cx, id.clone());
                                    crate::navigation::go_back(window, cx);
                                }),
                        )
                    }),
            );
        }

        // ---- fixed header ---------------------------------------------------
        let mut header = v_flex().w_full().child(title);
        if let Some(description) = description {
            header = header.child(description);
        }
        header = header.child(controls);
        if let Some(error) = self.error.clone() {
            header = header.child(
                div()
                    .px(px(DETAIL_GUTTER))
                    .pt_1()
                    .text_xs()
                    .text_color(danger)
                    .child(error),
            );
        }
        if let Some(inputs) = self.render_inputs(&action, editable, cx) {
            header = header
                .child(
                    div()
                        .px(px(DETAIL_GUTTER))
                        .pt_3()
                        .pb_1()
                        .child(group_label("Inputs", cx)),
                )
                // Ten input rows would eat the pane — the definition table
                // scrolls inside the header instead.
                .child(
                    div()
                        .id("action-inputs-scroll")
                        .px(px(DETAIL_GUTTER))
                        .max_h(px(208.))
                        .overflow_y_scroll()
                        .child(inputs),
                );
        }
        // EXP-426: the PROMPT label is gone — the pinned formatting bar (or
        // the body itself) separates the sections; only the builtin note
        // stays.
        if action.builtin {
            header = header.child(
                v_flex().px(px(DETAIL_GUTTER)).pt_3().pb_1().child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child("Built-in action: shipped with the app, not editable."),
                ),
            );
        }
        if editable {
            // EXP-417: the formatting bar is pinned here, so a long prompt
            // never scrolls it away. Same inset as the editor slot below.
            header = header.child(
                div()
                    .px(px(DETAIL_GUTTER - WYSIWYG_BLOCK_PADDING_X))
                    .children(self.body_editor.clone().and_then(|editor| {
                        editor.update(cx, |editor, cx| editor.toolbar_row(window, cx))
                    })),
            );
        }

        v_flex()
            .size_full()
            .child(
                v_flex()
                    .w_full()
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                    .child(centered_column(header)),
            )
            .child(
                div()
                    .id("action-detail-scroll")
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.body_scroll)
                    // EXP-421: the editor records its slot WIDTH off this
                    // column, so it must stay definite (`w_full` + `min_w`),
                    // and the `min_h_full` chain is what lets the editor's
                    // trailing filler absorb the leftover height.
                    .child(centered_scroll_column(
                        // EXP-426: breathing room under the pinned toolbar —
                        // the embedded editor carries no insets of its own.
                        v_flex().pt_2().pb_6().child(self.render_prompt(editable, cx)),
                    )),
            )
            .into_any_element()
    }
}
