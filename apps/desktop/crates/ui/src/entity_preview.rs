//! EXP-920 — the hover CARD and the click TARGET behind an entity chip
//! ([`crate::entity_chip`]) under a settled Exponential tool row.
//!
//! The pure rule (label, icon concept, `list` grouping) is
//! `domain::entity_preview`; this module resolves a wire ref
//! ([`steer::EntityRef`]) against the desktop's OWN synced store: the card
//! per kind, and where a click goes. A kind whose row is not synced here
//! (another team's board, a trashed one, a brand-new row the shape has not
//! landed) has NO card and an inert chip — except `repository` and `thread`,
//! which never sync and get a slim card over what the answer itself named.
//!
//! The web twin is `components/entity-preview-card.tsx`; iOS/Android mirror
//! the same per-kind content.

use gpui::{
    div, AnyElement, App, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::entity_preview::{self as rule, EntityRefView};
use steer::EntityRef;

use crate::icons::{registry, ExpIcon};
use crate::issue_preview::card_frame;
use crate::navigation::{navigate, Screen};

/// The issues a board card lists before it says `+N more`.
const BOARD_CARD_ISSUES: usize = 6;
/// A comment card's body excerpt, in characters.
const COMMENT_EXCERPT_CHARS: usize = 240;

/// The domain rule's borrowed view of a wire ref.
pub(crate) fn view(r#ref: &EntityRef) -> EntityRefView<'_> {
    EntityRefView {
        kind: &r#ref.kind,
        id: &r#ref.id,
        identifier: r#ref.identifier.as_deref(),
        title: r#ref.title.as_deref(),
        count: r#ref.count,
    }
}

/// An icon CONCEPT name (`domain::entity_preview::ENTITY_REF_ICON`) → its
/// registry glyph. `None` for a concept this build does not carry (the
/// generated registry has no by-concept lookup, so the table is spelled out
/// here and locked by test against every contract kind).
pub(crate) fn concept_icon(concept: &str) -> Option<ExpIcon> {
    Some(match concept {
        "ui-issue" => registry::UI_ISSUE,
        "nav-boards" => registry::NAV_BOARDS,
        "nav-actions" => registry::NAV_ACTIONS,
        "nav-automations" => registry::NAV_AUTOMATIONS,
        "notification-issue-comment" => registry::NOTIFICATION_ISSUE_COMMENT,
        "coding-running" => registry::CODING_RUNNING,
        "settings-labels" => registry::SETTINGS_LABELS,
        "settings-statuses" => registry::SETTINGS_STATUSES,
        "nav-workflows" => registry::NAV_WORKFLOWS,
        "ui-device" => registry::UI_DEVICE,
        "ui-avatar-placeholder" => registry::UI_AVATAR_PLACEHOLDER,
        "ui-repository" => registry::UI_REPOSITORY,
        "ui-team" => registry::UI_TEAM,
        "ui-invite" => registry::UI_INVITE,
        "nav-notifications" => registry::NAV_NOTIFICATIONS,
        "nav-support" => registry::NAV_SUPPORT,
        "ui-attach" => registry::UI_ATTACH,
        "ui-checklist" => registry::UI_CHECKLIST,
        _ => return None,
    })
}

/// The glyph a ref's chip leads with when no synced row lends it a better
/// one (an issue's status glyph): its kind's concept, a list its member
/// kind's, an unknown kind the list glyph.
pub(crate) fn ref_icon(r#ref: &EntityRef) -> ExpIcon {
    concept_icon(rule::entity_ref_icon(&r#ref.kind, &r#ref.id)).unwrap_or(registry::UI_CHECKLIST)
}

// ---------------------------------------------------------------------------
// Store lookups
// ---------------------------------------------------------------------------

/// An issue by row id OR by identifier (`EXP-42`): a tool input names an
/// issue either way, and the publisher forwards what it was given.
pub(crate) fn find_issue(id: &str, cx: &App) -> Option<domain::rows::Issue> {
    let store = sync::Store::try_global(cx)?;
    let issues = store.collections().issues.read(cx);
    if let Some(issue) = issues.get(id) {
        return Some(issue.clone());
    }
    issues.iter().find(|issue| issue.identifier == id).cloned()
}

/// A device by its machine `device_id` (what a `{deviceId, label}` answer
/// carries) or its row id.
fn find_device(id: &str, cx: &App) -> Option<domain::rows::DeviceRow> {
    let store = sync::Store::try_global(cx)?;
    let devices = store.collections().devices.read(cx);
    if let Some(device) = devices.get(id) {
        return Some(device.clone());
    }
    devices
        .iter()
        .find(|device| device.device_id.as_deref() == Some(id))
        .cloned()
}

/// What the click-target rule needs to know about a ref's row here: whether
/// it is synced, and (for a comment / an attachment) the issue it belongs
/// to. Pure over the store so [`target_for`] can be unit-tested without one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EntityRowFacts {
    pub(crate) synced: bool,
    pub(crate) issue_id: Option<String>,
}

pub(crate) fn row_facts(kind: &str, id: &str, cx: &App) -> EntityRowFacts {
    let Some(store) = sync::Store::try_global(cx) else {
        return EntityRowFacts::default();
    };
    let collections = store.collections().clone();
    let synced = |present: bool| EntityRowFacts { synced: present, issue_id: None };
    match kind {
        "issue" => synced(find_issue(id, cx).is_some()),
        "board" => synced(collections.boards.read(cx).get(id).is_some()),
        "action" => synced(collections.actions.read(cx).get(id).is_some()),
        "automation" => synced(collections.automations.read(cx).get(id).is_some()),
        "comment" => match collections.comments.read(cx).get(id) {
            Some(comment) => EntityRowFacts { synced: true, issue_id: Some(comment.issue_id.clone()) },
            None => EntityRowFacts::default(),
        },
        "session" => synced(collections.coding_sessions.read(cx).get(id).is_some()),
        "label" => synced(collections.labels.read(cx).get(id).is_some()),
        "status" => synced(collections.issue_statuses.read(cx).get(id).is_some()),
        "workflow" => synced(collections.workflows.read(cx).get(id).is_some()),
        "device" => synced(find_device(id, cx).is_some()),
        "member" => synced(collections.users.read(cx).get(id).is_some()),
        "team" => synced(collections.teams.read(cx).get(id).is_some()),
        "invite" => synced(collections.team_invites.read(cx).get(id).is_some()),
        "notification" => synced(collections.notifications.read(cx).get(id).is_some()),
        "attachment" => match collections.attachments.read(cx).get(id) {
            Some(attachment) => EntityRowFacts { synced: true, issue_id: attachment.issue_id.clone() },
            None => EntityRowFacts::default(),
        },
        _ => EntityRowFacts::default(),
    }
}

// ---------------------------------------------------------------------------
// Click targets
// ---------------------------------------------------------------------------

/// Where a chip click goes. Screens navigate; a run opens through
/// [`crate::session_screen::open_session`] (EXP-773: every entry point
/// funnels there); actions and automations open their edit dialogs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EntityTarget {
    Screen(Screen),
    Session(String),
    Action(String),
    Automation(String),
}

/// The click-target rule, pure over [`EntityRowFacts`]:
///
/// * a synced row opens its detail (issue, board, run, workflow), its edit
///   dialog (action, automation), the Devices page, Settings (label, status,
///   member, invite, team) or the Inbox (notification);
/// * a comment / an attachment opens the ISSUE it belongs to;
/// * `repository` and `thread` never sync: a repository opens Settings
///   (where repositories live), a thread its Support conversation (the screen
///   fetches it);
/// * a `list` chip is hover-only; an unsynced or unknown ref is inert.
pub(crate) fn target_for(kind: &str, id: &str, facts: &EntityRowFacts) -> Option<EntityTarget> {
    match kind {
        "repository" => return Some(EntityTarget::Screen(Screen::Settings)),
        "thread" => {
            return Some(EntityTarget::Screen(Screen::SupportThread { thread_id: id.to_string() }))
        }
        "list" => return None,
        _ => {}
    }
    if !facts.synced {
        return None;
    }
    Some(match kind {
        "issue" => EntityTarget::Screen(Screen::IssueDetail { issue_id: id.to_string() }),
        "board" => EntityTarget::Screen(Screen::BoardIssues { board_id: id.to_string() }),
        "session" => EntityTarget::Session(id.to_string()),
        "action" => EntityTarget::Action(id.to_string()),
        "automation" => EntityTarget::Automation(id.to_string()),
        "workflow" => EntityTarget::Screen(Screen::Workflow { workflow_id: id.to_string() }),
        "device" => EntityTarget::Screen(Screen::Devices),
        "label" | "status" | "member" | "invite" | "team" => EntityTarget::Screen(Screen::Settings),
        "notification" => {
            EntityTarget::Screen(Screen::Inbox { tab: crate::sidebar::InboxTab::Inbox })
        }
        "comment" | "attachment" => EntityTarget::Screen(Screen::IssueDetail {
            issue_id: facts.issue_id.clone()?,
        }),
        _ => return None,
    })
}

/// [`target_for`] over the live store. An issue ref naming an IDENTIFIER
/// resolves to the row's id first, so the detail screen gets a real id.
pub(crate) fn entity_target(kind: &str, id: &str, cx: &App) -> Option<EntityTarget> {
    if kind == "issue" {
        let issue = find_issue(id, cx)?;
        return target_for(kind, &issue.id, &EntityRowFacts { synced: true, issue_id: None });
    }
    target_for(kind, id, &row_facts(kind, id, cx))
}

pub(crate) fn open_target(target: EntityTarget, window: &mut Window, cx: &mut App) {
    match target {
        EntityTarget::Screen(screen) => navigate(window, cx, screen),
        EntityTarget::Session(session_id) => {
            crate::session_screen::open_session(&session_id, window, cx)
        }
        EntityTarget::Action(action_id) => crate::action_editor_dialog::open(window, cx, action_id),
        EntityTarget::Automation(automation_id) => {
            crate::automation_dialog::open_edit(window, cx, automation_id)
        }
    }
}

// ---------------------------------------------------------------------------
// The cards
// ---------------------------------------------------------------------------

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// A card's first line: a glyph and the entity's name.
fn header(glyph: impl IntoElement, title: impl Into<SharedString>) -> gpui::Div {
    h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_2()
        .child(div().flex_shrink_0().child(glyph))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .line_clamp(2)
                .child(title.into()),
        )
}

/// A quiet secondary line.
fn muted_line(text: impl Into<SharedString>, cx: &App) -> gpui::Div {
    div()
        .w_full()
        .min_w_0()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .line_clamp(2)
        .child(text.into())
}

fn kind_glyph(r#ref: &EntityRef, cx: &App) -> Icon {
    Icon::new(ref_icon(r#ref))
        .small()
        .text_color(cx.theme().muted_foreground)
}

/// The slim card for a kind that never syncs (`repository`, `thread`) or
/// whose row is not here: the ref's own title over its noun.
fn slim_card(r#ref: &EntityRef, cx: &App) -> AnyElement {
    let label = rule::entity_chip_label(&view(r#ref));
    let noun = rule::entity_kind_noun(&r#ref.kind, 1);
    card_frame(cx)
        .child(header(kind_glyph(r#ref, cx), label))
        .child(muted_line(capitalized(noun), cx))
        .into_any_element()
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The hover card for one chip, resolved from the synced store. `None` when
/// the row is not synced here (the chip then has no card at all).
pub(crate) fn card(r#ref: &EntityRef, members: &[EntityRef], cx: &mut App) -> Option<AnyElement> {
    match r#ref.kind.as_str() {
        "issue" => {
            let issue = find_issue(&r#ref.id, cx)?;
            crate::issue_preview::card(&issue.id, cx)
        }
        "board" => board_card(&r#ref.id, cx),
        "action" => action_card(&r#ref.id, cx),
        "automation" => automation_card(&r#ref.id, cx),
        "comment" => comment_card(&r#ref.id, cx),
        "session" => session_card(&r#ref.id, cx),
        "label" => label_card(&r#ref.id, cx),
        "status" => status_card(&r#ref.id, cx),
        "workflow" => workflow_card(&r#ref.id, cx),
        "device" => device_card(&r#ref.id, cx),
        "member" => member_card(&r#ref.id, cx),
        "team" => team_card(&r#ref.id, cx),
        "invite" => invite_card(&r#ref.id, cx),
        "notification" => notification_card(&r#ref.id, cx),
        "attachment" => attachment_card(&r#ref.id, cx),
        "repository" | "thread" => Some(slim_card(r#ref, cx)),
        "list" => Some(list_card(r#ref, members, cx)),
        _ => None,
    }
}

/// One issue line inside a card: status glyph + mono identifier + title.
fn issue_line(issue: &domain::rows::Issue, cx: &App) -> gpui::Div {
    let status = crate::queries::resolve_issue_status(cx, issue);
    let theme = cx.theme();
    h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_1p5()
        .text_xs()
        .child(
            div()
                .flex_shrink_0()
                .child(crate::icons::resolved_status_icon(&status, cx).xsmall()),
        )
        .child(
            div()
                .flex_shrink_0()
                .font_family(theme.mono_font_family.clone())
                .text_color(theme.muted_foreground)
                .child(SharedString::from(issue.identifier.clone())),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .child(SharedString::from(issue.title.clone())),
        )
}

fn board_card(board_id: &str, cx: &mut App) -> Option<AnyElement> {
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let board = collections.boards.read(cx).get(board_id).cloned()?;
    let mut open: Vec<domain::rows::Issue> = collections
        .issues
        .read(cx)
        .iter()
        .filter(|issue| issue.board_id == board.id)
        .filter(|issue| {
            !matches!(
                issue.status,
                domain::IssueStatus::Done
                    | domain::IssueStatus::Cancelled
                    | domain::IssueStatus::Duplicate
            )
        })
        .cloned()
        .collect();
    open.sort_by(|a, b| b.number.cmp(&a.number));
    let total = open.len();
    let mut card = card_frame(cx).child(header(
        crate::icons::board_icon(&board).small(),
        board.name.clone(),
    ));
    if total == 0 {
        card = card.child(muted_line("No open issues", cx));
    }
    let mut rows = v_flex().w_full().gap_1();
    for issue in open.iter().take(BOARD_CARD_ISSUES) {
        rows = rows.child(issue_line(issue, cx));
    }
    card = card.child(rows);
    if total > BOARD_CARD_ISSUES {
        card = card.child(muted_line(format!("+{} more", total - BOARD_CARD_ISSUES), cx));
    }
    Some(card.into_any_element())
}

fn action_card(action_id: &str, cx: &mut App) -> Option<AnyElement> {
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let action = collections.actions.read(cx).get(action_id).cloned()?;
    let name = action
        .name
        .clone()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Action".to_string());
    let mut card = card_frame(cx).child(header(
        crate::icons::action_icon(action.icon.as_deref()).small(),
        name,
    ));
    if let Some(description) = action
        .description
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        card = card.child(muted_line(description.to_string(), cx));
    }
    let mut facts: Vec<String> = Vec::new();
    // Repositories never sync (server-only rows): the card can say the
    // action is bound, not to what.
    if action.repository_id.is_some() {
        facts.push("Repository-bound".to_string());
    }
    let inputs = action
        .inputs
        .as_ref()
        .and_then(|inputs| inputs.as_array())
        .map(|inputs| inputs.len())
        .unwrap_or(0);
    facts.push(match inputs {
        1 => "1 input".to_string(),
        n => format!("{n} inputs"),
    });
    card = card.child(muted_line(facts.join(" · "), cx));
    Some(card.into_any_element())
}

fn automation_card(automation_id: &str, cx: &mut App) -> Option<AnyElement> {
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let automation = collections.automations.read(cx).get(automation_id).cloned()?;
    let action_name = automation
        .action_id
        .as_deref()
        .and_then(|action_id| collections.actions.read(cx).get(action_id).cloned())
        .and_then(|action| action.name)
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());
    let title = action_name
        .clone()
        .unwrap_or_else(|| "Automation".to_string());
    let trigger = crate::automation_editor::parsed_trigger(automation.trigger.as_ref())
        .as_ref()
        .map(coding::automations::trigger_summary)
        .unwrap_or_else(|| "Unsupported trigger".to_string());
    let device = automation.device_id.as_deref().and_then(|device_id| {
        find_device(device_id, cx).and_then(|device| device.label)
    });
    let mut card = card_frame(cx).child(header(
        Icon::new(registry::NAV_AUTOMATIONS)
            .small()
            .text_color(cx.theme().muted_foreground),
        title,
    ));
    if let Some(action_name) = action_name {
        card = card.child(muted_line(format!("Runs {action_name}"), cx));
    }
    card = card.child(muted_line(trigger, cx));
    let mut facts: Vec<String> = Vec::new();
    if let Some(device) = device {
        facts.push(device);
    }
    facts.push(if automation.is_enabled() { "Enabled" } else { "Disabled" }.to_string());
    card = card.child(muted_line(facts.join(" · "), cx));
    Some(card.into_any_element())
}

/// A crude plain-text excerpt of a GFM body: markers stripped, links reduced
/// to their text, whitespace collapsed, cut to [`COMMENT_EXCERPT_CHARS`].
pub(crate) fn plain_excerpt(body: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(body.len());
    let mut in_link_text = false;
    let mut skipping_url = false;
    for line in body.lines() {
        let line = line.trim_start_matches(|c: char| c == '#' || c == '>' || c == ' ');
        let line = line
            .trim_start_matches(|c: char| c == '-' || c == '*' || c == '+')
            .trim_start();
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            if skipping_url {
                if ch == ')' {
                    skipping_url = false;
                }
                continue;
            }
            match ch {
                '`' | '*' | '_' | '~' => {}
                // An image marker, not an exclamation.
                '!' if chars.peek() == Some(&'[') => {}
                '[' => in_link_text = true,
                ']' => in_link_text = false,
                '(' if !in_link_text && out.ends_with(|c: char| !c.is_whitespace()) => {
                    // `[text](url)` — the `]` just closed, the url follows.
                    skipping_url = true;
                }
                _ => out.push(ch),
            }
        }
        out.push(' ');
    }
    let collapsed: Vec<&str> = out.split_whitespace().collect();
    let text = collapsed.join(" ");
    if text.chars().count() <= max_chars {
        return text;
    }
    let kept: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

fn comment_card(comment_id: &str, cx: &mut App) -> Option<AnyElement> {
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let comment = collections.comments.read(cx).get(comment_id).cloned()?;
    let author = comment
        .author_id
        .as_deref()
        .and_then(|author_id| collections.users.read(cx).get(author_id).cloned());
    let author_label = crate::comments::author_label(author.as_ref());
    let author_id = comment
        .author_id
        .clone()
        .unwrap_or_else(|| "anonymous".to_string());
    let image = crate::user_avatar::cached_avatar_image(
        cx,
        author.as_ref().and_then(|user| user.image.as_deref()),
    );
    let when = comment
        .created_at
        .as_deref()
        .map(|at| crate::comments::relative_time(at, now_epoch()))
        .unwrap_or_default();
    let muted = cx.theme().muted_foreground;
    let mut byline = h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_1p5()
        .child(crate::user_avatar::avatar_element(
            &author_id,
            &author_label,
            image,
            gpui_component::Size::XSmall,
        ))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .child(SharedString::from(author_label)),
        );
    if !when.is_empty() {
        byline = byline.child(div().text_xs().text_color(muted).child(SharedString::from(when)));
    }
    let excerpt = plain_excerpt(comment.body.as_deref().unwrap_or_default(), COMMENT_EXCERPT_CHARS);
    let mut card = card_frame(cx).child(byline);
    if !excerpt.is_empty() {
        card = card.child(div().w_full().text_sm().line_clamp(4).child(SharedString::from(excerpt)));
    }
    if let Some(issue) = collections.issues.read(cx).get(&comment.issue_id).cloned() {
        card = card.child(issue_line(&issue, cx));
    }
    Some(card.into_any_element())
}

fn session_card(session_id: &str, cx: &mut App) -> Option<AnyElement> {
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let session = collections.coding_sessions.read(cx).get(session_id).cloned()?;
    let now = now_epoch();
    let muted = cx.theme().muted_foreground;
    let facts = crate::run_rows::RunListFacts::derive(&session, now, cx);
    let (identifier, title, dot, working, device) = match &facts {
        crate::run_rows::RunListFacts::Running(facts) => (
            facts.identifier.clone(),
            facts.title.clone(),
            facts.dot,
            facts.working,
            facts.device_label.clone(),
        ),
        crate::run_rows::RunListFacts::Past(facts) => (
            facts.identifier.clone(),
            facts.title.clone(),
            muted.opacity(0.4),
            false,
            crate::queries::session_device_presentation(
                &session,
                collections.devices.read(cx).iter(),
                now * 1_000,
            )
            .label,
        ),
    };
    let mark = coding::CodingAgent::parse(session.agent.as_deref().unwrap_or_default())
        .map(crate::coding_selects::agent_mark)
        .unwrap_or_else(|| Icon::new(registry::CODING_RUNNING).text_color(muted));
    let mut head = h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_2()
        .child(crate::surface::live_dot(dot, working))
        .child(div().flex_shrink_0().child(mark.small()));
    if let Some(identifier) = identifier {
        head = head.child(
            div()
                .flex_shrink_0()
                .text_xs()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(muted)
                .child(identifier),
        );
    }
    head = head.child(
        div()
            .min_w_0()
            .flex_1()
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .line_clamp(2)
            .child(title),
    );
    let started = crate::run_rows::run_started_at(&session)
        .map(|at| crate::comments::relative_time(at, now))
        .filter(|when| !when.is_empty())
        .map(|when| format!("started {when}"));
    let mut byline: Vec<String> = Vec::new();
    if let Some(device) = device.map(|label| label.trim().to_string()).filter(|l| !l.is_empty()) {
        byline.push(device);
    }
    if let Some(started) = started {
        byline.push(started);
    }
    let mut card = card_frame(cx).child(head);
    if !byline.is_empty() {
        card = card.child(muted_line(byline.join(" · "), cx));
    }
    Some(card.into_any_element())
}

fn label_card(label_id: &str, cx: &mut App) -> Option<AnyElement> {
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let label = collections.labels.read(cx).get(label_id).cloned()?;
    let tint = label
        .color
        .as_deref()
        .and_then(crate::issue_header::parse_hex_color)
        .unwrap_or(cx.theme().muted_foreground);
    Some(
        card_frame(cx)
            .child(header(
                div().size_2p5().rounded_full().bg(tint),
                label.name.clone(),
            ))
            .child(muted_line("Label", cx))
            .into_any_element(),
    )
}

fn status_card(status_id: &str, cx: &mut App) -> Option<AnyElement> {
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let row = collections.issue_statuses.read(cx).get(status_id).cloned()?;
    let sorted = crate::queries::team_statuses(cx, &row.team_id);
    let index = sorted.iter().position(|candidate| candidate.id == row.id)?;
    let resolved = domain::statuses::resolve_row(&sorted, index);
    let category = capitalized(&row.category);
    Some(
        card_frame(cx)
            .child(header(
                crate::icons::resolved_status_icon(&resolved, cx).small(),
                row.name.clone(),
            ))
            .child(muted_line(category, cx))
            .into_any_element(),
    )
}

fn workflow_card(workflow_id: &str, cx: &mut App) -> Option<AnyElement> {
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let workflow = collections.workflows.read(cx).get(workflow_id).cloned()?;
    let nodes = collections
        .workflow_nodes
        .read(cx)
        .iter()
        .filter(|node| node.workflow_id.as_deref() == Some(workflow.id.as_str()))
        .count();
    let name = workflow
        .name
        .clone()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Workflow".to_string());
    let status = workflow
        .status
        .as_deref()
        .map(capitalized)
        .unwrap_or_default();
    let mut facts: Vec<String> = Vec::new();
    if !status.is_empty() {
        facts.push(status);
    }
    facts.push(match nodes {
        1 => "1 node".to_string(),
        n => format!("{n} nodes"),
    });
    Some(
        card_frame(cx)
            .child(header(
                Icon::new(registry::NAV_WORKFLOWS)
                    .small()
                    .text_color(cx.theme().muted_foreground),
                name,
            ))
            .child(muted_line(facts.join(" · "), cx))
            .into_any_element(),
    )
}

fn device_card(device_id: &str, cx: &mut App) -> Option<AnyElement> {
    let device = find_device(device_id, cx)?;
    let now_ms = now_epoch() * 1_000;
    let online = crate::device_settings::row_is_online(device.last_seen_at.as_deref(), now_ms);
    let label = device
        .label
        .clone()
        .map(|label| label.trim().to_string())
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| "Device".to_string());
    let glyph = crate::icons::device_icon(device.icon.as_deref(), device.is_server());
    let tone = if online {
        theme::tokens::GREEN.to_hsla()
    } else {
        cx.theme().muted_foreground.opacity(0.4)
    };
    Some(
        card_frame(cx)
            .child(header(
                Icon::new(glyph).small().text_color(cx.theme().muted_foreground),
                label,
            ))
            .child(
                h_flex()
                    .items_center()
                    .gap_1p5()
                    .child(crate::surface::live_dot(tone, false))
                    .child(muted_line(if online { "Online" } else { "Offline" }, cx)),
            )
            .into_any_element(),
    )
}

fn member_card(user_id: &str, cx: &mut App) -> Option<AnyElement> {
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let user = collections.users.read(cx).get(user_id).cloned()?;
    let label = crate::comments::user_label(&user.id, Some(&user));
    let image = crate::user_avatar::cached_avatar_image(cx, user.image.as_deref());
    let role = collections
        .team_members
        .read(cx)
        .iter()
        .find(|member| member.user_id == user.id)
        .and_then(|member| member.role.clone())
        .map(|role| capitalized(&role));
    let mut facts: Vec<String> = Vec::new();
    if let Some(email) = user.email.clone().filter(|email| email != &label) {
        facts.push(email);
    }
    if let Some(role) = role {
        facts.push(role);
    }
    let mut card = card_frame(cx).child(header(
        crate::user_avatar::avatar_element(&user.id, &label, image, gpui_component::Size::Small),
        label.clone(),
    ));
    if !facts.is_empty() {
        card = card.child(muted_line(facts.join(" · "), cx));
    }
    Some(card.into_any_element())
}

fn team_card(team_id: &str, cx: &mut App) -> Option<AnyElement> {
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let team = collections.teams.read(cx).get(team_id).cloned()?;
    let members = collections
        .team_members
        .read(cx)
        .iter()
        .filter(|member| member.team_id == team.id)
        .count();
    Some(
        card_frame(cx)
            .child(header(crate::user_avatar::team_avatar(&team.name, 20.), team.name.clone()))
            .child(muted_line(
                match members {
                    1 => "1 member".to_string(),
                    n => format!("{n} members"),
                },
                cx,
            ))
            .into_any_element(),
    )
}

fn invite_card(invite_id: &str, cx: &mut App) -> Option<AnyElement> {
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let invite = collections.team_invites.read(cx).get(invite_id).cloned()?;
    let email = invite
        .email
        .clone()
        .map(|email| email.trim().to_string())
        .filter(|email| !email.is_empty())
        .unwrap_or_else(|| "Invite link".to_string());
    let mut facts: Vec<String> = Vec::new();
    if let Some(role) = invite.role.as_deref() {
        facts.push(capitalized(role));
    }
    facts.push(if invite.accepted_at.is_some() { "Accepted" } else { "Pending" }.to_string());
    Some(
        card_frame(cx)
            .child(header(
                Icon::new(registry::UI_INVITE)
                    .small()
                    .text_color(cx.theme().muted_foreground),
                email,
            ))
            .child(muted_line(facts.join(" · "), cx))
            .into_any_element(),
    )
}

fn notification_card(notification_id: &str, cx: &mut App) -> Option<AnyElement> {
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let notification = collections.notifications.read(cx).get(notification_id).cloned()?;
    let title = notification
        .title
        .clone()
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Notification".to_string());
    let mut card = card_frame(cx).child(header(
        Icon::new(registry::NAV_NOTIFICATIONS)
            .small()
            .text_color(cx.theme().muted_foreground),
        title,
    ));
    if let Some(body) = notification
        .body
        .as_deref()
        .map(|body| plain_excerpt(body, COMMENT_EXCERPT_CHARS))
        .filter(|body| !body.is_empty())
    {
        card = card.child(muted_line(body, cx));
    }
    Some(card.into_any_element())
}

fn attachment_card(attachment_id: &str, cx: &mut App) -> Option<AnyElement> {
    let collections = sync::Store::try_global(cx)?.collections().clone();
    let attachment = collections.attachments.read(cx).get(attachment_id).cloned()?;
    let filename = attachment
        .filename
        .clone()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Attachment".to_string());
    let mut facts: Vec<String> = Vec::new();
    if let Some(size) = attachment.size_bytes {
        facts.push(crate::issue_files::format_bytes(size));
    }
    if let Some(content_type) = attachment.content_type.clone() {
        facts.push(content_type);
    }
    let mut card = card_frame(cx).child(header(
        Icon::new(registry::UI_ATTACH)
            .small()
            .text_color(cx.theme().muted_foreground),
        filename,
    ));
    if !facts.is_empty() {
        card = card.child(muted_line(facts.join(" · "), cx));
    }
    Some(card.into_any_element())
}

/// One member row of a list card: a synced issue draws its status glyph +
/// identifier + title; anything else its kind glyph + label. Clickable when
/// the member resolves to a target (the click closes the card).
fn list_member_row(index: usize, member: &EntityRef, cx: &mut App) -> AnyElement {
    let target = entity_target(&member.kind, &member.id, cx);
    let body = match (member.kind.as_str(), find_issue(&member.id, cx)) {
        ("issue", Some(issue)) => issue_line(&issue, cx),
        _ => h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_1p5()
            .text_xs()
            .child(div().flex_shrink_0().child(kind_glyph(member, cx).xsmall()))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .child(SharedString::from(rule::entity_chip_label(&view(member)))),
            ),
    };
    let row = div()
        .id(ElementId::from(SharedString::from(format!("entity-list-row-{index}"))))
        .w_full()
        .min_w_0()
        .px_1()
        .py_0p5()
        .rounded(cx.theme().radius)
        .child(body);
    match target {
        Some(target) => row
            .cursor_pointer()
            .hover(|style| style.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
            .on_click(move |_, window, cx| {
                let host = crate::issue_preview::host_for_window(window, cx);
                host.update(cx, |host, cx| host.dismiss(cx));
                open_target(target.clone(), window, cx);
            })
            .into_any_element(),
        None => row.into_any_element(),
    }
}

/// A `list` chip's card: the chip's label as the header (`3 issues`), the
/// member refs as rows, `+N more` when the answer carried more than it named.
fn list_card(r#ref: &EntityRef, members: &[EntityRef], cx: &mut App) -> AnyElement {
    let label = rule::entity_chip_label(&view(r#ref));
    let mut card = card_frame(cx).child(header(kind_glyph(r#ref, cx), label));
    if !members.is_empty() {
        let mut rows = v_flex().w_full().gap_0p5();
        for (index, member) in members.iter().enumerate() {
            rows = rows.child(list_member_row(index, member, cx));
        }
        card = card.child(rows);
    }
    let count = r#ref.count.unwrap_or(0) as usize;
    if count > members.len() {
        card = card.child(muted_line(format!("+{} more", count - members.len()), cx));
    }
    card.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every contract kind's icon concept resolves to a registry glyph here
    /// (the domain crate locks the concept table; this is the desktop half).
    #[test]
    fn every_entity_kind_concept_has_a_registry_glyph() {
        for kind in domain::contract::ENTITY_REF_KIND_VALUES {
            let concept = domain::entity_preview::entity_kind_icon(kind)
                .unwrap_or_else(|| panic!("{kind} has no concept"));
            assert!(concept_icon(concept).is_some(), "{kind}: {concept} has no registry glyph");
        }
        assert!(concept_icon("not-a-concept").is_none());
    }

    fn synced() -> EntityRowFacts {
        EntityRowFacts { synced: true, issue_id: None }
    }

    #[test]
    fn synced_rows_open_their_detail() {
        assert_eq!(
            target_for("issue", "i-1", &synced()),
            Some(EntityTarget::Screen(Screen::IssueDetail { issue_id: "i-1".into() }))
        );
        assert_eq!(
            target_for("board", "b-1", &synced()),
            Some(EntityTarget::Screen(Screen::BoardIssues { board_id: "b-1".into() }))
        );
        assert_eq!(target_for("session", "s-1", &synced()), Some(EntityTarget::Session("s-1".into())));
        assert_eq!(target_for("action", "a-1", &synced()), Some(EntityTarget::Action("a-1".into())));
        assert_eq!(
            target_for("automation", "au-1", &synced()),
            Some(EntityTarget::Automation("au-1".into()))
        );
        assert_eq!(
            target_for("workflow", "w-1", &synced()),
            Some(EntityTarget::Screen(Screen::Workflow { workflow_id: "w-1".into() }))
        );
        assert_eq!(target_for("device", "d-1", &synced()), Some(EntityTarget::Screen(Screen::Devices)));
        for kind in ["label", "status", "member", "invite", "team"] {
            assert_eq!(
                target_for(kind, "x", &synced()),
                Some(EntityTarget::Screen(Screen::Settings)),
                "{kind}"
            );
        }
        assert_eq!(
            target_for("notification", "n-1", &synced()),
            Some(EntityTarget::Screen(Screen::Inbox { tab: crate::sidebar::InboxTab::Inbox }))
        );
    }

    #[test]
    fn comments_and_attachments_open_their_issue() {
        let facts = EntityRowFacts { synced: true, issue_id: Some("i-9".into()) };
        for kind in ["comment", "attachment"] {
            assert_eq!(
                target_for(kind, "x", &facts),
                Some(EntityTarget::Screen(Screen::IssueDetail { issue_id: "i-9".into() })),
                "{kind}"
            );
        }
        // A draft attachment has no issue: nowhere to go.
        assert_eq!(target_for("attachment", "x", &synced()), None);
    }

    #[test]
    fn unsynced_lists_and_unknown_kinds_are_inert() {
        let none = EntityRowFacts::default();
        for kind in ["issue", "board", "session", "action", "automation", "workflow", "device",
            "label", "status", "member", "invite", "team", "notification", "comment", "attachment"]
        {
            assert_eq!(target_for(kind, "x", &none), None, "{kind}");
        }
        assert_eq!(target_for("list", "issue", &synced()), None);
        assert_eq!(target_for("thing", "x", &synced()), None);
    }

    #[test]
    fn never_synced_kinds_still_open_their_page() {
        let none = EntityRowFacts::default();
        assert_eq!(
            target_for("repository", "r-1", &none),
            Some(EntityTarget::Screen(Screen::Settings))
        );
        assert_eq!(
            target_for("thread", "th-1", &none),
            Some(EntityTarget::Screen(Screen::SupportThread { thread_id: "th-1".into() }))
        );
    }

    #[test]
    fn plain_excerpt_strips_markdown_and_cuts() {
        assert_eq!(
            plain_excerpt("## Heading\n\n- **bold** and _em_ with `code`\n> quoted [link](https://x.y) end!", 240),
            "Heading bold and em with code quoted link end!"
        );
        assert_eq!(plain_excerpt("see ![shot](/api/attachments/1) here (ok)", 240), "see shot here (ok)");
        let long = "word ".repeat(100);
        let cut = plain_excerpt(&long, 40);
        assert_eq!(cut.chars().count(), 40);
        assert!(cut.ends_with('…'));
        assert!(!cut.ends_with(" …"));
        assert_eq!(plain_excerpt("", 10), "");
    }
}
