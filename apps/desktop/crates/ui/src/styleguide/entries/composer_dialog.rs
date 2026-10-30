//! EXP-1019/EXP-1249/EXP-1334 — the `composer-dialog` entry (3 Special
//! components): THE launcher as a play button opens it, and THE composer of
//! a thread and a session view: one card everywhere, case for case with the
//! web entry. Drawn from the REAL parts the app composes
//! ([`crate::composer::session_composer_box`]: the card with the "+" and
//! its "+", Send ⇄ Stop; a session keeps only its context ring: S-3 removed
//! the options line under every composer) and the "+" menu drawn OPEN beside
//! it (a real `PopupMenu` built row by row off
//! `chat_screen::composer_menu_rows`: a start = the launch surface on
//! Temporary session, a session = its surface, the model only). The
//! specimen ([`ComposerSpecimen`], [`plus_menu_open`]) is shared with the
//! `session-thread` entry, like the web `composer-specimen.tsx`. The agent
//! case keeps the home placeholder (the agent row above the card picks the
//! recipient, styleguide `composer-agent-row`).

use gpui::{
    div, px, AnyElement, App, Context, Div, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled as _, Window,
};
use gpui_component::menu::PopupMenu;
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use crate::chat_screen::{
    composer_menu_rows, ComposerMenuConditions, ComposerMenuRow, ComposerSurface, MENU_ADD_FILE,
    MENU_COMPUTER_USE, MENU_IMPLEMENT_ISSUE, MENU_MCP_SERVERS, MENU_ULTRACODE, PLUS_TOOLTIP,
};
use crate::composer::{
    composer_tool, session_composer_card, session_composer_field, session_context_ring,
    ContextRingProps, SessionComposerBox, SessionComposerControls, SessionComposerSubmit, SubmitKind,
};
use crate::icons::registry;
use crate::mention_input::MentionInput;
use crate::surface::{glass_pill, PillMode, PillSize};

pub(crate) const ID: &str = "composer-dialog";
pub(crate) const OWNER: &str = "EXP-1019";

/// The composer's phase (web `ComposerPhase`): before a start, a running
/// session, an idle thread (= its dispatcher session).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Start,
    Running,
    Idle,
}

/// The sample data the web specimen shows.
pub(super) const SAMPLE_DEVICE: &str = "mint · This device";
pub(super) const SAMPLE_ACCOUNT: &str = "danny@example.dev";
pub(super) const SAMPLE_REPOSITORY: &str = "Niach/exponential";

/// The model on the line, THE model list's alias: `opus`, `haiku` while idle.
pub(super) fn sample_model(phase: Phase) -> &'static str {
    if phase == Phase::Idle {
        "haiku"
    } else {
        "opus"
    }
}

/// The thread composer's placeholder, the fixture's (`Type / for commands`).
pub(super) fn thread_placeholder() -> String {
    domain::session_tree::THREAD.copy.placeholder.clone()
}

/// The run's context window as the ring reads it: 42% running, 12% idle.
fn sample_usage(phase: Phase) -> steer::SessionUsage {
    steer::SessionUsage {
        context_used: if phase == Phase::Running { 84_000 } else { 24_000 },
        context_size: 200_000,
        cost_usd: None,
    }
}

/// The context ring at the line's end (its usage sheet on a click).
pub(super) fn context_ring(prefix: &str, phase: Phase, cx: &App) -> Option<AnyElement> {
    session_context_ring(
        ContextRingProps {
            id: SharedString::from(format!("{prefix}-context")),
            session_id: String::new(),
            device_id: None,
            agent: None,
            local: false,
            working: phase == Phase::Running,
            usage: Some(sample_usage(phase)),
            layout: Vec::new(),
            legend_open: false,
            on_toggle_legend: std::rc::Rc::new(|_, _| {}),
        },
        cx,
    )
}

/// THE whole composer (web `ComposerSpecimen`): the card (the field, the
/// "+" + the inline tools, Send ⇄ Stop), the options line under it. A view:
/// the field and the model pin's pick live across frames.
pub(super) struct ComposerSpecimen {
    prefix: &'static str,
    phase: Phase,
    mention: Entity<MentionInput>,
    /// EXP-1211: the quick replies above the card (none = the plain box).
    replies: Option<domain::session_tree::ComposerReplies>,
    /// The agent case (no context ring before a conversation exists).
    to_agent: bool,
}

impl ComposerSpecimen {
    fn new(
        prefix: &'static str,
        phase: Phase,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (_, mention) = session_composer_field(SharedString::from(placeholder.to_string()), window, cx);
        Self {
            prefix,
            phase,
            mention,
            replies: None,
            to_agent: false,
        }
    }
}

impl Render for ComposerSpecimen {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let prefix = self.prefix;
        let phase = self.phase;
        let id = |part: &str| SharedString::from(format!("{prefix}-{part}"));
        let submit = SessionComposerSubmit::send(id("submit"), false, |_, _, _| {}).kind(if phase == Phase::Running {
            SubmitKind::Stop
        } else {
            SubmitKind::Send
        });
        // S-3: NO options line under any composer (Device, Account, Model
        // and Repository are "+" rows); a session keeps its context ring.
        let controls = if self.to_agent || phase == Phase::Start {
            SessionComposerControls::default()
        } else {
            SessionComposerControls { device_pin: None, model_pin: None, context_ring: context_ring(prefix, phase, cx) }
        };
        let replies = self
            .replies
            .as_ref()
            .map(|replies| crate::composer::composer_replies_row(replies, false, cx));
        let card = session_composer_card(
            SessionComposerBox::new(self.mention.clone(), submit)
                .replies(replies)
                // N-12: the "+" (the options menu) is the only tool.
                .tool(composer_tool(id("plus"), registry::UI_ADD, cx).tooltip(PLUS_TOOLTIP))
                .controls(controls),
            cx,
        );
        v_flex().w_full().min_w_0().py_2().child(card)
    }
}

/// The composer at `phase` with `placeholder`, kept across frames under
/// `key` (its element ids' prefix too).
pub(super) fn composer_specimen(
    key: &'static str,
    phase: Phase,
    placeholder: &str,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    composer_specimen_with_replies(key, phase, placeholder, None, window, cx)
}

/// [`composer_specimen`] with EXP-1211's quick replies above the card.
pub(super) fn composer_specimen_with_replies(
    key: &'static str,
    phase: Phase,
    placeholder: &str,
    replies: Option<domain::session_tree::ComposerReplies>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let placeholder = placeholder.to_string();
    window
        .use_keyed_state(key, cx, move |window, cx| ComposerSpecimen {
            replies,
            ..ComposerSpecimen::new(key, phase, &placeholder, window, cx)
        })
        .into_any_element()
}

/// NEW-3 — the composer messaging an agent (no options line), kept under
/// `key`.
fn agent_composer_specimen(key: &'static str, window: &mut Window, cx: &mut App) -> AnyElement {
    window
        .use_keyed_state(key, cx, move |window, cx| ComposerSpecimen {
            to_agent: true,
            ..ComposerSpecimen::new(
                key,
                Phase::Start,
                crate::home_greeting::HOME_PLACEHOLDER,
                window,
                cx,
            )
        })
        .into_any_element()
}

/// A `PopupMenu` drawn OPEN in place (the `issue-context-menu` pattern): an
/// entry keeps it under a key, so it is built once.
pub(super) struct OpenMenu {
    menu: Entity<PopupMenu>,
}

impl OpenMenu {
    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        build: impl FnOnce(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu,
    ) -> Self {
        Self {
            menu: PopupMenu::build(window, cx, build),
        }
    }
}

impl Render for OpenMenu {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        crate::controls::with_pointer_layer(self.menu.clone())
    }
}

/// The "+" menu's rows at `phase` (web `composerMenuEntries`): the
/// fixture's rows with every condition met, minus Subagents
/// (`sessions.send` does not carry it); in a session the subject rows
/// (Implement issue ›, Message an agent ›) go: they exist only before a
/// start.
pub(super) fn plus_menu_rows(phase: Phase) -> Vec<ComposerMenuRow> {
    // S-3: a start = the launch surface on Temporary session (every runner
    // and work row); a session = the session surface (the model only).
    let (surface, temporary) = match phase {
        Phase::Start => (ComposerSurface::Launch, true),
        _ => (ComposerSurface::Session, false),
    };
    composer_menu_rows(
        surface,
        ComposerMenuConditions {
            temporary,
            device: temporary,
            account: temporary,
            model: true,
            repositories: true,
            subagent_model: true,
            ultracode: true,
            mcp: true,
            computer_use: true,
            prompts: true,
        },
    )
    .into_iter()
    .filter(|row| *row != ComposerMenuRow::Subagents)
    .collect()
}

/// The row's concept icon in the crate's icon slot (the real menu's).
fn menu_icon(row: ComposerMenuRow) -> Icon {
    Icon::new(row.icon().unwrap_or(registry::UI_ADD))
}

/// The "+" at `phase`, the rows' sample values the web's: Effort High, 2 MCP
/// servers, Ultracode off, Computer use on.
fn plus_menu(
    mut menu: PopupMenu,
    phase: Phase,
    prefix: &'static str,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    use crate::controls::{pointer_menu_item, pointer_toggle_item};
    menu = menu.min_w(px(260.));
    for row in plus_menu_rows(phase) {
        menu = match row {
            ComposerMenuRow::Separator => menu.separator(),
            ComposerMenuRow::ImplementIssue => {
                menu.submenu_with_icon(Some(menu_icon(row)), MENU_IMPLEMENT_ISSUE, window, cx, |sub, _, _| sub)
            }
            ComposerMenuRow::AddFile => {
                menu.item(pointer_menu_item(None, MENU_ADD_FILE, |_| None, |_, _| {}).icon(menu_icon(row)))
            }
            // #EXP-1434: Prompts › (the team's prompts' search list).
            ComposerMenuRow::Prompts => {
                menu.submenu_with_icon(Some(menu_icon(row)), row.label(), window, cx, |sub, _, _| sub)
            }
            ComposerMenuRow::Effort => {
                menu.submenu_with_icon(Some(menu_icon(row)), "Effort · High", window, cx, |sub, _, _| sub)
            }
            ComposerMenuRow::Device => menu.submenu_with_icon(
                Some(menu_icon(row)),
                format!("{} · {SAMPLE_DEVICE}", row.label()),
                window,
                cx,
                |sub, _, _| sub,
            ),
            ComposerMenuRow::Account => menu.submenu_with_icon(
                Some(menu_icon(row)),
                format!("{} · {SAMPLE_ACCOUNT}", row.label()),
                window,
                cx,
                |sub, _, _| sub,
            ),
            ComposerMenuRow::Model => menu.submenu_with_icon(
                Some(menu_icon(row)),
                format!("{} · {}", row.label(), crate::steer_viewer::model_label(sample_model(phase))),
                window,
                cx,
                |sub, _, _| sub,
            ),
            ComposerMenuRow::Repository => menu.submenu_with_icon(
                Some(menu_icon(row)),
                format!("{} · {SAMPLE_REPOSITORY}", row.label()),
                window,
                cx,
                |sub, _, _| sub,
            ),
            ComposerMenuRow::Subagents => menu,
            ComposerMenuRow::Ultracode => menu.item(
                pointer_toggle_item(format!("{prefix}-ultracode"), None, MENU_ULTRACODE, false, |_, _, _| {})
                    .icon(menu_icon(row)),
            ),
            ComposerMenuRow::McpServers => menu.submenu_with_icon(
                Some(menu_icon(row)),
                format!("{MENU_MCP_SERVERS} · 2"),
                window,
                cx,
                |sub, _, _| sub,
            ),
            ComposerMenuRow::ComputerUse => menu.item(
                pointer_toggle_item(format!("{prefix}-computer-use"), None, MENU_COMPUTER_USE, true, |_, _, _| {})
                    .icon(menu_icon(row)),
            ),
        };
    }
    menu
}

/// The "+" drawn open at `phase`, kept under `key`.
pub(super) fn plus_menu_open(key: &'static str, phase: Phase, window: &mut Window, cx: &mut App) -> AnyElement {
    window
        .use_keyed_state(key, cx, move |window, cx| {
            OpenMenu::new(window, cx, move |menu, window, cx| plus_menu(menu, phase, key, window, cx))
        })
        .into_any_element()
}

/// A composer column's width (web `w-[36rem]`).
const COLUMN_W: f32 = 576.;

fn caption(text: &'static str, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().muted_foreground).child(text)
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    // NEW-2: a prompt picked off the / menu: its chip IS the headline (no
    // verb), the launcher's main element.
    let headline = h_flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .px_1()
        .child(
            glass_pill("sg-composer-dialog-action", PillSize::Sm, PillMode::Readonly, cx)
                .child(Icon::new(registry::NAV_ACTIONS).xsmall().text_color(muted))
                .child(div().text_xs().child("Triage new issues")),
        );
    let placeholder = thread_placeholder();
    let action_start = composer_specimen(
        "sg-composer-dialog-action-start",
        Phase::Start,
        domain::contract::COMPOSER_UI_INSTRUCTIONS_PLACEHOLDER,
        window,
        cx,
    );
    let running = composer_specimen("sg-composer-dialog-running", Phase::Running, &placeholder, window, cx);
    let idle = composer_specimen("sg-composer-dialog-idle", Phase::Idle, &placeholder, window, cx);
    let chat_start = composer_specimen(
        "sg-composer-dialog-chat-start",
        Phase::Start,
        domain::contract::COMPOSER_UI_CHAT_PLACEHOLDER,
        window,
        cx,
    );
    let agent_start = agent_composer_specimen("sg-composer-dialog-agent-start", window, cx);
    let start_menu = plus_menu_open("sg-composer-dialog-plus-start", Phase::Start, window, cx);
    let session_menu = plus_menu_open("sg-composer-dialog-plus-session", Phase::Running, window, cx);
    v_flex()
        .gap_10()
        .child(
            h_flex()
                .flex_wrap()
                .items_start()
                .gap_8()
                .child(
                    v_flex()
                        .w(px(COLUMN_W))
                        .max_w_full()
                        .gap_2()
                        .child(caption(
                            "A prompt off the / menu: its chip IS the headline, and the send is the only thing left to do.",
                            cx,
                        ))
                        .child(headline)
                        .child(action_start),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .child(start_menu)
                        .child(caption("the \"+\" before a start: composer-menu.json, every condition met", cx)),
                ),
        )
        .child(
            h_flex()
                .flex_wrap()
                .items_start()
                .gap_8()
                .child(
                    v_flex()
                        .w(px(COLUMN_W))
                        .max_w_full()
                        .gap_2()
                        .child(caption("A running session: Stop, model … the context ring under the card.", cx))
                        .child(running)
                        .child(caption("An idle thread: the same composer (it messages the dispatcher).", cx))
                        .child(idle),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .child(session_menu)
                        .child(caption(
                            "the \"+\" in a session: no subject rows, the options change the live session",
                            cx,
                        )),
                ),
        )
        .child(
            v_flex()
                .w(px(COLUMN_W))
                .max_w_full()
                .gap_2()
                .child(caption("A chat in the dialog: no subject, the contract's placeholder.", cx))
                .child(chat_start),
        )
        .child(
            v_flex()
                .w(px(COLUMN_W))
                .max_w_full()
                .gap_2()
                .child(caption("Message an agent: one agent chip, no options line, the submit sends.", cx))
                .child(agent_start),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The "+" rows: before a start the fixture's rows with every condition
    /// met minus Subagents; in a session the subject rows go and the
    /// separators stay tidy.
    #[test]
    fn the_plus_rows_follow_the_phase() {
        use ComposerMenuRow::*;
        assert_eq!(
            plus_menu_rows(Phase::Start),
            vec![
                AddFile,
                ImplementIssue,
                Prompts,
                Separator,
                Device,
                Account,
                Model,
                Repository,
                Separator,
                Effort,
                Ultracode,
                Separator,
                McpServers,
                ComputerUse
            ]
        );
        assert_eq!(plus_menu_rows(Phase::Running), vec![AddFile, ImplementIssue, Prompts, Separator, Model]);
        assert_eq!(plus_menu_rows(Phase::Idle), plus_menu_rows(Phase::Running));
    }

    /// The sample data is the web specimen's: opus (haiku idle), 42% / 12%.
    #[test]
    fn the_sample_data_is_the_webs() {
        assert_eq!(sample_model(Phase::Start), "opus");
        assert_eq!(sample_model(Phase::Running), "opus");
        assert_eq!(sample_model(Phase::Idle), "haiku");
        assert_eq!(crate::usage_bar::context_percent(Some(&sample_usage(Phase::Running))), Some(42));
        assert_eq!(crate::usage_bar::context_percent(Some(&sample_usage(Phase::Idle))), Some(12));
        assert_eq!(SAMPLE_DEVICE, "mint · This device");
        assert_eq!(thread_placeholder(), "Type / for commands");
    }
}
