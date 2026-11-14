//! Session 58cb32df (#EXP-1373, `fixtures/chats.json` `menuCases` +
//! `agentMenuCases`): the chat's and the agent's menus on the IDE, the twin
//! of web `components/chat/chat-menu.tsx`. ONE set of entries behind every
//! way in: a chat row's right-click (the Chats page, the rail's Active rows,
//! through THE [`crate::controls::PointerContextMenuExt`]) and the chat
//! header's `…`; the agent row's right-click and the agent chat header's `…`.
//!
//!   Delete           `sessions.delete` (the server ends what is live first)
//!   Compact / New    SEND `/compact` / `/new` (one path with the typed ones)
//!   Agent settings   the agent's settings IN its chat (owners; #FEED-239:
//!                    never a navigation to the Settings screen)
//!
//! #FEED-239: every `…` is THE session screen menu
//! (session-screen.json `sessionScreenMenu`): [`screen_menu_items`] draws
//! its entries for a chat, an agent, the Assistant and an app alike.
//!
//! S-3 (owner 2026-10-28): no Stop/End entry anywhere; the composer's
//! stop-turn is the one stop.
//!
//! A delete asks first (`copy.confirmDelete`); after it the chat's tab and
//! screen close back to the list. Failures toast `copy.toasts.*`.

use std::rc::Rc;

use gpui::{div, px, AnyElement, App, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::menu::PopupMenu;
use gpui_component::Icon;

use domain::chats::{chat_menu_entries, ChatMenuEntry, CHATS};
use domain::rows::SessionRow;
use domain::session_screen::{
    session_screen_menu, session_screen_menu_icon, session_screen_menu_label, SessionScreenKind,
    SessionScreenMenuEntry, SessionScreenMenuInput, SESSION_SCREEN,
};

use crate::icons::registry;
use crate::navigation::Screen;

/// A chat = a ROOT session that is neither an agent conversation nor an
/// app's editor session (`sessions.delete` refuses those).
pub(crate) fn is_chat_root(row: &SessionRow) -> bool {
    row.parent_id.is_none() && row.agent_id.is_none() && row.app_id.is_none()
}

/// web `canSteerSession`: a shared session, or the viewer's own.
pub(crate) fn can_steer(row: &SessionRow, cx: &App) -> bool {
    if row.shared {
        return true;
    }
    crate::queries::active_account(cx).is_some_and(|me| row.user_id.as_deref() == Some(me.user_id.as_str()))
}

/// web `useChatMenuEntries`' `canDelete` (the server's `sessions.delete`
/// rule): the chat's starter or a team owner.
pub(crate) fn can_delete(row: &SessionRow, cx: &App) -> bool {
    let starter =
        crate::queries::active_account(cx).is_some_and(|me| row.user_id.as_deref() == Some(me.user_id.as_str()));
    starter || row.team_id.as_deref().is_some_and(|team_id| crate::settings::is_owner(cx, team_id))
}

/// The chat menu's entries for `root_id` (`chatMenuEntries`); none for a
/// row that is not a chat root or one the viewer cannot steer, and no delete
/// entries for a viewer who is neither its starter nor a team owner.
pub(crate) fn chat_entries(root_id: &str, cx: &App) -> Vec<ChatMenuEntry> {
    let Some(root) = crate::session_data::session(root_id, cx).filter(is_chat_root) else {
        return Vec::new();
    };
    chat_menu_entries(root.status.as_deref().unwrap_or_default(), can_steer(&root, cx), can_delete(&root, cx))
}

/// Append the chat's entries to `menu` (a row's context menu, the header
/// `…`). `separator` = a rule before them when the menu already holds rows.
pub(crate) fn chat_menu_items(mut menu: PopupMenu, root_id: &str, separator: bool, cx: &App) -> PopupMenu {
    let entries = chat_entries(root_id, cx);
    if entries.is_empty() {
        return menu;
    }
    if separator {
        menu = menu.separator();
    }
    chat_entry_items(menu, root_id, &entries, cx)
}

/// THE chat menu's rows for `entries` (the styleguide's specimen too).
pub(crate) fn chat_entry_items(mut menu: PopupMenu, root_id: &str, entries: &[ChatMenuEntry], _cx: &App) -> PopupMenu {
    for entry in entries.iter().copied() {
        let root_id = root_id.to_string();
        menu = match entry {
            ChatMenuEntry::Delete => menu.item(
                crate::controls::danger_pointer_menu_item(registry::UI_DELETE, entry.label(), move |window, cx| {
                    confirm_delete(&root_id, window, cx)
                }),
            ),
        };
    }
    menu
}

/// A failed chat action's toast (`copy.toasts.*` + the server's reason).
fn failed(title: &str, err: Option<&api::ApiError>, cx: &mut App) {
    let mut toast = crate::toast::Toast::error(SharedString::from(title.to_string()));
    if let Some(description) = err.and_then(crate::action_error::action_error_description) {
        toast = toast.description(description);
    }
    crate::toast::show_in_active_window(toast, cx);
}

/// Delete asks first ×4: `copy.confirmDelete`, the destructive confirm =
/// `copy.menu.delete`.
pub(crate) fn confirm_delete(root_id: &str, window: &mut Window, cx: &mut App) {
    let root_id = root_id.to_string();
    let spec = delete_alert(move |_, cx| {
        delete_chat(&root_id, cx);
        true
    });
    crate::native_dialog::open_alert(window, cx, spec);
}

/// THE delete confirm's spec (the styleguide draws the same).
pub(crate) fn delete_alert(on_ok: impl Fn(&mut Window, &mut App) -> bool + 'static) -> crate::native_dialog::AlertSpec {
    delete_alert_for(1, on_ok)
}

/// #EXP-1388: the confirm for `count` chats (`chat_delete_title`).
pub(crate) fn delete_alert_for(
    count: usize,
    on_ok: impl Fn(&mut Window, &mut App) -> bool + 'static,
) -> crate::native_dialog::AlertSpec {
    let copy = &CHATS.copy;
    // #EXP-1553: web's compact confirm: "Delete this chat?" bold over the
    // body (the window keeps the short `Delete` name), no empty band.
    crate::native_dialog::AlertSpec::new(
        copy.menu.delete.clone(),
        copy.confirm_delete.body.clone(),
        copy.menu.delete.clone(),
    )
    .question(domain::chats::chat_delete_title(count))
    .ok_variant(gpui_component::button::ButtonVariant::Danger)
    .on_ok(on_ok)
}

/// `sessions.delete` on the root; on success the chat's tab and screen close
/// back to the Chats page in every window.
pub(crate) fn delete_chat(root_id: &str, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        failed(&CHATS.copy.toasts.delete_failed, None, cx);
        return;
    };
    let root_id = root_id.to_string();
    cx.spawn(async move |cx| {
        let id = root_id.clone();
        let result = cx
            .background_executor()
            .spawn(async move { api::sessions::delete(&trpc, &id) })
            .await;
        let _ = cx.update(|cx| match result {
            Ok(_) => close_chat_screens(&root_id, cx),
            Err(err) => {
                log::warn!("[chats] delete failed: {err}");
                failed(&CHATS.copy.toasts.delete_failed, Some(&err), cx);
            }
        });
    })
    .detach();
}

/// #EXP-1388: the selection's rows as the bar reads them (`rootStatus`,
/// `unreadCount`, `canSteer`, `canDelete`), in `ids` order; an id that is
/// no chat root drops out.
pub(crate) fn selection_rows(
    ids: &[String],
    rows: &[domain::chats::ChatRow],
    cx: &App,
) -> Vec<domain::chats::ChatSelectionRow> {
    ids.iter()
        .filter_map(|id| {
            let root = crate::session_data::session(id, cx)?;
            let unread_count = rows.iter().find(|row| &row.session_id == id).map_or(0, |row| row.unread_count);
            let steer = is_chat_root(&root) && can_steer(&root, cx);
            Some(domain::chats::ChatSelectionRow {
                session_id: id.clone(),
                root_status: root.status.clone().unwrap_or_default(),
                unread_count,
                can_steer: steer,
                can_delete: steer && can_delete(&root, cx),
            })
        })
        .collect()
}

/// #EXP-1388: run a selection bar action over `ids` (the caller already left
/// the selection). Mark read/unread and Archive/Unarchive batch (200 ids a
/// call); Stop = `sessions.end` per chat; Delete asks first
/// (`chat_delete_title`) then `sessions.delete` per chat. Any failure = ONE
/// toast, `chat_selection_failed_title` (`{n} of {m}` when only some failed).
pub(crate) fn run_selection_action(
    action: domain::chats::ChatSelectionAction,
    ids: Vec<String>,
    window: &mut Window,
    cx: &mut App,
) {
    use domain::chats::ChatSelectionAction as A;
    if ids.is_empty() {
        return;
    }
    if action == A::Delete {
        let spec = delete_alert_for(ids.len(), move |_, cx| {
            spawn_selection(A::Delete, ids.clone(), cx);
            true
        });
        crate::native_dialog::open_alert(window, cx, spec);
        return;
    }
    spawn_selection(action, ids, cx);
}

/// The selection's server calls on one background task.
fn spawn_selection(action: domain::chats::ChatSelectionAction, ids: Vec<String>, cx: &mut App) {
    use domain::chats::ChatSelectionAction as A;
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        failed(&CHATS.copy.toasts.selection_failed, None, cx);
        return;
    };
    cx.spawn(async move |cx| {
        let total = ids.len();
        let (done, error): (Vec<String>, Option<api::ApiError>) = cx
            .background_executor()
            .spawn(async move {
                let mut done = Vec::new();
                let mut first_err = None;
                let mut record = |id: &[String], result: Result<api::sessions::TxOutput, api::ApiError>| match result {
                    Ok(_) => done.extend_from_slice(id),
                    Err(err) => {
                        first_err.get_or_insert(err);
                    }
                };
                match action {
                    A::MarkRead | A::MarkUnread | A::Archive | A::Unarchive => {
                        for chunk in ids.chunks(SELECTION_CHUNK) {
                            let result = match action {
                                A::MarkRead => api::sessions::set_read(&trpc, chunk, true),
                                A::MarkUnread => api::sessions::set_read(&trpc, chunk, false),
                                A::Archive => api::sessions::set_archived(&trpc, chunk, true),
                                _ => api::sessions::set_archived(&trpc, chunk, false),
                            };
                            record(chunk, result);
                        }
                    }
                    A::Delete => {
                        for id in &ids {
                            record(std::slice::from_ref(id), api::sessions::delete(&trpc, id));
                        }
                    }
                }
                (done, first_err)
            })
            .await;
        let _ = cx.update(|cx| {
            if action == A::Delete {
                // The deleted chats' screens close, after a partial failure too.
                for id in &done {
                    close_chat_screens(id, cx);
                }
            }
            if let Some(err) = &error {
                log::warn!("[chats] selection {} failed: {err}", action.key());
                let title = domain::chats::chat_selection_failed_title(total - done.len(), total);
                failed(&title, Some(err), cx);
            }
        });
    })
    .detach();
}

/// `sessions.setRead` / `setArchived` take up to 200 ids a call.
const SELECTION_CHUNK: usize = 200;

/// The deleted chat's screens close: the root's chat and every session of
/// its tree (a child's own tab would sit on rows that no longer exist) lose
/// their tabs, and a window still on one lands on the Chats page. The tree
/// is read from the store's `root_id`s (the rows may not have synced away
/// yet; the root's own screen closes regardless).
fn close_chat_screens(root_id: &str, cx: &mut App) {
    let mut ids: Vec<String> = sync::Store::try_global(cx)
        .map(|store| {
            store
                .collections()
                .sessions
                .read(cx)
                .iter()
                .filter(|session| session.id != root_id && session.root_id_or_self() == root_id)
                .map(|session| session.id.clone())
                .collect()
        })
        .unwrap_or_default();
    ids.insert(0, root_id.to_string());
    let screens: Vec<Screen> = ids
        .into_iter()
        .flat_map(|session_id| [Screen::Chat { session_id: session_id.clone() }, Screen::Session { session_id }])
        .collect();
    for handle in cx.windows() {
        let screens = screens.clone();
        let _ = handle.update(cx, |_, window, cx| {
            if let Some(tabs) = crate::screens::screens_for_window(window, cx) {
                tabs.update(cx, |tabs, cx| {
                    for screen in &screens {
                        tabs.close_screen_tab(screen, window, cx);
                    }
                });
            }
            let nav = crate::navigation::nav_for_window(window, cx);
            if crate::navigation::resolved_screen(&nav, cx).is_some_and(|current| screens.contains(&current)) {
                crate::navigation::navigate(window, cx, Screen::Chats);
            }
        });
    }
}

/// A `…` entry's press.
pub(crate) type OnMenuEntry = Rc<dyn Fn(SessionScreenMenuEntry, &mut Window, &mut App)>;

/// #FEED-239 THE `…`'s rows (session-screen.json `sessionScreenMenu`):
/// `entries` in order, labels `copy.menu` by `kind`, icons `menuIcons`,
/// Delete destructive, a rule per `Separator`.
pub(crate) fn screen_menu_items(
    mut menu: PopupMenu,
    kind: &str,
    entries: &[SessionScreenMenuEntry],
    on_entry: OnMenuEntry,
) -> PopupMenu {
    for entry in entries.iter().copied() {
        let label = session_screen_menu_label(kind, entry);
        let icon = registry::concept_by_name(session_screen_menu_icon(entry));
        let on_entry = on_entry.clone();
        menu = match entry {
            SessionScreenMenuEntry::Separator => menu.separator(),
            SessionScreenMenuEntry::Delete => menu.item(crate::controls::danger_pointer_menu_item(
                icon.unwrap_or(registry::UI_DELETE),
                label,
                move |window, cx| on_entry(entry, window, cx),
            )),
            _ => menu.item(crate::controls::pointer_menu_item(icon, label, |_| None, move |window, cx| {
                on_entry(entry, window, cx)
            })),
        };
    }
    menu
}

/// THE `…` trigger of the session screen header (the ghost `ui-more`,
/// testId `testIds.menu`) over `build`'s rows; `None` = no entries.
pub(crate) fn screen_menu_button(
    has_entries: bool,
    build: impl Fn(PopupMenu, &mut Window, &mut gpui::Context<PopupMenu>) -> PopupMenu + 'static,
    cx: &App,
) -> Option<AnyElement> {
    if !has_entries {
        return None;
    }
    let trigger = crate::controls::ghost_icon_button(
        SharedString::from(SESSION_SCREEN.test_ids.menu.clone()),
        Icon::new(registry::UI_MORE),
        cx,
    )
    .accessibility_label("More");
    Some(
        div()
            .flex_shrink_0()
            .mr(px(-crate::work_header::GHOST_ICON_HANG))
            .child(crate::controls::PointerMenu::for_button(trigger, build))
            .into_any_element(),
    )
}

/// A chat's `…` entries (kind `session`: Delete for its starter or an
/// owner, when the viewer may steer it).
pub(crate) fn chat_screen_entries(root_id: &str, link_issue: bool, cx: &App) -> Vec<SessionScreenMenuEntry> {
    let delete = chat_entries(root_id, cx).contains(&ChatMenuEntry::Delete);
    session_screen_menu(SessionScreenMenuInput {
        kind: SessionScreenKind::Session.as_str(),
        link_issue,
        delete,
        ..Default::default()
    })
}

/// A chat's `…` press (Delete asks first).
pub(crate) fn run_chat_screen_entry(entry: SessionScreenMenuEntry, root_id: &str, window: &mut Window, cx: &mut App) {
    if entry == SessionScreenMenuEntry::Delete {
        confirm_delete(root_id, window, cx);
    }
}

/// The agent's kind (`agent` | `assistant`) and its `…` entries for the
/// viewer: the conversation pair for a member, settings for a team owner
/// (the Assistant: its owner).
pub(crate) fn agent_screen_entries(
    agent_id: &str,
    link_issue: bool,
    cx: &App,
) -> (SessionScreenKind, Vec<SessionScreenMenuEntry>) {
    let Some(agent) = crate::session_data::agent_def(agent_id, cx) else {
        return (SessionScreenKind::Agent, Vec::new());
    };
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    let assistant = agent.is_assistant();
    let kind = if assistant { SessionScreenKind::Assistant } else { SessionScreenKind::Agent };
    let team = agent.team_id.as_deref();
    let mine = me.is_some() && agent.owner_id == me;
    let member = team.is_some_and(|team_id| crate::settings::is_member(cx, team_id)) || (assistant && mine);
    if !member {
        return (kind, Vec::new());
    }
    let settings = if assistant { mine } else { team.is_some_and(|team_id| crate::settings::is_owner(cx, team_id)) };
    let entries = session_screen_menu(SessionScreenMenuInput {
        kind: kind.as_str(),
        member,
        link_issue,
        settings,
        ..Default::default()
    });
    (kind, entries)
}

/// The agent's `…` press: Compact / New SEND `menuSends` into the viewer's
/// conversation; settings = `on_settings` (the header: the pane; a row:
/// [`crate::agent_screen::open_agent_settings`]).
pub(crate) fn run_agent_screen_entry(
    entry: SessionScreenMenuEntry,
    agent_id: &str,
    on_settings: &dyn Fn(&mut Window, &mut App),
    window: &mut Window,
    cx: &mut App,
) {
    let sends = &SESSION_SCREEN.menu_sends;
    match entry {
        SessionScreenMenuEntry::Compact => send_to_agent(agent_id, &sends.compact, cx),
        SessionScreenMenuEntry::NewConversation => send_to_agent(agent_id, &sends.new_conversation, cx),
        SessionScreenMenuEntry::Settings => on_settings(window, cx),
        _ => {}
    }
}

/// A rail / chat-list agent row's right-click: THE `…` of its chat (kind
/// agent / assistant, phone false); its settings open that agent's chat
/// with the settings open.
pub(crate) fn agent_menu_items(menu: PopupMenu, agent_id: &str, cx: &App) -> PopupMenu {
    let (kind, entries) = agent_screen_entries(agent_id, false, cx);
    let id = agent_id.to_string();
    let on_entry: OnMenuEntry = Rc::new(move |entry, window, cx| {
        let target = id.clone();
        let on_settings = move |window: &mut Window, cx: &mut App| {
            crate::agent_screen::open_agent_settings(window, cx, target.clone())
        };
        run_agent_screen_entry(entry, &id, &on_settings, window, cx);
    });
    screen_menu_items(menu, kind.as_str(), &entries, on_entry)
}

/// Compact / New conversation SEND their builtin line through THE send path
/// (`sessions.send({agentId, text})` lands in the viewer's conversation).
pub(crate) fn send_to_agent(agent_id: &str, text: &str, cx: &mut App) {
    let Some(team_id) = crate::session_data::agent_def(agent_id, cx).and_then(|agent| agent.team_id) else {
        return;
    };
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        crate::action_error::failed_action("send", "the message", None, cx);
        return;
    };
    let input = crate::chat_launch::agent_send_input(&team_id, agent_id, text, Vec::new());
    let plan = crate::sealing::plan_send(&input, cx);
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { crate::sealing::send_blocking(&trpc, &input, &plan) })
            .await;
        if let Err(err) = result {
            let _ = cx.update(|cx| crate::action_error::failed_action_err("send", "the message", &err, cx));
        }
    })
    .detach();
}

/// A press inside an inline preview (`replyCases`): a `reply` event with
/// text posts it as the person's next message to `session_id`.
pub(crate) fn send_surface_reply(session_id: &str, text: &str, cx: &mut App) {
    let Some(team_id) = crate::session_data::session(session_id, cx).and_then(|row| row.team_id) else {
        return;
    };
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        crate::action_error::failed_action("send", "the message", None, cx);
        return;
    };
    let input = api::sessions::SendInput {
        team_id: Some(team_id),
        session_id: Some(session_id.to_string()),
        text: Some(text.to_string()),
        ..Default::default()
    };
    let plan = crate::sealing::plan_send(&input, cx);
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { crate::sealing::send_blocking(&trpc, &input, &plan) })
            .await;
        if let Err(err) = result {
            let _ = cx.update(|cx| crate::action_error::failed_action_err("send", "the message", &err, cx));
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(value: serde_json::Value) -> SessionRow {
        serde_json::from_value(value).unwrap()
    }

    /// Only a plain root is a chat (`sessions.delete` refuses the rest).
    #[test]
    fn only_a_plain_root_is_a_chat() {
        assert!(is_chat_root(&row(serde_json::json!({"id": "r"}))));
        assert!(!is_chat_root(&row(serde_json::json!({"id": "c", "parent_id": "r"}))));
        assert!(!is_chat_root(&row(serde_json::json!({"id": "a", "agent_id": "ag"}))));
        assert!(!is_chat_root(&row(serde_json::json!({"id": "e", "app_id": "ap"}))));
    }

    /// The entries' words and icons are the fixture's.
    #[test]
    fn the_entries_read_the_fixture_copy() {
        assert_eq!(ChatMenuEntry::Delete.label(), "Delete");
        assert_eq!(session_screen_menu_label("agent", SessionScreenMenuEntry::NewConversation), "New conversation");
        // #FEED-239: every `…` entry's icon is a registry concept.
        for entry in [
            SessionScreenMenuEntry::Publish,
            SessionScreenMenuEntry::Compact,
            SessionScreenMenuEntry::NewConversation,
            SessionScreenMenuEntry::Settings,
            SessionScreenMenuEntry::Export,
            SessionScreenMenuEntry::Delete,
        ] {
            assert!(registry::concept_by_name(session_screen_menu_icon(entry)).is_some(), "{entry:?}");
        }
    }
}
