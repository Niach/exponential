//! The ONE shared Start-coding dialog (release rework Phase 4) — the
//! single-issue Play button and the release-detail "Start coding" button both
//! land here (`open_for_issue` / `open_for_release`), replacing the instant
//! zero-options issue launch and the bespoke release dialog.
//!
//! Shared surface: Model + Effort [`ChoiceSelect`]s (defaults from
//! [`coding::Settings`]) and the "Plan mode" checkbox (NATIVE Claude plan
//! mode — `--permission-mode plan`; issue default ON, release default OFF)
//! with inline error rendering. The Issue variant adds the per-session
//! "Keep private" opt-out (moved out of `StartCodingControl`); the Release
//! variant adds:
//!
//! - an issue CHECKLIST grouped by resolved repository (per-issue
//!   `repositories.forIssue` probes on the background executor): repo-less
//!   issues are greyed out and excluded; `done`/`cancelled`/`duplicate`/
//!   PR-merged issues pre-uncheck but stay checkable;
//! - ONE repo group per run, and (fix F6) at most [`MAX_ISSUES_PER_RUN`]
//!   checked issues per run — `--agents` always rides argv and Windows caps
//!   the whole command line at 32,767 chars;
//! - Subagent model/effort selects and the ULTRACODE switch — enabled for
//!   EVERY model (`--effort ultracode` is model-independent, no Opus pin);
//!   while ON the main Effort select disables ("ultracode sets effort").
//!
//! Launch = snapshot → [`coding::prepare`] on the background executor → the
//! shared `coding_flow::spawn_into_window` foreground spawn. A
//! `Prepared::Disabled` reason renders inline and keeps the dialog open.

use std::collections::{HashMap, HashSet};

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    scroll::ScrollableElement as _,
    select::Select,
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
};
use sync::Store;

use api::repositories::IssueRepository;
use coding::{
    IssueLaunchOptions, LaunchOrigin, Prepared, PrepareRequest, ReleaseIssueSpec,
    ReleaseLaunchOptions, ReleaseLaunchRequest, RepoGroup,
};
use domain::IssueStatus;

use crate::coding_flow::{self, CodingHub, SessionSubject};
use crate::coding_selects::{
    choice_select, selected, ChoiceSelect, EFFORT_CHOICES, MODEL_CHOICES,
    SUBAGENT_EFFORT_CHOICES, SUBAGENT_MODEL_CHOICES,
};
use crate::queries;

/// Soft cost warning threshold: more checked issues than this shows the
/// "token-expensive" note (no hard gate — coding is unmetered).
const COST_NOTE_THRESHOLD: usize = 6;

/// FIX F6 hard cap: `--agents` always rides argv and every checked issue adds
/// a subagent definition — ~40 issues would blow Windows' 32,767-char command
/// line cap, so one run takes at most this many checked issues.
const MAX_ISSUES_PER_RUN: usize = 30;

/// Open the dialog for a SINGLE ISSUE. A no-op when the issue row isn't
/// synced (racing a delete).
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
    // §P7: only a live-public feedback board offers the keep-private opt-out.
    let live_public = coding_flow::issue_project(&issue_id, cx)
        .map(|project| project.is_live_public_coding())
        .unwrap_or(false);
    let title: SharedString = format!("Start coding on {}", issue.identifier).into();
    let view = cx.new(|cx| {
        StartCodingDialogView::new_issue(issue.id.clone(), issue.identifier, live_public, window, cx)
    });
    window.open_dialog(cx, move |dialog, _, cx| {
        let busy = view.read(cx).launching;
        dialog
            .w(px(420.))
            .title(title.clone())
            .overlay_closable(!busy)
            .keyboard(!busy)
            .child(view.clone())
    });
}

/// Open the dialog for a whole RELEASE. A no-op when the release row isn't
/// synced (racing a delete).
pub fn open_for_release(window: &mut Window, cx: &mut App, release_id: String) {
    let Some(release) = Store::global(cx)
        .collections()
        .releases
        .read(cx)
        .get(&release_id)
        .cloned()
    else {
        log::warn!("[ui] start-coding dialog for unknown release {release_id}");
        return;
    };
    let view = cx.new(|cx| StartCodingDialogView::new_release(&release, window, cx));
    window.open_dialog(cx, move |dialog, window, cx| {
        let busy = view.read(cx).launching;
        let max_height = window.viewport_size().height * 0.85;
        dialog
            .w(px(560.))
            .max_h(max_height)
            .title("Start coding on release")
            .overlay_closable(!busy)
            .keyboard(!busy)
            .child(view.clone())
    });
}

/// One checklist row, snapshotted from the sync store at open (titles and
/// descriptions ride into the launch request verbatim — the launcher never
/// re-reads the collections). Per-issue model/effort overrides are deleted
/// (the rework's hard cut) — subagents share the dialog-level defaults.
struct IssueRow {
    issue_id: String,
    identifier: String,
    title: String,
    description: Option<String>,
    /// The pre-uncheck reason (`done`/`cancelled`/`duplicate`/PR-merged rows
    /// start unchecked but stay checkable for a re-run), shown muted next to
    /// the title. `None` = default-checked.
    state_hint: Option<&'static str>,
}

/// One issue's `repositories.forIssue` probe state.
enum RepoState {
    Loading,
    /// `Ready(None)` = no repository linked (excluded from the run).
    Ready(Option<IssueRepository>),
    /// Transport failure — the issue can't resolve a group, so it is
    /// excluded like a repo-less one (the message says why).
    Error(String),
}

/// The two launch shapes the ONE dialog serves.
enum Variant {
    Issue {
        issue_id: String,
        identifier: String,
        /// The project streams coding publicly (`publicShowCoding='live'`).
        live_public: bool,
        /// §P7 per-session opt-out (only offered while `live_public`).
        keep_private: bool,
    },
    Release {
        release_id: String,
        release_name: String,
        rows: Vec<IssueRow>,
        /// issue id → probe state.
        repos: HashMap<String, RepoState>,
        /// Stale-probe guard (a retry re-probes; old results must not land).
        probe_generation: u64,
        checked: HashSet<String>,
        subagent_model: ChoiceSelect,
        subagent_effort: ChoiceSelect,
        /// Dynamic workflows (`--effort ultracode`) — any model, no pin.
        ultracode: bool,
    },
}

pub struct StartCodingDialogView {
    variant: Variant,
    model: ChoiceSelect,
    effort: ChoiceSelect,
    /// Native Claude plan mode (`--permission-mode plan`).
    plan_mode: bool,
    launching: bool,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl StartCodingDialogView {
    fn new_issue(
        issue_id: String,
        identifier: String,
        live_public: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let hub = CodingHub::global(cx);
        let settings = hub.read(cx).settings.clone();
        Self {
            variant: Variant::Issue {
                issue_id,
                identifier,
                live_public,
                keep_private: false,
            },
            model: choice_select(&MODEL_CHOICES, &settings.claude_model, window, cx),
            effort: choice_select(&EFFORT_CHOICES, &settings.claude_effort, window, cx),
            plan_mode: settings.issue_plan_mode,
            launching: false,
            error: None,
            _subscriptions: Vec::new(),
        }
    }

    fn new_release(
        release: &domain::rows::Release,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let hub = CodingHub::global(cx);
        let settings = hub.read(cx).settings.clone();

        // Snapshot the member issues (stable order: project, then number).
        let mut issues = queries::release_issues(cx, &release.id);
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
                if state_hint.is_none() {
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

        let subscriptions = vec![
            // Doctor lands / re-runs → the footer gate moves.
            cx.observe(&hub, |_: &mut Self, _, cx| cx.notify()),
        ];

        let mut this = Self {
            variant: Variant::Release {
                release_id: release.id.clone(),
                release_name: release
                    .name
                    .clone()
                    .unwrap_or_else(|| "Untitled release".to_string()),
                rows,
                repos: HashMap::new(),
                probe_generation: 0,
                checked,
                subagent_model: choice_select(
                    &SUBAGENT_MODEL_CHOICES,
                    &settings.subagent_model,
                    window,
                    cx,
                ),
                subagent_effort: choice_select(
                    &SUBAGENT_EFFORT_CHOICES,
                    &settings.subagent_effort,
                    window,
                    cx,
                ),
                ultracode: settings.release_ultracode,
            },
            model: choice_select(&MODEL_CHOICES, &settings.claude_model, window, cx),
            effort: choice_select(&EFFORT_CHOICES, &settings.claude_effort, window, cx),
            plan_mode: settings.release_plan_mode,
            launching: false,
            error: None,
            _subscriptions: subscriptions,
        };
        this.spawn_probes(cx);
        this
    }

    /// Kick one `repositories.forIssue` probe per checklist row (background
    /// executor, generation-guarded like `StartCodingControl::ensure_probe`).
    fn spawn_probes(&mut self, cx: &mut gpui::Context<Self>) {
        let Variant::Release {
            rows,
            repos,
            probe_generation,
            ..
        } = &mut self.variant
        else {
            return;
        };
        *probe_generation += 1;
        let generation = *probe_generation;
        let row_ids: Vec<String> = rows.iter().map(|row| row.issue_id.clone()).collect();
        for issue_id in row_ids {
            let Some(trpc) = queries::trpc_client(cx) else {
                repos.insert(issue_id, RepoState::Error("Not signed in.".to_string()));
                continue;
            };
            repos.insert(issue_id.clone(), RepoState::Loading);
            let probe_id = issue_id.clone();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api::repositories::for_issue(&trpc, &probe_id) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    let Variant::Release {
                        repos,
                        probe_generation,
                        checked,
                        ..
                    } = &mut this.variant
                    else {
                        return;
                    };
                    if *probe_generation != generation {
                        return; // superseded by a retry
                    }
                    let state = match result {
                        Ok(repo) => RepoState::Ready(repo),
                        Err(err) => RepoState::Error(err.to_string()),
                    };
                    // Unresolvable issues can never launch — uncheck them.
                    if !matches!(state, RepoState::Ready(Some(_))) {
                        checked.remove(&issue_id);
                    }
                    repos.insert(issue_id.clone(), state);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// Why the launch button is disabled right now; `None` = launchable.
    /// Issue runs have no checklist to gate — the shared prepare re-checks
    /// the doctor/repo and renders any `DisabledReason` inline.
    fn launch_blocker(&self, cx: &mut gpui::Context<Self>) -> Option<SharedString> {
        if self.launching {
            return Some("Starting…".into());
        }
        let Variant::Release {
            rows,
            repos,
            checked,
            ..
        } = &self.variant
        else {
            return None;
        };
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
        if checked.is_empty() {
            return Some("Select at least one issue.".into());
        }
        // FIX F6: the per-run argv cap (see MAX_ISSUES_PER_RUN).
        if checked.len() > MAX_ISSUES_PER_RUN {
            return Some(
                format!(
                    "At most {MAX_ISSUES_PER_RUN} issues per run — split the release run."
                )
                .into(),
            );
        }
        let mut group: Option<&str> = None;
        for row in rows {
            if !checked.contains(&row.issue_id) {
                continue;
            }
            match repos.get(&row.issue_id) {
                Some(RepoState::Ready(Some(repo))) => match group {
                    None => group = Some(&repo.repository_id),
                    Some(existing) if existing == repo.repository_id => {}
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

    /// Snapshot the checked group into a [`ReleaseLaunchRequest`]. `None` on
    /// a racing probe (the blocker just re-checked) — bail quietly.
    fn release_request(&self, cx: &App) -> Option<ReleaseLaunchRequest> {
        let Variant::Release {
            release_id,
            release_name,
            rows,
            repos,
            checked,
            subagent_model,
            subagent_effort,
            ultracode,
            ..
        } = &self.variant
        else {
            return None;
        };
        let mut repo: Option<RepoGroup> = None;
        let mut issues: Vec<ReleaseIssueSpec> = Vec::new();
        for row in rows {
            if !checked.contains(&row.issue_id) {
                continue;
            }
            let Some(RepoState::Ready(Some(resolved))) = repos.get(&row.issue_id) else {
                return None;
            };
            if repo.is_none() {
                repo = Some(RepoGroup {
                    repository_id: resolved.repository_id.clone(),
                    full_name: resolved.full_name.clone(),
                    default_branch: resolved.default_branch.clone(),
                });
            }
            issues.push(ReleaseIssueSpec {
                issue_id: row.issue_id.clone(),
                issue_identifier: row.identifier.clone(),
                title: row.title.clone(),
                description: row.description.clone(),
            });
        }
        let options = ReleaseLaunchOptions {
            main_model: selected(&self.model, cx),
            // Ignored by the argv while ultracode is on (ultracode IS the
            // effort level); blank = omit the flag.
            main_effort: non_blank(&selected(&self.effort, cx)),
            subagent_model: selected(subagent_model, cx),
            subagent_effort: non_blank(&selected(subagent_effort, cx)),
            ultracode: *ultracode,
            plan_mode: self.plan_mode,
        };
        Some(ReleaseLaunchRequest {
            release_id: release_id.clone(),
            release_name: release_name.clone(),
            repo: repo?,
            issues,
            device_label: coding::default_device_label(),
            origin: LaunchOrigin::Local,
            options,
        })
    }

    /// The launch: build the per-variant [`PrepareRequest`], prepare on the
    /// background executor, spawn on the foreground (the shared path).
    fn launch(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.launching {
            return;
        }
        let issue_launch = match &self.variant {
            Variant::Issue {
                issue_id,
                keep_private,
                ..
            } => Some((issue_id.clone(), *keep_private)),
            Variant::Release { .. } => None,
        };
        let (request, subject, keep_private) = match issue_launch {
            Some((issue_id, keep_private)) => {
                let options = IssueLaunchOptions {
                    model: selected(&self.model, cx),
                    effort: selected(&self.effort, cx),
                    plan_mode: self.plan_mode,
                };
                let Some((request, deps)) =
                    coding_flow::build_launch(&issue_id, LaunchOrigin::Local, options, cx)
                else {
                    self.error =
                        Some("Sign in and wait for sync before starting a session.".into());
                    cx.notify();
                    return;
                };
                return self.run_prepare(
                    PrepareRequest::Issue(request),
                    deps,
                    SessionSubject::Issue(issue_id),
                    keep_private,
                    window,
                    cx,
                );
            }
            None => {
                if self.launch_blocker(cx).is_some() {
                    return;
                }
                let Some(request) = self.release_request(cx) else {
                    return;
                };
                let release_id = request.release_id.clone();
                (request, SessionSubject::Release(release_id), false)
            }
        };
        let Some(deps) = coding_flow::build_release_deps(cx) else {
            self.error = Some("Sign in and wait for sync before starting a session.".into());
            cx.notify();
            return;
        };
        self.run_prepare(
            PrepareRequest::Release(request),
            deps,
            subject,
            keep_private,
            window,
            cx,
        );
    }

    /// Shared prepare→spawn tail for both variants: background
    /// [`coding::prepare`], then `coding_flow::spawn_into_window` on the
    /// foreground; a `Disabled` reason (or spawn error) renders inline.
    fn run_prepare(
        &mut self,
        request: PrepareRequest,
        deps: coding::CodingDeps,
        subject: SessionSubject,
        keep_private: bool,
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
                        match coding_flow::spawn_into_window(
                            prepared,
                            subject,
                            keep_private,
                            window,
                            cx,
                        ) {
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

    /// One checklist row: checkbox + identifier + title (+ state hint).
    fn issue_row(
        &self,
        ix: usize,
        enabled: bool,
        trailing_note: Option<SharedString>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let Variant::Release { rows, checked, .. } = &self.variant else {
            return div().into_any_element();
        };
        let row = &rows[ix];
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let is_checked = checked.contains(&row.issue_id);
        let toggle_id = row.issue_id.clone();

        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(
                Checkbox::new(SharedString::from(format!("sc-check-{}", row.issue_id)))
                    .checked(is_checked)
                    .disabled(!enabled)
                    .on_click(cx.listener(move |this, on: &bool, _, cx| {
                        let Variant::Release { checked, .. } = &mut this.variant else {
                            return;
                        };
                        if *on {
                            checked.insert(toggle_id.clone());
                        } else {
                            checked.remove(&toggle_id);
                        }
                        cx.notify();
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
                    .text_color(if enabled { theme.foreground } else { muted })
                    .child(SharedString::from(row.title.clone())),
            )
            .when_some(
                trailing_note.or_else(|| row.state_hint.map(SharedString::from)),
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

    /// A muted group/section header line inside the checklist.
    fn group_header(label: SharedString, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        div()
            .pt_1()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().muted_foreground)
            .child(label)
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

    /// Footer: blocker copy (release) + Cancel + Start.
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

    fn render_issue(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let (identifier, live_public, keep_private) = match &self.variant {
            Variant::Issue {
                identifier,
                live_public,
                keep_private,
                ..
            } => (identifier.clone(), *live_public, *keep_private),
            Variant::Release { .. } => return div().into_any_element(),
        };
        let muted = cx.theme().muted_foreground;

        let mut body = v_flex()
            .gap_3()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(format!(
                        "Claude works on {identifier} in its own worktree and opens the \
                         pull request when done."
                    ))),
            )
            .child(
                h_flex()
                    .gap_3()
                    .w_full()
                    .child(Self::labeled_field(
                        "Model",
                        Select::new(&self.model).small().into_any_element(),
                        None,
                        cx,
                    ))
                    .child(Self::labeled_field(
                        "Effort",
                        Select::new(&self.effort).small().into_any_element(),
                        None,
                        cx,
                    )),
            )
            .child(self.plan_mode_row(cx));

        // §P7 opt-out: on a live feedback board, let the user keep THIS
        // session out of the public activity stream before starting it.
        if live_public {
            body = body.child(
                v_flex()
                    .gap_0p5()
                    .child(
                        Checkbox::new("sc-keep-private")
                            .label("Keep private")
                            .checked(keep_private)
                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                if let Variant::Issue { keep_private, .. } = &mut this.variant {
                                    *keep_private = *on;
                                    cx.notify();
                                }
                            })),
                    )
                    .child(div().pl_6().text_xs().text_color(muted).child(
                        "This project streams coding sessions publicly. Check to keep \
                         this session out of the public view.",
                    )),
            );
        }

        if let Some(error) = &self.error {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        let blocker = self.launching.then(|| SharedString::from("Starting…"));
        body.child(self.footer(blocker, cx)).into_any_element()
    }

    fn render_release(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let theme_muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let warning = cx.theme().warning;

        // ---- checklist, grouped by resolved repo ----
        struct GroupView {
            repo: IssueRepository,
            rows: Vec<usize>,
        }
        let (release_name, ultracode, checked_count, checked_groups, groups, pending, excluded) = {
            let Variant::Release {
                release_name,
                rows,
                repos,
                checked,
                ultracode,
                ..
            } = &self.variant
            else {
                return div().into_any_element();
            };
            let mut groups: Vec<GroupView> = Vec::new();
            let mut pending: Vec<usize> = Vec::new();
            let mut excluded: Vec<(usize, SharedString)> = Vec::new();
            for (ix, row) in rows.iter().enumerate() {
                match repos.get(&row.issue_id) {
                    None | Some(RepoState::Loading) => pending.push(ix),
                    Some(RepoState::Ready(None)) => {
                        excluded.push((ix, "no repository linked".into()))
                    }
                    Some(RepoState::Error(err)) => {
                        excluded.push((ix, format!("repository check failed: {err}").into()))
                    }
                    Some(RepoState::Ready(Some(repo))) => {
                        match groups
                            .iter_mut()
                            .find(|group| group.repo.repository_id == repo.repository_id)
                        {
                            Some(group) => group.rows.push(ix),
                            None => groups.push(GroupView {
                                repo: repo.clone(),
                                rows: vec![ix],
                            }),
                        }
                    }
                }
            }
            let checked_groups = groups
                .iter()
                .filter(|group| {
                    group
                        .rows
                        .iter()
                        .any(|ix| checked.contains(&rows[*ix].issue_id))
                })
                .count();
            (
                release_name.clone(),
                *ultracode,
                checked.len(),
                checked_groups,
                groups,
                pending,
                excluded,
            )
        };

        let rows_empty = matches!(&self.variant, Variant::Release { rows, .. } if rows.is_empty());
        let mut checklist = v_flex().gap_1();
        if rows_empty {
            checklist = checklist.child(
                div()
                    .text_sm()
                    .text_color(theme_muted)
                    .child("This release has no issues."),
            );
        }
        for group in &groups {
            checklist = checklist.child(Self::group_header(
                SharedString::from(group.repo.full_name.clone()),
                cx,
            ));
            for &ix in &group.rows {
                checklist = checklist.child(self.issue_row(ix, true, None, cx));
            }
        }
        if !pending.is_empty() {
            checklist = checklist.child(Self::group_header("Resolving repository…".into(), cx));
            for ix in pending {
                checklist = checklist.child(self.issue_row(ix, true, None, cx));
            }
        }
        for (ix, note) in excluded {
            checklist = checklist.child(self.issue_row(ix, false, Some(note), cx));
        }

        // ---- model/effort selects (main + subagent) ----
        let main_row = h_flex()
            .gap_3()
            .w_full()
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
        let subagent_row = {
            let Variant::Release {
                subagent_model,
                subagent_effort,
                ..
            } = &self.variant
            else {
                return div().into_any_element();
            };
            h_flex()
                .gap_3()
                .w_full()
                .child(Self::labeled_field(
                    "Subagent model",
                    Select::new(subagent_model).small().into_any_element(),
                    None,
                    cx,
                ))
                .child(Self::labeled_field(
                    "Subagent effort",
                    Select::new(subagent_effort).small().into_any_element(),
                    None,
                    cx,
                ))
        };

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
                                "Runs the orchestrator with --effort ultracode — works \
                                 with any model.",
                            )),
                    )
                    .child(
                        Switch::new("sc-ultracode")
                            .checked(ultracode)
                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                if let Variant::Release { ultracode, .. } = &mut this.variant {
                                    *ultracode = *on;
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(self.plan_mode_row(cx));

        let blocker = self.launch_blocker(cx);
        let mut body = v_flex()
            .gap_3()
            .child(
                div()
                    .text_xs()
                    .text_color(theme_muted)
                    .child(SharedString::from(format!(
                        "One Claude orchestrator implements the checked issues of \
                         “{release_name}” — one subagent per issue.",
                    ))),
            )
            .child(
                div()
                    .id("sc-issues-scroll")
                    .max_h(px(240.))
                    .overflow_y_scrollbar()
                    .child(checklist),
            )
            .child(main_row)
            .child(subagent_row)
            .child(toggles);

        if checked_groups > 1 {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(warning)
                    .child("One repository per run — deselect the others."),
            );
        }
        if checked_count > MAX_ISSUES_PER_RUN {
            body = body.child(div().text_xs().text_color(warning).child(
                SharedString::from(format!(
                    "At most {MAX_ISSUES_PER_RUN} issues per run — split the release run \
                     across several sessions.",
                )),
            ));
        } else if checked_count > COST_NOTE_THRESHOLD {
            body = body.child(div().text_xs().text_color(warning).child(
                "Large releases spawn many subagents — this can be token-expensive.",
            ));
        }
        if let Some(error) = &self.error {
            body = body.child(div().text_sm().text_color(danger).child(error.clone()));
        }

        body.child(self.footer(blocker, cx)).into_any_element()
    }
}

/// Trimmed value, `None` when blank (= omit the flag / inherit).
fn non_blank(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

impl Render for StartCodingDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if matches!(self.variant, Variant::Issue { .. }) {
            self.render_issue(cx)
        } else {
            self.render_release(cx)
        }
    }
}
