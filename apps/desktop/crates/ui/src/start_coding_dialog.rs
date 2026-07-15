//! The ONE shared Start-coding dialog — the issue-detail Play button and the
//! bulk bar's "Start coding" action both land here (`open_for_issue` /
//! `open_for_selection`). One surface, two run modes decided by the checked
//! count:
//!
//! - **1 issue** → today's plain single-issue session (its own worktree +
//!   `exp/<IDENTIFIER>` branch, `PrepareRequest::Issue`).
//! - **2+ issues** → a BATCH run (EXP-106): ONE Claude session on ONE
//!   `exp/batch-<id8>` branch implementing every checked issue and opening
//!   ONE combined PR (`PrepareRequest::Batch`). Deliberately loose — no
//!   per-issue subagent definitions, no waves; Claude organizes the work.
//!
//! The multi-issue PICKER is always present: a searchable checklist scoped to
//! the pre-seeded issues' project(s), OPEN issues only (EXP-119 —
//! `done`/`cancelled`/`duplicate`/PR-merged rows are hidden; other projects'
//! issues too). Pre-seeded ids are exempt from both filters and force-check:
//! the explicit pick wins, which keeps the Play-button re-run of a done issue
//! working. Repo probes (`repositories.forIssue`, background executor,
//! generation-guarded) run LAZILY for checked issues only — ONE repository
//! per run is enforced.
//!
//! Options: Model + Effort [`ChoiceSelect`]s, the ULTRACODE switch
//! (`--effort ultracode`, model-independent; disables the Effort select) and
//! the native "Plan mode" checkbox. Defaults follow the mode:
//! [`coding::Settings`]' `issue_*` pair for a single selection, the `batch_*`
//! pair for 2+; flipping modes re-applies that mode's defaults.
//!
//! Launch = snapshot → [`coding::prepare`] on the background executor → the
//! shared `coding_flow::spawn_into_window` foreground spawn. A
//! `Prepared::Disabled` reason renders inline and keeps the dialog open.

use std::collections::{HashMap, HashSet};

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, Entity, InteractiveElement as _,
    IntoElement, ParentElement, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::{Scrollbar, ScrollbarAxis},
    select::Select,
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
};
use sync::Store;

use api::repositories::IssueRepository;
use coding::{
    BatchIssueSpec, BatchLaunchRequest, LaunchOptions, LaunchOrigin, Prepared, PrepareRequest,
    RepoGroup,
};
use domain::IssueStatus;

use crate::coding_flow::{self, CodingHub, SessionSubject};
use crate::coding_selects::{choice_select, selected, ChoiceSelect, EFFORT_CHOICES, MODEL_CHOICES};
use crate::queries;

/// Soft cost warning threshold: more checked issues than this shows the
/// "token-expensive" note (no hard gate — coding is unmetered).
const COST_NOTE_THRESHOLD: usize = 6;

/// Hard cap per run: every checked issue adds a prompt section, and a batch
/// beyond this size stops being one coherent session anyway.
const MAX_ISSUES_PER_RUN: usize = 30;

/// Unchecked search matches rendered at once — a workspace can hold hundreds
/// of issues, and the checklist is a plain (non-virtual) list.
const MAX_UNCHECKED_ROWS: usize = 50;

/// Open the dialog from an issue's Play button: pre-seed that issue checked.
/// A no-op when the issue row isn't synced (racing a delete).
pub fn open_for_issue(window: &mut Window, cx: &mut App, issue_id: String) {
    let Some(issue) = Store::global(cx)
        .collections()
        .issues
        .read(cx)
        .get(&issue_id)
        .cloned()
    else {
        log::warn!("[ui] start-coding dialog for unknown issue {issue_id}");
        return;
    };
    let Some(workspace_id) = Store::global(cx)
        .collections()
        .projects
        .read(cx)
        .get(&issue.project_id)
        .map(|project| project.workspace_id.clone())
    else {
        log::warn!("[ui] start-coding dialog: project not synced for {issue_id}");
        return;
    };
    open(window, cx, workspace_id, vec![issue.id]);
}

/// Open the dialog from the bulk bar with the selection pre-checked.
pub fn open_for_selection(
    window: &mut Window,
    cx: &mut App,
    workspace_id: String,
    issue_ids: Vec<String>,
) {
    open(window, cx, workspace_id, issue_ids);
}

fn open(window: &mut Window, cx: &mut App, workspace_id: String, preselected: Vec<String>) {
    let view =
        cx.new(|cx| StartCodingDialogView::new(workspace_id, preselected, window, cx));
    window.open_dialog(cx, move |dialog, window, cx| {
        let busy = view.read(cx).launching;
        let max_height = window.viewport_size().height * 0.85;
        dialog
            .w(px(560.))
            .max_h(max_height)
            .title("Start coding")
            .overlay_closable(!busy)
            .keyboard(!busy)
            .child(view.clone())
    });
}

/// One checklist row, snapshotted from the sync store at open (titles and
/// descriptions ride into the launch request verbatim — the launcher never
/// re-reads the collections).
struct IssueRow {
    issue_id: String,
    identifier: String,
    title: String,
    description: Option<String>,
    /// The closed-state note (`done`/`cancelled`/`duplicate`/PR-merged),
    /// shown muted next to the title. Only pre-seeded rows can carry one
    /// (EXP-119 filters closed rows out of the pool) — it flags a re-run.
    /// `None` = plain row.
    state_hint: Option<&'static str>,
}

/// One issue's `repositories.forIssue` probe state.
enum RepoState {
    Loading,
    /// `Ready(None)` = no repository linked (excluded from the run).
    Ready(Option<IssueRepository>),
    /// Transport failure — the issue can't resolve a repo, so it is
    /// excluded like a repo-less one (the message says why).
    Error(String),
}

/// Which settings pair the ultracode/plan-mode toggles were last defaulted
/// from — flipping the checked count across the 1↔2 boundary re-applies the
/// other mode's defaults (user tweaks persist WITHIN a mode).
#[derive(Clone, Copy, PartialEq, Eq)]
enum DefaultsMode {
    Single,
    Batch,
}

pub struct StartCodingDialogView {
    workspace_id: String,
    /// Every non-archived workspace issue, project→number ordered.
    rows: Vec<IssueRow>,
    /// issue id → probe state (LAZY: only checked issues probe).
    repos: HashMap<String, RepoState>,
    /// Stale-probe guard (old results must not land after a re-open).
    probe_generation: u64,
    checked: HashSet<String>,
    search: Entity<InputState>,
    /// Scroll position of the checklist (view state so it survives
    /// re-renders — the EXP-67 scroll-pane idiom, bounded by `max_h`).
    list_scroll: ScrollHandle,
    model: ChoiceSelect,
    effort: ChoiceSelect,
    /// Dynamic workflows (`--effort ultracode`) — any model, no pin.
    ultracode: bool,
    /// Native Claude plan mode (`--permission-mode plan`).
    plan_mode: bool,
    defaults_mode: DefaultsMode,
    launching: bool,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl StartCodingDialogView {
    fn new(
        workspace_id: String,
        preselected: Vec<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let hub = CodingHub::global(cx);
        let settings = hub.read(cx).settings.clone();

        // Snapshot the picker's candidate pool (EXP-119): the pre-seeded
        // issues' project(s) only, OPEN issues only — unrelated projects and
        // closed rows just buried the launchable ones. Pre-seeded ids are
        // exempt from both filters: the explicit pick wins (the Play-button
        // re-run of a done issue), and a checked id MUST keep its row —
        // `batch_request`/`launch_blocker` iterate `rows` and would silently
        // drop it from the run otherwise.
        let preselected: HashSet<String> = preselected.into_iter().collect();
        let mut issues = queries::workspace_issues(cx, &workspace_id);
        // `workspace_issues` hides ARCHIVED rows, but the Play button resolves
        // its seed from the raw collection — re-read any missing seed raw so
        // an archived pick shows up force-checked instead of silently
        // vanishing from the run (`batch_request` iterates `rows`).
        for seed in &preselected {
            if !issues.iter().any(|issue| &issue.id == seed) {
                if let Some(issue) = Store::global(cx).collections().issues.read(cx).get(seed) {
                    issues.push(issue.clone());
                }
            }
        }
        let scope_projects: HashSet<String> = issues
            .iter()
            .filter(|issue| preselected.contains(&issue.id))
            .map(|issue| issue.project_id.clone())
            .collect();
        issues.retain(|issue| {
            if preselected.contains(&issue.id) {
                return true;
            }
            // No resolvable seed (racing a delete) → keep the whole
            // workspace pool rather than an empty picker.
            if !scope_projects.is_empty() && !scope_projects.contains(&issue.project_id) {
                return false;
            }
            let closed = matches!(
                issue.status,
                IssueStatus::Done | IssueStatus::Cancelled | IssueStatus::Duplicate
            ) || issue.pr_state.as_deref() == Some("merged");
            !closed
        });
        issues.sort_by(|a, b| {
            a.project_id
                .cmp(&b.project_id)
                .then_with(|| a.number.cmp(&b.number))
        });
        let mut checked = HashSet::new();
        let rows: Vec<IssueRow> = issues
            .into_iter()
            .map(|issue| {
                let merged = issue.pr_state.as_deref() == Some("merged");
                let state_hint = if merged {
                    Some("PR merged")
                } else {
                    match issue.status {
                        IssueStatus::Done => Some("done"),
                        IssueStatus::Cancelled => Some("cancelled"),
                        IssueStatus::Duplicate => Some("duplicate"),
                        _ => None,
                    }
                };
                // Pre-seeded ids force-check regardless of the state hint —
                // the user explicitly picked them.
                if preselected.contains(&issue.id) {
                    checked.insert(issue.id.clone());
                }
                IssueRow {
                    issue_id: issue.id,
                    identifier: issue.identifier,
                    title: issue.title,
                    description: issue.description,
                    state_hint,
                }
            })
            .collect();

        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search issues…"));
        let subscriptions = vec![
            // Doctor lands / re-runs → the footer gate moves.
            cx.observe(&hub, |_: &mut Self, _, cx| cx.notify()),
            cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];

        let defaults_mode = if checked.len() >= 2 {
            DefaultsMode::Batch
        } else {
            DefaultsMode::Single
        };
        let (ultracode, plan_mode) = match defaults_mode {
            DefaultsMode::Single => (settings.issue_ultracode, settings.issue_plan_mode),
            DefaultsMode::Batch => (settings.batch_ultracode, settings.batch_plan_mode),
        };

        let mut this = Self {
            workspace_id,
            rows,
            repos: HashMap::new(),
            probe_generation: 0,
            checked,
            search,
            list_scroll: ScrollHandle::new(),
            model: choice_select(&MODEL_CHOICES, &settings.claude_model, window, cx),
            effort: choice_select(&EFFORT_CHOICES, &settings.claude_effort, window, cx),
            ultracode,
            plan_mode,
            defaults_mode,
            launching: false,
            error: None,
            _subscriptions: subscriptions,
        };
        this.probe_generation += 1;
        let ids: Vec<String> = this.checked.iter().cloned().collect();
        for issue_id in ids {
            this.ensure_probe(issue_id, cx);
        }
        this
    }

    /// Kick ONE `repositories.forIssue` probe for `issue_id` if it never ran
    /// (background executor, generation-guarded like
    /// `StartCodingControl::ensure_probe`). Lazy by design: only checked
    /// issues probe — a whole-workspace eager fan-out would be hundreds of
    /// tRPC calls.
    fn ensure_probe(&mut self, issue_id: String, cx: &mut gpui::Context<Self>) {
        if self.repos.contains_key(&issue_id) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.repos
                .insert(issue_id, RepoState::Error("Not signed in.".to_string()));
            return;
        };
        self.repos.insert(issue_id.clone(), RepoState::Loading);
        let generation = self.probe_generation;
        let probe_id = issue_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::repositories::for_issue(&trpc, &probe_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.probe_generation != generation {
                    return; // superseded
                }
                let state = match result {
                    Ok(repo) => RepoState::Ready(repo),
                    Err(err) => RepoState::Error(err.to_string()),
                };
                // Unresolvable issues can never launch — uncheck them.
                if !matches!(state, RepoState::Ready(Some(_))) {
                    this.checked.remove(&issue_id);
                    this.apply_mode_defaults(cx);
                }
                this.repos.insert(issue_id.clone(), state);
                cx.notify();
            });
        })
        .detach();
    }

    /// Re-apply the per-mode settings defaults when the checked count crosses
    /// the 1↔2+ boundary (user tweaks persist within a mode).
    fn apply_mode_defaults(&mut self, cx: &mut gpui::Context<Self>) {
        let mode = if self.checked.len() >= 2 {
            DefaultsMode::Batch
        } else {
            DefaultsMode::Single
        };
        if mode == self.defaults_mode {
            return;
        }
        self.defaults_mode = mode;
        let settings = CodingHub::global(cx).read(cx).settings.clone();
        (self.ultracode, self.plan_mode) = match mode {
            DefaultsMode::Single => (settings.issue_ultracode, settings.issue_plan_mode),
            DefaultsMode::Batch => (settings.batch_ultracode, settings.batch_plan_mode),
        };
    }

    fn toggle_checked(&mut self, issue_id: String, on: bool, cx: &mut gpui::Context<Self>) {
        if on {
            self.checked.insert(issue_id.clone());
            self.ensure_probe(issue_id, cx);
        } else {
            self.checked.remove(&issue_id);
        }
        self.apply_mode_defaults(cx);
        cx.notify();
    }

    /// Why the launch button is disabled right now; `None` = launchable.
    fn launch_blocker(&self, cx: &mut gpui::Context<Self>) -> Option<SharedString> {
        if self.launching {
            return Some("Starting…".into());
        }
        let hub = CodingHub::global(cx);
        match hub.read(cx).doctor.report.as_ref() {
            None => return Some("Checking local tools…".into()),
            Some(report) => {
                if let Some(failed) = report.first_failure() {
                    return Some(
                        failed
                            .error
                            .clone()
                            .unwrap_or_else(|| format!("{} is not available", failed.tool))
                            .into(),
                    );
                }
            }
        }
        if self.checked.is_empty() {
            return Some("Select at least one issue.".into());
        }
        if self.checked.len() > MAX_ISSUES_PER_RUN {
            return Some(
                format!("At most {MAX_ISSUES_PER_RUN} issues per run — split the batch.").into(),
            );
        }
        let mut repo: Option<&str> = None;
        for row in &self.rows {
            if !self.checked.contains(&row.issue_id) {
                continue;
            }
            match self.repos.get(&row.issue_id) {
                Some(RepoState::Ready(Some(resolved))) => match repo {
                    None => repo = Some(&resolved.repository_id),
                    Some(existing) if existing == resolved.repository_id => {}
                    Some(_) => {
                        return Some("One repository per run — deselect the others.".into())
                    }
                },
                // Still resolving (or unresolvable-but-checked — transient).
                _ => return Some("Checking linked repositories…".into()),
            }
        }
        None
    }

    /// The dialog's model/effort/mode choices as launch options.
    fn options(&self, cx: &App) -> LaunchOptions {
        LaunchOptions {
            model: selected(&self.model, cx),
            // Ignored by the argv while ultracode is on (ultracode IS the
            // effort level); blank = omit the flag.
            effort: selected(&self.effort, cx),
            ultracode: self.ultracode,
            plan_mode: self.plan_mode,
        }
    }

    /// Snapshot the checked set into a [`BatchLaunchRequest`] (2+ checked).
    /// `None` on a racing probe (the blocker just re-checked) — bail quietly.
    fn batch_request(&self, cx: &App) -> Option<BatchLaunchRequest> {
        let mut repo: Option<RepoGroup> = None;
        let mut issues: Vec<BatchIssueSpec> = Vec::new();
        for row in &self.rows {
            if !self.checked.contains(&row.issue_id) {
                continue;
            }
            let Some(RepoState::Ready(Some(resolved))) = self.repos.get(&row.issue_id) else {
                return None;
            };
            if repo.is_none() {
                repo = Some(RepoGroup {
                    repository_id: resolved.repository_id.clone(),
                    full_name: resolved.full_name.clone(),
                    default_branch: resolved.default_branch.clone(),
                });
            }
            issues.push(BatchIssueSpec {
                issue_id: row.issue_id.clone(),
                issue_identifier: row.identifier.clone(),
                title: row.title.clone(),
                description: row.description.clone(),
            });
        }
        Some(BatchLaunchRequest {
            batch_id: coding::new_batch_id(),
            workspace_id: self.workspace_id.clone(),
            repo: repo?,
            issues,
            device_label: coding::default_device_label(),
            origin: LaunchOrigin::Local,
            options: self.options(cx),
        })
    }

    /// The launch: 1 checked issue = the plain single-issue path, 2+ = a
    /// batch run. Prepare on the background executor, spawn on the foreground
    /// (the shared path).
    fn launch(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.launching || self.launch_blocker(cx).is_some() {
            return;
        }
        if self.checked.len() == 1 {
            let issue_id = self.checked.iter().next().cloned().expect("one checked");
            let options = self.options(cx);
            let Some((request, deps)) =
                coding_flow::build_launch(&issue_id, LaunchOrigin::Local, options, cx)
            else {
                self.error = Some("Sign in and wait for sync before starting a session.".into());
                cx.notify();
                return;
            };
            return self.run_prepare(
                PrepareRequest::Issue(request),
                deps,
                SessionSubject::Issue(issue_id),
                window,
                cx,
            );
        }
        let Some(request) = self.batch_request(cx) else {
            return;
        };
        let batch_id = request.batch_id.clone();
        let Some(deps) = coding_flow::build_batch_deps(cx) else {
            self.error = Some("Sign in and wait for sync before starting a session.".into());
            cx.notify();
            return;
        };
        self.run_prepare(
            PrepareRequest::Batch(request),
            deps,
            SessionSubject::Batch(batch_id),
            window,
            cx,
        );
    }

    /// Shared prepare→spawn tail for both modes: background
    /// [`coding::prepare`], then `coding_flow::spawn_into_window` on the
    /// foreground; a `Disabled` reason (or spawn error) renders inline.
    fn run_prepare(
        &mut self,
        request: PrepareRequest,
        deps: coding::CodingDeps,
        subject: SessionSubject,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.launching = true;
        self.error = None;
        cx.notify();

        cx.spawn_in(window, async move |this, window| {
            let prepared = window
                .background_executor()
                .spawn(async move { coding::prepare(&request, &deps) })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.launching = false;
                match prepared {
                    Ok(Prepared::Ready(prepared)) => {
                        match coding_flow::spawn_into_window(prepared, subject, window, cx) {
                            Ok(()) => {
                                window.close_dialog(cx);
                            }
                            Err(message) => this.error = Some(message.into()),
                        }
                    }
                    // Explain inline, never crash — the exact §7 copy.
                    Ok(Prepared::Disabled(reason)) => this.error = Some(reason.message().into()),
                    Err(err) => {
                        this.error =
                            Some(format!("Could not start the coding session: {err}").into())
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // -- render pieces --------------------------------------------------------

    /// One checklist row: checkbox + identifier + title (+ state hint or the
    /// probe's exclusion note).
    fn issue_row(&self, ix: usize, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let row = &self.rows[ix];
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let is_checked = self.checked.contains(&row.issue_id);
        // A checked row whose probe failed/excluded it renders the reason
        // (the probe auto-unchecks; this covers the transient frame).
        let probe_note: Option<SharedString> = match self.repos.get(&row.issue_id) {
            Some(RepoState::Ready(None)) => Some("no repository linked".into()),
            Some(RepoState::Error(err)) => {
                Some(format!("repository check failed: {err}").into())
            }
            Some(RepoState::Loading) if is_checked => Some("resolving repository…".into()),
            _ => None,
        };
        let toggle_id = row.issue_id.clone();

        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(
                Checkbox::new(SharedString::from(format!("sc-check-{}", row.issue_id)))
                    .checked(is_checked)
                    .on_click(cx.listener(move |this, on: &bool, _, cx| {
                        this.toggle_checked(toggle_id.clone(), *on, cx);
                    })),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(SharedString::from(row.identifier.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .truncate()
                    .text_color(theme.foreground)
                    .child(SharedString::from(row.title.clone())),
            )
            .when_some(
                probe_note.or_else(|| row.state_hint.map(SharedString::from)),
                |this, note| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(note),
                    )
                },
            )
            .into_any_element()
    }

    /// A labeled field column with an optional muted hint under the control.
    fn labeled_field(
        label: &'static str,
        field: gpui::AnyElement,
        hint: Option<&'static str>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .flex_1()
            .gap_1()
            .child(div().text_xs().text_color(muted).child(label))
            .child(field)
            .when_some(hint, |this, hint| {
                this.child(div().text_xs().text_color(muted.opacity(0.7)).child(hint))
            })
    }

    /// The shared "Plan mode" checkbox + its native-plan-mode hint.
    fn plan_mode_row(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_0p5()
            .child(
                Checkbox::new("sc-plan-mode")
                    .label("Plan mode")
                    .checked(self.plan_mode)
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.plan_mode = *on;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .pl_6()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Present a plan for approval before making changes \
                         (native Claude plan mode). After approving, \
                         Shift+Tab switches to skip-permissions for a \
                         prompt-free run.",
                    ),
            )
    }

    /// Footer: blocker copy + Cancel + Start.
    fn footer(
        &self,
        blocker: Option<SharedString>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let mut footer = h_flex().items_center().gap_2().pt_1();
        if let Some(reason) = &blocker {
            if !self.launching {
                footer = footer.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .truncate()
                        .text_color(cx.theme().muted_foreground)
                        .child(reason.clone()),
                );
            }
        }
        footer
            .child(div().flex_1())
            .child(
                Button::new("sc-cancel")
                    .outline()
                    .small()
                    .label("Cancel")
                    .disabled(self.launching)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.launching {
                            return;
                        }
                        window.close_dialog(cx);
                    })),
            )
            .child(
                Button::new("sc-start")
                    .primary()
                    .small()
                    .label(if self.launching {
                        "Starting…"
                    } else {
                        "Start coding"
                    })
                    .loading(self.launching)
                    .disabled(blocker.is_some())
                    .on_click(cx.listener(|this, _, window, cx| this.launch(window, cx))),
            )
    }
}

impl Render for StartCodingDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme_muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let warning = cx.theme().warning;
        let checked_count = self.checked.len();
        let ultracode = self.ultracode;

        // ---- searchable checklist: checked rows pinned first, then the
        //      unchecked search matches (capped — see MAX_UNCHECKED_ROWS) ----
        let query = self.search.read(cx).value().trim().to_lowercase();
        let mut checked_ixs: Vec<usize> = Vec::new();
        let mut match_ixs: Vec<usize> = Vec::new();
        for (ix, row) in self.rows.iter().enumerate() {
            if self.checked.contains(&row.issue_id) {
                checked_ixs.push(ix);
            } else if query.is_empty()
                || row.identifier.to_lowercase().contains(&query)
                || row.title.to_lowercase().contains(&query)
            {
                match_ixs.push(ix);
            }
        }
        let hidden = match_ixs.len().saturating_sub(MAX_UNCHECKED_ROWS);
        let no_matches = !query.is_empty() && match_ixs.is_empty() && !self.rows.is_empty();
        match_ixs.truncate(MAX_UNCHECKED_ROWS);

        let mut checklist = v_flex().gap_1();
        if self.rows.is_empty() {
            checklist = checklist.child(
                div()
                    .text_sm()
                    .text_color(theme_muted)
                    .child("No open issues in this project."),
            );
        }
        for ix in checked_ixs {
            checklist = checklist.child(self.issue_row(ix, cx));
        }
        for ix in match_ixs {
            checklist = checklist.child(self.issue_row(ix, cx));
        }
        if no_matches {
            // Without this the scoped pool renders a silently blank list —
            // the filter (open issues, this project only) is invisible.
            checklist = checklist.child(
                div()
                    .text_xs()
                    .text_color(theme_muted)
                    .child("No matches — only open issues from this project are shown."),
            );
        }
        if hidden > 0 {
            checklist = checklist.child(
                div()
                    .text_xs()
                    .text_color(theme_muted)
                    .child(SharedString::from(format!(
                        "+{hidden} more — refine your search."
                    ))),
            );
        }

        // ---- model/effort selects ----
        let main_row = h_flex()
            .gap_3()
            .w_full()
            // Top-align (h_flex centers): the Effort column grows an
            // "ultracode sets effort" hint line, which would otherwise sink
            // the Model label below the shared baseline.
            .items_start()
            .child(Self::labeled_field(
                "Model",
                Select::new(&self.model).small().into_any_element(),
                None,
                cx,
            ))
            .child(Self::labeled_field(
                "Effort",
                Select::new(&self.effort)
                    .small()
                    .disabled(ultracode)
                    .into_any_element(),
                ultracode.then_some("ultracode sets effort"),
                cx,
            ));

        // ---- toggles ----
        let toggles = v_flex()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(div().text_sm().child("Dynamic workflows (ultracode)"))
                            .child(div().text_xs().text_color(theme_muted).child(
                                "Runs Claude with --effort ultracode — works with any model.",
                            )),
                    )
                    .child(
                        Switch::new("sc-ultracode")
                            .checked(ultracode)
                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                this.ultracode = *on;
                                cx.notify();
                            })),
                    ),
            )
            .child(self.plan_mode_row(cx));

        let intro: SharedString = if checked_count >= 2 {
            format!(
                "One Claude session implements the {checked_count} checked issues on one \
                 branch and opens one combined PR."
            )
            .into()
        } else {
            "Claude works on the checked issue in its own worktree and opens the pull \
             request when done. Check more issues for a batch run."
                .into()
        };

        let blocker = self.launch_blocker(cx);
        let mut body = v_flex()
            .gap_3()
            .child(div().text_xs().text_color(theme_muted).child(intro))
            .child(Input::new(&self.search).small())
            // Bounded, actually-scrollable checklist (EXP-119): compose the
            // EXP-67 scroll-pane primitives directly — gpui-component's
            // `overflow_y_scrollbar` wrapper drops the wrapped element's
            // `max_h`, so the 240px bound never constrained the list and it
            // pushed the dialog body instead of scrolling.
            .child(
                div()
                    .relative()
                    .max_h(px(320.))
                    .child(
                        div()
                            .id("sc-issues-scroll")
                            .max_h(px(320.))
                            .overflow_y_scroll()
                            .track_scroll(&self.list_scroll)
                            .child(checklist),
                    )
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .child(
                                Scrollbar::new(&self.list_scroll)
                                    .axis(ScrollbarAxis::Vertical),
                            ),
                    ),
            )
            .child(main_row)
            .child(toggles);

        if checked_count > MAX_ISSUES_PER_RUN {
            body = body.child(div().text_xs().text_color(warning).child(SharedString::from(
                format!("At most {MAX_ISSUES_PER_RUN} issues per run — split the batch."),
            )));
        } else if checked_count > COST_NOTE_THRESHOLD {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(warning)
                    .child("Large batches can be token-expensive."),
            );
        }
        if let Some(error) = &self.error {
            body = body.child(div().text_sm().text_color(danger).child(error.clone()));
        }

        body.child(self.footer(blocker, cx)).into_any_element()
    }
}
