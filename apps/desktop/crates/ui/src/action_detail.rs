//! Full-page action detail (EXP-277) — the Actions tool-window rows' center
//! screen, mirroring the issue detail's shape: the markdown prompt in a
//! centered column and a properties-style right sidebar.
//!
//! Data model: the synced `actions` shape carries the body-less row (name,
//! description, inputs, repo binding); the prompt body is tRPC-only
//! (`actions.get` — EXP-268), fetched per [`ActionDetailView::set_action`]
//! and refetched when the synced row's `updated_at` moves (covers remote/MCP
//! edits). Builtins never navigate here (no stable body — their rows open the
//! start dialog instead); a builtin or unknown id renders the not-found state.
//!
//! **EXP-282 — this screen IS the action editor.** The raw editor dialog is
//! gone; owners edit everything in place, each field mutating on its own
//! through `actions.update` (the issue-detail contract — no submit button, no
//! dirty dialog): name and description are borderless inputs saved on
//! blur/Enter, the prompt toggles between rendered markdown and a markdown
//! SOURCE editor, and the sidebar's Repository picker + Inputs definition
//! rows save immediately. Non-owners get the same screen read-only.
//!
//! Editor choice for the prompt: a plain multi-line `Input` over the WYSIWYG
//! description editor. An action body is agent-facing SOURCE — the vendored
//! WYSIWYG normalizes render-equivalent markdown (setext→ATX, `_i_`→`*i*`,
//! `1)`→`1.`) on every save, silently rewriting a hand-tuned prompt, and its
//! no-image mode stages pastes as `draft://` that the save path strips. A
//! source field is lossless and honest about what the agent will receive.

use std::collections::HashSet;

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
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    skeleton::Skeleton,
    text::TextView,
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _,
};
use sync::Store;

use crate::action_run::{fetch_repositories, ActionRepoRow};
use crate::icons::ExpIcon;
use crate::issue_detail::centered_column;
use crate::properties_panel::property_group;
use crate::queries;

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
    action_id: Option<String>,
    /// The fetched prompt body (`None` = in flight).
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
    body_input: Entity<InputState>,
    /// What was last PUSHED into each field from the row/fetch (`None` = not
    /// seeded yet for this action). The echo guard: re-seeding only when the
    /// server value moves is what keeps a remote echo from clobbering the
    /// characters the owner is typing.
    seeded_name: Option<String>,
    seeded_description: Option<String>,
    seeded_body: Option<String>,
    /// The prompt source editor is up (owner toggled it from the header).
    editing_body: bool,
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
        let description_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Add a one-line description…")
        });
        let body_input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(16)
                .placeholder("The markdown prompt this action runs…")
        });
        let input_labels: Vec<Entity<InputState>> = (0..MAX_ACTION_INPUTS)
            .map(|_| cx.new(|cx| InputState::new(window, cx).placeholder("Label")))
            .collect();

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
        // The prompt saves on blur and leaves edit mode with it (clicking
        // "Done" blurs the field first, so one path covers both).
        subscriptions.push(cx.subscribe_in(
            &body_input,
            window,
            |this, _, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Blur) {
                    this.save_body(cx);
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

        Self {
            action_id: None,
            body: None,
            body_error: None,
            fetched_updated_at: None,
            repos: Vec::new(),
            repos_loaded: false,
            body_scroll: gpui::ScrollHandle::new(),
            name_input,
            description_input,
            body_input,
            seeded_name: None,
            seeded_description: None,
            seeded_body: None,
            editing_body: false,
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
        self.save_body(cx);
        self.save_inputs(cx);
        self.action_id = Some(action_id);
        self.body = None;
        self.body_error = None;
        self.fetched_updated_at = None;
        self.repos = Vec::new();
        self.repos_loaded = false;
        // Every editor belongs to the PREVIOUS action — drop the seeds so the
        // next render refills the fields from the new row (this entry point
        // has no `Window`, so the actual `set_value`s happen in `render`).
        self.seeded_name = None;
        self.seeded_description = None;
        self.seeded_body = None;
        self.editing_body = false;
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

    /// The synced row, if visible in this team.
    fn action(&self, cx: &App) -> Option<api::actions::Action> {
        let action_id = self.action_id.as_deref()?;
        if api::actions::is_builtin_action_id(action_id) {
            return None;
        }
        Store::global(cx)
            .collections()
            .actions
            .read(cx)
            .get(action_id)
            .map(api::actions::from_row)
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
        self.fetched_updated_at = self
            .action(cx)
            .and_then(|action| action.updated_at.clone());
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
    /// let the Electric echo re-render). Failures surface in the sidebar.
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
        let Some(action) = self.action(cx) else {
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
        let Some(action) = self.action(cx) else {
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

    /// Save the prompt source and leave edit mode. A blank body is refused
    /// locally (the server's `bodySchema` rejects it) instead of round-
    /// tripping a 400 the owner can't act on.
    fn save_body(&mut self, cx: &mut gpui::Context<Self>) {
        // Only the open source editor can have unsaved prompt bytes. Without
        // this gate a blur/navigation while the body is still loading would
        // compare an EMPTY field against `None` and raise the empty-prompt
        // error out of nowhere.
        if !self.editing_body {
            return;
        }
        self.editing_body = false;
        let Some(action) = self.action(cx) else {
            return;
        };
        // Deliberately NOT trimmed — leading/trailing markdown whitespace is
        // prompt content (the server takes the same stance).
        let body = self.body_input.read(cx).value().to_string();
        if Some(body.as_str()) == self.body.as_deref() {
            cx.notify();
            return;
        }
        if body.trim().is_empty() {
            self.error = Some("The prompt must not be empty.".into());
            cx.notify();
            return;
        }
        self.body = Some(SharedString::from(body.clone()));
        self.seeded_body = Some(body.clone());
        let mut input = api::actions::ActionUpdate::new(action.id);
        input.body = Some(body);
        self.spawn_update(input, cx);
    }

    fn save_repository(&mut self, repository_id: Option<String>, cx: &mut gpui::Context<Self>) {
        let Some(action) = self.action(cx) else {
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
        let Some(action) = self.action(cx) else {
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
        // The prompt field only refills while the source editor is CLOSED —
        // a refetch landing mid-edit must not overwrite the owner's typing.
        if !self.editing_body {
            let body = self.body.as_ref().map(|body| body.to_string());
            if self.seeded_body != body {
                self.seeded_body = body.clone();
                if let Some(body) = body {
                    self.body_input
                        .update(cx, |state, cx| state.set_value(body, window, cx));
                }
            }
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

    // -- sidebar controls ---------------------------------------------------

    /// Repository: an owner picker over the team's repos (plus the repo-less
    /// scratch option) once the fetch landed; a read-only chip otherwise.
    fn render_repository(
        &self,
        action: &api::actions::Action,
        owner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let current = action.repository_id.as_deref().map(|repo_id| {
            self.repos
                .iter()
                .find(|row| row.id == repo_id)
                .map(|row| row.full_name.clone())
                .unwrap_or_else(|| "Repository".to_string())
        });

        if !owner || !self.repos_loaded {
            return match current {
                Some(name) => h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_1p5()
                    .items_center()
                    .text_xs()
                    .child(
                        div()
                            .flex_shrink_0()
                            .child(Icon::from(ExpIcon::GitMerge).xsmall().text_color(muted)),
                    )
                    .child(div().min_w_0().truncate().child(SharedString::from(name)))
                    .into_any_element(),
                None => div()
                    .text_xs()
                    .text_color(muted)
                    .child("No repository (scratch run)")
                    .into_any_element(),
            };
        }

        let label: SharedString = match current {
            Some(name) => name.into(),
            None => "No repository (scratch run)".into(),
        };
        let repos = self.repos.clone();
        let view = cx.entity().downgrade();
        Button::new("action-detail-repo")
            .ghost()
            .xsmall()
            .w_full()
            .label(label)
            .dropdown_menu(move |mut menu, _window, _cx| {
                let none_view = view.clone();
                menu = menu.item(
                    PopupMenuItem::new("No repository (scratch run)").on_click(
                        move |_, _, cx| {
                            if let Some(view) = none_view.upgrade() {
                                view.update(cx, |view, cx| view.save_repository(None, cx));
                            }
                        },
                    ),
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

    /// Inputs: editable definition rows for the owner (label field + type
    /// picker + required toggle + remove, plus "Add input"), read-only
    /// `label · type · required` lines otherwise.
    fn render_inputs(
        &self,
        action: &api::actions::Action,
        owner: bool,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let muted = cx.theme().muted_foreground;
        if !owner {
            if action.inputs.is_empty() {
                return None;
            }
            let mut lines = v_flex().w_full().gap_1();
            for input in &action.inputs {
                lines = lines.child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .gap_1()
                        .items_center()
                        .text_xs()
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .child(SharedString::from(input.label.clone())),
                        )
                        .child(
                            div().flex_shrink_0().text_color(muted).child(
                                SharedString::from(format!(
                                    "· {}{}",
                                    input.input_type,
                                    if input.required { " · required" } else { "" }
                                )),
                            ),
                        ),
                );
            }
            return Some(lines.into_any_element());
        }

        let mut rows = v_flex().w_full().gap_2();
        for (ix, draft) in self.input_drafts.iter().enumerate() {
            let Some(field) = self.input_labels.get(ix) else {
                break;
            };
            let type_label: SharedString = draft.input_type.clone().into();
            let required = draft.required;
            rows = rows.child(
                v_flex()
                    .w_full()
                    .min_w_0()
                    .gap_1()
                    .child(Input::new(field).xsmall())
                    .child(
                        h_flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap_1()
                            .child(
                                Button::new(("action-input-type", ix))
                                    .ghost()
                                    .xsmall()
                                    .label(type_label)
                                    .dropdown_menu({
                                        let view = cx.entity().downgrade();
                                        move |mut menu, _window, _cx| {
                                            for value in
                                                domain::contract::ACTION_INPUT_TYPE_VALUES
                                            {
                                                let view = view.clone();
                                                menu = menu.item(
                                                    PopupMenuItem::new(*value).on_click(
                                                        move |_, _, cx| {
                                                            let Some(view) = view.upgrade()
                                                            else {
                                                                return;
                                                            };
                                                            view.update(cx, |view, cx| {
                                                                if let Some(draft) = view
                                                                    .input_drafts
                                                                    .get_mut(ix)
                                                                {
                                                                    draft.input_type =
                                                                        value.to_string();
                                                                }
                                                                view.save_inputs(cx);
                                                                cx.notify();
                                                            });
                                                        },
                                                    ),
                                                );
                                            }
                                            menu
                                        }
                                    }),
                            )
                            .child(
                                Checkbox::new(("action-input-required", ix))
                                    .label("Required")
                                    .checked(required)
                                    .on_click(cx.listener(move |this, on: &bool, _, cx| {
                                        if let Some(draft) = this.input_drafts.get_mut(ix) {
                                            draft.required = *on;
                                        }
                                        this.save_inputs(cx);
                                        cx.notify();
                                    })),
                            )
                            .child(div().flex_1().min_w_0())
                            .child(
                                Button::new(("action-input-remove", ix))
                                    .ghost()
                                    .xsmall()
                                    .icon(Icon::new(IconName::Close).text_color(muted))
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
        if self.input_drafts.len() < MAX_ACTION_INPUTS {
            rows = rows.child(
                Button::new("action-input-add")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Plus)
                    .label("Add input")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.add_input(window, cx);
                    })),
            );
        }
        Some(rows.into_any_element())
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
        let ready = Store::global(cx).collections().actions.read(cx).is_ready();
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
        let Some(action) = self.action(cx) else {
            return self.not_found(cx);
        };

        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        // Builtins never resolve through `action()`, so an editable row here
        // is always a real DB row — ownership is the only gate.
        let owner = crate::settings::is_owner(cx, &action.team_id);
        self.sync_editors(&action, window, cx);

        // ---- center column: name, description, prompt ----------------------
        let name: gpui::AnyElement = if owner {
            // Borderless title input (issue-detail parity): looks like the
            // heading, saves on blur/Enter.
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

        let description: Option<gpui::AnyElement> = if owner {
            Some(
                Input::new(&self.description_input)
                    .appearance(false)
                    .text_sm()
                    .text_color(muted)
                    .px_0()
                    .h_auto()
                    .into_any_element(),
            )
        } else {
            action
                .description
                .clone()
                .filter(|text| !text.trim().is_empty())
                .map(|text| {
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(SharedString::from(text))
                        .into_any_element()
                })
        };

        // EXP-282: PROMPT header carries the owner's Edit/Done toggle. A
        // click-to-edit card was the alternative, but the read view is
        // selectable markdown — a click after a drag-select would swap the
        // editor in and throw the selection away.
        let prompt_header = h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(muted)
                    .child("PROMPT"),
            )
            .when(owner && self.body.is_some(), |this| {
                let editing = self.editing_body;
                this.child(
                    Button::new("action-body-edit")
                        .ghost()
                        .xsmall()
                        .label(if editing { "Done" } else { "Edit" })
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            if editing {
                                // Blur commits through the field's own
                                // subscription; this is the belt for a
                                // click that never moved focus.
                                this.save_body(cx);
                            } else {
                                this.editing_body = true;
                                this.body_input
                                    .update(cx, |state, cx| state.focus(window, cx));
                            }
                            cx.notify();
                        })),
                )
            });

        let prompt: gpui::AnyElement = if let Some(error) = self.body_error.clone() {
            div()
                .text_xs()
                .text_color(danger)
                .child(error)
                .into_any_element()
        } else if owner && self.editing_body {
            crate::surface::glass_card()
                .p_2()
                .child(Input::new(&self.body_input).appearance(false))
                .into_any_element()
        } else if let Some(body) = self.body.clone() {
            crate::surface::glass_card()
                .p_4()
                .child(
                    TextView::markdown("action-body", body)
                        .style(crate::surface::markdown_style())
                        .selectable(true),
                )
                .into_any_element()
        } else {
            div()
                .text_xs()
                .text_color(muted)
                .child("Loading prompt…")
                .into_any_element()
        };

        let mut column = v_flex()
            .w_full()
            .px_4()
            .pt_3()
            .pb_6()
            .gap_3()
            .child(name);
        if let Some(description) = description {
            column = column.child(description);
        }
        column = column.child(prompt_header).child(prompt);

        // ---- sidebar --------------------------------------------------------
        let run_id = action.id.clone();
        let run_team = action.team_id.clone();
        // EXP-282: Run then Delete lead the sidebar (the two whole-action
        // verbs), both full-width; everything below is a property group.
        let mut sidebar = crate::surface::glass_sidebar().child(
            Button::new("action-detail-run")
                .primary()
                .small()
                .w_full()
                .icon(Icon::from(ExpIcon::Play))
                .label("Run on this device")
                .on_click(move |_: &ClickEvent, window, cx| {
                    crate::start_coding_dialog::open_for_action(
                        window,
                        cx,
                        run_team.clone(),
                        run_id.clone(),
                    );
                }),
        );

        if owner {
            let delete_id = action.id.clone();
            let delete_name = action.name.clone();
            sidebar = sidebar.child(
                Button::new("action-detail-delete")
                    .danger()
                    .small()
                    .w_full()
                    .label("Delete action…")
                    .on_click(move |_: &ClickEvent, window, cx| {
                        crate::actions_panel::prompt_delete_action(
                            window,
                            cx,
                            delete_id.clone(),
                            delete_name.clone(),
                        );
                    }),
            );
        }

        let repository = self.render_repository(&action, owner, cx);
        sidebar = sidebar.child(property_group("Repository", repository, cx));
        if let Some(inputs) = self.render_inputs(&action, owner, cx) {
            sidebar = sidebar.child(property_group("Inputs", inputs, cx));
        }
        if let Some(error) = self.error.clone() {
            sidebar = sidebar.child(div().text_xs().text_color(danger).child(error));
        }

        let body = h_flex()
            .flex_1()
            .min_h_0()
            .items_start()
            .overflow_hidden()
            .child(
                div()
                    .id("action-detail-scroll")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.body_scroll)
                    .child(centered_column(column)),
            )
            .child(sidebar);

        v_flex().size_full().child(body).into_any_element()
    }
}
