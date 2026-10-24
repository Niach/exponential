//! Thread 58cb32df (#EXP-1373): `chat-menus` (Special components): THE chat
//! menu and THE agent menu ×4 (`fixtures/chats.json` `menuCases` +
//! `agentMenuCases`), the twin of web `chat-menus.tsx`. The demo IS the
//! builders every way in calls ([`crate::chat_actions::chat_entry_items`]
//! behind a chat row's right-click and the thread header's `…`,
//! [`crate::chat_actions::agent_entry_items`] behind the rail's agent row
//! and the agent chat's `…`) over the fixture's entries, drawn open in
//! place, plus THE delete confirm ([`crate::chat_actions::delete_alert`]).

use gpui::{div, px, App, Context, Div, Entity, IntoElement, ParentElement as _, Render, Styled as _, Window};
use gpui_component::menu::PopupMenu;
use gpui_component::ActiveTheme as _;

use domain::chats::{agent_menu_entries, thread_menu_entries, AgentMenuEntry, ThreadMenuEntry};

pub(crate) const ID: &str = "chat-menus";
pub(crate) const OWNER: &str = "EXP-1373";

/// The specimens: (caption, the menu).
enum Specimen {
    Chat(Vec<ThreadMenuEntry>),
    Agent(Vec<AgentMenuEntry>),
}

fn specimens() -> Vec<(&'static str, Specimen)> {
    vec![
        ("A live chat", Specimen::Chat(thread_menu_entries("running", true))),
        ("An ended chat", Specimen::Chat(thread_menu_entries("ended", true))),
        ("The agent menu (busy, owner)", Specimen::Agent(agent_menu_entries(true, true))),
        ("The agent menu (idle, member)", Specimen::Agent(agent_menu_entries(false, false))),
    ]
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
                        Specimen::Agent(entries) => crate::chat_actions::agent_entry_items(menu, "sg-agent", entries),
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
        let muted = cx.theme().muted_foreground;
        div().flex().flex_wrap().items_start().gap_8().children(self.menus.iter().map(|(caption, menu)| {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(div().text_xs().text_color(muted).child(*caption))
                .child(menu.clone())
        }))
    }
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let demo = window.use_keyed_state("sg-chat-menus", cx, MenusDemo::new);
    let confirm = crate::native_dialog::alert_specimen(
        "sg-chat-delete-confirm",
        crate::chat_actions::delete_alert(|_, _| true),
        window,
        cx,
    );
    div().flex().flex_col().gap_8().child(demo).child(confirm)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specimens are the fixture's cases.
    #[test]
    fn the_specimens_are_the_fixture_entries() {
        let specimens = specimens();
        assert!(matches!(&specimens[0].1, Specimen::Chat(entries) if entries == &[ThreadMenuEntry::Stop, ThreadMenuEntry::StopAndDelete]));
        assert!(matches!(&specimens[1].1, Specimen::Chat(entries) if entries == &[ThreadMenuEntry::Delete]));
        assert!(matches!(&specimens[2].1, Specimen::Agent(entries) if entries.len() == 4));
    }
}
