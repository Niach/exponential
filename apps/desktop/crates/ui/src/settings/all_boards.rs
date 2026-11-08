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

use gpui::{
    div, App, InteractiveElement as _, StatefulInteractiveElement as _, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{
    v_flex, ActiveTheme as _, Disableable as _, Icon,
};
use sync::Store;

use api::boards::{ArchivedBoard, TrashedBoard};

use crate::icons::{board_icon_name_glyph, registry};
use crate::navigation::{active_team_id, navigate, Navigation, Screen};
use crate::queries;
use crate::sidebar::select_settings_section;
use crate::surface::{glass_pill_button, PillSize};

use super::{parse_hex_color, section, SettingsSection};

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
    /// The board being unarchived or restored — disables its row's button.
    pending: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl AllBoardsPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
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
            loaded_team: None,
            account_id: None,
            generation: 0,
            pending: None,
            _subscriptions: subscriptions,
        }
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
        }

        self.loaded = true;
        self.loaded_team = Some(team_id.to_string());
        self.generation += 1;
        let generation = self.generation;
        let team_id = team_id.to_string();

        cx.spawn(async move |this, cx| {
            let (archived, trash) = cx
                .background_executor()
                .spawn(async move {
                    let archived = api::boards::boards_list_archived(&trpc, &team_id);
                    let trash = api::boards::boards_list_deleted(&trpc, &team_id);
                    (archived, trash)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer fetch
                }
                // FEED-79 (web parity): a failed read is quiet, the band
                // keeps the rows it last had.
                match archived {
                    Ok(rows) => this.archived = rows,
                    Err(error) => log::warn!("[ui] boards.listArchived failed: {error}"),
                }
                match trash {
                    Ok(rows) => this.trash = rows,
                    Err(error) => log::warn!("[ui] boards.listDeleted failed: {error}"),
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

    /// One Trash row: the purge countdown and Restore.
    fn trash_row(&self, board: &TrashedBoard, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let pending = self.pending.as_deref() == Some(board.id.as_str());
        let board_for_click = board.clone();
        board_row(
            board.icon.as_deref(),
            board.color.as_deref(),
            &board.name,
            purge_countdown(board.purge_at.as_deref(), chrono::Utc::now()),
            glass_pill_button(
                SharedString::from(format!("restore-{}", board.id)),
                PillSize::Sm,
                cx,
            )
            .label("Restore")
            .disabled(pending)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.restore(&board_for_click, window, cx);
            })),
            cx,
        )
    }

    fn archived_row(&self, board: &ArchivedBoard, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let pending = self.pending.as_deref() == Some(board.id.as_str());
        let board_for_click = board.clone();
        board_row(
            board.icon.as_deref(),
            board.color.as_deref(),
            &board.name,
            archived_on(board.archived_at.as_deref()),
            glass_pill_button(
                SharedString::from(format!("unarchive-{}", board.id)),
                PillSize::Sm,
                cx,
            )
            .label(if pending { "Unarchiving\u{2026}" } else { "Unarchive" })
            .disabled(pending)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.unarchive(&board_for_click, window, cx);
            })),
            cx,
        )
    }
}

/// One put-away board row. EXP-1076: a board is an ENTITY, so its list is
/// the hairline LADDER: `flat_row` at the settings row rhythm (the web
/// `SETTINGS_LIST_CLASS` twin): the tinted glyph, name, the caption, the action pill.
fn board_row(
    icon: Option<&str>,
    color: Option<&str>,
    name: &str,
    caption: SharedString,
    action: gpui_component::button::Button,
    cx: &App,
) -> gpui::Div {
    crate::surface::flat_row()
        .flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_3()
        .px_3()
        .py_2()
        // The board's own colour, like the live rows and web's `BoardGlyph`.
        .child(board_icon_name_glyph(icon.unwrap_or_default()).text_color(
            color.and_then(parse_hex_color).unwrap_or(cx.theme().muted_foreground),
        ))
        .child(board_name(name))
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(caption),
        )
        .child(action)
}

fn board_name(name: &str) -> gpui::Div {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_sm()
        .font_weight(gpui::FontWeight::MEDIUM)
        .child(SharedString::from(name.to_string()))
}

/// Web `formatPurgeCountdown`: "Purges in ~Nh" (hours rounded up), "Purging
/// soon" once due, "Purges soon" without a timestamp.
fn purge_countdown(purge_at: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> SharedString {
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
fn archived_on(archived_at: Option<&str>) -> SharedString {
    match archived_at.filter(|at| !at.is_empty()) {
        Some(at) => format!("Archived {}", super::storage::format_created_date(at)).into(),
        None => "Archived".into(),
    }
}

impl Render for AllBoardsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AllBoardsPane").entered();
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return v_flex().child(crate::controls::empty_state_list(
                gpui_component::Icon::new(crate::icons::registry::UI_TEAM),
                crate::empty_state_copy::NO_TEAM_SELECTED,
                cx,
            ));
        };
        self.ensure_loaded(&team_id, cx);

        // The live band: "New board" as its trailing pill (web parity), the
        // synced boards in the nav's order, a row opens that board's pane.
        let new_board = {
            let team_id = team_id.clone();
            glass_pill_button("all-boards-new-board", PillSize::Sm, cx)
                .icon(Icon::new(registry::UI_ADD))
                .label("New board")
                .on_click(move |_, window, cx| {
                    crate::create_board_dialog::open(window, cx, team_id.clone());
                })
                .into_any_element()
        };
        let boards = Store::global(cx).collections().boards_in_team(&team_id, cx);
        let mut live = section(cx).child(crate::surface::glass_section_band(
            None,
            "Boards",
            Some(new_board),
            cx,
        ));
        if boards.is_empty() {
            live = live.child(crate::controls::empty_state_list(
                gpui_component::Icon::new(registry::SETTINGS_BOARDS),
                crate::empty_state_copy::TEAM_BOARDS,
                cx,
            ));
        } else {
            live = live.child(v_flex().w_full().min_w_0().children(boards.iter().map(|board| {
                let color = board
                    .color
                    .as_deref()
                    .and_then(parse_hex_color)
                    .unwrap_or(cx.theme().muted_foreground);
                let target = SettingsSection::Board(board.id.clone());
                crate::surface::list_row(
                    crate::surface::flat_row()
                        .flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .py_2()
                        .child(crate::icons::board_icon(board).text_color(color))
                        .child(board_name(&board.name))
                        .id(SharedString::from(format!("all-boards-row-{}", board.id)))
                        .cursor_pointer()
                        .on_click(move |_, window, cx| {
                            select_settings_section(window, cx, target.clone());
                            navigate(window, cx, Screen::Settings);
                        }),
                )
            })));
        }

        let mut page = v_flex().gap_6().child(live);
        if !self.archived.is_empty() {
            let rows = self.archived.clone();
            page = page.child(
                section(cx)
                    .child(crate::surface::glass_section_band(None, "Archived", None, cx))
                    .child(v_flex().w_full().min_w_0().children(
                        rows.iter().map(|board| crate::surface::list_row(self.archived_row(board, cx))),
                    )),
            );
        }
        if !self.trash.is_empty() {
            let rows = self.trash.clone();
            page = page.child(
                section(cx)
                    .child(crate::surface::glass_section_band(None, "Trash", None, cx))
                    .child(v_flex().w_full().min_w_0().children(
                        rows.iter().map(|board| crate::surface::list_row(self.trash_row(board, cx))),
                    )),
            );
        }
        page
    }
}

#[cfg(test)]
mod tests {
    use super::{archived_on, purge_countdown};

    #[test]
    fn archived_caption_matches_web() {
        assert_eq!(archived_on(Some("2026-07-01T10:00:00.000Z")).as_ref(), "Archived Jul 1, 2026");
        assert_eq!(archived_on(None).as_ref(), "Archived");
    }

    #[test]
    fn purge_countdown_matches_web() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-07-01T10:00:00.000Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert_eq!(purge_countdown(Some("2026-07-03T03:30:00.000Z"), now).as_ref(), "Purges in ~42h");
        assert_eq!(purge_countdown(Some("2026-07-01T09:00:00.000Z"), now).as_ref(), "Purging soon");
        assert_eq!(purge_countdown(None, now).as_ref(), "Purges soon");
    }
}
