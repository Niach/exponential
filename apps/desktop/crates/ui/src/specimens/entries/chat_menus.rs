//! Session 58cb32df (#EXP-1373): `chat-menus` (Special components): THE chat
//! menu and THE agent menu ×4 (`fixtures/chats.json` `menuCases` +
//! `agentMenuCases`), the twin of web `chat-menus.tsx`. The demo IS the
//! builders every way in calls ([`crate::chat_actions::chat_entry_items`]
//! behind a chat row's right-click, #FEED-239's
//! [`crate::chat_actions::screen_menu_items`] behind the rail's agent row
//! and every session screen's `…`) over the fixture's entries, drawn open
//! in place, plus THE delete confirm ([`crate::chat_actions::delete_alert`]).

use gpui::{div, px, App, Context, Div, Entity, IntoElement, ParentElement as _, Render, Styled as _, Window};
use gpui_component::menu::PopupMenu;
use gpui_component::ActiveTheme as _;

use domain::chats::{chat_menu_entries, ChatMenuEntry};
use domain::session_screen::{session_screen_menu, SessionScreenMenuEntry, SessionScreenMenuInput};

pub(crate) const ID: &str = "chat-menus";
pub(crate) const OWNER: &str = "EXP-1373";

/// The specimens: (caption, the menu).
enum Specimen {
    Chat(Vec<ChatMenuEntry>),
    Agent(Vec<SessionScreenMenuEntry>),
}

fn specimens() -> Vec<(&'static str, Specimen)> {
    // #EXP-1553: web's `ChatMenusSpecimen` set and captions, one for one
    // (S-3: Delete only, live or ended; no Stop anywhere).
    vec![
        ("A live chat", Specimen::Chat(chat_menu_entries("running", true, true))),
        ("An ended chat", Specimen::Chat(chat_menu_entries("ended", true, true))),
        ("The agent menu (busy, owner)", Specimen::Agent(agent_menu(true))),
        ("The agent menu (idle, member)", Specimen::Agent(agent_menu(false))),
    ]
}

/// #FEED-239: THE session screen menu of an agent (kind agent, a member).
fn agent_menu(owner: bool) -> Vec<SessionScreenMenuEntry> {
    session_screen_menu(SessionScreenMenuInput { kind: "agent", member: true, link_issue: true, settings: owner, ..Default::default() })
}

struct MenusDemo {
    menus: Vec<(&'static str, Entity<PopupMenu>)>,
}

impl MenusDemo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let menus = specimens()
            .into_iter()
            .map(|(caption, specimen)| {
                let menu = PopupMenu::build(window, cx, move |menu, _window, cx| {
                    let menu = menu.min_w(px(220.));
                    match &specimen {
                        Specimen::Chat(entries) => crate::chat_actions::chat_entry_items(menu, "sg-chat", entries, cx),
                        Specimen::Agent(entries) => crate::chat_actions::screen_menu_items(
                            menu,
                            "agent",
                            entries,
                            std::rc::Rc::new(|_, _, _| {}),
                        ),
                    }
                });
                (caption, menu)
            })
            .collect();
        Self { menus }
    }
}

impl Render for MenusDemo {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "MenusDemo").entered();
        let muted = cx.theme().muted_foreground;
        div().flex().flex_wrap().items_start().gap_8().children(self.menus.iter().map(|(caption, menu)| {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().text_xs().text_color(muted).child(*caption))
                .child(crate::controls::with_pointer_layer(menu.clone()))
        }))
    }
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let demo = window.use_keyed_state("sg-chat-menus", cx, MenusDemo::new);
    // #EXP-1553: web's `DeleteConfirm`: its caption over the prompt card
    // (the question inside it; no window-title band).
    let confirm = crate::native_dialog::alert_card_specimen(
        "sg-chat-delete-confirm",
        crate::chat_actions::delete_alert(|_, _| true),
        window,
        cx,
    );
    let muted = cx.theme().muted_foreground;
    div().flex().flex_col().gap_8().child(demo).child(
        div().flex().flex_col().gap_2().child(div().text_xs().text_color(muted).child("The delete confirm")).child(confirm),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specimens are the fixture's cases.
    #[test]
    fn the_specimens_are_the_fixture_entries() {
        let specimens = specimens();
        assert!(matches!(&specimens[0].1, Specimen::Chat(entries) if entries == &[ChatMenuEntry::Delete]));
        assert!(matches!(&specimens[1].1, Specimen::Chat(entries) if entries == &[ChatMenuEntry::Delete]));
        // #EXP-1834: Link or file an issue, a rule, the pair, a rule, Agent settings.
        assert!(matches!(&specimens[2].1, Specimen::Agent(entries) if entries.len() == 6));
    }
}
