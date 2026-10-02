//! EXP-1121 — the IDE's "Ready to code?" checklist behind Start coding.
//!
//! The model is `domain::coding_readiness` (fixture-locked ×4, copy
//! included); this module feeds it from what the IDE already has and draws
//! it:
//!
//! - **GitHub** — `integrations.github.status` + `repositories.list` for the
//!   board's team, asked only while the board has no repository (a failed
//!   read counts as CONNECTED: never a false "not connected");
//! - **Repository** — the synced board row's `repository_id`, labelled by the
//!   control's `repositories.forIssue` probe (`""` until it lands);
//! - **Device** — THIS machine, always online: the IDE can always start a
//!   run on itself (`queries::launch_devices`' rule), so the device step is
//!   met here by construction. Its fixes still map (Open Devices → the
//!   Devices view, Set up a server → copy the install one-liner) and "Get
//!   the desktop app" is dropped — this IS the desktop app.
//!
//! Remote start is not a gate on the desktop (`remoteStartEnabled: true`):
//! the local machine needs no relay.
//!
//! Not ready = an amber dot + the caption + a DASHED amber capsule; a click
//! on any of them opens the popover. The popover's rows tick live from the
//! synced rows (board, devices) the control observes; the footer's Start
//! coding is the control's own launch.

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, ClickEvent, Entity,
    FontWeight, InteractiveElement as _, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::InputState,
    popover::PopoverState,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use serde::{Deserialize, Serialize};

use domain::coding_readiness::{
    copy, picker_used_by, CodingReadiness, CodingReadinessInput,
    ReadinessDevice, ReadinessFix, ReadinessGithub, ReadinessStep, ReadinessStepKey,
    ReadinessStepState,
};

use crate::coding_flow::StartCodingControl;
use crate::controls::{search_field, SearchFieldSize};
use crate::icons::registry;

/// The popover's width (the mockup's card).
pub(crate) const POPOVER_W: f32 = 348.;

// ---------------------------------------------------------------------------
// Server reads
// ---------------------------------------------------------------------------

/// One connected repo of the team (`repositories.list`, only the fields the
/// picker draws).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TeamRepo {
    pub id: String,
    /// `owner/name`.
    pub full_name: String,
    #[serde(default)]
    pub boards: Vec<TeamRepoBoard>,
}

/// A board a connected repo backs.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct TeamRepoBoard {
    pub id: String,
    pub name: String,
}

fn fetch_team_repos(trpc: &api::TrpcClient, team_id: &str) -> Result<Vec<TeamRepo>, api::ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.query_with_input("repositories.list", &Input { team_id })
}

/// What the GitHub step (and the repository picker) read for one team.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GithubFacts {
    pub connected: bool,
    /// The connected account (`acme-inc`), the met row's detail.
    pub label: Option<String>,
    /// `None` = the read failed (the picker offers a retry).
    pub repos: Option<Vec<TeamRepo>>,
}

/// Blocking: both reads for `team_id` (background executor only).
pub(crate) fn fetch_github_facts(trpc: &api::TrpcClient, team_id: &str) -> GithubFacts {
    let status = crate::github_connect::fetch_github_status(trpc, team_id);
    let repos = fetch_team_repos(trpc, team_id);
    match (status, repos) {
        (Ok(status), Ok(repos)) => GithubFacts {
            connected: status.installed || !status.installations.is_empty() || !repos.is_empty(),
            label: status
                .installations
                .iter()
                .find_map(|inst| inst.account_login.clone())
                .filter(|login| !login.is_empty()),
            repos: Some(repos),
        },
        (status, repos) => {
            if let Err(err) = &status {
                log::warn!("[ui] readiness: integrations.github.status failed: {err}");
            }
            // Never a false "not connected" — the repository step (and its
            // picker's retry) carries the failure instead.
            GithubFacts {
                connected: true,
                label: None,
                repos: repos.ok(),
            }
        }
    }
}

/// The team's GitHub facts, per team.
pub(crate) enum GithubProbe {
    Idle,
    Loading { team_id: String },
    Ready { team_id: String, facts: GithubFacts },
}

impl GithubProbe {
    pub(crate) fn facts_for(&self, team_id: &str) -> Option<&GithubFacts> {
        match self {
            GithubProbe::Ready { team_id: id, facts } if id == team_id => Some(facts),
            _ => None,
        }
    }

    pub(crate) fn covers(&self, team_id: &str) -> bool {
        match self {
            GithubProbe::Idle => false,
            GithubProbe::Loading { team_id: id } | GithubProbe::Ready { team_id: id, .. } => {
                id == team_id
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Pure pieces
// ---------------------------------------------------------------------------

/// The board the checklist is about, as the model and the fixes need it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ReadinessSubject {
    pub team_id: String,
    pub team_name: String,
    pub board_id: String,
    pub board_name: String,
    pub board_slug: String,
    /// `boards.repository_id`.
    pub repository_id: Option<String>,
}

/// The model input on the desktop (see the module doc for the device and
/// remote-start rules).
pub(crate) fn readiness_input(
    subject: &ReadinessSubject,
    probed_full_name: Option<&str>,
    github: Option<&GithubFacts>,
    local_device_label: &str,
    now_ms: i64,
) -> CodingReadinessInput {
    CodingReadinessInput {
        is_member: true,
        remote_start_enabled: Some(true),
        team_name: subject.team_name.clone(),
        board_name: subject.board_name.clone(),
        board_repository: subject
            .repository_id
            .as_ref()
            .map(|_| probed_full_name.unwrap_or_default().to_string()),
        github: github.map(|facts| ReadinessGithub {
            connected: facts.connected,
            label: facts.label.clone(),
        }),
        devices: Some(vec![ReadinessDevice {
            label: local_device_label.to_string(),
            own: true,
            online: true,
            last_seen_at_ms: Some(now_ms),
        }]),
        now_ms,
    }
}

/// The fixes the desktop shows for a step: the model's list minus "Get the
/// desktop app" (this IS the desktop app), and minus "Board settings" for a
/// non-owner — that pane is owner-only (`settings::section_visible`), so it
/// would land them on Members (web `FixButtons` hides it the same way).
pub(crate) fn desktop_fixes(step: &ReadinessStep, owner: bool) -> Vec<ReadinessFix> {
    step.fixes
        .iter()
        .copied()
        .filter(|fix| *fix != ReadinessFix::GetDesktopApp)
        .filter(|fix| owner || *fix != ReadinessFix::BoardSettings)
        .collect()
}

/// One picker row: the repo's index in the team list and its hint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PickerRow {
    pub index: usize,
    pub hint: Option<String>,
}

/// The repository's own name (after `/`) IS the board's name or slug,
/// case-insensitively — web `repoMatchesBoard`, fixture `picker` ×4.
fn repo_matches_board(full_name: &str, board_name: &str, board_slug: &str) -> bool {
    let name = full_name.rsplit('/').next().unwrap_or(full_name).to_lowercase();
    !name.is_empty() && (name == board_name.to_lowercase() || name == board_slug.to_lowercase())
}

/// The picker's rows: repos whose NAME matches the board's first ("matches
/// board"), both halves in server order, the rest hinting the first OTHER
/// board they back ("used by Website"); `query` filters on the full name.
/// Web `readinessRepoRows`, fixture-locked ×4.
pub(crate) fn picker_rows(
    repos: &[TeamRepo],
    board_id: &str,
    board_name: &str,
    board_slug: &str,
    query: &str,
) -> Vec<PickerRow> {
    let query = query.trim().to_lowercase();
    let mut matched = Vec::new();
    let mut rest = Vec::new();
    for (index, repo) in repos.iter().enumerate() {
        if !query.is_empty() && !repo.full_name.to_lowercase().contains(&query) {
            continue;
        }
        if repo_matches_board(&repo.full_name, board_name, board_slug) {
            matched.push(PickerRow {
                index,
                hint: Some(copy::PICKER_MATCHES_BOARD.to_string()),
            });
        } else {
            let hint = repo
                .boards
                .iter()
                .find(|other| other.id != board_id)
                .map(|other| picker_used_by(&other.name));
            rest.push(PickerRow { index, hint });
        }
    }
    matched.extend(rest);
    matched
}

// ---------------------------------------------------------------------------
// The not-ready trigger
// ---------------------------------------------------------------------------

/// `[amber dot] Needs a repository [- - Start coding - -]`, one click target
/// wrapped so `Popover::trigger` takes it (a `Selectable` painting nothing of
/// its own).
#[derive(IntoElement)]
pub(crate) struct ReadinessTrigger {
    caption: Option<SharedString>,
    selected: bool,
}

impl ReadinessTrigger {
    pub(crate) fn new(caption: Option<String>) -> Self {
        Self {
            caption: caption.map(SharedString::from),
            selected: false,
        }
    }
}

impl gpui_component::Selectable for ReadinessTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl RenderOnce for ReadinessTrigger {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let warning = cx.theme().warning;
        let muted = cx.theme().muted_foreground;
        let size = crate::surface::PillSize::Sm;
        let capsule = crate::surface::glass_pill(
            "start-coding-readiness-pill",
            size,
            crate::surface::PillMode::Action,
            cx,
        )
        // Dashed amber only while a step is missing (the caption names it);
        // a popover left open past the last tick draws the plain pill.
        .when(self.caption.is_some(), |pill| {
            pill.border_dashed()
                .border_color(warning.opacity(if self.selected { 0.9 } else { 0.6 }))
        })
        .child(
            Icon::new(registry::ACTION_RUN)
                .with_size(px(size.glyph()))
                .text_color(muted),
        )
        .child(copy::START);
        h_flex()
            .id("start-coding-readiness")
            .gap_2()
            .items_center()
            .cursor_pointer()
            .when_some(self.caption, |row, caption| {
                row.child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .text_xs()
                        .text_color(warning)
                        .child(crate::surface::pill_dot(warning))
                        .child(caption),
                )
            })
            .child(capsule)
    }
}

// ---------------------------------------------------------------------------
// The popover
// ---------------------------------------------------------------------------

/// A step's glyph inside its ring.
fn step_glyph(key: ReadinessStepKey) -> crate::icons::ExpIcon {
    match key {
        ReadinessStepKey::Github => registry::UI_GITHUB,
        ReadinessStepKey::Repository => registry::UI_BRANCH,
        ReadinessStepKey::Device => registry::NAV_DEVICES,
    }
}

/// The 20px ring every row leads with: green tick (met), amber ring (current),
/// dashed grey ring (pending).
fn step_ring(step: &ReadinessStep, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let ring = div()
        .flex_shrink_0()
        .size(px(20.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .border_1();
    match step.state {
        ReadinessStepState::Met => ring
            .border_color(theme.success.opacity(0.35))
            .bg(theme.success.opacity(0.15))
            .child(Icon::new(registry::UI_CHECK).size_3().text_color(theme.success)),
        ReadinessStepState::Current => ring
            .border_color(theme.warning)
            .child(Icon::new(step_glyph(step.key)).size_3().text_color(theme.warning)),
        ReadinessStepState::Pending => ring
            .border_dashed()
            .border_color(theme.muted_foreground.opacity(0.6))
            .child(Icon::new(step_glyph(step.key)).size_3().text_color(theme.muted_foreground)),
    }
    .into_any_element()
}

/// The three-segment progress bar under the summary.
fn progress(readiness: &CodingReadiness, cx: &App) -> AnyElement {
    let theme = cx.theme();
    h_flex()
        .w_full()
        .gap_1()
        .children(readiness.steps.iter().map(|step| {
            let color = match step.state {
                ReadinessStepState::Met => theme.success,
                ReadinessStepState::Current => theme.warning,
                ReadinessStepState::Pending => theme.border,
            };
            div().flex_1().h(px(3.)).rounded_full().bg(color)
        }))
        .into_any_element()
}

/// What the popover needs from the control, read fresh on every render.
pub(crate) struct PopoverFacts {
    pub readiness: CodingReadiness,
    pub subject: ReadinessSubject,
    pub picker_open: bool,
    pub linking: bool,
    pub link_error: Option<SharedString>,
    pub repos: Option<Option<Vec<TeamRepo>>>,
    pub query: Option<Entity<InputState>>,
    pub launch_blocked: Option<SharedString>,
}

/// The popover body. `control` is the owning [`StartCodingControl`].
pub(crate) fn render_popover(
    control: &Entity<StartCodingControl>,
    state: &mut PopoverState,
    window: &mut Window,
    cx: &mut gpui::Context<PopoverState>,
) -> AnyElement {
    let _ = state;
    let Some(facts) = StartCodingControl::popover_facts(control, cx) else {
        return div().into_any_element();
    };
    let popover = cx.entity();
    let theme = cx.theme().clone();
    let (fg, muted) = (theme.foreground, theme.muted_foreground);

    let header = v_flex()
        .gap_2()
        .px_4()
        .pt_3()
        .pb_3()
        .child(
            h_flex()
                .items_start()
                .gap_2()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(fg)
                                .child(copy::TITLE),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(SharedString::from(facts.readiness.summary.clone())),
                        ),
                )
                .child({
                    let popover = popover.clone();
                    Button::new("readiness-close")
                        .ghost()
                        .xsmall()
                        .icon(Icon::new(registry::UI_CLOSE).text_color(muted))
                        .tooltip(copy::CLOSE)
                        .on_click(move |_, window, cx| {
                            popover.update(cx, |state, cx| state.dismiss(window, cx));
                        })
                }),
        )
        .child(progress(&facts.readiness, cx));

    let mut rows = v_flex().w_full();
    for step in &facts.readiness.steps {
        rows = rows.child(render_step(step, &facts, control, &popover, window, cx));
    }

    let ready = facts.readiness.ready && facts.launch_blocked.is_none();
    let footer = div().w_full().px_4().py_3().border_t_1().border_color(theme.border).child({
        let control = control.clone();
        let popover = popover.clone();
        let mut start = if ready {
            crate::surface::glass_pill_button_primary(
                "readiness-start",
                crate::surface::PillSize::Md,
            )
        } else {
            crate::surface::glass_pill_button("readiness-start", crate::surface::PillSize::Md, cx)
        }
        .w_full()
        .icon(Icon::new(registry::ACTION_RUN).text_color(if ready {
            theme.primary_foreground
        } else {
            muted
        }))
        .label(copy::START)
        .disabled(!ready);
        if let Some(reason) = facts.launch_blocked.clone().filter(|_| facts.readiness.ready) {
            start = start.tooltip(reason);
        }
        start.on_click(move |_, window, cx| {
            popover.update(cx, |state, cx| state.dismiss(window, cx));
            control.update(cx, |this, cx| this.launch_from_readiness(window, cx));
        })
    });

    v_flex()
        .w(px(POPOVER_W))
        .child(header)
        .child(rows.border_t_1().border_color(theme.border))
        .child(footer)
        .into_any_element()
}

fn render_step(
    step: &ReadinessStep,
    facts: &PopoverFacts,
    control: &Entity<StartCodingControl>,
    popover: &Entity<PopoverState>,
    window: &mut Window,
    cx: &mut gpui::Context<PopoverState>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let (fg, muted, warning) = (theme.foreground, theme.muted_foreground, theme.warning);
    let row_id = SharedString::from(format!("readiness-step-{:?}", step.key));
    let mut row = h_flex()
        .id(row_id)
        .w_full()
        .items_start()
        .gap_3()
        .px_4()
        .py_2p5()
        .child(step_ring(step, cx));
    match step.state {
        ReadinessStepState::Met => {
            row = row.items_center().child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(muted)
                            .child(SharedString::from(step.title.clone())),
                    )
                    .when_some(step.detail.clone(), |line, detail| {
                        line.child(
                            div()
                                .flex_shrink_0()
                                .max_w(px(160.))
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_xs()
                                .text_color(muted)
                                .child(SharedString::from(detail)),
                        )
                    }),
            );
        }
        ReadinessStepState::Current | ReadinessStepState::Pending => {
            let current = step.state == ReadinessStepState::Current;
            let mut column = v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(
                    div()
                        .text_sm()
                        .text_color(if current { fg } else { fg.opacity(0.8) })
                        .child(SharedString::from(step.title.clone())),
                )
                .when_some(step.body.clone(), |column, body| {
                    column.child(div().text_xs().text_color(muted).child(SharedString::from(body)))
                });
            if current {
                row = row.bg(warning.opacity(0.06));
                let picker = step.key == ReadinessStepKey::Repository && facts.picker_open;
                if picker {
                    column = column.child(render_picker(facts, control, window, cx));
                } else {
                    let owner = crate::settings::is_owner(cx, &facts.subject.team_id);
                    let fixes = desktop_fixes(step, owner);
                    if !fixes.is_empty() {
                        column = column.child(
                            h_flex().pt_2().gap_2().flex_wrap().children(
                                fixes.into_iter().enumerate().map(|(index, fix)| {
                                    fix_button(fix, index == 0, facts, control, popover, cx)
                                }),
                            ),
                        );
                    }
                }
                if let Some(error) = facts.link_error.clone() {
                    column = column.child(div().pt_1().text_xs().text_color(theme.danger).child(error));
                }
            }
            row = row.child(column);
        }
    }
    row.into_any_element()
}

/// One fix button: the first primary (white), the rest glass; Board settings
/// trails a chevron.
fn fix_button(
    fix: ReadinessFix,
    primary: bool,
    facts: &PopoverFacts,
    control: &Entity<StartCodingControl>,
    popover: &Entity<PopoverState>,
    cx: &App,
) -> AnyElement {
    let id = SharedString::from(format!("readiness-fix-{fix:?}"));
    let size = crate::surface::PillSize::Sm;
    let mut button = if primary {
        crate::surface::glass_pill_button_primary(id, size)
    } else {
        crate::surface::glass_pill_button(id, size, cx)
    };
    let glyph = match fix {
        ReadinessFix::ChooseRepository => Some(registry::UI_BRANCH),
        ReadinessFix::ConnectGithub => Some(registry::UI_GITHUB),
        ReadinessFix::OpenDevices => Some(registry::NAV_DEVICES),
        ReadinessFix::SetUpServer => Some(registry::UI_COPY),
        _ => None,
    };
    if let Some(glyph) = glyph {
        button = button.icon(Icon::new(glyph).with_size(px(size.glyph())));
    }
    button = if fix == ReadinessFix::BoardSettings {
        button.child(
            h_flex()
                .gap_1()
                .items_center()
                .child(fix.label())
                .child(Icon::new(registry::UI_CHEVRON_RIGHT).size_3()),
        )
    } else {
        button.label(fix.label())
    };
    let board_id = facts.subject.board_id.clone();
    let control = control.clone();
    let popover = popover.clone();
    button
        .disabled(facts.linking)
        .on_click(move |_: &ClickEvent, window, cx| match fix {
            ReadinessFix::ChooseRepository => {
                control.update(cx, |this, cx| this.open_repo_picker(window, cx));
            }
            ReadinessFix::ConnectGithub => {
                popover.update(cx, |state, cx| state.dismiss(window, cx));
                crate::sidebar::select_settings_section(
                    window,
                    cx,
                    crate::settings::SettingsSection::Repositories,
                );
            }
            ReadinessFix::BoardSettings => {
                popover.update(cx, |state, cx| state.dismiss(window, cx));
                crate::sidebar::select_settings_section(
                    window,
                    cx,
                    crate::settings::SettingsSection::Board(board_id.clone()),
                );
            }
            ReadinessFix::OpenDevices => {
                popover.update(cx, |state, cx| state.dismiss(window, cx));
                crate::navigation::navigate(window, cx, crate::navigation::Screen::Devices);
            }
            ReadinessFix::SetUpServer => {
                let snippet = crate::machines::server_install_snippet(cx);
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(snippet));
                crate::toast::success("Copied install command", window, cx);
            }
            // Never shown on the desktop (`desktop_fixes`).
            ReadinessFix::GetDesktopApp => {}
        })
        .into_any_element()
}

/// The inline repository picker (the repository row's fix): search, the
/// team's repos board-match first, then "Add another repository from
/// GitHub…" → Settings › Repositories.
fn render_picker(
    facts: &PopoverFacts,
    control: &Entity<StartCodingControl>,
    window: &mut Window,
    cx: &mut gpui::Context<PopoverState>,
) -> AnyElement {
    let theme = cx.theme().clone();
    let muted = theme.muted_foreground;
    let mut list = v_flex()
        .mt_2()
        .w_full()
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .overflow_hidden();
    let filter = facts
        .query
        .as_ref()
        .map(|query| query.read(cx).value().to_string())
        .unwrap_or_default();
    if let Some(query) = &facts.query {
        list = list
            .child(search_field(query, SearchFieldSize::Sm, window, cx).appearance(false))
            .child(div().h(px(1.)).w_full().bg(theme.border.opacity(0.5)));
    }
    match &facts.repos {
        None => {
            list = list.child(
                div().px_2().py_1p5().text_xs().text_color(muted).child("Loading repositories\u{2026}"),
            );
        }
        Some(None) => {
            let control = control.clone();
            list = list
                .child(
                    div()
                        .px_2()
                        .py_1p5()
                        .text_xs()
                        .text_color(theme.danger)
                        .child("Couldn\u{2019}t load repositories."),
                )
                .child(
                    crate::pickers::picker_row("readiness-picker-retry", cx)
                        .text_xs()
                        .child("Retry")
                        .on_click(move |_, _, cx| {
                            control.update(cx, |this, cx| this.refresh_github(cx));
                        }),
                );
        }
        Some(Some(repos)) => {
            let rows = picker_rows(
                repos,
                &facts.subject.board_id,
                &facts.subject.board_name,
                &facts.subject.board_slug,
                &filter,
            );
            if repos.is_empty() {
                list = list.child(
                    div().px_2().py_1p5().text_xs().text_color(muted).child(copy::PICKER_EMPTY),
                );
            }
            let mut scroll = v_flex()
                .id("readiness-picker-rows")
                .w_full()
                .max_h(px(200.))
                .overflow_y_scroll();
            for row in rows {
                let repo = &repos[row.index];
                let control = control.clone();
                let repo_id = repo.id.clone();
                scroll = scroll.child(
                    crate::pickers::picker_row(
                        SharedString::from(format!("readiness-repo-{}", repo.id)),
                        cx,
                    )
                    .gap_2()
                    .child(Icon::new(registry::UI_GITHUB).size_3().text_color(muted))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_xs()
                            .child(SharedString::from(repo.full_name.clone())),
                    )
                    .when_some(row.hint, |line, hint| {
                        line.child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(muted.opacity(0.8))
                                .child(SharedString::from(hint)),
                        )
                    })
                    .when(!facts.linking, |line| {
                        line.on_click(move |_, _, cx| {
                            let repo_id = repo_id.clone();
                            control.update(cx, |this, cx| this.link_repository(repo_id, cx));
                        })
                    }),
                );
            }
            list = list.child(scroll);
        }
    }
    list = list
        .child(div().h(px(1.)).w_full().bg(theme.border.opacity(0.5)))
        .child(
            crate::pickers::picker_row("readiness-picker-add", cx)
                .gap_2()
                .text_xs()
                .text_color(muted)
                .child(Icon::new(registry::UI_ADD).size_3())
                .child(copy::PICKER_ADD_FROM_GITHUB)
                .on_click(|_, window, cx| {
                    crate::sidebar::select_settings_section(
                        window,
                        cx,
                        crate::settings::SettingsSection::Repositories,
                    );
                }),
        );
    list.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::coding_readiness::coding_readiness;

    fn repo(id: &str, full_name: &str, boards: &[(&str, &str)]) -> TeamRepo {
        TeamRepo {
            id: id.to_string(),
            full_name: full_name.to_string(),
            boards: boards
                .iter()
                .map(|(id, name)| TeamRepoBoard {
                    id: id.to_string(),
                    name: name.to_string(),
                })
                .collect(),
        }
    }

    fn subject(repository_id: Option<&str>) -> ReadinessSubject {
        ReadinessSubject {
            team_id: "t-1".into(),
            team_name: "Acme".into(),
            board_id: "b-app".into(),
            board_name: "App".into(),
            board_slug: "app".into(),
            repository_id: repository_id.map(str::to_string),
        }
    }

    /// EXP-1121: the picker rule, off the SAME fixture web and the natives
    /// run (`picker`).
    #[test]
    fn picker_matches_the_shared_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/coding-readiness.json"
        ))
        .expect("fixture");
        for case in fixture["picker"].as_array().expect("picker cases") {
            let repos: Vec<TeamRepo> =
                serde_json::from_value(case["repos"].clone()).expect("repos");
            let board = &case["board"];
            let rows = picker_rows(
                &repos,
                board["id"].as_str().unwrap(),
                board["name"].as_str().unwrap(),
                board["slug"].as_str().unwrap(),
                case["query"].as_str().unwrap(),
            );
            let got: Vec<(String, Option<String>)> = rows
                .iter()
                .map(|row| (repos[row.index].id.clone(), row.hint.clone()))
                .collect();
            let want: Vec<(String, Option<String>)> = case["expected"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    (
                        row["id"].as_str().unwrap().to_string(),
                        row["tag"].as_str().map(str::to_string),
                    )
                })
                .collect();
            assert_eq!(got, want, "{}", case["name"]);
        }
    }

    /// The mockup's picker: the board-name match first, the used-by hint on
    /// a repo another board already backs, the rest bare.
    #[test]
    fn picker_puts_the_board_match_first() {
        let repos = vec![
            repo("r-web", "acme-inc/website", &[("b-web", "Website")]),
            repo("r-app", "acme-inc/app", &[]),
            repo("r-api", "acme-inc/api", &[]),
        ];
        let rows = picker_rows(&repos, "b-app", "App", "app", "");
        assert_eq!(
            rows,
            vec![
                PickerRow { index: 1, hint: Some("matches board".into()) },
                PickerRow { index: 0, hint: Some("used by Website".into()) },
                PickerRow { index: 2, hint: None },
            ]
        );
        let filtered = picker_rows(&repos, "b-app", "App", "app", "WEB");
        assert_eq!(filtered, vec![PickerRow { index: 0, hint: Some("used by Website".into()) }]);
    }

    /// A repo-less board reads "Needs a repository" once GitHub is known,
    /// loading (inert) before; the device step is always met here.
    #[test]
    fn desktop_input_drives_the_model() {
        let facts = GithubFacts {
            connected: true,
            label: Some("acme-inc".into()),
            repos: Some(vec![]),
        };
        let loading = coding_readiness(&readiness_input(&subject(None), None, None, "Mac", 0));
        assert!(loading.visible && loading.loading && loading.caption.is_none());

        let missing =
            coding_readiness(&readiness_input(&subject(None), None, Some(&facts), "Mac", 0));
        assert_eq!(missing.caption.as_deref(), Some("Needs a repository"));
        assert_eq!(missing.steps[2].state, ReadinessStepState::Met);
        assert_eq!(missing.steps[2].detail.as_deref(), Some("Mac"));

        // A linked board whose probe has not landed is met (no detail).
        let linked =
            coding_readiness(&readiness_input(&subject(Some("r-app")), None, None, "Mac", 0));
        assert!(linked.ready);
        assert_eq!(linked.steps[1].detail, None);
        let labelled = coding_readiness(&readiness_input(
            &subject(Some("r-app")),
            Some("acme-inc/app"),
            None,
            "Mac",
            0,
        ));
        assert_eq!(labelled.steps[1].detail.as_deref(), Some("acme-inc/app"));
    }

    /// "Get the desktop app" never renders on the desktop app itself.
    #[test]
    fn desktop_drops_the_get_the_desktop_app_fix() {
        let step = ReadinessStep {
            key: ReadinessStepKey::Device,
            state: ReadinessStepState::Current,
            title: String::new(),
            body: None,
            detail: None,
            fixes: vec![
                ReadinessFix::OpenDevices,
                ReadinessFix::GetDesktopApp,
                ReadinessFix::SetUpServer,
            ],
        };
        assert_eq!(
            desktop_fixes(&step, true),
            vec![ReadinessFix::OpenDevices, ReadinessFix::SetUpServer]
        );
    }

    /// "Board settings" opens an owner-only pane: a member never sees it.
    #[test]
    fn desktop_hides_board_settings_from_non_owners() {
        let step = ReadinessStep {
            key: ReadinessStepKey::Repository,
            state: ReadinessStepState::Current,
            title: String::new(),
            body: None,
            detail: None,
            fixes: vec![ReadinessFix::ChooseRepository, ReadinessFix::BoardSettings],
        };
        assert_eq!(
            desktop_fixes(&step, true),
            vec![ReadinessFix::ChooseRepository, ReadinessFix::BoardSettings]
        );
        assert_eq!(desktop_fixes(&step, false), vec![ReadinessFix::ChooseRepository]);
    }
}
