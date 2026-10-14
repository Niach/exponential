//! EXP-1284/EXP-1294: `thread-parts` (Special components): the thread
//! screen's pieces, the IDE half of the styleguide entry — the REAL painters
//! (`crate::thread_screen`) over the contract model
//! (`domain::session_tree::build_session_thread`), fed
//! `fixtures/session-thread.json`'s first case: the title, the ONE tab strip
//! (Conversation · the issue chips · + · Guide with its dot), then every
//! conversation row in order — the root's own session card, the person's
//! bubble, the agent's prose, an event line, a child's session card, a
//! question answered in place and the queued reply. The rows the first case
//! has no example of (a plan, a subagent, a picture) follow from the second
//! case, then THE session composer box with the fixture's placeholder.
//!
//! No `sync::Store` here: authors read `Someone`, no `#IDENT` pills, and the
//! image cache has no transport (the picture paints its placeholder box).

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, Context, Div, Entity,
    IntoElement as _, ParentElement as _, Render, SharedString, Styled as _, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::rows::{MessageRow, PullRequestRow, SessionIssueRow, SessionRow};
use domain::session_tree::{
    build_session_thread, SessionThreadInput, SessionThreadRow, SessionThreadView, ThreadTab, TreeIssue,
    THREAD,
};

use crate::icons::registry;
use crate::markdown::ImageCache;
use crate::mention_input::MentionInput;
use crate::thread_screen::{
    agent_prose, bubble_caption, card_row, clock, guide_dot, message_markdown, muted_line, strip_chip,
    strip_divider, strip_issue_body, title_row, user_bubble,
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

/// Fixture case `index`, built through the shared model.
pub(super) fn case(index: usize) -> Case {
    let fixture: serde_json::Value =
        serde_json::from_str(fixture_rows::SESSION_THREAD_JSON).expect("session-thread.json parses");
    let input = &fixture["cases"][index]["input"];
    let root: SessionRow = fixture_rows::row(&input["root"]);
    let sessions: Vec<SessionRow> = fixture_rows::rows(&input["sessions"]);
    let links: Vec<SessionIssueRow> = fixture_rows::rows(&input["sessionIssues"]);
    let issues: Vec<TreeIssue> = input["issues"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|issue| TreeIssue {
            id: issue["id"].as_str().unwrap_or_default().to_string(),
            identifier: issue["identifier"].as_str().unwrap_or_default().to_string(),
            title: issue["title"].as_str().unwrap_or_default().to_string(),
        })
        .collect();
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

/// The strip the thread screen draws over `view.tabs`: Conversation active,
/// the `+`, the divider, Guide.
fn strip(case: &Case, cx: &App) -> AnyElement {
    let mut chips: Vec<AnyElement> = Vec::new();
    let mut trailing: Vec<AnyElement> = Vec::new();
    for tab in &case.view.tabs {
        match tab {
            ThreadTab::Conversation => chips.push(
                strip_chip("sg-thread-conversation", true, cx)
                    .child(Icon::new(registry::ACTION_CHAT).xsmall())
                    .child(SharedString::from(THREAD.copy.conversation.clone()))
                    .into_any_element(),
            ),
            ThreadTab::Issue { issue_id, identifier } => {
                let title = case
                    .issues
                    .iter()
                    .find(|issue| issue.id == *issue_id)
                    .map(|issue| issue.title.as_str())
                    .unwrap_or_default();
                chips.push(
                    strip_chip(SharedString::from(format!("sg-thread-issue-{issue_id}")), false, cx)
                        .children(strip_issue_body(None, identifier, title, false, cx))
                        .into_any_element(),
                );
            }
            ThreadTab::Add => {
                chips.push(
                    crate::controls::ghost_icon_button("sg-thread-add", Icon::new(registry::UI_ADD), cx)
                        .into_any_element(),
                );
                trailing.push(strip_divider(cx));
            }
            // The session chips land with the in-place session view (1b).
            ThreadTab::Session { .. } => {}
            ThreadTab::Guide { dot } => trailing.push(
                strip_chip("sg-thread-guide", false, cx)
                    .child(Icon::new(registry::GUIDE_CHANGES).xsmall())
                    .child(SharedString::from(THREAD.copy.guide.clone()))
                    .when(*dot, |chip| chip.child(guide_dot(cx)))
                    .into_any_element(),
            ),
        }
    }
    h_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .items_center()
        .children(chips)
        .children(trailing)
        .into_any_element()
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

/// THE session composer box's field, kept across frames.
struct ComposerDemo {
    mention: Entity<MentionInput>,
}

impl ComposerDemo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (_, mention) = crate::composer::session_composer_field(THREAD.copy.placeholder.clone(), window, cx);
        Self { mention }
    }
}

impl Render for ComposerDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let submit = crate::composer::SessionComposerSubmit::send("sg-thread-composer", true, |_, _, _| {});
        crate::composer::session_composer_box(
            crate::composer::SessionComposerBox::new(self.mention.clone(), submit).tool(
                crate::composer::composer_tool("sg-thread-attach", crate::icons::registry::UI_ADD, cx)
                    .tooltip("Add file or image"),
            ),
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
    v_flex()
        .w(px(760.))
        .gap_4()
        .child(title_row(&first.view.title, first.view.private, cx))
        .child(strip(&first, cx))
        .child(div().w_full().h(px(1.)).bg(cx.theme().border))
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
        let kinds: std::collections::BTreeSet<&str> = first
            .view
            .rows
            .iter()
            .chain(extra_rows(&second.view))
            .map(|row| match row {
                SessionThreadRow::Message { .. } => "message",
                SessionThreadRow::Session { .. } => "session",
                SessionThreadRow::Question { .. } => "question",
                SessionThreadRow::Plan { .. } => "plan",
                SessionThreadRow::Subagent { .. } => "subagent",
                SessionThreadRow::Picture { .. } => "picture",
                SessionThreadRow::Event { .. } => "event",
            })
            .collect();
        assert_eq!(kinds.len(), 7, "{kinds:?}");
        assert!(matches!(first.view.rows.first(), Some(SessionThreadRow::Session { .. })));
        assert_eq!(first.view.title, "Implement EXP-12 + 1");
    }
}
