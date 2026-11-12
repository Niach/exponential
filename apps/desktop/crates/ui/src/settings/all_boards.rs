//! Settings → Boards → All boards (#NEW-37, settings-nav.json `boards`).
//!
//! Web parity: `BoardsListBand` in `components/team/boards-section.tsx`. ONE
//! list of the team's boards: the `Boards` band (New board) over the live
//! synced boards (a row opens that board's pane), then the `Archived` and
//! `Trash` bands, each drawn only while non-empty, with Unarchive / Restore
//! and the archived date / purge countdown. Owner-only here: members read the
//! live boards straight off the nav, and the two server reads are owners'.
//!
//! Archiving is the NON-PURGING sibling of the 48h trash: an archived or
//! trashed board and every one of its issues drop out of the Electric shapes
//! server-side (EXP-500, REV2-103), so the tRPC reads (`boards.listArchived`,
//! `boards.listDeleted`) are the only way to see either. Both are fetched on
//! open + after every action (never optimistic: the row leaves the server list
//! and re-enters sync at its own pace); a failed read is quiet (FEED-79).
//!
//! #EXP-1797 (issue-lifecycle.json `trash`): under them the `Deleted issues`
//! band, the team's issue trash (`issues.listDeleted`, any member), drawn
//! only while non-empty, for every member: the same put-away row (the
//! issue's board glyph, `{identifier} {title}`, the purge countdown) with
//! `Restore` and, for owners, `Delete forever` behind the `purge-issue`
//! prompt. The board bands stay owner-only.

use std::rc::Rc;

use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};
use sync::Store;

use api::boards::{ArchivedBoard, TrashedBoard};
use api::issues::TrashedIssue;
use domain::issue_trash::{self, TrashAction};
use eui_gpui::binding::Binding;
use eui_gpui::view::SurfaceView;

use crate::icons::registry;
use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, navigate, Navigation, Screen};
use crate::queries;
use crate::sidebar::select_settings_section;

use super::doc::{empty_state, hex, mount, not, path, IdeToasts, Ink, SM, XS};
use super::{parse_hex_color, SettingsSection};

pub struct AllBoardsPane {
    nav: Entity<Navigation>,
    /// Whether the server reads for `loaded_team` are fetched / in flight.
    loaded: bool,
    /// The archived boards (`boards.listArchived`).
    archived: Vec<ArchivedBoard>,
    /// The team the current `load` belongs to; a switch re-fetches.
    loaded_team: Option<String>,
    /// The account it was fetched as — a re-login must re-fetch.
    account_id: Option<String>,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    /// The trashed boards (`boards.listDeleted`).
    trash: Vec<TrashedBoard>,
    /// The team's issue trash (`issues.listDeleted`), newest deleted first.
    issues: Vec<TrashedIssue>,
    /// The board (or trashed issue) being unarchived, restored or purged —
    /// disables its row's buttons.
    pending: Option<String>,
    #[cfg(test)]
    shot_team: Option<String>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

impl AllBoardsPane {
    pub fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // The actions leave the binding for the pane: New board opens its
        // dialog, a live row its pane, Unarchive / Restore write.
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["new", "open", "unarchive", "restore", "restoreIssue", "purgeIssue"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let key = event.context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update and these write it.
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, &key, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let view = mount("settings-all-boards", document(&Ink::of(cx)), &binding, window, cx);
        // The live band follows the synced boards.
        let boards = Store::global(cx).collections().boards.clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&boards, |_, _, cx| cx.notify()),
        ];
        // The trash countdown ("Purges in ~Nh") re-renders every minute.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(60))
                .await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        })
        .detach();
        Self {
            nav,
            loaded: false,
            archived: Vec::new(),
            trash: Vec::new(),
            issues: Vec::new(),
            loaded_team: None,
            account_id: None,
            generation: 0,
            pending: None,
            #[cfg(test)]
            shot_team: None,
            binding,
            view,
            _subscriptions: subscriptions,
        }
    }

    /// The team the pane shows (the pictures pin one: the headless store
    /// syncs none).
    fn team_id(&self, cx: &App) -> Option<String> {
        #[cfg(test)]
        if let Some(team) = &self.shot_team {
            return Some(team.clone());
        }
        active_team_id(&self.nav, cx)
    }

    /// Whether the viewer owns `team_id` (the pictures draw an owner's page:
    /// the nav only opens it for owners).
    fn owner(&self, team_id: &str, cx: &App) -> bool {
        #[cfg(test)]
        if self.shot_team.is_some() {
            return true;
        }
        super::is_owner(cx, team_id)
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = crate::navigation::nav_for_window(window, cx);
        Self::new(nav, window, cx)
    }

    /// The pane with these put-away boards (the server reads pinned).
    #[cfg(test)]
    pub(crate) fn show(&mut self, archived: Vec<ArchivedBoard>, trash: Vec<TrashedBoard>, cx: &mut gpui::Context<Self>) {
        self.show_with_issues(archived, trash, Vec::new(), cx);
    }

    /// [`Self::show`] plus the issue trash.
    #[cfg(test)]
    pub(crate) fn show_with_issues(&mut self, archived: Vec<ArchivedBoard>, trash: Vec<TrashedBoard>, issues: Vec<TrashedIssue>, cx: &mut gpui::Context<Self>) {
        self.issues = issues;
        self.shot_team = Some("team-shots".into());
        self.loaded_team = self.shot_team.clone();
        self.account_id = Store::global(cx).session(cx).account_id().map(str::to_string);
        self.loaded = true;
        self.archived = archived;
        self.trash = trash;
        cx.notify();
    }

    /// A document action: `new` (the create-board dialog), `open` (a live
    /// board's pane), `unarchive` / `restore` on the board `key`,
    /// `restoreIssue` / `purgeIssue` on the trashed issue `key`.
    fn action(&mut self, name: &str, key: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match name {
            "new" => {
                if let Some(team_id) = self.team_id(cx) {
                    crate::create_board_dialog::open(window, cx, team_id);
                }
            }
            "open" => {
                select_settings_section(window, cx, SettingsSection::Board(key.to_string()));
                navigate(window, cx, Screen::Settings);
            }
            "unarchive" if self.pending.is_none() => {
                if let Some(board) = self.archived.iter().find(|board| board.id == key).cloned() {
                    self.unarchive(&board, window, cx);
                }
            }
            "restore" if self.pending.is_none() => {
                if let Some(board) = self.trash.iter().find(|board| board.id == key).cloned() {
                    self.restore(&board, window, cx);
                }
            }
            "restoreIssue" if self.pending.is_none() => {
                if let Some(issue) = self.issues.iter().find(|issue| issue.id == key).cloned() {
                    self.issue_action(TrashAction::Restore, issue, window, cx);
                }
            }
            "purgeIssue" if self.pending.is_none() => {
                if let Some(issue) = self.issues.iter().find(|issue| issue.id == key).cloned() {
                    let pane = cx.entity().downgrade();
                    let spec = AlertSpec::from_prompt(
                        "Delete issue forever",
                        &domain::prompts::purge_issue(&issue.identifier),
                    )
                    .on_ok(move |window, cx| {
                        let issue = issue.clone();
                        let _ = pane.update(cx, |pane, cx| pane.issue_action(TrashAction::Purge, issue, window, cx));
                        true
                    });
                    native_dialog::open_alert(window, cx, spec);
                }
            }
            _ => {}
        }
    }

    /// The pane's state as the document reads it.
    fn project(&self, team: Option<&str>, cx: &App) -> Value {
        let muted = cx.theme().muted_foreground;
        let tint = |color: Option<&str>| hex(color.and_then(parse_hex_color).unwrap_or(muted));
        let Some(team_id) = team else {
            return json!({"noTeam": true, "live": [], "archived": [], "trash": [], "issues": []});
        };
        let live: Vec<Value> = Store::global(cx)
            .collections()
            .boards_in_team(team_id, cx)
            .iter()
            .map(|board| {
                let fallback = if board.repository_id.is_some() { "code" } else { "square-kanban" };
                json!({"key": board.id, "name": board.name, "icon": glyph_name(board.icon.as_deref(), fallback), "color": tint(board.color.as_deref())})
            })
            .collect();
        let pending = |id: &str| self.pending.as_deref() == Some(id);
        let archived: Vec<Value> = self
            .archived
            .iter()
            .map(|board| json!({"key": board.id, "name": board.name, "icon": glyph_name(board.icon.as_deref(), "code"), "color": tint(board.color.as_deref()),
                "caption": archived_on(board.archived_at.as_deref()), "action": if pending(&board.id) { "Unarchiving\u{2026}" } else { "Unarchive" }, "pending": pending(&board.id)}))
            .collect();
        let trash: Vec<Value> = self
            .trash
            .iter()
            .map(|board| json!({"key": board.id, "name": board.name, "icon": glyph_name(board.icon.as_deref(), "code"), "color": tint(board.color.as_deref()),
                "caption": purge_countdown(board.purge_at.as_deref(), chrono::Utc::now()), "action": "Restore", "pending": pending(&board.id)}))
            .collect();
        // #EXP-1797: the issue trash, every member; Delete forever for owners.
        let role = if self.owner(team_id, cx) { domain::contract::TEAM_ROLE_OWNER } else { "member" };
        let purgeable = issue_trash::trash_actions(role).contains(&TrashAction::Purge);
        let boards = Store::global(cx).collections().boards.clone();
        let boards = boards.read(cx);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let issues: Vec<Value> = self
            .issues
            .iter()
            .map(|issue| {
                let board = issue.board_id.as_deref().and_then(|id| boards.get(id));
                let icon = board.map(|board| glyph_name(board.icon.as_deref(), if board.repository_id.is_some() { "code" } else { "square-kanban" }));
                json!({"key": issue.id, "name": issue_trash::trash_row_text(&issue.identifier, &issue.title),
                    "hasIcon": icon.is_some(), "icon": icon.unwrap_or_else(|| "square-kanban".into()), "color": tint(board.and_then(|board| board.color.as_deref())),
                    "caption": issue_trash::trash_caption(issue.purge_at.as_deref(), now_ms), "action": TrashAction::Restore.label(),
                    "purge": TrashAction::Purge.label(), "purgeable": purgeable, "pending": pending(&issue.id)})
            })
            .collect();
        json!({"noTeam": false, "noLive": live.is_empty(), "hasArchived": !archived.is_empty(), "hasTrash": !trash.is_empty(), "hasIssues": !issues.is_empty(),
            "live": live, "archived": archived, "trash": trash, "issues": issues})
    }

    /// Kick the server fetch when the pane is first shown or the team /
    /// account changed. Runs at render time so a hidden pane never fetches.
    fn ensure_loaded(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        let account_id = Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.account_id {
            self.account_id = account_id;
            self.loaded = false;
        }
        let same_team = self.loaded_team.as_deref() == Some(team_id);
        if same_team && self.loaded {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        if !same_team {
            self.archived.clear();
            self.trash.clear();
            self.issues.clear();
        }
        // The board reads are owners' (members only ever see the issue trash).
        let owner = self.owner(team_id, cx);

        self.loaded = true;
        self.loaded_team = Some(team_id.to_string());
        self.generation += 1;
        let generation = self.generation;
        let team_id = team_id.to_string();

        cx.spawn(async move |this, cx| {
            let (archived, trash, issues) = cx
                .background_executor()
                .spawn(async move {
                    let boards = owner.then(|| {
                        (api::boards::boards_list_archived(&trpc, &team_id), api::boards::boards_list_deleted(&trpc, &team_id))
                    });
                    let issues = api::issues::issues_list_deleted(&trpc, &team_id);
                    match boards {
                        Some((archived, trash)) => (Some(archived), Some(trash), issues),
                        None => (None, None, issues),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer fetch
                }
                // FEED-79 (web parity): a failed read is quiet, the band
                // keeps the rows it last had.
                match archived {
                    Some(Ok(rows)) => this.archived = rows,
                    Some(Err(error)) => log::warn!("[ui] boards.listArchived failed: {error}"),
                    None => this.archived.clear(),
                }
                match trash {
                    Some(Ok(rows)) => this.trash = rows,
                    Some(Err(error)) => log::warn!("[ui] boards.listDeleted failed: {error}"),
                    None => this.trash.clear(),
                }
                match issues {
                    Ok(rows) => this.issues = rows,
                    Err(error) => log::warn!("[ui] issues.listDeleted failed: {error}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Refetch (after an unarchive / restore): the next render re-fetches.
    fn refetch(&mut self, cx: &mut gpui::Context<Self>) {
        self.loaded = false;
        cx.notify();
    }

    /// `boards.unarchive` → refetch. No confirm: unarchiving only makes things
    /// visible again, and archiving is right there to undo it.
    fn unarchive(
        &mut self,
        board: &ArchivedBoard,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let handle = window.window_handle();
        let view = cx.entity().downgrade();
        let board_id = board.id.clone();
        let board_name = board.name.clone();
        self.pending = Some(board_id.clone());
        cx.notify();

        cx.spawn(async move |_, cx| {
            let called_id = board_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::boards::boards_unarchive(&trpc, &called_id) })
                .await;
            let _ = view.update(cx, |this, cx| {
                this.pending = None;
                // The list is a server read — refetch on success AND failure:
                // a failure can mean the board was trashed out from under us,
                // in which case it belongs to the Trash band now.
                this.refetch(cx);
                cx.notify();
            });
            if let Err(error) = result {
                log::warn!("[ui] boards.unarchive({board_id}) failed: {error}");
                let note = crate::action_error::failed_toast(
                    "unarchive",
                    &board_name,
                    crate::action_error::action_error_description(&error),
                );
                let _ = handle.update(cx, |_, window, cx| {
                    crate::toast::show(note, window, cx);
                });
            }
        })
        .detach();
    }

    /// `boards.restore` → refetch (web `handleRestore`): refetch on success
    /// AND failure, a purged board drops off the list.
    fn restore(&mut self, board: &TrashedBoard, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let handle = window.window_handle();
        let view = cx.entity().downgrade();
        let board_id = board.id.clone();
        let board_name = board.name.clone();
        self.pending = Some(board_id.clone());
        cx.notify();

        cx.spawn(async move |_, cx| {
            let called_id = board_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::boards::boards_restore(&trpc, &called_id) })
                .await;
            let _ = view.update(cx, |this, cx| {
                this.pending = None;
                this.refetch(cx);
            });
            if let Err(error) = result {
                log::warn!("[ui] boards.restore({board_id}) failed: {error}");
                let note = crate::action_error::failed_toast(
                    "restore",
                    &board_name,
                    crate::action_error::action_error_description(&error),
                );
                let _ = handle.update(cx, |_, window, cx| {
                    crate::toast::show(note, window, cx);
                });
            }
        })
        .detach();
    }
}

impl AllBoardsPane {
    /// #EXP-1797: `issues.restore` (Restore) or `issues.purge` (Delete
    /// forever, already confirmed) → refetch on success AND failure (web
    /// `run`); a failure toasts.
    fn issue_action(&mut self, action: TrashAction, issue: TrashedIssue, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let handle = window.window_handle();
        let view = cx.entity().downgrade();
        self.pending = Some(issue.id.clone());
        cx.notify();

        cx.spawn(async move |_, cx| {
            let id = issue.id.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    match action {
                        TrashAction::Restore => api::issues::issues_restore(&trpc, &id),
                        TrashAction::Purge => api::issues::issues_purge(&trpc, &id),
                    }
                })
                .await;
            let _ = view.update(cx, |this, cx| {
                this.pending = None;
                this.refetch(cx);
            });
            if let Err(error) = result {
                let (procedure, verb) = match action {
                    TrashAction::Restore => ("issues.restore", "restore"),
                    TrashAction::Purge => ("issues.purge", "delete"),
                };
                log::warn!("[ui] {procedure}({}) failed: {error}", issue.id);
                let note = crate::action_error::failed_toast(
                    verb,
                    &issue.identifier,
                    crate::action_error::action_error_description(&error),
                );
                let _ = handle.update(cx, |_, window, cx| {
                    crate::toast::show(note, window, cx);
                });
            }
        })
        .detach();
    }
}

/// A curated board icon name the IDE's registry knows, else `fallback`.
fn glyph_name(icon: Option<&str>, fallback: &str) -> String {
    icon.filter(|name| registry::icon_by_name(name).is_some()).unwrap_or(fallback).to_string()
}

/// Web `formatPurgeCountdown`: "Purges in ~Nh" (hours rounded up), "Purging
/// soon" once due, "Purges soon" without a timestamp.
fn purge_countdown(purge_at: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> String {
    let Some(at) = purge_at.filter(|at| !at.is_empty()) else {
        return "Purges soon".into();
    };
    let Ok(at) = chrono::DateTime::parse_from_rfc3339(at.trim()) else {
        return "Purges soon".into();
    };
    let ms = (at.with_timezone(&chrono::Utc) - now).num_milliseconds();
    if ms <= 0 {
        return "Purging soon".into();
    }
    let hours = (ms + 3_600_000 - 1) / 3_600_000;
    format!("Purges in ~{hours}h").into()
}

/// Web `formatArchivedOn`: "Archived Jul 1, 2026", bare "Archived" without a
/// timestamp.
fn archived_on(archived_at: Option<&str>) -> String {
    match archived_at.filter(|at| !at.is_empty()) {
        Some(at) => format!("Archived {}", super::storage::format_created_date(at)).into(),
        None => "Archived".into(),
    }
}

/// A board row (EXP-1076: a board is an ENTITY, so its list is the flat
/// ladder at the settings row rhythm, `px_3` / `py_2`, `gap_3`): the
/// board's own tinted glyph, the name, and for a put-away board the
/// caption and the action pill firing `put_away`. `purge` (the issue
/// trash, #EXP-1797) adds the owners' second pill firing it, drawn while the
/// row's `purgeable`, and hides the glyph without a `hasIcon` board.
fn board_row(ink: &Ink, list: &str, put_away: Option<&str>, purge: Option<&str>) -> Value {
    let glyph = ink.r(1.);
    let mut name = ink.text(&format!("{list}_name"), path("name"), SM, &ink.foreground);
    name["props"]["lines"] = json!(1);
    name["style"]["fontWeight"] = json!(500);
    name["style"]["minWidth"] = json!(0);
    name["style"]["flexGrow"] = json!(1);
    name["style"]["flexShrink"] = json!(1);
    name["style"]["flexBasis"] = json!(0);
    let mut icon = json!({"id": format!("{list}_icon"), "component": "Icon", "props": {"name": path("icon")}, "style": {"width": glyph, "height": glyph, "color": path("color"), "flexShrink": 0}});
    if purge.is_some() {
        icon["visible"] = path("hasIcon");
    }
    let mut children = vec![icon, name];
    if let Some(action) = put_away {
        let mut caption = ink.text(&format!("{list}_caption"), path("caption"), XS, &ink.muted);
        caption["style"]["flexShrink"] = json!(0);
        children.push(caption);
        let event = json!({"name": action, "context": {"key": path("key")}});
        children.extend(ink.pill_button(&format!("{list}_action"), None, path("action"), path("pending"), event));
    }
    if let Some(purge) = purge {
        let event = json!({"name": purge, "context": {"key": path("key")}});
        let pill = ink.pill_button(&format!("{list}_purge"), None, path("purge"), path("pending"), event);
        children.push(json!({"id": format!("{list}_purge_slot"), "component": "Box", "visible": path("purgeable"),
            "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0}, "children": pill.to_vec()}));
    }
    let mut row = json!({"id": format!("{list}_row"), "component": "Box",
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md},
        "children": children});
    if put_away.is_none() {
        row["props"] = json!({"pressable": true});
        row["accessibility"] = json!({"label": path("name")});
        row["on"] = json!({"press": {"event": {"name": "open", "context": {"key": path("key")}}}});
        row["style"]["cursor"] = json!("pointer");
    }
    row
}

/// One band's flat list of board rows.
fn ladder(ink: &Ink, list: &str, put_away: Option<&str>, purge: Option<&str>, visible: Value) -> Value {
    json!({"id": format!("{list}_rows"), "component": "List", "visible": visible, "props": {"gap": "none"},
        "template": {"component": format!("{list}_wrap"), "path": format!("/pane/{list}"), "key": "key"},
        "children": [{"id": format!("{list}_wrap"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [board_row(ink, list, put_away, purge)]}]})
}

/// The pane's tree: the Boards band (New board) over the live boards, then
/// the Archived, Trash and Deleted issues bands while non-empty; no team = the no-team
/// state.
fn document(ink: &Ink) -> Value {
    let section = |id: &str, visible: Value, children: Vec<Value>| json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0}, "children": children});
    let [new_board, _] = ink.pill_button("new", Some("ui-add"), json!("New board"), json!(false), json!({"name": "new"}));
    let live = section("live", json!(true), vec![
        ink.band("live_band", "Boards", Some(new_board)),
        empty_state(ink, "live_empty", "settings-boards", crate::empty_state_copy::TEAM_BOARDS, path("/pane/noLive")),
        ladder(ink, "live", None, None, not(path("/pane/noLive"))),
    ]);
    let archived = section("archived", path("/pane/hasArchived"), vec![ink.band("archived_band", "Archived", None), ladder(ink, "archived", Some("unarchive"), None, json!(true))]);
    let trash = section("trash", path("/pane/hasTrash"), vec![ink.band("trash_band", "Trash", None), ladder(ink, "trash", Some("restore"), None, json!(true))]);
    let issues = section("issues", path("/pane/hasIssues"), vec![
        ink.band("issues_band", issue_trash::BAND, None),
        ladder(ink, "issues", Some("restoreIssue"), Some("purgeIssue"), json!(true)),
    ]);
    // The page: `v_flex().gap_6()`.
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [
            {"id": "page", "component": "Box", "visible": not(path("/pane/noTeam")), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(1.5), "width": "100%"}, "children": [live, archived, trash, issues]},
            empty_state(ink, "no_team", "ui-team", crate::empty_state_copy::NO_TEAM_SELECTED, path("/pane/noTeam")),
        ]})
}

impl Render for AllBoardsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AllBoardsPane").entered();
        let team = self.team_id(cx);
        if let Some(team_id) = &team {
            self.ensure_loaded(team_id, cx);
        }
        let projection = self.project(team.as_deref(), cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{archived_on, purge_countdown};

    #[test]
    fn archived_caption_matches_web() {
        assert_eq!(archived_on(Some("2026-07-01T10:00:00.000Z")).as_str(), "Archived Jul 1, 2026");
        assert_eq!(archived_on(None).as_str(), "Archived");
    }

    #[test]
    fn purge_countdown_matches_web() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-07-01T10:00:00.000Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(purge_countdown(Some("2026-07-03T03:30:00.000Z"), now).as_str(), "Purges in ~42h");
        assert_eq!(purge_countdown(Some("2026-07-01T09:00:00.000Z"), now).as_str(), "Purging soon");
        assert_eq!(purge_countdown(None, now).as_str(), "Purges soon");
    }
}
