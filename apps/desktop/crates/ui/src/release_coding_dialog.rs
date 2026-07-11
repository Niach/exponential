//! Release coding dialog (EXP-56 P8) — the "Start coding" launcher for a
//! whole release, opened from the Releases tool window's detail header.
//!
//! Shape mirrors the other create dialogs (`window.open_dialog`, own footer):
//!
//! - an issue CHECKLIST grouped by resolved repository (per-issue
//!   `repositories.forIssue` probes on the background executor, like
//!   `StartCodingControl::ensure_probe`): repo-less issues are greyed out and
//!   excluded; `done`/PR-merged issues pre-uncheck but stay checkable;
//! - ONE repo group per run — while checked issues span more than one group
//!   the launch button is disabled with "One repository per run — deselect
//!   the others" (a workspace release may span repos; v1 launches one group
//!   at a time);
//! - MAIN + SUBAGENT model/effort inputs (defaults from
//!   [`coding::Settings`]), per-checked-issue override expanders;
//! - the ULTRACODE toggle (dynamic workflows — pins the main model to Opus;
//!   disabled when the installed CLI lacks `--settings`) and the AUTONOMOUS
//!   toggle (no plan approval gate);
//! - launch = snapshot → [`coding::prepare_release_launch`] on the background
//!   executor → the SAME `coding_flow::spawn_into_window` foreground spawn as
//!   the single-issue path, with [`SessionSubject::Release`]. A
//!   `Prepared::Disabled` reason renders inline and keeps the dialog open.

use std::collections::{HashMap, HashSet};

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement as _,
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
};
use sync::Store;

use api::repositories::IssueRepository;
use coding::{
    probe_claude_flags, ClaudeFlagSupport, LaunchOrigin, Prepared, ReleaseIssueSpec,
    ReleaseLaunchOptions, ReleaseLaunchRequest, RepoGroup, Tool,
};
use domain::IssueStatus;

use crate::coding_flow::{self, CodingHub, SessionSubject};
use crate::queries;

/// Soft cost warning threshold: more checked issues than this shows the
/// "token-expensive" note (no hard gate — coding is unmetered).
const COST_NOTE_THRESHOLD: usize = 6;

/// Open the dialog for `release_id`. A no-op when the release row isn't
/// synced (racing a delete).
pub fn open(window: &mut Window, cx: &mut App, release_id: String) {
    let Some(release) = Store::global(cx)
        .collections()
        .releases
        .read(cx)
        .get(&release_id)
        .cloned()
    else {
        log::warn!("[ui] release coding dialog for unknown release {release_id}");
        return;
    };
    let view = cx.new(|cx| ReleaseCodingDialogView::new(&release, window, cx));
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
/// re-reads the collections).
struct IssueRow {
    issue_id: String,
    identifier: String,
    title: String,
    description: Option<String>,
    /// The pre-uncheck reason (`done` / PR-merged / dropped rows start
    /// unchecked but stay checkable for a re-run), shown muted next to the
    /// title. `None` = default-checked.
    state_hint: Option<&'static str>,
    /// Per-issue subagent overrides (blank = inherit the dialog defaults).
    model_input: Entity<InputState>,
    effort_input: Entity<InputState>,
    expanded: bool,
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

pub struct ReleaseCodingDialogView {
    release_id: String,
    release_name: String,
    rows: Vec<IssueRow>,
    /// issue id → probe state.
    repos: HashMap<String, RepoState>,
    /// Stale-probe guard (a retry re-probes; old results must not land).
    probe_generation: u64,
    checked: HashSet<String>,
    main_model: Entity<InputState>,
    main_effort: Entity<InputState>,
    subagent_model: Entity<InputState>,
    subagent_effort: Entity<InputState>,
    ultracode: bool,
    autonomous: bool,
    /// Launch-flag support of the installed Claude CLI: the hub doctor's
    /// probe when its report targets Claude, else a background
    /// [`probe_claude_flags`] kicked at open. `None` = still unknown (the UI
    /// assumes support; the launcher degrades for real either way).
    flags: Option<ClaudeFlagSupport>,
    launching: bool,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl ReleaseCodingDialogView {
    fn new(
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
                    model_input: cx.new(|cx| InputState::new(window, cx).placeholder("inherit")),
                    effort_input: cx.new(|cx| InputState::new(window, cx).placeholder("inherit")),
                    expanded: false,
                }
            })
            .collect();

        // Model/effort prefills from the launcher settings (EXP-56 P8):
        // subagent model resolves blank → the Claude model.
        let main_model = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(coding::settings::DEFAULT_CLAUDE_MODEL)
                .default_value(settings.claude_model.clone())
        });
        let main_effort = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("CLI default")
                .default_value(settings.claude_effort.clone())
        });
        let subagent_model_default = if settings.subagent_model.trim().is_empty() {
            settings.claude_model.clone()
        } else {
            settings.subagent_model.clone()
        };
        let subagent_model = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(coding::settings::DEFAULT_CLAUDE_MODEL)
                .default_value(subagent_model_default)
        });
        let subagent_effort = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("inherit")
                .default_value(settings.subagent_effort.clone())
        });

        // Launch-flag support: reuse the hub doctor's probe when it targeted
        // Claude; under the codex opt-in (or before the first report) probe
        // the Claude binary directly in the background.
        let flags = hub
            .read(cx)
            .doctor
            .report
            .as_ref()
            .filter(|report| report.agent.tool == Tool::Claude)
            .map(|report| report.claude_flags);
        if flags.is_none() {
            let program = settings.resolved_claude_path();
            cx.spawn(async move |this, cx| {
                let flags = cx
                    .background_executor()
                    .spawn(async move { probe_claude_flags(&program) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.flags.is_none() {
                        this.flags = Some(flags);
                        cx.notify();
                    }
                });
            })
            .detach();
        }

        let subscriptions = vec![
            // Doctor lands / re-runs → the footer gate + flag state move.
            cx.observe(&hub, |this: &mut Self, hub, cx| {
                if this.flags.is_none() {
                    this.flags = hub
                        .read(cx)
                        .doctor
                        .report
                        .as_ref()
                        .filter(|report| report.agent.tool == Tool::Claude)
                        .map(|report| report.claude_flags);
                }
                cx.notify();
            }),
        ];

        let mut this = Self {
            release_id: release.id.clone(),
            release_name: release
                .name
                .clone()
                .unwrap_or_else(|| "Untitled release".to_string()),
            rows,
            repos: HashMap::new(),
            probe_generation: 0,
            checked,
            main_model,
            main_effort,
            subagent_model,
            subagent_effort,
            ultracode: settings.release_ultracode,
            autonomous: settings.release_autonomous,
            flags,
            launching: false,
            error: None,
            _subscriptions: subscriptions,
        };
        this.spawn_probes(cx);
        this
    }

    /// Kick one `repositories.forIssue` probe per row (background executor,
    /// generation-guarded like `StartCodingControl::ensure_probe`).
    fn spawn_probes(&mut self, cx: &mut gpui::Context<Self>) {
        self.probe_generation += 1;
        let generation = self.probe_generation;
        for row in &self.rows {
            let issue_id = row.issue_id.clone();
            let Some(trpc) = queries::trpc_client(cx) else {
                self.repos
                    .insert(issue_id, RepoState::Error("Not signed in.".to_string()));
                continue;
            };
            self.repos.insert(issue_id.clone(), RepoState::Loading);
            let probe_id = issue_id.clone();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api::repositories::for_issue(&trpc, &probe_id) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.probe_generation != generation {
                        return; // superseded by a retry
                    }
                    let state = match result {
                        Ok(repo) => RepoState::Ready(repo),
                        Err(err) => RepoState::Error(err.to_string()),
                    };
                    // Unresolvable issues can never launch — uncheck them.
                    if !matches!(state, RepoState::Ready(Some(_))) {
                        this.checked.remove(&issue_id);
                    }
                    this.repos.insert(issue_id.clone(), state);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    /// Whether the installed CLI supports `--settings` (ultracode). Unknown
    /// (`None`) counts as supported — the launcher degrades for real anyway.
    fn settings_flag_supported(&self) -> bool {
        self.flags.map(|flags| flags.settings).unwrap_or(true)
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
        let mut group: Option<&str> = None;
        for row in &self.rows {
            if !self.checked.contains(&row.issue_id) {
                continue;
            }
            match self.repos.get(&row.issue_id) {
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

    /// The launch: snapshot the checked group into a
    /// [`ReleaseLaunchRequest`], prepare on the background executor, spawn on
    /// the foreground (the shared release path).
    fn launch(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.launch_blocker(cx).is_some() {
            return;
        }

        let mut repo: Option<RepoGroup> = None;
        let mut issues: Vec<ReleaseIssueSpec> = Vec::new();
        for row in &self.rows {
            if !self.checked.contains(&row.issue_id) {
                continue;
            }
            let Some(RepoState::Ready(Some(resolved))) = self.repos.get(&row.issue_id) else {
                return; // blocker just re-checked; racing probe — bail quietly
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
                model_override: non_blank(&row.model_input.read(cx).value()),
                effort_override: non_blank(&row.effort_input.read(cx).value()),
            });
        }
        let Some(repo) = repo else { return };

        let hub = CodingHub::global(cx);
        let settings = hub.read(cx).settings.clone();
        let options = ReleaseLaunchOptions {
            main_model: non_blank(&self.main_model.read(cx).value())
                .unwrap_or_else(|| settings.claude_model.clone()),
            main_effort: non_blank(&self.main_effort.read(cx).value()),
            subagent_model: non_blank(&self.subagent_model.read(cx).value()).unwrap_or_default(),
            subagent_effort: non_blank(&self.subagent_effort.read(cx).value()),
            ultracode: self.ultracode && self.settings_flag_supported(),
            autonomous: self.autonomous,
        };
        let request = ReleaseLaunchRequest {
            release_id: self.release_id.clone(),
            release_name: self.release_name.clone(),
            repo,
            issues,
            device_label: coding::default_device_label(),
            origin: LaunchOrigin::Local,
            options,
        };
        let Some(deps) = coding_flow::build_release_deps(cx) else {
            self.error = Some("Sign in and wait for sync before starting a session.".into());
            cx.notify();
            return;
        };

        self.launching = true;
        self.error = None;
        cx.notify();

        let release_id = self.release_id.clone();
        cx.spawn_in(window, async move |this, window| {
            let prepared = window
                .background_executor()
                .spawn(async move { coding::prepare_release_launch(&request, &deps) })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.launching = false;
                match prepared {
                    Ok(Prepared::Ready(prepared)) => {
                        match coding_flow::spawn_into_window(
                            prepared,
                            SessionSubject::Release(release_id.clone()),
                            false,
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
                            Some(format!("Could not start the release session: {err}").into())
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    // -- render pieces --------------------------------------------------------

    /// One checklist row: checkbox + identifier + title (+ state hint), the
    /// override expander while checked, and the expanded override inputs.
    fn issue_row(
        &self,
        ix: usize,
        enabled: bool,
        trailing_note: Option<SharedString>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let row = &self.rows[ix];
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let checked = self.checked.contains(&row.issue_id);
        let toggle_id = row.issue_id.clone();

        let mut body = v_flex().w_full().gap_1().child(
            h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .child(
                    Checkbox::new(SharedString::from(format!("rc-check-{}", row.issue_id)))
                        .checked(checked)
                        .disabled(!enabled)
                        .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                            if *checked {
                                this.checked.insert(toggle_id.clone());
                            } else {
                                this.checked.remove(&toggle_id);
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
                    trailing_note.or_else(|| {
                        row.state_hint.map(SharedString::from)
                    }),
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
                .when(checked, |this| {
                    this.child(
                        Button::new(SharedString::from(format!("rc-expand-{}", row.issue_id)))
                            .ghost()
                            .xsmall()
                            .icon(Icon::new(if row.expanded {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            }))
                            .tooltip("Per-issue model/effort override")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.rows[ix].expanded = !this.rows[ix].expanded;
                                cx.notify();
                            })),
                    )
                }),
        );
        if checked && row.expanded {
            body = body.child(
                h_flex()
                    .w_full()
                    .pl_6()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("model"),
                    )
                    .child(div().w(px(110.)).child(Input::new(&row.model_input).small()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("effort"),
                    )
                    .child(div().w(px(110.)).child(Input::new(&row.effort_input).small())),
            );
        }
        body.into_any_element()
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

    fn labeled_field(
        label: &'static str,
        field: gpui::AnyElement,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        v_flex()
            .flex_1()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            )
            .child(field)
    }
}

/// Trimmed input value, `None` when blank (= inherit).
fn non_blank(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

impl Render for ReleaseCodingDialogView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme_muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let warning = cx.theme().warning;

        // ---- checklist, grouped by resolved repo ----
        struct GroupView {
            repo: IssueRepository,
            rows: Vec<usize>,
        }
        let mut groups: Vec<GroupView> = Vec::new();
        let mut pending: Vec<usize> = Vec::new();
        let mut excluded: Vec<(usize, SharedString)> = Vec::new();
        for (ix, row) in self.rows.iter().enumerate() {
            match self.repos.get(&row.issue_id) {
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
                    .any(|ix| self.checked.contains(&self.rows[*ix].issue_id))
            })
            .count();

        let mut checklist = v_flex().gap_1();
        if self.rows.is_empty() {
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

        // ---- model/effort sections ----
        let ultracode_supported = self.settings_flag_supported();
        let ultracode_on = self.ultracode && ultracode_supported;
        let main_row = h_flex().gap_3().w_full().children(if ultracode_on {
            vec![
                Self::labeled_field(
                    "Main model",
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded(cx.theme().radius)
                                .border_1()
                                .border_color(cx.theme().border)
                                .text_sm()
                                .text_color(theme_muted)
                                .child("opus"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme_muted)
                                .child("Dynamic workflows require Opus"),
                        )
                        .into_any_element(),
                    cx,
                )
                .into_any_element(),
            ]
        } else {
            vec![
                Self::labeled_field(
                    "Main model",
                    Input::new(&self.main_model).small().into_any_element(),
                    cx,
                )
                .into_any_element(),
                Self::labeled_field(
                    "Main effort",
                    Input::new(&self.main_effort).small().into_any_element(),
                    cx,
                )
                .into_any_element(),
            ]
        });
        let subagent_row = h_flex()
            .gap_3()
            .w_full()
            .child(Self::labeled_field(
                "Subagent model",
                Input::new(&self.subagent_model).small().into_any_element(),
                cx,
            ))
            .child(Self::labeled_field(
                "Subagent effort",
                Input::new(&self.subagent_effort).small().into_any_element(),
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
                            .child(
                                div().text_xs().text_color(theme_muted).child(
                                    if ultracode_supported {
                                        "Opus orchestration with dynamic workflows."
                                    } else {
                                        "update Claude Code to use dynamic workflows"
                                    },
                                ),
                            ),
                    )
                    .child(
                        Switch::new("rc-ultracode")
                            .checked(ultracode_on)
                            .disabled(!ultracode_supported)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.ultracode = *checked;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(div().text_sm().child("Autonomous (no plan approval gate)"))
                    .child(
                        Switch::new("rc-autonomous")
                            .checked(self.autonomous)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.autonomous = *checked;
                                cx.notify();
                            })),
                    ),
            );

        // ---- footer ----
        let blocker = self.launch_blocker(cx);
        let disabled = blocker.is_some();
        let mut footer = h_flex().items_center().gap_2().pt_1();
        if let Some(reason) = &blocker {
            if !self.launching {
                footer = footer.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .truncate()
                        .text_color(theme_muted)
                        .child(reason.clone()),
                );
            }
        }
        footer = footer
            .child(div().flex_1())
            .child(
                Button::new("rc-cancel")
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
                Button::new("rc-start")
                    .primary()
                    .small()
                    .label(if self.launching {
                        "Starting…"
                    } else {
                        "Start coding"
                    })
                    .loading(self.launching)
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, window, cx| this.launch(window, cx))),
            );

        let mut body = v_flex()
            .gap_3()
            .child(
                div()
                    .text_xs()
                    .text_color(theme_muted)
                    .child(SharedString::from(format!(
                        "One Claude orchestrator implements the checked issues of “{}” — one subagent per issue.",
                        self.release_name
                    ))),
            )
            .child(
                div()
                    .id("rc-issues-scroll")
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
        if self.checked.len() > COST_NOTE_THRESHOLD {
            body = body.child(div().text_xs().text_color(warning).child(
                "Large releases spawn many subagents — this can be token-expensive.",
            ));
        }
        if let Some(error) = &self.error {
            body = body.child(div().text_sm().text_color(danger).child(error.clone()));
        }

        body.child(footer)
    }
}
