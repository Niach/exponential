//! Thread 58cb32df (#EXP-1373, `fixtures/chats.json` `menuCases` +
//! `agentMenuCases`): the chat's and the agent's menus on the IDE, the twin
//! of web `components/thread/chat-menu.tsx`. ONE set of entries behind every
//! way in: a chat row's right-click (the Chats page, the rail's Active rows,
//! through THE [`crate::controls::PointerContextMenuExt`]) and the thread
//! header's `…`; the agent row's right-click and the agent chat header's `…`.
//!
//!   Stop             `sessions.end` on the ROOT (the whole tree ends)
//!   Stop and delete  `sessions.delete` (the server ends what is live first)
//!   Delete           `sessions.delete` (an ended chat)
//!   Compact / New    SEND `/compact` / `/new` (one path with the typed ones)
//!   Stop (agent)     `sessions.stop`, the TURN interrupt
//!   Agent settings   the agent's settings page (owners)
//!
//! A delete asks first (`copy.confirmDelete`); after it the chat's tab and
//! screen close back to the list. Failures toast `copy.toasts.*`.

use gpui::{App, SharedString, Window};
use gpui_component::menu::PopupMenu;

use domain::chats::{agent_menu_entries, thread_menu_entries, AgentMenuEntry, ThreadMenuEntry, CHATS};
use domain::rows::SessionRow;

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

/// The chat menu's entries for `root_id` (`threadMenuEntries`); none for a
/// row that is not a chat root or one the viewer cannot steer, and no delete
/// entries for a viewer who is neither its starter nor a team owner.
pub(crate) fn chat_entries(root_id: &str, cx: &App) -> Vec<ThreadMenuEntry> {
    let Some(root) = crate::session_data::session(root_id, cx).filter(is_chat_root) else {
        return Vec::new();
    };
    thread_menu_entries(root.status.as_deref().unwrap_or_default(), can_steer(&root, cx), can_delete(&root, cx))
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
pub(crate) fn chat_entry_items(mut menu: PopupMenu, root_id: &str, entries: &[ThreadMenuEntry], _cx: &App) -> PopupMenu {
    for entry in entries.iter().copied() {
        let root_id = root_id.to_string();
        menu = match entry {
            ThreadMenuEntry::Stop => menu.item(
                crate::controls::pointer_menu_item(Some(registry::CODING_STOP), entry.label(), |_| None, move |_, cx| {
                    stop_chat(&root_id, cx);
                }),
            ),
            ThreadMenuEntry::StopAndDelete | ThreadMenuEntry::Delete => menu.item(
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

/// Stop = `sessions.end` on the ROOT (the whole tree ends; a later message
/// still resumes it).
pub(crate) fn stop_chat(root_id: &str, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        failed(&CHATS.copy.toasts.stop_failed, None, cx);
        return;
    };
    let root_id = root_id.to_string();
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::sessions::end(&trpc, &root_id) })
            .await;
        if let Err(err) = result {
            log::warn!("[chats] stop failed: {err}");
            let _ = cx.update(|cx| failed(&CHATS.copy.toasts.stop_failed, Some(&err), cx));
        }
    })
    .detach();
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
    crate::native_dialog::AlertSpec::new(
        domain::chats::chat_delete_title(count),
        copy.confirm_delete.body.clone(),
        copy.menu.delete.clone(),
    )
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
/// `copy.toasts.selectionFailed` toast.
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
                    A::Stop | A::Delete => {
                        for id in &ids {
                            let result = if action == A::Stop {
                                api::sessions::end(&trpc, id)
                            } else {
                                api::sessions::delete(&trpc, id)
                            };
                            record(std::slice::from_ref(id), result);
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
                failed(&CHATS.copy.toasts.selection_failed, Some(err), cx);
            }
        });
    })
    .detach();
}

/// `sessions.setRead` / `setArchived` take up to 200 ids a call.
const SELECTION_CHUNK: usize = 200;

/// The deleted chat's screens close: the root's thread and every session of
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
        .flat_map(|session_id| [Screen::Thread { session_id: session_id.clone() }, Screen::Session { session_id }])
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
                crate::navigation::navigate(window, cx, Screen::Threads);
            }
        });
    }
}

/// The agent menu's entries for the viewer (`agentMenuEntries`): busy =
/// the viewer's conversation with it works; settings for team owners.
pub(crate) fn agent_entries(agent_id: &str, cx: &App) -> Vec<AgentMenuEntry> {
    let Some(team_id) = crate::session_data::agent_def(agent_id, cx).and_then(|agent| agent.team_id) else {
        return Vec::new();
    };
    if !crate::settings::is_member(cx, &team_id) {
        return Vec::new();
    }
    let busy = crate::session_data::my_conversation(agent_id, cx).is_some_and(|row| row.busy);
    agent_menu_entries(busy, crate::settings::is_owner(cx, &team_id))
}

/// Append the agent's entries to `menu` (the rail row's right-click, the
/// agent chat header's `…`): the conversation pair, a rule, then Stop and
/// Agent settings.
pub(crate) fn agent_menu_items(menu: PopupMenu, agent_id: &str, cx: &App) -> PopupMenu {
    let entries = agent_entries(agent_id, cx);
    agent_entry_items(menu, agent_id, &entries)
}

/// THE agent menu's rows for `entries` (the styleguide's specimen too).
pub(crate) fn agent_entry_items(mut menu: PopupMenu, agent_id: &str, entries: &[AgentMenuEntry]) -> PopupMenu {
    for (index, entry) in entries.iter().copied().enumerate() {
        if index > 0 && matches!(entry, AgentMenuEntry::Stop | AgentMenuEntry::Settings) && entries[index - 1] != AgentMenuEntry::Stop {
            menu = menu.separator();
        }
        let agent_id = agent_id.to_string();
        let icon = registry::concept_by_name(entry.icon());
        menu = menu.item(crate::controls::pointer_menu_item(icon, entry.label(), |_| None, move |window, cx| {
            run_agent_entry(entry, &agent_id, window, cx);
        }));
    }
    menu
}

fn run_agent_entry(entry: AgentMenuEntry, agent_id: &str, window: &mut Window, cx: &mut App) {
    if let Some(line) = entry.sends() {
        send_to_agent(agent_id, line, cx);
        return;
    }
    match entry {
        AgentMenuEntry::Stop => {
            if let Some(conversation) = crate::session_data::my_conversation(agent_id, cx) {
                crate::steer_viewer::stop_session(&conversation.id, cx);
            }
        }
        AgentMenuEntry::Settings => {
            crate::sidebar::select_settings_section(
                window,
                cx,
                crate::settings::SettingsSection::Agent(agent_id.to_string()),
            );
            crate::navigation::navigate(window, cx, Screen::Settings);
        }
        AgentMenuEntry::Compact | AgentMenuEntry::NewConversation => {}
    }
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
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::sessions::send(&trpc, &input) })
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
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { api::sessions::send(&trpc, &input) })
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
        assert_eq!(ThreadMenuEntry::StopAndDelete.label(), "Stop and delete");
        assert_eq!(AgentMenuEntry::NewConversation.label(), "New conversation");
        for entry in [AgentMenuEntry::Compact, AgentMenuEntry::NewConversation, AgentMenuEntry::Stop, AgentMenuEntry::Settings] {
            assert!(registry::concept_by_name(entry.icon()).is_some(), "{entry:?}");
        }
    }
}
