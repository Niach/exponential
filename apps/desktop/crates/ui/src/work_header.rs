//! EXP-877 — the ONE work header shared by the issue face, the run face and
//! the diff face of a top tab (issue detail + session screen), byte-identical
//! with the web `WorkHeader`.
//!
//! EXP-1162 (contract `detail-chrome.json`): a compact FIXED bar — the
//! [`collapsed_title`] on its left, on its right the [`face_toggle`]
//! (`Issue | Run | Changes | Results`, the changes item wearing `+N −M` once
//! its counts are known), then, for an issue, its pin and `…` menu. The
//! issue face's large title (the editable input, [`TITLE_SIZE`]) and its
//! property tray are the first rows of its SCROLLING body, and the bar's
//! title appears only once that row has scrolled under it; the run faces
//! have no title row, so they are always collapsed and keep the tray fixed
//! under the bar. The tray trails `[Merge PR] [the ONE coding action]`. The
//! coding action is derived from the run STATE ([`coding_action`]), never
//! from the face on show: an own live run → Stop, an own ended resumable
//! run → Resume, else Start coding. No hairline: the body wears the edge
//! strips ([`work_body`]). The column caps at [`WORK_COLUMN_W`], the same as
//! the issue body, transcript and diff.

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, InteractiveElement as _,
    IntoElement, ParentElement, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::Button, h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;

use crate::changes_bar::MergeTarget;
use crate::coding_flow::{LocalSessions, StartCodingControl};
use crate::icons::{registry, ExpIcon};
use crate::issue_detail::centered_column;
use crate::session_screen::ResumePath;
use crate::surface::{glass_pill_button, glass_pill_button_primary, PillSize};

/// The shared work column width — web `max-w-4xl` (896px): header, issue
/// body, transcript and the full-page diff all cap to it.
pub(crate) const WORK_COLUMN_W: f32 = 896.;

/// EXP-1191 — the work column's ONE content box: [`WORK_COLUMN_W`] inset by
/// this much on both sides (web `md:px-5`, 1.25rem). Every face (issue,
/// draft, run, changes, results) puts its VISIBLE edges on it: text starts,
/// chip / card / box outer borders, hairline dividers, the leading glyph of
/// a hover-filled row (its fill may hang outside), the bar's collapsed
/// title and trailing cluster. A block that pads itself (the WYSIWYG
/// editor's `WYSIWYG_BLOCK_PADDING_X`) takes the REMAINDER, never adds to it.
pub(crate) const WORK_GUTTER: f32 = 20.;

/// A 32px ghost icon button's box-to-glyph inset (`controls::
/// ghost_icon_button`): a trailing / leading ghost glyph hangs its hit box
/// into the gutter by this much so the GLYPH sits on the content edge.
pub(crate) const GHOST_ICON_HANG: f32 = 8.;

/// EXP-926 / FEED-45 — ONE size rule per PLACEMENT, and the placement is the
/// only thing that decides it.
///
/// A header action (Merge PR, Stop, Resume, Fix conflicts) sits in exactly two
/// places: NEXT TO the face toggle in row 1 — a run with no issue, or a batch,
/// which has no property card to put it in — or INSIDE that card's chip row
/// beside status, priority, labels, due date and board. Beside the toggle it
/// matches the TOGGLE ([`PillSize::Lg`], the 36px control); in the tray it
/// matches the CHIPS ([`PillSize::Sm`], what `pickers::chip_button` wears).
/// They used to be chip-sized in both, so the header's own cluster read as a
/// row of stray chips floating next to a control twice their height.
pub(crate) fn header_action_size(in_tray: bool) -> PillSize {
    if in_tray {
        PillSize::Sm
    } else {
        PillSize::Lg
    }
}

// ---------------------------------------------------------------------------
// Face toggle
// ---------------------------------------------------------------------------

/// Which face of a top tab is up. `Diff` = the CHANGES face — the run's
/// worktree diff while a run of mine has one, else (EXP-889) the issue's own
/// open pull request; `Results` (EXP-879) = a run's published report: my
/// run's sub-face, or (EXP-933) a teammate's report shown on the issue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Face {
    Issue,
    Run,
    Diff,
    Results,
}

/// What the toggle offers: an `Issue` item for an issue-bound tab, a `Run`
/// item when a run exists (its id), a Changes item when there is a diff to
/// read. Unavailable items are HIDDEN, never disabled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FaceToggle {
    pub issue: bool,
    pub run: Option<String>,
    /// The RUN's worktree diff (`+N −M`) when it has changes. `Some` also
    /// means the Changes item OPENS that run's diff face.
    pub diff: Option<(u32, u32)>,
    /// EXP-889: the ISSUE has an open pull request, so its FILES are a
    /// Changes face of their own — independent of any run (the web
    /// `work-faces.ts`: `hasChanges = diffStats.fileCount > 0 || prState ===
    /// 'open'`, `availableFaces` pushes `changes` outside the `hasRun`
    /// branch). Counts are not known until the files are fetched, so the
    /// item wears the word [`CHANGES_FACE_LABEL`] until `diff` is `Some`.
    pub pr_changes: bool,
    /// EXP-879/EXP-933: the issue's results run
    /// ([`issue_results_run`]) or this run has published something (a
    /// picture or report text), so the Results item exists — with or
    /// without a run of mine, like the web `availableFaces`.
    pub results: bool,
    pub active: Face,
    /// EXP-886 / EXP-950: the issue's runs of mine
    /// ([`crate::run_rows::issue_run_entries`]); EXP-974: for an issue-less
    /// run, its resume chain ([`crate::run_rows::chain_run_entries`]). With
    /// MORE THAN ONE the Run item reads "Runs" ([`run_face_label`]) and
    /// carries a caret: the label still opens the tab's run, the caret's
    /// menu picks between them — and the toggle shows for that caret alone.
    pub runs: Vec<RunEntry>,
    /// The run the menu checks: the one on show (session screen) or the one
    /// the Run face opens (issue face).
    pub checked_run: Option<String>,
    /// EXP-1162: the facts behind the face dots ([`FaceToggle::dots`]).
    pub state: FaceState,
}

/// EXP-1162 FACE DOTS — what the toggle's dots say (contract
/// `detail-chrome.json` `faceDots`): the tab's run is live, that live run
/// waits on a person, the issue's (or run's) pull request is open. FACE
/// MARKS: the run's agent (`None` = an unknown one, the generic glyph) and
/// what it is doing (EXP-1184, [`crate::queries::coding_session_display`];
/// `None` while it is not live or its host is offline), for the Run tab's
/// mark.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FaceState {
    pub run_live: bool,
    pub needs_input: bool,
    pub pr_open: bool,
    pub agent: Option<coding::CodingAgent>,
    pub run_state: Option<crate::queries::CodingSessionDisplay>,
}

/// EXP-1162 FACE MARKS: how a segment draws its tone — the Run segment as
/// the run's agent brand mark LEADING its label (badged amber while it
/// needs input), every other segment as a dot trailing its label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FaceMark {
    Agent { attention: bool },
    Dot(domain::detail_chrome::FaceDotTone),
}

/// The [`FaceMark`] a segment wears for the tone the rule gave it.
pub(crate) fn face_mark(face: Face, tone: domain::detail_chrome::FaceDotTone) -> FaceMark {
    use domain::detail_chrome::FaceDotTone;
    match face {
        Face::Run => FaceMark::Agent {
            attention: tone == FaceDotTone::NeedsInput,
        },
        _ => FaceMark::Dot(tone),
    }
}

/// EXP-950: one row of the Run item's menu — `<device> · <when>`
/// ([`crate::run_rows::issue_run_label`]), live runs marked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunEntry {
    pub id: String,
    pub label: String,
    pub live: bool,
}

/// Byte-identical with the web (`RUN_FACE_LABEL` / `RUNS_FACE_LABEL`).
pub(crate) const RUN_FACE_LABEL: &str = "Run";
pub(crate) const RUNS_FACE_LABEL: &str = "Runs";
/// EXP-889, byte-identical with the web `CHANGES_FACE_LABEL` (`work-faces.ts`)
/// and the natives — the Changes face's ONE name, worn whenever its counts
/// are not (yet) known.
pub(crate) const CHANGES_FACE_LABEL: &str = "Changes";
/// EXP-879, byte-identical with the web `RESULTS_FACE_LABEL` and with the
/// iOS/Android strings — the fourth face's ONE name on every client.
pub(crate) const RESULTS_FACE_LABEL: &str = "Results";

/// The Run item's label: the plural once the issue has several runs of mine.
pub(crate) fn run_face_label(multiple_runs: bool) -> &'static str {
    if multiple_runs {
        RUNS_FACE_LABEL
    } else {
        RUN_FACE_LABEL
    }
}

impl FaceToggle {
    /// EXP-886: several runs of mine — the plural label and (EXP-950) the
    /// caret's menu.
    pub(crate) fn multiple_runs(&self) -> bool {
        self.runs.len() > 1
    }

    /// EXP-950: is there a control at all? Two faces make a toggle; a lone
    /// Run item still shows while it carries the run menu.
    pub(crate) fn is_shown(&self) -> bool {
        let items = self.items();
        items.len() > 1 || (self.multiple_runs() && items.contains(&Face::Run))
    }

    /// EXP-889: is there a Changes face at all? The web rule verbatim
    /// (`hasChanges = diffStats.fileCount > 0 || issue.prState === 'open'`):
    /// the run's worktree diff OR the issue's open PR.
    pub(crate) fn has_changes(&self) -> bool {
        self.diff.is_some() || self.pr_changes
    }

    /// The items in web order, pure so the visibility rule can be pinned —
    /// the web `availableFaces` (`lib/work-faces.ts`) one for one: issue,
    /// run, changes, results. Changes is INDEPENDENT of the run (EXP-889:
    /// "an issue with an open PR and no run of mine still has its PR
    /// files"); Results stays a sub-face of the run.
    pub(crate) fn items(&self) -> Vec<Face> {
        let mut items = Vec::with_capacity(4);
        if self.issue {
            items.push(Face::Issue);
        }
        if self.run.is_some() {
            items.push(Face::Run);
        }
        if self.has_changes() {
            items.push(Face::Diff);
        }
        // EXP-879: Results comes LAST — issue, run, changes, results.
        // EXP-933: independent of a run of MINE — a teammate's report is the
        // issue's Results too (web `availableFaces`).
        if self.results {
            items.push(Face::Results);
        }
        items
    }

    /// EXP-1162: which segment wears which session-dot tone — the shared
    /// rule ([`domain::detail_chrome::face_dots`]) over the items on show.
    pub(crate) fn dots(&self) -> Vec<(Face, domain::detail_chrome::FaceDotTone)> {
        use domain::detail_chrome::DetailFace;
        let faces: Vec<DetailFace> = self
            .items()
            .into_iter()
            .map(|face| match face {
                Face::Issue => DetailFace::Issue,
                Face::Run => DetailFace::Run,
                Face::Diff => DetailFace::Changes,
                Face::Results => DetailFace::Results,
            })
            .collect();
        domain::detail_chrome::face_dots(
            &faces,
            self.state.run_live,
            self.state.needs_input,
            self.state.pr_open,
        )
        .into_iter()
        .map(|(face, tone)| {
            let face = match face {
                DetailFace::Issue => Face::Issue,
                DetailFace::Run => Face::Run,
                DetailFace::Changes => Face::Diff,
                DetailFace::Results => Face::Results,
            };
            (face, tone)
        })
        .collect()
    }
}

/// EXP-1162: the face-dot facts for a tab whose Run face shows `run_id`
/// (the bound or shown run, read off the synced rows): live per
/// [`crate::queries::coding_session_is_live`], waiting per its synced
/// `needs_input`; `pr_open` is the caller's (the issue's, or an issue-less
/// run's own PR).
pub(crate) fn face_state(run_id: Option<&str>, pr_open: bool, cx: &App) -> FaceState {
    let row = run_id.and_then(|run_id| {
        Store::try_global(cx)?
            .collections()
            .coding_sessions
            .read(cx)
            .get(run_id)
            .cloned()
    });
    let now = chrono::Utc::now().timestamp();
    let run_live = row
        .as_ref()
        .is_some_and(|row| crate::queries::coding_session_is_live(row, now));
    // EXP-1184: the ONE display rule, fed like every other surface — the
    // local engine's turn signal for a run hosted here, the issue's PR state
    // before the run's own (EXP-734); an offline host is paused: no state.
    let run_state = row.as_ref().filter(|_| run_live).and_then(|row| {
        let store = Store::try_global(cx)?;
        let collections = store.collections();
        let pr_state = row
            .issue_id
            .as_deref()
            .and_then(|id| collections.issues.read(cx).get(id).and_then(|issue| issue.pr_state.clone()))
            .or_else(|| row.pr_state.clone());
        let busy = crate::queries::session_agent_busy(
            row,
            crate::queries::local_agent_busy(&row.id, cx),
            now,
        );
        let display = crate::queries::coding_session_display(row, busy, pr_state.as_deref());
        let presentation = crate::queries::session_device_presentation(
            row,
            collections.devices.read(cx).iter(),
            now * 1_000,
        );
        (!crate::queries::session_is_paused(display, row.status.as_deref(), &presentation))
            .then_some(display)
    });
    FaceState {
        run_live,
        needs_input: run_live && row.as_ref().and_then(|row| row.needs_input) == Some(true),
        pr_open,
        // An absent agent id is claude (`codingSessions.start`'s default, the
        // rail's fallback); an id this build does not know is `None`.
        agent: match row.as_ref().and_then(|row| row.agent.as_deref()) {
            None => Some(coding::CodingAgent::default()),
            Some(id) => coding::CodingAgent::parse(id),
        },
        run_state,
    }
}

/// EXP-1162: a face dot's words — the segment's tooltip.
pub(crate) fn face_dot_label(tone: domain::detail_chrome::FaceDotTone) -> &'static str {
    use domain::detail_chrome::FaceDotTone;
    match tone {
        FaceDotTone::Running => "Running",
        FaceDotTone::NeedsInput => "Needs input",
        FaceDotTone::Review => "Pull request open",
    }
}

/// EXP-1184: a segment's tooltip — the Run segment names what its run is
/// doing (web `RUN_STATE_LABEL`), any other its dot's words.
pub(crate) fn face_tooltip(
    face: Face,
    tone: domain::detail_chrome::FaceDotTone,
    run_state: Option<crate::queries::CodingSessionDisplay>,
) -> &'static str {
    use crate::queries::CodingSessionDisplay;
    match (face, run_state) {
        (Face::Run, Some(CodingSessionDisplay::Working)) => "Working",
        (Face::Run, Some(CodingSessionDisplay::NeedsInput)) => "Needs input",
        (Face::Run, Some(CodingSessionDisplay::Review)) => "Pull request open",
        (Face::Run, Some(CodingSessionDisplay::Done)) => "Done",
        _ => face_dot_label(tone),
    }
}

/// EXP-1162: a face dot's colour — a row of the ONE session-dot table
/// ([`crate::queries::session_dot_tone`]), never a colour of its own.
pub(crate) fn face_dot_color(tone: domain::detail_chrome::FaceDotTone, cx: &App) -> gpui::Hsla {
    use crate::queries::SessionDotFacts;
    use domain::detail_chrome::FaceDotTone;
    let facts = match tone {
        FaceDotTone::Running => SessionDotFacts { running: true, ..Default::default() },
        FaceDotTone::NeedsInput => SessionDotFacts {
            running: true,
            needs_input: true,
            ..Default::default()
        },
        FaceDotTone::Review => SessionDotFacts { review: true, ..Default::default() },
    };
    crate::queries::session_dot_tone(facts, cx.theme().muted_foreground)
}

/// The dot itself: `faceDot` wide, `faceDotGap` after the label.
fn face_dot(tone: domain::detail_chrome::FaceDotTone, cx: &App) -> gpui::Div {
    div()
        .flex_shrink_0()
        .ml(px(domain::detail_chrome::FACE_DOT_GAP))
        .size(px(domain::detail_chrome::FACE_DOT))
        .rounded_full()
        .bg(face_dot_color(tone, cx))
}

/// EXP-1162/EXP-1184 FACE MARKS: the Run segment's lead — the rail's run
/// mark ([`crate::coding_selects::run_lead`]), `faceMark` square,
/// `faceMarkGap` before the label: the agent's working mark mid-turn, else
/// the brand mark with the run state's badge. A run whose state is unknown
/// still wears the amber badge while the contract's tone says it waits.
fn face_agent_mark(state: FaceState, attention: bool) -> AnyElement {
    use domain::detail_chrome::{FACE_MARK, FACE_MARK_BADGE, FACE_MARK_GAP};
    let run_state = state
        .run_state
        .or(attention.then_some(crate::queries::CodingSessionDisplay::NeedsInput));
    crate::coding_selects::run_lead(state.agent, FACE_MARK, FACE_MARK_BADGE, run_state)
        .mr(px(FACE_MARK_GAP))
        .into_any_element()
}

/// The callback a toggle pick lands on.
pub(crate) type OnPickFace = Rc<dyn Fn(Face, &mut Window, &mut App)>;
/// EXP-950: the callback a run-menu pick lands on (the picked run's id). The
/// caller decides what picking the checked run means.
pub(crate) type OnPickRun = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// The `Issue | Run | +N -M` segmented control. `None` below two items — a
/// one-item toggle names nothing to switch to — unless (EXP-950) the lone
/// Run item carries the run menu ([`FaceToggle::is_shown`]).
pub(crate) fn face_toggle(
    spec: FaceToggle,
    on_pick: OnPickFace,
    on_pick_run: OnPickRun,
    cx: &App,
) -> Option<AnyElement> {
    if !spec.is_shown() {
        return None;
    }
    let items = spec.items();
    let dots = spec.dots();
    let dot_of = |face: Face| dots.iter().find(|(dotted, _)| *dotted == face).map(|(_, tone)| *tone);
    // The web `TabsList` capsule as-is (h-9, 3px inset, `px-3 text-sm`
    // triggers) — `controls::segmented` already mirrors it; only the width
    // changes from full to content.
    let mut control = crate::controls::segmented(cx).w_auto().flex_shrink_0();
    for face in items {
        let active = spec.active == face;
        let on_pick = on_pick.clone();
        // EXP-1162: the segment's state (its words as the tooltip): the Run
        // segment's agent mark LEADS its label, any other's dot trails it.
        let dot = dot_of(face);
        let lead = dot.and_then(|tone| match face_mark(face, tone) {
            FaceMark::Agent { attention } => Some(face_agent_mark(spec.state, attention)),
            FaceMark::Dot(_) => None,
        });
        let trailing_dot = dot.filter(|tone| matches!(face_mark(face, *tone), FaceMark::Dot(_)));
        if face == Face::Run && spec.multiple_runs() {
            // EXP-950: the capsule holds TWO siblings — the label (the face
            // pick) and the caret (the run menu) — so a caret click never
            // reaches the label's handler.
            let label = div()
                .id("tab-face-run")
                .h_full()
                .flex()
                .items_center()
                .pl_3()
                .pr_1()
                .children(lead)
                .child(run_face_label(true))
                .when_some(dot, |label, tone| {
                    let words = face_tooltip(face, tone, spec.state.run_state);
                    label.tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(words).build(window, cx)
                    })
                })
                .when(!active, |label| {
                    label.on_click(move |_, window, cx| on_pick(face, window, cx))
                });
            let item = crate::controls::segmented_item(active, cx)
                .flex_none()
                .px_0()
                .gap_0()
                .text_sm()
                .child(label)
                .child(run_menu(&spec, on_pick_run.clone(), cx));
            control = control.child(item);
            continue;
        }
        let item = crate::controls::segmented_item(active, cx)
            .id(match face {
                Face::Issue => "tab-face-issue",
                Face::Run => "tab-face-run",
                Face::Diff => "tab-face-diff",
                Face::Results => "tab-face-results",
            })
            .flex_none()
            .px_3()
            // The contract's gaps are exact: no segment gap on top of them.
            .gap_0()
            .text_sm()
            .children(lead)
            .map(|item| match face {
                Face::Issue => item.child("Issue"),
                Face::Run => item.child(run_face_label(false)),
                Face::Diff => match spec.diff {
                    // EXP-895: the ONE counts renderer — the contract's
                    // labels (`+N` / `−M`, U+2212) in the shared tints.
                    Some((additions, deletions)) => {
                        item.child(crate::diff_pane::counts(additions, deletions, cx))
                    }
                    // EXP-889: the issue's PR files are fetched when the face
                    // opens, so the item names itself until they land — the
                    // web's own `CHANGES_FACE_LABEL` row.
                    None => item.child(CHANGES_FACE_LABEL),
                },
                Face::Results => item.child(RESULTS_FACE_LABEL),
            })
            .children(trailing_dot.map(|tone| face_dot(tone, cx)))
            .when_some(dot, |item, tone| {
                let words = face_tooltip(face, tone, spec.state.run_state);
                item.tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(words).build(window, cx)
                })
            })
            .when(!active, |item| {
                item.on_click(move |_, window, cx| on_pick(face, window, cx))
            });
        control = control.child(item);
    }
    Some(control.into_any_element())
}

/// EXP-950: the Run item's caret over the issue's runs of mine (EXP-886's
/// switcher, folded into the toggle): each `<device> · <when>`, the checked
/// run wearing the check, a live one the running glyph. Web
/// `IssueRunMenuContent`.
fn run_menu(spec: &FaceToggle, on_pick_run: OnPickRun, cx: &App) -> AnyElement {
    use gpui_component::button::ButtonVariants as _;
    use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
    let entries = spec.runs.clone();
    let checked = spec.checked_run.clone();
    let muted = cx.theme().muted_foreground;
    Button::new("tab-face-run-menu")
        .ghost()
        .xsmall()
        .rounded_full()
        .cursor_pointer()
        .mr_1()
        .icon(
            Icon::new(registry::UI_CHEVRON_DOWN)
                .with_size(px(12.))
                .text_color(muted),
        )
        .tooltip("Switch run")
        .dropdown_menu(move |mut menu, _window, _cx| {
            for entry in entries.clone() {
                let is_checked = checked.as_deref() == Some(entry.id.as_str());
                let on_pick_run = on_pick_run.clone();
                let mut item = PopupMenuItem::new(entry.label).checked(is_checked);
                if entry.live && !is_checked {
                    // The running glyph marks a live run; the checked entry
                    // wears the check in that slot instead.
                    item = item.icon(Icon::new(registry::CODING_RUNNING));
                }
                menu = menu.item(
                    item.on_click(move |_, window, cx| on_pick_run(&entry.id, window, cx)),
                );
            }
            menu
        })
        .into_any_element()
}

// ---------------------------------------------------------------------------
// The ONE coding action
// ---------------------------------------------------------------------------

/// The header's one coding action, derived from the run state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CodingAction {
    /// My run is live: Stop (the run's own confirm).
    Stop {
        session_id: String,
        /// This process hosts the run (the in-process kill); otherwise the
        /// relay's `killSession` reaches the machine named by `device_label`.
        local: bool,
        device_label: Option<String>,
    },
    /// My newest run ended and a machine can take it back up.
    Resume {
        session_id: String,
        path: ResumePath,
        host_label: Option<String>,
    },
    /// Nothing of mine to act on: the launcher.
    Start,
}

/// The web `issue-coding-action.tsx` target order, pure: the bound run while
/// it is mine AND live; else my newest LIVE run on the issue (running /
/// in_review inside the stale window) — so an own live run never hides
/// behind a newer ended or stale one; else my newest run overall (whatever
/// its state — a newer stale-running row outranks an older ended one, and
/// then earns Start coding, never Resume on the older run). `None` = no run
/// of mine on the issue.
pub(crate) fn coding_target<'a>(
    rows: impl IntoIterator<Item = &'a domain::rows::CodingSession>,
    issue_id: &str,
    bound: Option<&str>,
    me: &str,
    now_epoch: i64,
) -> Option<&'a domain::rows::CodingSession> {
    let mine: Vec<&domain::rows::CodingSession> = rows
        .into_iter()
        .filter(|row| row.issue_id.as_deref() == Some(issue_id) && row.user_id.as_deref() == Some(me))
        .collect();
    let newest = |rows: &[&'a domain::rows::CodingSession]| -> Option<&'a domain::rows::CodingSession> {
        rows.iter()
            .copied()
            .max_by(|a, b| a.started_at.cmp(&b.started_at).then_with(|| a.id.cmp(&b.id)))
    };
    let live: Vec<&domain::rows::CodingSession> = mine
        .iter()
        .copied()
        .filter(|row| crate::queries::coding_session_is_live(row, now_epoch))
        .collect();
    if let Some(bound) = bound {
        if let Some(row) = live.iter().copied().find(|row| row.id == bound) {
            return Some(row);
        }
    }
    newest(&live).or_else(|| newest(&mine))
}

/// EXP-933 — the run whose Results an ISSUE shows. Results live on the run
/// but sync team-wide, and an agent message deep-links a teammate to the
/// ISSUE's Results, so this is not limited to my runs: [`coding_target`] when
/// that run has results, else the NEWEST run on the issue (`started_at`, tie
/// the larger id) by ANY member that has any. The twin of the web
/// `work-faces.ts` `issueResultsRun`, locked by
/// `packages/domain-contract/fixtures/session-results.json` (×4).
pub(crate) fn issue_results_run<'a>(
    rows: impl IntoIterator<Item = &'a domain::rows::CodingSession>,
    issue_id: &str,
    bound: Option<&str>,
    me: Option<&str>,
    now_epoch: i64,
) -> Option<&'a domain::rows::CodingSession> {
    let rows: Vec<&'a domain::rows::CodingSession> = rows
        .into_iter()
        .filter(|row| row.issue_id.as_deref() == Some(issue_id))
        .collect();
    let has = |row: &domain::rows::CodingSession| {
        domain::session_results::has_session_results(row.results.as_ref())
    };
    if let Some(me) = me {
        if let Some(own) = coding_target(rows.iter().copied(), issue_id, bound, me, now_epoch) {
            if has(own) {
                return Some(own);
            }
        }
    }
    rows.into_iter()
        .filter(|row| has(row))
        .max_by(|a, b| a.started_at.cmp(&b.started_at).then_with(|| a.id.cmp(&b.id)))
}

/// The pure rule. `target` is the run the tab is about ([`coding_target`]);
/// a run that is not mine (a teammate's) never yields anything but Start, a
/// live one Stop, an ENDED one Resume when a machine can take it, anything
/// else (a stale-running row, an unresumable end) Start.
pub(crate) fn coding_action(
    target: Option<&domain::rows::CodingSession>,
    me: Option<&str>,
    now_epoch: i64,
    local: bool,
    device_label: Option<String>,
    resume: Option<ResumePath>,
) -> CodingAction {
    let Some(target) = target else {
        return CodingAction::Start;
    };
    let own = target.user_id.is_some() && target.user_id.as_deref() == me;
    if !own {
        return CodingAction::Start;
    }
    if crate::queries::coding_session_is_live(target, now_epoch) {
        return CodingAction::Stop {
            session_id: target.id.clone(),
            local,
            device_label,
        };
    }
    // EXP-888: a sweep end is not an end — never offer Resume on it, that
    // would put a second agent on the run's worktree.
    let ended = crate::run_rows::run_has_ended(target);
    match resume {
        Some(path) if ended => CodingAction::Resume {
            session_id: target.id.clone(),
            path,
            host_label: device_label,
        },
        _ => CodingAction::Start,
    }
}

/// EXP-933 — [`issue_results_run`] over the synced rows, as its row: the
/// run whose Results the issue shows, mine or a teammate's. `None` when no
/// run on the issue published anything.
pub(crate) fn issue_results_row(
    issue_id: &str,
    bound: Option<&str>,
    cx: &App,
) -> Option<domain::rows::CodingSession> {
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    let store = Store::try_global(cx)?;
    let sessions = store.collections().coding_sessions.read(cx);
    let now = chrono::Utc::now().timestamp();
    issue_results_run(sessions.iter(), issue_id, bound, me.as_deref(), now).cloned()
}

/// EXP-933 — open `issue_id` on its RESULTS (the inbox's `agent_message`
/// rows and their OS notifications): the issue's results run when it is MINE
/// opens on that run's Results sub-face (the header toggle's own path); a
/// TEAMMATE's report opens as the issue tab's Results face; no results at all
/// = just the issue. `open` is the caller's own issue navigation (scoped on
/// its board, or from the inbox with its origin).
pub(crate) fn open_issue_results(
    issue_id: &str,
    window: &mut gpui::Window,
    cx: &mut App,
    open: impl FnOnce(&mut gpui::Window, &mut App),
) {
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    let row = issue_results_row(issue_id, None, cx);
    let mine = row
        .as_ref()
        .filter(|row| row.user_id.is_some() && row.user_id == me)
        .map(|row| row.id.clone());
    if row.is_some() && mine.is_none() {
        // Ask BEFORE the navigation re-points the shared detail view — the
        // switch keeps the request because it names the incoming issue.
        crate::screens::request_issue_results(issue_id, window, cx);
    }
    open(window, cx);
    if let Some(run_id) = mine {
        crate::screens::set_tab_face(
            issue_id,
            crate::screens::TabFace::Run,
            Some(run_id.clone()),
            window,
            cx,
        );
        crate::screens::set_run_face(&run_id, crate::screens::RunFace::Results, window, cx);
    }
}

/// [`coding_target`] over the synced rows, as its row. `None` when I have no
/// synced run on the issue.
pub(crate) fn issue_run_row(
    issue_id: &str,
    bound: Option<&str>,
    cx: &App,
) -> Option<domain::rows::CodingSession> {
    let me = crate::queries::active_account(cx)?.user_id;
    let store = Store::try_global(cx)?;
    let sessions = store.collections().coding_sessions.read(cx);
    let now = chrono::Utc::now().timestamp();
    coding_target(sessions.iter(), issue_id, bound, &me, now).cloned()
}

/// [`coding_action`] for an issue, off the synced rows, this process's local
/// registry and the devices rows. `resumable` is the caller's once-per-run
/// cache of the registry read (`(session_id, resumable)`): the registry is a
/// file, and a header repaints on every feed event.
pub(crate) fn issue_coding_action(
    issue_id: &str,
    bound: Option<&str>,
    resumable: &mut Option<(String, bool)>,
    cx: &mut App,
) -> CodingAction {
    let now = chrono::Utc::now().timestamp();
    // A local start ahead of its synced echo is still my live run.
    let local_by_issue = LocalSessions::global_ref(cx)
        .and_then(|sessions| sessions.read(cx).get(issue_id).map(|s| s.session_id.clone()));
    let target = issue_run_row(issue_id, bound, cx);
    let Some(target) = target else {
        return match local_by_issue {
            Some(session_id) => CodingAction::Stop {
                session_id,
                local: true,
                device_label: None,
            },
            None => CodingAction::Start,
        };
    };
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    let local = LocalSessions::global_ref(cx)
        .is_some_and(|sessions| sessions.read(cx).session_by_id(&target.id).is_some());
    let device_label = target
        .device_id
        .as_deref()
        .and_then(|device_id| crate::queries::device_label_for_id(cx, device_id));
    let ended = crate::run_rows::run_has_ended(&target);
    let resume = ended.then(|| resume_path_cached(&target, resumable, cx)).flatten();
    coding_action(Some(&target), me.as_deref(), now, local, device_label, resume)
}

/// [`crate::session_screen::resume_path_for`] over the cached registry read.
pub(crate) fn resume_path_cached(
    session: &domain::rows::CodingSession,
    resumable: &mut Option<(String, bool)>,
    cx: &mut App,
) -> Option<ResumePath> {
    let local = match resumable {
        Some((id, known)) if *id == session.id => *known,
        _ => {
            let known = crate::coding_flow::run_is_resumable_ref(&session.id, cx);
            *resumable = Some((session.id.clone(), known));
            known
        }
    };
    let own_device_id = crate::queries::own_device_id(cx);
    let store = Store::try_global(cx)?;
    let own_device = session.device_id.as_deref() == Some(own_device_id.as_str());
    crate::session_screen::resume_path_for(
        local,
        own_device,
        session,
        store.collections().devices.read(cx).iter(),
        chrono::Utc::now().timestamp_millis(),
    )
}

/// The pill for a [`CodingAction`]. `start` is the launcher entity of the
/// host that has one (the issue detail / an issue-bound session screen);
/// without it a `Start` renders nothing.
pub(crate) fn coding_action_button(
    action: CodingAction,
    start: Option<&gpui::Entity<StartCodingControl>>,
    // EXP-926: the placement's size — the toggle's beside the toggle, the
    // chips' inside the tray.
    size: PillSize,
    cx: &App,
) -> Option<AnyElement> {
    match action {
        CodingAction::Stop {
            session_id,
            local,
            device_label,
        } => Some(
            crate::session_screen::stop_session_pill("work-stop", size, cx)
                .on_click(move |_, window, cx| {
                    let host = local
                        .then(|| {
                            LocalSessions::global_ref(cx).and_then(|sessions| {
                                sessions
                                    .read(cx)
                                    .session_by_id(&session_id)
                                    .map(|session| session.host.clone())
                            })
                        })
                        .flatten();
                    crate::session_bar::prompt_kill_session(
                        host,
                        device_label.clone(),
                        session_id.clone(),
                        window,
                        cx,
                    );
                })
                .into_any_element(),
        ),
        CodingAction::Resume {
            session_id,
            path,
            host_label,
        } => {
            let button = resume_pill("work-resume", size, cx);
            Some(match path {
                ResumePath::Local => button
                    .on_click(move |_, window, cx| {
                        // The ONE desktop resume entry point: the transport
                        // comes from the recorded run, never from the setting.
                        crate::action_run::resume_run(
                            session_id.clone(),
                            Some(window.window_handle()),
                            false,
                            coding::LaunchOrigin::Local,
                            cx,
                        );
                    })
                    .into_any_element(),
                ResumePath::Remote { device_id } => {
                    // EXP-800: the run relaunches on the machine that hosted
                    // it; the tooltip says so before the click.
                    let label = host_label.unwrap_or_else(|| "its machine".to_string());
                    let tooltip = SharedString::from(format!("Resume on {label}"));
                    button
                        .tooltip(tooltip)
                        .on_click(move |_, window, cx| {
                            crate::session_screen::resume_remote(
                                session_id.clone(),
                                device_id.clone(),
                                label.clone(),
                                window,
                                cx,
                            );
                        })
                        .into_any_element()
                }
            })
        }
        CodingAction::Start => start.map(|start| start.clone().into_any_element()),
    }
}

/// EXP-916 — the ONE way out to GitHub: a ghost glyph in the right cluster,
/// present exactly while the subject HAS a pull request. `None` otherwise —
/// an empty link is worse than no link. EXP-949: it rides the CHANGES face
/// alone (the detail's PR pane, a run's diff face, the review header), never
/// Issue, Run or Results — the callers gate it. Callers name the element so
/// two clusters can carry it at once.
pub(crate) fn github_button(
    id: impl Into<gpui::ElementId>,
    pr_url: Option<&str>,
    cx: &mut App,
) -> Option<AnyElement> {
    let url = pr_url.map(str::trim).filter(|url| !url.is_empty())?.to_string();
    Some(
        crate::controls::ghost_icon_button(id, Icon::new(registry::UI_GITHUB), cx)
            .tooltip(domain::contract::DIFF_UI_OPEN_ON_GITHUB)
            .on_click(move |_, _, cx| {
                crate::settings::open_url(cx, url.clone());
            })
            .into_any_element(),
    )
}

/// The Resume pill — glass, `Sm`, the resume glyph and the word "Resume".
pub(crate) fn resume_pill(id: impl Into<gpui::ElementId>, size: PillSize, cx: &App) -> Button {
    glass_pill_button(id, size, cx)
        .icon(
            Icon::new(registry::RUN_RESUME)
                .with_size(px(size.glyph()))
                .text_color(cx.theme().muted_foreground),
        )
        .label("Resume")
        .tooltip("Resume this run")
}

// ---------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------

/// The ONE Merge look: a `Sm` pill (primary or glass) with the merge glyph,
/// two-click armed (`Merge PR` → `Confirm merge`, then `Merging…` until the
/// Electric echo settles) through [`crate::pr_merge::two_click`].
pub(crate) fn merge_pill(
    id: impl Into<gpui::ElementId>,
    target: &MergeTarget,
    primary: bool,
    size: PillSize,
    cx: &mut App,
) -> AnyElement {
    merge_pill_labeled(id, target, primary, None, size, cx)
}

/// EXP-917 — [`merge_pill`] with its RESTING label overridden: the swapped
/// slot's secondary reads "Retry merge" (the Reviews row's word, and the
/// web's), never a second "Merge PR" beside "Fix conflicts". The armed and
/// in-flight labels stay the shared ones.
pub(crate) fn merge_pill_labeled(
    id: impl Into<gpui::ElementId>,
    target: &MergeTarget,
    primary: bool,
    resting_label: Option<&'static str>,
    // EXP-926: the placement's size, never the pill's own opinion.
    size: PillSize,
    cx: &mut App,
) -> AnyElement {
    let key = target.key();
    let (armed, merging) = {
        let state = crate::pr_merge::MergeState::global(cx);
        let state = state.read(cx);
        (state.armed(&key), state.merging(&key))
    };
    let glyph = if armed {
        cx.theme().danger
    } else if primary {
        cx.theme().primary_foreground
    } else {
        cx.theme().muted_foreground
    };
    let target = target.clone();
    let mut button = if primary {
        glass_pill_button_primary(id, size)
    } else {
        glass_pill_button(id, size, cx)
    }
    .icon(
        Icon::from(ExpIcon::GitMerge)
            .with_size(px(size.glyph()))
            .text_color(glyph),
    )
    // EXP-916: the settled label is the CONTRACT's, so the pill reads the
    // same word on all four clients.
    .label(if merging {
        "Merging…"
    } else if armed {
        "Confirm merge"
    } else {
        resting_label.unwrap_or(domain::contract::DIFF_UI_MERGE_PR)
    })
    .tooltip(target.tooltip())
    .on_click(move |_, window, cx| {
        // EXP-1145: a member of an open PR stack asks first; a run's own
        // chore PR (a Session target) is never a stack member.
        if let MergeTarget::Issue { issue_id } = &target {
            if crate::pr_merge::ask_stack_merge(issue_id, window, cx) {
                return;
            }
        }
        crate::pr_merge::two_click(target.op(), None, None, cx);
    });
    if merging {
        button = button.disabled(true);
    }
    button.into_any_element()
}

/// The word the swapped slot's secondary Merge wears — the Reviews row's and
/// the web's (`session-merge-button.tsx`). No contract constant exists for it
/// yet; when one lands, this is the ONE place to point at it.
pub(crate) const RETRY_MERGE_LABEL: &str = "Retry merge";

/// EXP-799 / EXP-917: whether an ISSUE target's Merge slot swaps to a
/// primary "Fix conflicts" beside a glass "Retry merge". MERGE failures only:
/// the fix run ends in a merge, so a failed CLOSE must never offer it; and
/// only a REAL content conflict (EXP-533) — an offline or policy-refused
/// merge has nothing an agent could rebase; and only with a recorded branch,
/// which is what the run rebases. Pure, so the rule is a test; mirrors web
/// `canOfferFixConflicts`, Android `canOfferFixConflicts` and iOS
/// `canFixConflicts`.
pub(crate) fn merge_slot_swapped(
    pr_open: bool,
    merge_failed: bool,
    is_conflict: bool,
    has_branch: bool,
) -> bool {
    pr_open && merge_failed && is_conflict && has_branch
}

/// EXP-917 — whether the fix-conflicts composer can actually resolve this
/// issue. Its `pr` input is REQUIRED and names the representative issue of an
/// OPEN pull request, resolved through the ACTIVE team's boards
/// (`action_run::resolve_fix_conflicts_target`): the issue and its board must
/// be synced, and the board must belong to the team this window is scoped to.
/// The issue header's own button always bailed on an unknown board; the
/// shared slot keeps that guard (and tightens it to the team), so a click can
/// never open the composer with an empty required input.
pub(crate) fn fix_conflicts_target_resolves(issue_id: &str, window: &Window, cx: &mut App) -> bool {
    let issue_team = Store::try_global(cx).and_then(|store| {
        let collections = store.collections();
        let board_id = collections
            .issues
            .read(cx)
            .get(issue_id)
            .map(|issue| issue.board_id.clone())?;
        collections
            .boards
            .read(cx)
            .get(&board_id)
            .map(|board| board.team_id.clone())
    });
    let Some(issue_team) = issue_team else {
        return false;
    };
    let nav = crate::navigation::nav_for_window(window, cx);
    crate::navigation::active_team_id(&nav, cx).as_deref() == Some(issue_team.as_str())
}

/// The "Fix conflicts" pill (EXP-313): the primary `Sm` capsule that opens
/// the Agent page composer with the fix-conflicts builtin and this issue's
/// PR preselected (EXP-825). "Fixing…" while an actual fix run works the
/// branch; disabled with the reason when this machine has no agent CLI
/// (EXP-367) — never hidden.
pub(crate) fn fix_conflicts_pill(
    id: impl Into<gpui::ElementId>,
    issue_id: &str,
    branch: Option<&str>,
    size: PillSize,
    cx: &mut App,
) -> AnyElement {
    let fixing = branch.is_some_and(|branch| {
        LocalSessions::global_ref(cx)
            .is_some_and(|sessions| sessions.read(cx).is_branch_fixing(branch))
    });
    let no_agent = crate::coding_flow::no_agent_reason(cx);
    let issue_id = issue_id.to_string();
    let mut button = glass_pill_button_primary(id, size)
        .icon(
            Icon::from(ExpIcon::GitBranch)
                .with_size(px(size.glyph()))
                .text_color(cx.theme().primary_foreground),
        )
        .label(if fixing { "Fixing…" } else { "Fix conflicts" })
        .tooltip(
            no_agent
                .clone()
                .unwrap_or_else(|| "Run the fix-conflicts action on this pull request".into()),
        )
        .on_click(move |_, window, cx| {
            // EXP-917: the composer's `pr` input is REQUIRED and resolves the
            // issue through the active team's boards — an issue whose board
            // is not in this window's scope (a team switch mid-conflict, a
            // board that left the shape) would open the run with an empty
            // required input. Refuse instead, the guard the issue header's
            // own button always had.
            if !fix_conflicts_target_resolves(&issue_id, window, cx) {
                log::warn!("[ui] fix conflicts skipped: {issue_id} is outside the active team");
                return;
            }
            crate::navigation::navigate_to_chat(
                window,
                cx,
                crate::navigation::ChatSeed::fix_conflicts(issue_id.clone()),
            );
        });
    if fixing || no_agent.is_some() {
        button = button.disabled(true);
    }
    button.into_any_element()
}

/// EXP-917: the merge SLOT every Merge PR surface renders — the plain
/// [`merge_pill`] until a merge of an ISSUE target is refused by a real
/// conflict, then `[Fix conflicts] [Retry merge]` in that same slot (the
/// swap [`merge_slot_swapped`] decides). ONE place, so no surface can wire
/// the plain pill and forget the swap again: the issue tray, the run header
/// (a batch run merges through its representative issue) and both Changes
/// bars all come through here. A SESSION target (a run's own chore PR,
/// EXP-734) never swaps — the builtin takes an issue — and captions its
/// refusal instead ([`merge_error_caption`]). The swap is short-lived by
/// construction: `MergeState` drops a failure whose row re-synced, so the
/// next echo restores the plain pill.
pub(crate) fn merge_slot(
    id: &str,
    target: &MergeTarget,
    primary: bool,
    size: PillSize,
    cx: &mut App,
) -> AnyElement {
    let Some(issue) = merge_slot_swap_issue(target, cx) else {
        return merge_pill(SharedString::from(id.to_string()), target, primary, size, cx);
    };
    // Fix conflicts takes the primary paint; Merge steps down to the glass
    // "Retry merge" beside it — never a dead end, the conflict may have been
    // resolved outside that run (a teammate rebased and pushed). The pair
    // never shrinks (the header's left side gives way first, like the diff
    // bar's).
    h_flex()
        .flex_shrink_0()
        .items_center()
        .gap_1()
        .child(fix_conflicts_pill(
            SharedString::from(format!("{id}-fix")),
            &issue.id,
            issue.branch.as_deref(),
            size,
            cx,
        ))
        .child(merge_pill_labeled(
            SharedString::from(id.to_string()),
            target,
            false,
            Some(RETRY_MERGE_LABEL),
            size,
            cx,
        ))
        .into_any_element()
}

/// EXP-917 — the issue whose merge refusal SWAPPED this target's slot, or
/// `None` when it renders the plain pill. Read by [`merge_slot`] itself and
/// by [`merge_error_caption`], so "the slot swapped" is one answer and the
/// caption never duplicates a conflict the swap already explains.
fn merge_slot_swap_issue(target: &MergeTarget, cx: &mut App) -> Option<domain::rows::Issue> {
    let MergeTarget::Issue { issue_id } = target else {
        // A SESSION target (a run's own chore PR, EXP-734) never swaps — the
        // builtin takes an issue — and captions its refusal instead.
        return None;
    };
    let issue = Store::try_global(cx)
        .and_then(|store| store.collections().issues.read(cx).get(issue_id).cloned())?;
    let (merge_failed, is_conflict) = {
        let state = crate::pr_merge::MergeState::global(cx);
        let state = state.read(cx);
        (
            state.failed_op(issue_id) == Some(crate::pr_merge::FailedOp::Merge),
            state.is_conflict(issue_id),
        )
    };
    merge_slot_swapped(
        issue.pr_state.as_deref() == Some("open"),
        merge_failed,
        is_conflict,
        issue.branch.is_some(),
    )
    .then_some(issue)
}

/// EXP-917: the refusal caption for a merge target, for a header that has no
/// property tray to carry it (an issue-less run's — the issue tray's
/// `agent_row` renders the same line). A merge that fails for a reason no run
/// can fix (offline, stale base, no GitHub App, a run's own chore PR) still
/// gets a visible message under the pill, never only a log line — but a
/// CONFLICT is not one of those: it already swapped the slot to
/// `[Fix conflicts] [Retry merge]`, which says it better than a red line
/// repeating GitHub's wording under it.
pub(crate) fn merge_error_caption(target: &MergeTarget, cx: &mut App) -> Option<AnyElement> {
    if merge_slot_swap_issue(target, cx).is_some() {
        return None;
    }
    let state = crate::pr_merge::MergeState::global(cx);
    let error = state.read(cx).error(&target.key())?;
    Some(
        v_flex()
            .w_full()
            .px(px(WORK_GUTTER))
            .pb_2()
            .child(
                div()
                    .min_w_0()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(error),
            )
            .into_any_element(),
    )
}

// ---------------------------------------------------------------------------
// The header frame
// ---------------------------------------------------------------------------

/// The issue face's LARGE title block (EXP-1162: the first row of its
/// scrolling body; the run faces wear only the compact title): the web's
/// title Textarea to the pixel, `pt-4 pb-1`, 24px semibold on a 32px line
/// (NOT gpui's `text_2xl`, which is 21px on the 14px rem).
pub(crate) const TITLE_PT: f32 = 16.;
pub(crate) const TITLE_PB: f32 = 4.;
pub(crate) const TITLE_SIZE: f32 = 24.;
pub(crate) const TITLE_LINE: f32 = 32.;
/// The multi-line widget's own insets (`Size::Medium` `input_py` /
/// `input_px`), applied underneath any refined style: the editable title's
/// wrapper gives them back so its text lands on the block above.
pub(crate) const TITLE_WIDGET_PY: f32 = 8.;
pub(crate) const TITLE_WIDGET_PX: f32 = 10.;
/// Web `pb-3` under a fixed tray.
const HEADER_PB: f32 = 12.;
/// EXP-1162: the compact bar's vertical inset around its 36px controls.
pub(crate) const BAR_PY: f32 = 8.;
/// The collapsed title's two lines: the mono identifier over the title.
const COLLAPSED_ID_SIZE: f32 = 11.;
const COLLAPSED_ID_LINE: f32 = 14.;
const COLLAPSED_TITLE_SIZE: f32 = 14.;
const COLLAPSED_TITLE_LINE: f32 = 20.;

/// EXP-1162 — the COLLAPSED title, the compact bar's left side: the mono
/// identifier (small, muted) over the title on ONE truncated line. `fade_in`
/// = the issue face's break (contract `detail-chrome.json`): the element
/// fades in over `collapseMs` while rising `collapseRise`, keyed by the id so
/// it replays each time the title breaks again. Faces that are ALWAYS
/// collapsed (Run, Changes, Results) pass `None` and never animate.
pub(crate) fn collapsed_title(
    identifier: Option<SharedString>,
    title: impl Into<SharedString>,
    fade_in: Option<gpui::ElementId>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let block = v_flex()
        .relative()
        .w_full()
        .min_w_0()
        .children(identifier.map(|identifier| {
            div()
                .min_w_0()
                .truncate()
                .font_family(theme.mono_font_family.clone())
                .text_size(px(COLLAPSED_ID_SIZE))
                .line_height(px(COLLAPSED_ID_LINE))
                .text_color(theme.muted_foreground)
                .child(identifier)
        }))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(COLLAPSED_TITLE_SIZE))
                .line_height(px(COLLAPSED_TITLE_LINE))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(title.into()),
        );
    match fade_in {
        None => block.into_any_element(),
        Some(id) => {
            use gpui::AnimationExt as _;
            let rise = domain::detail_chrome::COLLAPSE_RISE;
            block
                .with_animation(
                    id,
                    gpui::Animation::new(std::time::Duration::from_millis(
                        domain::detail_chrome::COLLAPSE_MS,
                    )),
                    move |block, delta| block.opacity(delta).top(px(rise * (1. - delta))),
                )
                .into_any_element()
        }
    }
}

/// The pieces of one header.
pub(crate) struct WorkHeader {
    /// EXP-1162: the compact bar's left side — the [`collapsed_title`], or
    /// `None` while the issue face's own large title row is on show.
    pub title: Option<AnyElement>,
    /// The compact bar's right: `[badge] [toggle] [pin] [menu]` (plus, on a
    /// collapsed issue face or an issue-less run, Merge / Stop / Resume).
    pub right: Vec<AnyElement>,
    /// A FIXED property tray under the bar — the run faces' (their transcript
    /// is bottom-anchored, nothing to scroll it away with). The issue face
    /// carries its tray in its scrolling body instead.
    pub tray: Option<AnyElement>,
    /// A slim extra row under the tray (the merge-error caption).
    pub extra: Option<AnyElement>,
}

/// EXP-1162 — the fixed header frame: the compact bar, then the fixed tray
/// (if any), centered to [`WORK_COLUMN_W`]. No hairline: the body under it
/// wears [`work_body`]'s top edge strip instead.
pub(crate) fn render_work_header(header: WorkHeader, _cx: &App) -> AnyElement {
    let WorkHeader {
        title,
        right,
        tray,
        extra,
    } = header;
    let fixed_tray = tray.is_some();
    let bar = h_flex()
        .w_full()
        .items_center()
        .gap_3()
        .py(px(BAR_PY))
        .px(px(WORK_GUTTER))
        // The bar keeps the toggle's height with or without a title.
        .min_h(px(theme::tokens::size::CONTROL_LG + 2. * BAR_PY))
        .child(div().flex_1().min_w_0().children(title))
        .child(
            h_flex()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .children(right),
        );
    v_flex()
        .w_full()
        .flex_shrink_0()
        .when(fixed_tray, |header| header.pb(px(HEADER_PB)))
        .child(centered_column(
            v_flex().child(bar).children(tray).children(extra),
        ))
        .into_any_element()
}

/// EXP-1162 — the compact bar's height: one toggle-rung control + its inset.
pub(crate) const BAR_H: f32 = theme::tokens::size::CONTROL_LG + 2. * BAR_PY;
/// Until the cluster has been measured once, a floating title row keeps this
/// far clear of it (toggle `Issue | Run` + pin + `…`, with room to spare).
pub(crate) const CLUSTER_W_FALLBACK: f32 = 240.;
/// The gap a floating title row keeps from the cluster beside it.
pub(crate) const CLUSTER_GAP: f32 = 12.;

/// The cluster measure a floating bar reports back to its view: the width
/// cell and the view to repaint when it changes.
pub(crate) type ClusterMeasure = (Rc<std::cell::Cell<Option<f32>>>, gpui::EntityId);

/// EXP-1162 — the Issue face's FLOATING bar (web `WorkHeader floating`): it
/// overlays the top of the scrolling body (the caller's wrapper is
/// `relative()`, this is its LAST child) and takes no row of its own. At
/// rest it paints nothing but its right cluster, on the large title's first
/// line; the title row keeps clear of the cluster's width (`measure`, only
/// passed at rest — the collapsed cluster grows Merge / Stop / Resume, and
/// the title's wrap must not follow it). Collapsed, it shows the compact
/// title and fades in its ground — the panel at `scrim` + the `edgeTop`
/// strip under it — over `collapseMs`. Nothing but the cluster's buttons
/// carries a hitbox, so the bar's empty left never blocks the title editor
/// (or anything else) under it, nor the scroll wheel.
pub(crate) fn render_floating_bar(
    title: Option<AnyElement>,
    right: Vec<AnyElement>,
    collapsed: bool,
    measure: Option<ClusterMeasure>,
    below: Option<AnyElement>,
) -> AnyElement {
    let cluster = h_flex()
        .relative()
        .flex_shrink_0()
        .items_center()
        .gap_1()
        .children(right)
        .when_some(measure, |cluster, (cell, entity_id)| {
            cluster.child(
                gpui::canvas(
                    move |bounds, _, cx| {
                        let width = f32::from(bounds.size.width);
                        let known = cell.get();
                        cell.set(Some(width));
                        if known.is_none_or(|known| (known - width).abs() > 0.5) {
                            cx.defer(move |cx| cx.notify(entity_id));
                        }
                    },
                    |_, _: (), _, _| {},
                )
                .absolute()
                .size_full(),
            )
        });
    // EXP-1191: with the tray pinned under the bar the fade hangs under the
    // TRAY's ground instead.
    let pinned = below.is_some();
    let ground = collapsed.then(|| {
        use gpui::AnimationExt as _;
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(BAR_H))
            .bg(crate::surface::scrim_ground())
            .when(!pinned, |ground| {
                ground.child(crate::surface::edge_fade_top_scrim().top(px(BAR_H)))
            })
            .with_animation(
                "work-bar-ground",
                gpui::Animation::new(std::time::Duration::from_millis(
                    domain::detail_chrome::COLLAPSE_MS,
                )),
                |ground, delta| ground.opacity(delta),
            )
    });
    let bar = div()
        .relative()
        .h(px(BAR_H))
        .children(ground)
        .child(centered_column(
            h_flex()
                .h(px(BAR_H))
                .items_center()
                .gap_3()
                .px(px(WORK_GUTTER))
                .child(div().flex_1().min_w_0().children(title))
                .child(cluster),
        ));
    // EXP-1191: the pinned tray, on the same ground, the fade under it.
    let below = below.map(|below| {
        div()
            .relative()
            .pb(px(12.))
            .bg(crate::surface::scrim_ground())
            .child(
                crate::surface::edge_fade_top_scrim()
                    .top(gpui::relative(1.)),
            )
            .child(centered_column(div().child(below)))
    });
    v_flex()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .child(bar)
        .children(below)
        .into_any_element()
}

/// EXP-1162 — the area under the work header: `body` (a scroll container or
/// a pane with its own) with the TOP edge strip overlaid on it, and the
/// bottom one when `bottom_edge` (the body runs to the pane's end).
pub(crate) fn work_body(body: AnyElement, bottom_edge: bool) -> AnyElement {
    div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .w_full()
        .child(body)
        .child(crate::surface::edge_fade_top())
        .when(bottom_edge, |body| body.child(crate::surface::edge_fade_bottom()))
        .into_any_element()
}

// ---------------------------------------------------------------------------
// EXP-1170 — the issue face's scrolling title rows, shared by the detail and
// the New issue page (the detail in DRAFT mode)
// ---------------------------------------------------------------------------

/// The large editable title (the `TITLE_*` block): the borderless
/// auto-grow textarea at [`TITLE_SIZE`]. `under_parent_line` = a line above
/// already took the header's top inset. `on_tab` moves focus into the
/// description (web EXP-10 parity) and answers whether it did — only then is
/// the Tab consumed; Shift+Enter is swallowed so no keyboard path puts a
/// newline in a title (EXP-230).
pub(crate) fn title_input_row(
    input: &gpui::Entity<gpui_component::input::TextareaState>,
    under_parent_line: bool,
    on_tab: impl Fn(&mut Window, &mut App) -> bool + 'static,
) -> gpui::Div {
    use gpui_component::input::{self, Textarea};
    let top = if under_parent_line {
        0.
    } else {
        TITLE_PT - TITLE_WIDGET_PY
    };
    div()
        // EXP-877: the multi-line widget insets its text box by
        // `TITLE_WIDGET_PX` / `TITLE_WIDGET_PY` underneath any refined style
        // (no public size knob on `Textarea`), so the wrapper gives that much
        // back on the sides and on top, and pulls the bottom in to `TITLE_PB`
        // with a negative margin: title at `WORK_GUTTER` / `TITLE_PT`,
        // `TITLE_PB` under it.
        .px(px(WORK_GUTTER - TITLE_WIDGET_PX))
        .pt(px(top))
        .pb(px(0.))
        .mb(px(TITLE_PB - TITLE_WIDGET_PY))
        // Tab jumps from the title into the description editor. Capture runs
        // before the input's own Tab handling; Shift+Tab (`OutdentInline`) is
        // a different action, so it keeps its default behavior.
        .capture_action(move |_: &input::IndentInline, window, cx| {
            if on_tab(window, cx) {
                cx.stop_propagation();
            }
        })
        // Shift+Enter would insert a newline in the auto-grow input
        // (`submit_on_enter` only intercepts plain Enter) — swallow it.
        .capture_action(|action: &input::Enter, _window, cx| {
            if action.shift {
                cx.stop_propagation();
            }
        })
        // Style the INPUT itself (EXP-181): the widget's own text size and
        // padding override wrapper styles; `refine_style` runs last, so these
        // win. The explicit line height lifts the widget's fixed 1.25rem
        // (which would clip 24px glyphs), and h_auto releases its fixed box.
        .child(
            Textarea::new(input)
                .appearance(false)
                .text_size(px(TITLE_SIZE))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .line_height(px(TITLE_LINE))
                .px_0()
                .h_auto(),
        )
}

/// EXP-1162 — has the large title row scrolled under the floating bar? Its
/// measured content bottom (`title_bottom`, `None` until the first paint)
/// plus the body's scroll offset against the bar's bottom edge.
pub(crate) fn title_collapsed(
    body_scroll: &gpui::ScrollHandle,
    title_bottom: &std::cell::Cell<Option<f32>>,
) -> bool {
    domain::detail_chrome::is_title_collapsed(
        true,
        title_bottom
            .get()
            .map(|bottom| f64::from(bottom + f32::from(body_scroll.offset().y))),
        f64::from(BAR_H),
    )
}

/// EXP-1191 — whether the Issue face's tray reached the bar's bottom edge
/// (its measured content top + the scroll offset vs `BAR_H`): from there on
/// it stays PINNED under the bar, the web `WorkStickyTray`.
pub(crate) fn tray_pinned(
    body_scroll: &gpui::ScrollHandle,
    tray_top: &std::cell::Cell<Option<f32>>,
) -> bool {
    tray_top
        .get()
        .is_some_and(|top| top + f32::from(body_scroll.offset().y) <= BAR_H)
}

/// The measuring state a floating title row needs: the body's scroll handle,
/// the row's measured content bottom, the tray's measured content top and
/// the bar cluster's measured width — all owned by the view (they outlive
/// one frame).
pub(crate) struct TitleChrome<'a> {
    pub body_scroll: &'a gpui::ScrollHandle,
    pub title_bottom: &'a Rc<std::cell::Cell<Option<f32>>>,
    /// EXP-1191: the tray slot's content top ([`tray_pinned`]) and its
    /// height (the spacer it leaves in the body while pinned).
    pub tray_top: &'a Rc<std::cell::Cell<Option<f32>>>,
    pub tray_h: &'a Rc<std::cell::Cell<Option<f32>>>,
    pub cluster_w: &'a Rc<std::cell::Cell<Option<f32>>>,
    /// The view to repaint when a measure moves the break.
    pub entity_id: gpui::EntityId,
}

/// EXP-1162 — the issue face's floating header AND the rows it hands to the
/// scrolling body: `title` (the parent line + the large title) shifted so
/// its first line centres on the bar and kept clear of the cluster, then
/// `below` (the property tray, the merge caption). Returns `(bar, rows)`;
/// the bar is [`render_floating_bar`] over `compact` (the collapsed title,
/// shown only once `collapsed`) and `right` (the cluster).
///
/// EXP-1191: the tray stays in view on every face. `pinned` (the view's
/// [`tray_pinned`]) is the frame its slot reached the bar: from there the
/// ONE tray rides the bar, under it (gpui has no `position: sticky`; the
/// tray holds entities that render once), and the body keeps a spacer of
/// its measured height, so nothing moves.
#[allow(clippy::too_many_arguments)]
pub(crate) fn scrolling_title_rows(
    chrome: TitleChrome<'_>,
    collapsed: bool,
    title: Vec<AnyElement>,
    tray: AnyElement,
    extra: Vec<AnyElement>,
    compact: Option<AnyElement>,
    right: Vec<AnyElement>,
    pinned: bool,
) -> (AnyElement, AnyElement) {
    // The row's bottom edge, measured as it paints. The canvas reports it in
    // content space for the next render and, when the break it implies
    // differs from the one this frame was rendered with (a scroll offset
    // restored before the first measure, a title that grew a line), asks for
    // ONE repaint — the only notify.
    let probe = {
        let handle = chrome.body_scroll.clone();
        let cell = chrome.title_bottom.clone();
        let entity_id = chrome.entity_id;
        gpui::canvas(
            move |bounds, _window, cx| {
                let viewport_top = handle.bounds().top();
                let bottom = bounds.bottom();
                cell.set(Some(f32::from(bottom - viewport_top - handle.offset().y)));
                let now = domain::detail_chrome::is_title_collapsed(
                    true,
                    Some(f64::from(f32::from(bottom))),
                    f64::from(f32::from(viewport_top) + BAR_H),
                );
                if now != collapsed {
                    cx.defer(move |cx| cx.notify(entity_id));
                }
            },
            |_, _: (), _, _| {},
        )
        .absolute()
        .size_full()
    };
    // EXP-1162 (web twin): the floating bar's cluster sits ON the title's
    // first line — the row keeps clear of the cluster's measured width and
    // shifts up so that line centres on the bar's (`TITLE_PT` + half a
    // `TITLE_LINE` → `BAR_H / 2`). Shifted, the row's box would end ABOVE the
    // bar's bottom edge and read collapsed at rest, so it keeps a `slack` pad
    // that reaches `TITLE_PB` past the bar and hands it back with a negative
    // margin: the tray does not move, the break comes after a few px of
    // scroll.
    let cluster_w = chrome.cluster_w.get().unwrap_or(CLUSTER_W_FALLBACK);
    let shift = BAR_H / 2. - TITLE_PT - TITLE_LINE / 2.;
    let slack = (BAR_H - (shift + TITLE_PT + TITLE_LINE)).max(0.);
    let title = v_flex()
        .relative()
        .w_full()
        .min_w_0()
        .mt(px(shift))
        .pb(px(slack))
        .mb(px(-slack))
        .pr(px(cluster_w + CLUSTER_GAP))
        .children(title)
        .child(probe);
    // EXP-1191: the tray slot's top edge (and, unpinned, its height),
    // measured like the title's bottom; a frame that crossed the pin line
    // asks for one repaint.
    let tray_probe = {
        let handle = chrome.body_scroll.clone();
        let top_cell = chrome.tray_top.clone();
        let h_cell = chrome.tray_h.clone();
        let entity_id = chrome.entity_id;
        gpui::canvas(
            move |bounds, _window, cx| {
                let viewport_top = handle.bounds().top();
                let top = bounds.top();
                top_cell.set(Some(f32::from(top - viewport_top - handle.offset().y)));
                if !pinned {
                    h_cell.set(Some(f32::from(bounds.size.height)));
                }
                let now = f32::from(top - viewport_top) <= BAR_H;
                if now != pinned {
                    cx.defer(move |cx| cx.notify(entity_id));
                }
            },
            |_, _: (), _, _| {},
        )
        .absolute()
        .size_full()
    };
    let tray_h = chrome.tray_h.get();
    let (slot, pinned_tray) = match (pinned, tray_h) {
        (true, Some(height)) => (div().h(px(height)), Some(tray)),
        _ => (div().child(tray), None),
    };
    let slot = slot.relative().w_full().min_w_0().child(tray_probe);
    let rows = v_flex()
        .w_full()
        .min_w_0()
        .child(title)
        .child(slot)
        .children(extra)
        .into_any_element();
    // At rest the cluster reports its width for the title row above.
    let measure = (!collapsed).then(|| (chrome.cluster_w.clone(), chrome.entity_id));
    let header = render_floating_bar(
        compact,
        right,
        collapsed,
        measure,
        pinned_tray,
    );
    (header, rows)
}

/// EXP-417/EXP-568 — the property TRAY under the title: ONE plain row
/// (EXP-1191: no card chrome, the first chip on the title's left edge), the
/// property chips growing from the left, `actions` floating on its right
/// edge (`ml_auto`, so they keep their distance even after wrapping).
/// `flex_1 + min_w_0` give the tray the column's definite width, which is
/// what its own `flex_wrap` wraps the chips against.
pub(crate) fn property_tray(chips: Vec<AnyElement>, actions: Vec<AnyElement>) -> AnyElement {
    let properties = crate::surface::property_row()
        .children(chips)
        .when(!actions.is_empty(), |tray| {
            tray.child(
                h_flex()
                    .ml_auto()
                    .flex_shrink_0()
                    .items_center()
                    .gap_1()
                    .children(actions),
            )
        });
    h_flex()
        .w_full()
        .items_center()
        .px(px(WORK_GUTTER))
        // Web `pt-3` between the title row and the tray.
        .pt(px(12.))
        .child(properties.flex_1().min_w_0())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-799 / EXP-917: the Merge slot swaps to Fix conflicts + Retry merge
    /// on a real conflict of a failed MERGE with a recorded branch — and on
    /// nothing else.
    #[test]
    fn merge_slot_swaps_only_on_a_real_merge_conflict() {
        // A conflict-classified merge failure on an open PR with a branch.
        assert!(merge_slot_swapped(true, true, true, true));
        // A merge refused for another reason (offline, stale base, no App).
        assert!(!merge_slot_swapped(true, true, false, true));
        // A failed CLOSE, even one the server called a conflict.
        assert!(!merge_slot_swapped(true, false, true, true));
        // No recorded branch: nothing for the run to rebase.
        assert!(!merge_slot_swapped(true, true, true, false));
        // The PR is no longer open — the slot is gone anyway.
        assert!(!merge_slot_swapped(false, true, true, true));
    }

    /// EXP-926 / FEED-45 — ONE size rule per placement, pinned as a rule
    /// rather than as a pixel: beside the toggle the actions wear the
    /// toggle's own rung, in the tray they wear the chips'.
    #[test]
    fn a_header_action_takes_its_size_from_its_placement() {
        assert_eq!(header_action_size(false), PillSize::Lg);
        assert_eq!(header_action_size(true), PillSize::Sm);
        // Beside the toggle means the TOGGLE's height — the 36px control the
        // segmented capsule is (`controls::segmented`, web `h-9`).
        assert_eq!(
            header_action_size(false).height(),
            theme::tokens::size::CONTROL_LG
        );
        // In the tray means the CHIPS' — what `pickers::chip_button` wears.
        assert_eq!(
            header_action_size(true).height(),
            theme::tokens::size::CONTROL_SM
        );
    }

    /// EXP-889 — the tray's pills are ONE box. The reporter's Merge PR pill
    /// stood a rung taller (and a type size bigger) than the status /
    /// priority / label / Stop pills beside it, so this DRAWS the tray and
    /// measures every capsule in it: the merge pill must be exactly
    /// [`PillSize::Sm`] tall, like every sibling.
    ///
    /// The probe is a `canvas` sized `absolute().size_full()` inside a
    /// wrapper around each pill: it reports the wrapper's laid-out box, i.e.
    /// the pill's own.
    #[gpui::test]
    async fn every_tray_pill_is_the_same_box(cx: &mut gpui::TestAppContext) {
        use std::cell::Cell;
        use std::rc::Rc;

        #[derive(Default)]
        struct Measured {
            chip: Rc<Cell<f32>>,
            stop: Rc<Cell<f32>>,
            merge: Rc<Cell<f32>>,
        }

        struct Tray(Measured);

        fn probe(out: Rc<Cell<f32>>, child: AnyElement) -> AnyElement {
            div()
                .relative()
                .flex_shrink_0()
                .child(child)
                .child(
                    gpui::canvas(
                        move |bounds, _, _| out.set(f32::from(bounds.size.height)),
                        |_, _: (), _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .into_any_element()
        }

        impl gpui::Render for Tray {
            fn render(
                &mut self,
                _window: &mut Window,
                cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                let target = MergeTarget::Issue {
                    issue_id: "issue-1".to_string(),
                };
                // The real tray, the real chips, the real trailing cluster
                // (`issue_header::chip_row`).
                crate::surface::property_row()
                    .child(probe(
                        self.0.chip.clone(),
                        crate::pickers::chip_button("prop-status", cx)
                            .label("In Review")
                            .into_any_element(),
                    ))
                    .child(
                        h_flex()
                            .ml_auto()
                            .flex_shrink_0()
                            .items_center()
                            .gap_1()
                            .child(probe(
                                self.0.merge.clone(),
                                merge_pill(
                                    "header-merge-pr",
                                    &target,
                                    true,
                                    header_action_size(true),
                                    cx,
                                ),
                            ))
                            .child(probe(
                                self.0.stop.clone(),
                                crate::session_screen::stop_session_pill(
                                    "work-stop",
                                    header_action_size(true),
                                    cx,
                                )
                                .into_any_element(),
                            )),
                    )
            }
        }

        let measured = Measured::default();
        let (chip, stop, merge) = (
            measured.chip.clone(),
            measured.stop.clone(),
            measured.merge.clone(),
        );
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (_view, cx) = cx.add_window_view(|_, _| Tray(measured));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let expected = PillSize::Sm.height();
        assert!(
            (chip.get() - expected).abs() < 2.,
            "a property chip is the Sm rung: {} vs {expected}",
            chip.get()
        );
        assert!(
            (stop.get() - expected).abs() < 2.,
            "the Stop pill is the Sm rung: {} vs {expected}",
            stop.get()
        );
        assert_eq!(
            merge.get(),
            stop.get(),
            "EXP-889: the Merge pill is the SAME box as the Stop pill beside it",
        );
        assert!(
            (merge.get() - expected).abs() < 2.,
            "EXP-889: the Merge pill is the Sm rung: {} vs {expected}",
            merge.get()
        );
    }

    /// EXP-926 / FEED-45 — the OTHER placement, drawn: a run with no issue
    /// (or a batch) has no property card, so Merge PR and Stop stand in row 1
    /// NEXT TO the face toggle. There they must be the toggle's own box —
    /// chip-sized capsules beside a control half again their height read as
    /// strays. Same probe as the tray's test.
    #[gpui::test]
    async fn the_actions_beside_the_toggle_match_the_toggle(cx: &mut gpui::TestAppContext) {
        use std::cell::Cell;
        use std::rc::Rc;

        #[derive(Default)]
        struct Measured {
            toggle: Rc<Cell<f32>>,
            stop: Rc<Cell<f32>>,
            merge: Rc<Cell<f32>>,
        }

        struct Cluster(Measured);

        fn probe(out: Rc<Cell<f32>>, child: AnyElement) -> AnyElement {
            div()
                .relative()
                .flex_shrink_0()
                .child(child)
                .child(
                    gpui::canvas(
                        move |bounds, _, _| out.set(f32::from(bounds.size.height)),
                        |_, _: (), _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .into_any_element()
        }

        impl gpui::Render for Cluster {
            fn render(
                &mut self,
                _window: &mut Window,
                cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                let target = MergeTarget::Session {
                    session_id: "run-1".to_string(),
                };
                let toggle = face_toggle(
                    FaceToggle {
                        issue: false,
                        run: Some("run-1".to_string()),
                        diff: Some((3, 1)),
                        pr_changes: false,
                        results: false,
                        active: Face::Run,
                        runs: Vec::new(),
                        checked_run: None,
                        state: FaceState::default(),
                    },
                    Rc::new(|_, _, _| {}),
                    Rc::new(|_, _, _| {}),
                    cx,
                )
                .expect("two items is a toggle");
                // The real row-1 right cluster (`render_work_header`'s).
                h_flex()
                    .items_center()
                    .gap_1()
                    .child(probe(self.0.toggle.clone(), toggle))
                    .child(probe(
                        self.0.merge.clone(),
                        merge_pill("session-merge", &target, true, header_action_size(false), cx),
                    ))
                    .child(probe(
                        self.0.stop.clone(),
                        crate::session_screen::stop_session_pill(
                            "work-stop",
                            header_action_size(false),
                            cx,
                        )
                        .into_any_element(),
                    ))
            }
        }

        let measured = Measured::default();
        let (toggle, stop, merge) = (
            measured.toggle.clone(),
            measured.stop.clone(),
            measured.merge.clone(),
        );
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (_view, cx) = cx.add_window_view(|_, _| Cluster(measured));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let expected = crate::controls::CTL_LG_H;
        assert!(
            (toggle.get() - expected).abs() < 2.,
            "the face toggle is the Lg rung: {} vs {expected}",
            toggle.get()
        );
        assert!(
            (merge.get() - expected).abs() < 2.,
            "EXP-926: the Merge pill matches the toggle: {} vs {expected}",
            merge.get()
        );
        assert!(
            (stop.get() - expected).abs() < 2.,
            "EXP-926: the Stop pill matches the toggle: {} vs {expected}",
            stop.get()
        );
    }

    /// EXP-1162 — the header's new shape, drawn: the compact bar is ONE
    /// control tall (the toggle's rung + its inset), whether its left side
    /// is empty (the issue face at rest) or carries the collapsed title, and
    /// that title is the identifier over ONE truncated line however long the
    /// issue's title is. No tray = nothing under the bar.
    #[gpui::test]
    async fn the_compact_bar_is_one_control_tall_with_a_one_line_title(
        cx: &mut gpui::TestAppContext,
    ) {
        use std::cell::Cell;
        use std::rc::Rc;

        fn probe(out: Rc<Cell<f32>>, child: AnyElement) -> AnyElement {
            div()
                .relative()
                .w_full()
                .min_w_0()
                .child(child)
                .child(
                    gpui::canvas(
                        move |bounds, _, _| out.set(f32::from(bounds.size.height)),
                        |_, _: (), _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .into_any_element()
        }

        struct Bar {
            with_title: bool,
            header: Rc<Cell<f32>>,
            title: Rc<Cell<f32>>,
        }

        impl gpui::Render for Bar {
            fn render(
                &mut self,
                _window: &mut Window,
                cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                let title = self.with_title.then(|| {
                    probe(
                        self.title.clone(),
                        collapsed_title(
                            Some("EXP-1162".into()),
                            "A very long issue title that could never fit on one line of a \
                             compact bar, not even on the widest monitor anybody owns today",
                            None,
                            cx,
                        ),
                    )
                });
                let stop = crate::session_screen::stop_session_pill(
                    "work-stop",
                    header_action_size(false),
                    cx,
                )
                .into_any_element();
                div().w(px(480.)).child(probe(
                    self.header.clone(),
                    render_work_header(
                        WorkHeader {
                            title,
                            right: vec![stop],
                            tray: None,
                            extra: None,
                        },
                        cx,
                    ),
                ))
            }
        }

        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let bar = theme::tokens::size::CONTROL_LG + 2. * BAR_PY;
        for with_title in [false, true] {
            let (header, title) = (Rc::new(Cell::new(0.)), Rc::new(Cell::new(0.)));
            let (_view, cx) = cx.add_window_view(|_, _| Bar {
                with_title,
                header: header.clone(),
                title: title.clone(),
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert!(
                (header.get() - bar).abs() < 1.,
                "the bar is one control tall (title: {with_title}): {} vs {bar}",
                header.get()
            );
            if with_title {
                let lines = COLLAPSED_ID_LINE + COLLAPSED_TITLE_LINE;
                assert!(
                    (title.get() - lines).abs() < 1.,
                    "identifier over ONE title line: {} vs {lines}",
                    title.get()
                );
                assert!(lines <= theme::tokens::size::CONTROL_LG);
            }
        }
    }

    /// EXP-1162 (web `floating` twin): the Issue face's bar takes NO row —
    /// laid over a body it leaves that body's height alone, at rest and
    /// collapsed — and it reports its cluster's width only when asked.
    #[gpui::test]
    async fn the_floating_bar_takes_no_row_and_measures_its_cluster(
        cx: &mut gpui::TestAppContext,
    ) {
        use std::cell::Cell;
        use std::rc::Rc;

        struct Face {
            collapsed: bool,
            cluster: Rc<Cell<Option<f32>>>,
            height: Rc<Cell<f32>>,
        }

        impl gpui::Render for Face {
            fn render(
                &mut self,
                _window: &mut Window,
                cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                let height = self.height.clone();
                let stop = crate::session_screen::stop_session_pill(
                    "work-stop",
                    header_action_size(false),
                    cx,
                )
                .into_any_element();
                let title = self.collapsed.then(|| {
                    collapsed_title(Some("EXP-1162".into()), "Title", None, cx)
                });
                let measure = (!self.collapsed).then(|| (self.cluster.clone(), cx.entity_id()));
                div().w(px(640.)).child(
                    div()
                        .relative()
                        .w_full()
                        .child(div().h(px(300.)))
                        .child(render_floating_bar(title, vec![stop], self.collapsed, measure, None))
                        .child(
                            gpui::canvas(
                                move |bounds, _, _| height.set(f32::from(bounds.size.height)),
                                |_, _: (), _, _| {},
                            )
                            .absolute()
                            .size_full(),
                        ),
                )
            }
        }

        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        assert_eq!(BAR_H, theme::tokens::size::CONTROL_LG + 2. * BAR_PY);
        for collapsed in [false, true] {
            let (cluster, height) = (Rc::new(Cell::new(None)), Rc::new(Cell::new(0.)));
            let (_view, cx) = cx.add_window_view(|_, _| Face {
                collapsed,
                cluster: cluster.clone(),
                height: height.clone(),
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert_eq!(height.get(), 300., "the bar adds no row (collapsed: {collapsed})");
            match cluster.get() {
                Some(width) => {
                    assert!(!collapsed, "only a resting bar measures its cluster");
                    let pill = header_action_size(false).height();
                    assert!(width >= pill, "the cluster is at least its pill: {width}");
                }
                None => assert!(collapsed, "a resting bar measures its cluster"),
            }
        }
    }

    /// EXP-1162 FACE DOTS: the toggle hands its items to the shared rule —
    /// the Changes item is the contract's `changes`, Results wins the PR dot
    /// when both show — and each tone names itself and wears a row of the
    /// ONE session-dot table.
    #[gpui::test]
    fn the_face_toggle_dots_follow_the_contract(cx: &mut gpui::TestAppContext) {
        use domain::detail_chrome::FaceDotTone;
        let spec = |results: bool, state: FaceState| FaceToggle {
            issue: true,
            run: Some("run-1".to_string()),
            diff: Some((3, 1)),
            pr_changes: true,
            results,
            active: Face::Issue,
            runs: Vec::new(),
            checked_run: None,
            state,
        };
        let live = FaceState { run_live: true, needs_input: false, pr_open: true, ..Default::default() };
        assert_eq!(
            spec(false, live).dots(),
            vec![(Face::Run, FaceDotTone::Running), (Face::Diff, FaceDotTone::Review)]
        );
        assert_eq!(
            spec(true, live).dots(),
            vec![(Face::Run, FaceDotTone::Running), (Face::Results, FaceDotTone::Review)]
        );
        let waiting = FaceState { needs_input: true, pr_open: false, ..live };
        assert_eq!(spec(false, waiting).dots(), vec![(Face::Run, FaceDotTone::NeedsInput)]);
        assert!(spec(true, FaceState::default()).dots().is_empty());

        // FACE MARKS: the Run segment draws its tone as the agent's brand
        // mark (badged while it needs input), never a dot; Results / Changes
        // keep the `review` dot.
        assert_eq!(face_mark(Face::Run, FaceDotTone::Running), FaceMark::Agent { attention: false });
        assert_eq!(face_mark(Face::Run, FaceDotTone::NeedsInput), FaceMark::Agent { attention: true });
        assert_eq!(face_mark(Face::Results, FaceDotTone::Review), FaceMark::Dot(FaceDotTone::Review));
        assert_eq!(face_mark(Face::Diff, FaceDotTone::Review), FaceMark::Dot(FaceDotTone::Review));
        let marks = |state: FaceState| {
            spec(true, state)
                .dots()
                .into_iter()
                .map(|(face, tone)| (face, face_mark(face, tone)))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            marks(waiting),
            vec![(Face::Run, FaceMark::Agent { attention: true })]
        );
        assert_eq!(
            marks(live),
            vec![
                (Face::Run, FaceMark::Agent { attention: false }),
                (Face::Results, FaceMark::Dot(FaceDotTone::Review)),
            ]
        );

        assert_eq!(face_dot_label(FaceDotTone::Running), "Running");
        assert_eq!(face_dot_label(FaceDotTone::NeedsInput), "Needs input");
        assert_eq!(face_dot_label(FaceDotTone::Review), "Pull request open");
        // EXP-1184: the Run segment names its run's state; others their dot.
        {
            use crate::queries::CodingSessionDisplay as D;
            assert_eq!(face_tooltip(Face::Run, FaceDotTone::Running, Some(D::Working)), "Working");
            assert_eq!(face_tooltip(Face::Run, FaceDotTone::Running, Some(D::Done)), "Done");
            assert_eq!(
                face_tooltip(Face::Run, FaceDotTone::Running, Some(D::Review)),
                "Pull request open"
            );
            assert_eq!(
                face_tooltip(Face::Run, FaceDotTone::NeedsInput, Some(D::NeedsInput)),
                "Needs input"
            );
            assert_eq!(face_tooltip(Face::Run, FaceDotTone::Running, None), "Running");
            assert_eq!(
                face_tooltip(Face::Results, FaceDotTone::Review, Some(D::Working)),
                "Pull request open"
            );
        }
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
            assert_eq!(face_dot_color(FaceDotTone::Running, cx), theme::tokens::GREEN.to_hsla());
            assert_eq!(face_dot_color(FaceDotTone::NeedsInput, cx), theme::tokens::YELLOW.to_hsla());
            assert_eq!(face_dot_color(FaceDotTone::Review, cx), theme::tokens::GREEN.to_hsla());
        });
    }

    /// EXP-1162: the edge strips and the break's motion are the CONTRACT's
    /// numbers, read through the domain mirror of `detail-chrome.json`.
    #[test]
    fn the_edge_strips_wear_the_contract_sizes() {
        assert_eq!(domain::detail_chrome::EDGE_TOP, 24.);
        assert_eq!(domain::detail_chrome::EDGE_BOTTOM, 32.);
        assert_eq!(domain::detail_chrome::COLLAPSE_MS, 160);
        assert_eq!(domain::detail_chrome::COLLAPSE_RISE, 4.);
    }

    /// EXP-895: the Diff face item prints the CONTRACT's labels through the
    /// ONE counts renderer ([`crate::diff_pane::counts`]) — `+N` and a U+2212
    /// MINUS SIGN, never an ASCII hyphen, on every desktop surface.
    #[test]
    fn the_diff_label_uses_the_minus_sign() {
        assert_eq!(domain::diff::additions_label(12), "+12");
        assert_eq!(domain::diff::deletions_label(2), "\u{2212}2");
        assert!(!domain::diff::deletions_label(2).contains('-'));
    }

    fn session(id: &str, user: Option<&str>, status: &str) -> domain::rows::CodingSession {
        // No `updated_at`: a live status without a heartbeat stamp counts as
        // live (`coding_session_is_live`), which keeps the fixture clock-free.
        session_at(id, user, status, "2026-01-01T00:00:00Z", None)
    }

    /// A row with an explicit start and, optionally, a heartbeat stamp.
    fn session_at(
        id: &str,
        user: Option<&str>,
        status: &str,
        started_at: &str,
        updated_at: Option<&str>,
    ) -> domain::rows::CodingSession {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "issue_id": "issue-1",
            "status": status,
            "started_at": started_at,
            "updated_at": updated_at,
            "user_id": user,
        }))
        .unwrap()
    }

    const NOW: i64 = 1_800_000_000;

    fn stamp(epoch: i64) -> String {
        chrono::DateTime::from_timestamp(epoch, 0)
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }

    /// EXP-933 — the shared fixture's `issueResultsRun` cases (×4): my
    /// coding target when it has results, else the newest run on the issue by
    /// ANY member with results.
    #[test]
    fn issue_results_run_matches_the_shared_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/session-results.json"
        ))
        .unwrap();
        let block = &fixture["issueResultsRun"];
        let now = chrono::DateTime::parse_from_rfc3339(block["now"].as_str().unwrap())
            .unwrap()
            .timestamp();
        let cases = block["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let rows: Vec<domain::rows::CodingSession> = case["rows"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    serde_json::from_value(serde_json::json!({
                        "id": row["id"],
                        "issue_id": row["issueId"],
                        "user_id": row["userId"],
                        "status": row["status"],
                        "started_at": row["startedAt"],
                        "updated_at": row["updatedAt"],
                        "results": row["results"],
                    }))
                    .unwrap()
                })
                .collect();
            let picked = issue_results_run(
                rows.iter(),
                case["issueId"].as_str().unwrap(),
                case["boundId"].as_str(),
                case["me"].as_str(),
                now,
            );
            assert_eq!(
                picked.map(|row| row.id.as_str()),
                case["expected"].as_str(),
                "fixture case: {name}"
            );
        }
    }

    /// An own LIVE run never hides behind a NEWER ended one: the target is
    /// the live row and the action is Stop (the web `ownLive` order).
    #[test]
    fn a_live_run_behind_a_newer_ended_one_is_still_stop() {
        let running = domain::contract::CODING_SESSION_STATUS_RUNNING;
        let ended = domain::contract::CODING_SESSION_STATUS_ENDED;
        let older_live = session_at("a", Some("me"), running, &stamp(NOW - 3_600), Some(&stamp(NOW - 10)));
        let newer_ended = session_at("b", Some("me"), ended, &stamp(NOW - 60), Some(&stamp(NOW - 30)));
        let rows = [newer_ended.clone(), older_live.clone()];
        let target = coding_target(rows.iter(), "issue-1", None, "me", NOW);
        assert_eq!(target.map(|row| row.id.as_str()), Some("a"));
        // Even when the tab is bound to the ended run: bound only wins live.
        let bound = coding_target(rows.iter(), "issue-1", Some("b"), "me", NOW);
        assert_eq!(bound.map(|row| row.id.as_str()), Some("a"));
        assert!(matches!(
            coding_action(target, Some("me"), NOW, false, None, Some(ResumePath::Local)),
            CodingAction::Stop { ref session_id, .. } if session_id == "a"
        ));
    }

    /// A newer STALE running row (no heartbeat inside the window) is not
    /// live, so it is the newest-overall target, and it earns Start coding —
    /// never Resume on the older ended run behind it.
    #[test]
    fn a_newer_stale_run_over_an_older_ended_one_is_start() {
        let running = domain::contract::CODING_SESSION_STATUS_RUNNING;
        let ended = domain::contract::CODING_SESSION_STATUS_ENDED;
        let stale_secs = domain::contract::CODING_SESSION_STALE_MS / 1000 + 60;
        let older_ended = session_at("a", Some("me"), ended, &stamp(NOW - 7_200), Some(&stamp(NOW - 7_000)));
        let newer_stale = session_at("b", Some("me"), running, &stamp(NOW - 3_600), Some(&stamp(NOW - stale_secs)));
        let rows = [older_ended, newer_stale];
        let target = coding_target(rows.iter(), "issue-1", None, "me", NOW);
        assert_eq!(target.map(|row| row.id.as_str()), Some("b"));
        assert_eq!(
            coding_action(target, Some("me"), NOW, false, None, Some(ResumePath::Local)),
            CodingAction::Start
        );
        // A teammate's live row never enters the selection.
        let theirs = [session_at("c", Some("them"), running, &stamp(NOW), Some(&stamp(NOW)))];
        assert!(coding_target(theirs.iter(), "issue-1", None, "me", NOW).is_none());
    }

    /// An own LIVE run is Stop, whichever machine hosts it.
    #[test]
    fn own_live_run_is_stop() {
        let row = session("s1", Some("me"), domain::contract::CODING_SESSION_STATUS_RUNNING);
        let action = coding_action(Some(&row), Some("me"), NOW, false, Some("Mac".into()), None);
        assert_eq!(
            action,
            CodingAction::Stop {
                session_id: "s1".into(),
                local: false,
                device_label: Some("Mac".into()),
            }
        );
        let local = coding_action(Some(&row), Some("me"), NOW, true, None, None);
        assert!(matches!(local, CodingAction::Stop { local: true, .. }));
    }

    /// An own ENDED run with a resume path is Resume; without one, Start.
    #[test]
    fn own_ended_run_resumes_only_with_a_path() {
        let row = session("s1", Some("me"), domain::contract::CODING_SESSION_STATUS_ENDED);
        let action = coding_action(
            Some(&row),
            Some("me"),
            NOW,
            false,
            Some("Mac".into()),
            Some(ResumePath::Local),
        );
        assert_eq!(
            action,
            CodingAction::Resume {
                session_id: "s1".into(),
                path: ResumePath::Local,
                host_label: Some("Mac".into()),
            }
        );
        assert_eq!(
            coding_action(Some(&row), Some("me"), NOW, false, None, None),
            CodingAction::Start
        );
    }

    /// Nothing of mine — no run, a teammate's run (live or not), or no
    /// signed-in account — is Start. A teammate's run never yields Stop.
    #[test]
    fn nothing_own_is_start() {
        assert_eq!(coding_action(None, Some("me"), NOW, false, None, None), CodingAction::Start);
        let theirs = session("s2", Some("them"), domain::contract::CODING_SESSION_STATUS_RUNNING);
        assert_eq!(
            coding_action(Some(&theirs), Some("me"), NOW, false, None, Some(ResumePath::Local)),
            CodingAction::Start
        );
        let anon = session("s3", None, domain::contract::CODING_SESSION_STATUS_RUNNING);
        assert_eq!(coding_action(Some(&anon), None, NOW, false, None, None), CodingAction::Start);
    }

    /// The toggle hides what is unavailable and needs two items to exist.
    #[test]
    fn face_toggle_items_follow_availability() {
        let toggle = |issue: bool, run: Option<&str>, diff: Option<(u32, u32)>, active: Face| FaceToggle {
            issue,
            run: run.map(str::to_string),
            diff,
            pr_changes: false,
            results: false,
            active,
            runs: Vec::new(),
                        checked_run: None,
                        state: FaceState::default(),
        };
        let issue_only = toggle(true, None, None, Face::Issue);
        assert_eq!(issue_only.items(), vec![Face::Issue]);
        let run_only = toggle(false, Some("r"), None, Face::Run);
        assert_eq!(run_only.items(), vec![Face::Run]);
        let with_run = toggle(true, Some("r"), None, Face::Issue);
        assert_eq!(with_run.items(), vec![Face::Issue, Face::Run]);
        let with_diff = toggle(false, Some("r"), Some((3, 1)), Face::Diff);
        assert_eq!(with_diff.items(), vec![Face::Run, Face::Diff]);
        // EXP-889: a run's diff carries the Changes item on its own, and so
        // does an issue with an open PR and NO run of mine — the item is
        // never gated on the run.
        let orphan_diff = toggle(true, None, Some((3, 1)), Face::Issue);
        assert_eq!(orphan_diff.items(), vec![Face::Issue, Face::Diff]);
    }

    /// EXP-889 — the reporter's tab: an issue whose PR is open, with no run
    /// of mine (or a live run whose worktree is clean, everything pushed),
    /// showed NO diff at all. Changes is a face of its OWN: the web
    /// `availableFaces` pushes it outside the `hasRun` branch, off
    /// `hasChanges = diffStats.fileCount > 0 || prState === 'open'`.
    #[test]
    fn an_open_pr_is_a_changes_face_without_a_run() {
        let toggle = |run: Option<&str>, diff: Option<(u32, u32)>, pr_changes: bool| FaceToggle {
            issue: true,
            run: run.map(str::to_string),
            diff,
            pr_changes,
            results: false,
            active: Face::Issue,
            runs: Vec::new(),
                        checked_run: None,
                        state: FaceState::default(),
        };
        // No run at all: Issue | Changes.
        let pr_only = toggle(None, None, true);
        assert!(pr_only.has_changes());
        assert_eq!(pr_only.items(), vec![Face::Issue, Face::Diff]);
        // A live run with an EMPTY worktree diff still reaches the PR files.
        assert_eq!(
            toggle(Some("r"), None, true).items(),
            vec![Face::Issue, Face::Run, Face::Diff],
        );
        // The run's own diff wins the item (its counts label it) and never
        // doubles it up.
        assert_eq!(
            toggle(Some("r"), Some((3, 1)), true).items(),
            vec![Face::Issue, Face::Run, Face::Diff],
        );
        // No PR, no diff: no Changes item.
        assert!(!toggle(Some("r"), None, false).has_changes());
        assert_eq!(
            toggle(Some("r"), None, false).items(),
            vec![Face::Issue, Face::Run],
        );
    }

    /// EXP-889: the Changes item's label — the counts once they are known,
    /// the web's own word until then. Byte-identical with `work-faces.ts`.
    #[test]
    fn the_changes_face_label_is_changes() {
        assert_eq!(CHANGES_FACE_LABEL, "Changes");
    }

    /// EXP-879: Results is the FOURTH face, always last — issue, run,
    /// changes, results. EXP-933: it no longer needs a run of mine — a
    /// teammate's report is the issue's Results too.
    #[test]
    fn the_results_face_comes_last_and_needs_no_run_of_mine() {
        let toggle = |issue: bool, run: Option<&str>, diff: Option<(u32, u32)>, results: bool| {
            FaceToggle {
                issue,
                run: run.map(str::to_string),
                diff,
                pr_changes: false,
                results,
                active: Face::Run,
                runs: Vec::new(),
                        checked_run: None,
                        state: FaceState::default(),
            }
        };
        assert_eq!(
            toggle(true, Some("r"), Some((3, 1)), true).items(),
            vec![Face::Issue, Face::Run, Face::Diff, Face::Results],
        );
        // No changes yet, results already published: the face still shows.
        assert_eq!(
            toggle(true, Some("r"), None, true).items(),
            vec![Face::Issue, Face::Run, Face::Results],
        );
        // A teammate's results with no run of mine: still a face.
        assert_eq!(
            toggle(true, None, None, true).items(),
            vec![Face::Issue, Face::Results],
        );
        assert_eq!(toggle(true, None, None, false).items(), vec![Face::Issue]);
    }

    /// EXP-879: the fourth face's label, byte-identical with the web and the
    /// natives.
    #[test]
    fn the_results_face_label_is_results() {
        assert_eq!(RESULTS_FACE_LABEL, "Results");
    }

    /// EXP-886: the Run item reads "Runs" once the issue has several runs of
    /// mine — byte-identical with the web `RUN_FACE_LABEL`/`RUNS_FACE_LABEL`.
    /// EXP-950: the run menu rides the Run item, so several runs keep a lone
    /// Run item on show — and nothing else does.
    #[test]
    fn a_lone_run_item_shows_only_for_its_run_menu() {
        let entry = |id: &str| RunEntry {
            id: id.to_string(),
            label: format!("macbook · {id}"),
            live: false,
        };
        let mut spec = FaceToggle {
            issue: false,
            run: Some("run-1".to_string()),
            diff: None,
            pr_changes: false,
            results: false,
            active: Face::Run,
            runs: vec![entry("run-1")],
            checked_run: Some("run-1".to_string()),
            state: FaceState::default(),
        };
        assert!(!spec.multiple_runs());
        assert!(!spec.is_shown(), "one face, one run: no control");
        spec.runs.push(entry("run-2"));
        assert!(spec.multiple_runs());
        assert_eq!(spec.items(), vec![Face::Run]);
        assert!(spec.is_shown(), "the caret keeps the lone Run item up");
        // Runs without a Run item (no run of mine bound) carry no caret.
        spec.run = None;
        assert!(!spec.is_shown());
        // Two faces are a toggle with or without runs.
        spec.issue = true;
        spec.run = Some("run-1".to_string());
        spec.runs.clear();
        assert!(spec.is_shown());
    }

    /// EXP-974: an ISSUE-LESS run's menu is its resume chain — with a
    /// predecessor (or a successor) in it the lone Run item wears the plural
    /// and the caret, exactly as an issue's several runs do; a run nothing
    /// resumed and that resumed nothing keeps the bare word and no control.
    #[test]
    fn an_issue_less_run_with_a_resume_chain_carries_the_caret() {
        let entry = |id: &str| RunEntry {
            id: id.to_string(),
            label: format!("macbook · {id}"),
            live: false,
        };
        let mut spec = FaceToggle {
            issue: false,
            run: Some("resume-1".to_string()),
            diff: Some((3, 1)),
            pr_changes: false,
            results: false,
            active: Face::Run,
            runs: vec![entry("resume-1")],
            checked_run: Some("resume-1".to_string()),
            state: FaceState::default(),
        };
        assert!(!spec.multiple_runs());
        assert_eq!(run_face_label(spec.multiple_runs()), "Run");
        // The row it continues joins the menu (newest first, the viewed
        // one checked).
        spec.runs.push(entry("original"));
        assert!(spec.multiple_runs());
        assert_eq!(run_face_label(spec.multiple_runs()), "Runs");
        assert!(spec.is_shown());
        assert_eq!(spec.items(), vec![Face::Run, Face::Diff]);
        assert_eq!(spec.checked_run.as_deref(), Some("resume-1"));
    }

    #[test]
    fn the_run_face_reads_runs_with_several_runs() {
        assert_eq!(run_face_label(false), "Run");
        assert_eq!(run_face_label(true), "Runs");
        assert_eq!(RUN_FACE_LABEL, "Run");
        assert_eq!(RUNS_FACE_LABEL, "Runs");
    }
}
