//! EXP-1284/EXP-1294: `thread-parts` (Special components): the thread
//! screen's pieces, the IDE half of the styleguide entry — the REAL painters
//! (`crate::thread_screen`) over the contract model
//! (`domain::session_tree::build_session_thread`), fed
//! `fixtures/session-thread.json`'s first case: the title, the ONE tab strip
//! (+ · Conversation · the issue chips · Guide with its dot), then every
//! conversation row in order (EXP-1334: no root card) — the person's
//! bubble, the agent's prose, an event line, a child's session card, a
//! question answered in place (an option with its description, EXP-1322)
//! and the queued reply. The rows the first case has no example of (a plan,
//! a subagent, a picture) follow from the second case, then (EXP-1323) the
//! dispatcher case's report line and suggestions card in every item state,
//! then EXP-1334's folded strip (wide, then narrow with a nested session
//! open), then THE session composer the thread hosts for its DISPATCHER
//! root: the inline tools in the card ("+", attach, `#`, emoji) and the run
//! controls on the options line UNDER it
//! (`crate::composer::SessionComposerControls`: the model pin, the Plan
//! switch, the usage ring), with the fixture's placeholder.
//!
//! No `sync::Store` here: authors read `Someone`, no `#IDENT` pills, and the
//! image cache has no transport (the picture paints its placeholder box).

use gpui::{
    div, px, AnyElement, App, Context, Div, Entity,
    IntoElement as _, ParentElement as _, Render, SharedString, Styled as _, Window,
};
use gpui_component::{v_flex, ActiveTheme as _};

use domain::rows::{MessageRow, PullRequestRow, SessionIssueRow, SessionRow};
use domain::session_tree::{
    build_session_thread, SessionThreadInput, SessionThreadRow, SessionThreadView, TreeIssue,
    THREAD,
};

use crate::icons::registry;
use crate::markdown::ImageCache;
use crate::mention_input::MentionInput;
use crate::thread_screen::{
    agent_prose, bubble_caption, card_row, clock, message_markdown, muted_line, thread_strip, title_row,
    user_bubble, StripIssue, StripPick, ThreadMenuTarget, ThreadStrip,
};

use super::fixture_rows;

pub(crate) const ID: &str = "thread-parts";
pub(crate) const OWNER: &str = "EXP-1293";

/// One fixture case: the view, plus the messages it was built from (a
/// bubble's caption reads its `createdAt`).
pub(super) struct Case {
    pub(super) view: SessionThreadView,
    messages: Vec<MessageRow>,
    issues: Vec<TreeIssue>,
}

/// EXP-1323: the index of the first fixture case whose name starts with
/// `prefix` (the dispatcher case: suggestions, a report, an aside).
pub(super) fn case_named(prefix: &str) -> usize {
    let fixture: serde_json::Value =
        serde_json::from_str(fixture_rows::SESSION_THREAD_JSON).expect("session-thread.json parses");
    fixture["cases"]
        .as_array()
        .and_then(|cases| {
            cases
                .iter()
                .position(|case| case["name"].as_str().is_some_and(|name| name.starts_with(prefix)))
        })
        .expect("the fixture case exists")
}

/// The EXP-1323 dispatcher case's name prefix.
pub(super) const DISPATCHER_CASE: &str = "EXP-1323: a live";

/// Fixture case `index`, built through the shared model.
pub(super) fn case(index: usize) -> Case {
    let fixture: serde_json::Value =
        serde_json::from_str(fixture_rows::SESSION_THREAD_JSON).expect("session-thread.json parses");
    let input = &fixture["cases"][index]["input"];
    let root: SessionRow = fixture_rows::row(&input["root"]);
    let sessions: Vec<SessionRow> = fixture_rows::rows(&input["sessions"]);
    let links: Vec<SessionIssueRow> = fixture_rows::rows(&input["sessionIssues"]);
    let issues: Vec<TreeIssue> = serde_json::from_value(input["issues"].clone()).unwrap_or_default();
    let messages: Vec<MessageRow> = fixture_rows::rows(&input["messages"]);
    let pulls: Vec<PullRequestRow> = fixture_rows::rows(&input["pullRequests"]);
    let view = build_session_thread(
        SessionThreadInput {
            root: &root,
            sessions: &sessions.iter().collect::<Vec<_>>(),
            session_issues: &links.iter().collect::<Vec<_>>(),
            issues: &issues,
            messages: &messages.iter().collect::<Vec<_>>(),
            pull_requests: &pulls.iter().collect::<Vec<_>>(),
            now_ms: fixture_rows::now_ms(&input["now"]),
        },
        None,
    );
    Case { view, messages, issues }
}

/// EXP-1334's case: folded issue + session chips, a child's asks.
pub(super) const FOLD_CASE: &str = "EXP-1334: the dispatcher";

/// THE strip the thread screen draws ([`thread_strip`]) over `view.tabs`,
/// `open` picked, laid out in `width` px through the contract's partition
/// over the MEASURED middle slot (like the screen).
fn strip(case: &Case, open: StripPick, width: f32, prefix: &'static str, window: &mut Window, cx: &mut App) -> AnyElement {
    let slot = window.use_keyed_state(SharedString::from(format!("{prefix}-slot")), cx, |_, _| None::<f32>);
    let available = *slot.read(cx);
    let issues = case
        .issues
        .iter()
        .map(|issue| {
            (
                issue.id.clone(),
                StripIssue {
                    title: issue.title.clone(),
                    status: None,
                    ringed: None,
                },
            )
        })
        .collect();
    let strip = thread_strip(
        ThreadStrip {
            prefix,
            tabs: case.view.tabs.clone(),
            issues,
            clis: Default::default(),
            open,
            available,
            menu: Some(ThreadMenuTarget {
                root_id: case.view.root_id.clone(),
                team_id: None,
                chip: None,
                linked: Vec::new(),
            }),
            on_pick: std::rc::Rc::new(|_, _, _| {}),
            on_slot: Some(std::rc::Rc::new(move |measured, cx| {
                let changed = slot.update(cx, |slot, _| {
                    let changed = slot.is_none_or(|prev| (prev - measured).abs() > 0.5);
                    *slot = Some(measured);
                    changed
                });
                if changed {
                    cx.refresh_windows();
                }
            })),
        },
        window,
        cx,
    );
    div().w(px(width)).max_w_full().child(strip).into_any_element()
}

/// One conversation row, the way the thread screen paints it.
pub(super) fn row(
    case: &Case,
    row: &SessionThreadRow,
    images: &Entity<ImageCache>,
    window: &Window,
    cx: &mut App,
) -> AnyElement {
    if let SessionThreadRow::Message { id, author, body, queued, .. } = row {
        let markdown = message_markdown(SharedString::from(format!("sg-thread-message-{id}")), body, None, images);
        return match author.as_str() {
            domain::contract::MESSAGE_AUTHOR_KIND_USER => {
                let at = case
                    .messages
                    .iter()
                    .find(|message| message.id == *id)
                    .and_then(|message| message.created_at.as_deref());
                let name = crate::comments::author_label(None);
                user_bubble(id, markdown, bubble_caption(Some(&name), &clock(at)), *queued, true, cx)
            }
            domain::contract::MESSAGE_AUTHOR_KIND_AGENT => agent_prose(markdown, cx),
            _ => muted_line(registry::UI_INFO, body.clone(), cx),
        };
    }
    card_row(row, &case.view.root_id, None, images, window, cx).unwrap_or_else(|| div().into_any_element())
}

/// The rows of the second case the first has no example of.
fn extra_rows(view: &SessionThreadView) -> Vec<&SessionThreadRow> {
    view.rows
        .iter()
        .filter(|row| {
            matches!(
                row,
                SessionThreadRow::Plan { .. } | SessionThreadRow::Subagent { .. } | SessionThreadRow::Picture { .. }
            )
        })
        .collect()
}

/// EXP-1323: the dispatcher case's report line and suggestions card.
fn dispatcher_rows(view: &SessionThreadView) -> Vec<&SessionThreadRow> {
    view.rows
        .iter()
        .filter(|row| matches!(row, SessionThreadRow::Report { .. } | SessionThreadRow::Suggestions { .. }))
        .collect()
}

/// THE session composer's field, kept across frames, and the demo
/// dispatcher's model pick (the pin is a live menu here).
struct ComposerDemo {
    mention: Entity<MentionInput>,
    model: String,
}

impl ComposerDemo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (_, mention) = crate::composer::session_composer_field(THREAD.copy.placeholder.clone(), window, cx);
        Self {
            mention,
            model: domain::contract::DEVICE_AGENT_DEFAULTS_THREAD_MODEL.to_string(),
        }
    }
}

impl Render for ComposerDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let submit = crate::composer::SessionComposerSubmit::send("sg-thread-composer", true, |_, _, _| {});
        let model = crate::composer::session_model_pin(
            "sg-thread-model",
            &self.model,
            true,
            true,
            |this: &mut Self, alias: &str, _, cx| {
                this.model = alias.to_string();
                cx.notify();
            },
            cx,
        );
        let ring = crate::composer::session_context_ring(
            crate::composer::ContextRingProps {
                id: "sg-thread-context".into(),
                session_id: String::new(),
                device_id: None,
                agent: Some(coding::CodingAgent::Claude),
                local: false,
                working: false,
                usage: Some(steer::SessionUsage {
                    context_used: 38_000,
                    context_size: 200_000,
                    cost_usd: None,
                }),
                layout: Vec::new(),
                legend_open: false,
                on_toggle_legend: std::rc::Rc::new(|_, _| {}),
            },
            cx,
        );
        crate::composer::session_composer_box(
            crate::composer::SessionComposerBox::new(self.mention.clone(), submit)
                // EXP-1334: the inline tools (the "+" options menu, attach,
                // `#`, emoji), the options line under the card.
                .tool(crate::composer::composer_tool("sg-thread-plus", crate::icons::registry::UI_ADD, cx).tooltip("Add"))
                .tool(
                    crate::composer::composer_tool("sg-thread-attach", crate::icons::registry::UI_ATTACH, cx)
                        .tooltip("Add file or image"),
                )
                .tool(crate::composer::composer_tool("sg-thread-issue", crate::icons::registry::EDITOR_ISSUE_REF, cx))
                .tool(crate::composer::composer_tool("sg-thread-emoji", crate::icons::registry::EDITOR_EMOJI, cx))
                .controls(crate::composer::SessionComposerControls {
                    model_pin: model,
                    plan: Some(
                        gpui_component::h_flex()
                            .gap_1p5()
                            .items_center()
                            .px_1()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(crate::composer::PLAN_MODE_FALLBACK)
                            .child(crate::controls::web_switch("sg-thread-plan").checked(false))
                            .into_any_element(),
                    ),
                    context_ring: ring,
                }),
            cx,
        )
    }
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window
        .use_keyed_state("sg-thread-parts-images", cx, |_, _| ImageCache::new(None))
        .clone();
    let composer = window.use_keyed_state("sg-thread-parts-composer", cx, ComposerDemo::new);
    let first = case(0);
    let second = case(1);
    let mut conversation = v_flex().w_full().gap_5();
    for item in &first.view.rows {
        conversation = conversation.child(row(&first, item, &images, window, cx));
    }
    for item in extra_rows(&second.view) {
        conversation = conversation.child(row(&second, item, &images, window, cx));
    }
    let dispatcher = case(case_named(DISPATCHER_CASE));
    for item in dispatcher_rows(&dispatcher.view) {
        conversation = conversation.child(row(&dispatcher, item, &images, window, cx));
    }
    // EXP-1334: the folded strip, wide (titles), then narrow (identifiers,
    // then "+N" per zone), a nested session open (`<chip> › <open one>`).
    let fold = case(case_named(FOLD_CASE));
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let title = title_row(&first.view.title, first.view.private, cx);
    let strips = [
        strip(&first, StripPick::Conversation, 760., "sg-thread", window, cx),
        strip(&fold, StripPick::Conversation, 760., "sg-thread-wide", window, cx),
        strip(&fold, StripPick::Issue("i-1".into()), 600., "sg-thread-mid", window, cx),
        strip(&fold, StripPick::Session("g2".into()), 480., "sg-thread-narrow", window, cx),
    ];
    let [plain, wide, mid, narrow] = strips;
    v_flex()
        .w(px(760.))
        .gap_4()
        .child(title)
        .child(plain)
        .child(div().w_full().h(px(1.)).bg(border))
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child("Folded chips: sub-issues and deeper sessions behind a chevron (760 · 600 · 480 px)"),
        )
        .child(wide)
        .child(mid)
        .child(narrow)
        .child(conversation)
        .child(composer)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo draws every row kind the conversation has, the session card
    /// first.
    #[test]
    fn the_demo_draws_every_row_kind() {
        let first = case(0);
        let second = case(1);
        let dispatcher = case(case_named(DISPATCHER_CASE));
        let kinds: std::collections::BTreeSet<&str> = first
            .view
            .rows
            .iter()
            .chain(extra_rows(&second.view))
            .chain(dispatcher_rows(&dispatcher.view))
            .map(|row| match row {
                SessionThreadRow::Message { .. } => "message",
                SessionThreadRow::Session { .. } => "session",
                SessionThreadRow::Question { .. } => "question",
                SessionThreadRow::Plan { .. } => "plan",
                SessionThreadRow::Subagent { .. } => "subagent",
                SessionThreadRow::Picture { .. } => "picture",
                SessionThreadRow::Event { .. } => "event",
                SessionThreadRow::Report { .. } => "report",
                SessionThreadRow::Suggestions { .. } => "suggestions",
            })
            .collect();
        assert_eq!(kinds.len(), 9, "{kinds:?}");
        // EXP-1334: the dispatcher has no card of its own.
        assert!(!matches!(first.view.rows.first(), Some(SessionThreadRow::Session { .. })));
        assert_eq!(first.view.title, "Implement EXP-12 + 1");
    }

    /// EXP-1322: the demo's open question shows an option's description
    /// under its label (the session view's card).
    #[test]
    fn the_open_question_carries_a_described_option() {
        let first = case(0);
        let described = first.view.rows.iter().any(|row| match row {
            SessionThreadRow::Question { options, answered: false, .. } => {
                options.iter().any(|option| option.description.is_some())
            }
            _ => false,
        });
        assert!(described, "an unanswered question with a described option");
    }
}
