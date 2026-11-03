//! Settings → Boards → Archived boards (EXP-500).
//!
//! Web parity: the "Archived boards" card in `components/team/boards-section.tsx`.
//! Owner-only, like the `boards.listArchived` / `boards.unarchive` procedures
//! behind it.
//!
//! Archiving is the NON-PURGING sibling of the 48h trash: an archived board
//! and every one of its issues drop out of the Electric shapes server-side, so
//! they are invisible on every client without a single client-side filter —
//! and so an archived board is NOT in the synced boards collection. That makes
//! this pane the only place one can be seen at all, and the tRPC read the only
//! way to fetch it. (The first archiving attempt synced the flag and asked all
//! four clients to filter it; it leaked and was deleted, REV2-103.)
//!
//! Reads are fetch-on-open + refetch after every unarchive (never optimistic:
//! the row leaves this server list and re-enters sync at its own pace).
//!
//! Web parity (`BoardsTrashPage`): under the archived list, the 48h TRASH
//! (`boards.listDeleted`, Restore = `boards.restore`), drawn only while a
//! board is pending deletion; its countdown re-renders every minute.

use gpui::{
    div, prelude::FluentBuilder as _, App, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;

use api::boards::{ArchivedBoard, TrashedBoard};

use crate::icons::{board_icon_name_glyph, registry};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;
use crate::surface::{glass_pill_button, PillSize};

use super::{section, settings_skeleton};

enum Load {
    Idle,
    Loading,
    Ready(Result<Vec<ArchivedBoard>, String>),
}

pub struct ArchivedBoardsPane {
    nav: Entity<Navigation>,
    load: Load,
    /// The team the current `load` belongs to; a switch re-fetches.
    loaded_team: Option<String>,
    /// The account it was fetched as — a re-login must re-fetch.
    account_id: Option<String>,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    /// The trashed boards (`boards.listDeleted`); a failed read is quiet,
    /// the section just stays away (web parity).
    trash: Vec<TrashedBoard>,
    /// The board being unarchived or restored — disables its row's button.
    pending: Option<String>,
    /// FEED-79: the load in flight is the person's Retry press (its failure
    /// toasts); a load on open fails quietly into the empty state.
    retry_pressed: bool,
    _subscriptions: Vec<Subscription>,
}

impl ArchivedBoardsPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
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
            load: Load::Idle,
            trash: Vec::new(),
            loaded_team: None,
            account_id: None,
            generation: 0,
            pending: None,
            retry_pressed: false,
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
            self.load = Load::Idle;
        }
        let same_team = self.loaded_team.as_deref() == Some(team_id);
        if same_team && !matches!(self.load, Load::Idle) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };

        self.load = Load::Loading;
        self.loaded_team = Some(team_id.to_string());
        self.generation += 1;
        let generation = self.generation;
        let team_id = team_id.to_string();

        cx.spawn(async move |this, cx| {
            let (result, trash) = cx
                .background_executor()
                .spawn(async move {
                    let archived = api::boards::boards_list_archived(&trpc, &team_id)
                        .map_err(|err| err.user_message());
                    let trash = api::boards::boards_list_deleted(&trpc, &team_id);
                    (archived, trash)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer fetch
                }
                match trash {
                    Ok(rows) => this.trash = rows,
                    Err(error) => log::warn!("[ui] boards.listDeleted failed: {error}"),
                }
                // FEED-79: a failed load = the empty state + Retry; only a
                // Retry press that fails again toasts.
                let attended = std::mem::take(&mut this.retry_pressed);
                if let Err(message) = &result {
                    crate::action_error::failed_load(
                        "the archived boards",
                        attended,
                        Some(message.clone()),
                        cx,
                    );
                }
                this.load = Load::Ready(result);
                cx.notify();
            });
        })
        .detach();
    }

    /// Refetch (after an unarchive): drop the cached list; the next render
    /// re-fetches.
    fn refetch(&mut self, cx: &mut gpui::Context<Self>) {
        self.load = Load::Idle;
        cx.notify();
    }

    /// The failed list's Retry: the same load, attended.
    fn retry_load(&mut self, cx: &mut gpui::Context<Self>) {
        self.retry_pressed = true;
        self.refetch(cx);
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
                // in which case it belongs to the web trash card now and must
                // drop off this list.
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

    /// One Trash row: the archived row's shape with the purge countdown and
    /// Restore.
    fn trash_row(&self, board: &TrashedBoard, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let pending = self.pending.as_deref() == Some(board.id.as_str());
        let board_for_click = board.clone();
        board_row(
            &board.id,
            board.icon.as_deref(),
            &board.name,
            board.prefix.clone(),
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

    fn row(&self, board: &ArchivedBoard, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let pending = self.pending.as_deref() == Some(board.id.as_str());
        let board_for_click = board.clone();
        board_row(
            &board.id,
            board.icon.as_deref(),
            &board.name,
            board.prefix.clone(),
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

/// One board row of either list. EXP-1076: a board is an ENTITY, so its
/// list is the hairline LADDER, not the inset-grouped block a FORM's fields
/// wear: `list_row` + `flat_row` at the settings row rhythm (the web
/// `SETTINGS_LIST_CLASS` twin): glyph, name, the prefix as a mono readonly
/// pill, the caption, the action pill.
fn board_row(
    id: &str,
    icon: Option<&str>,
    name: &str,
    prefix: Option<String>,
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
        .child(board_icon_name_glyph(icon.unwrap_or_default()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(SharedString::from(name.to_string())),
        )
        .when_some(prefix.filter(|prefix| !prefix.is_empty()), |row, prefix| {
            row.child(
                crate::surface::glass_pill(
                    SharedString::from(format!("board-prefix-{id}")),
                    PillSize::Sm,
                    crate::surface::PillMode::Readonly,
                    cx,
                )
                .font_family(theme::terminal::FONT_FAMILY)
                .child(prefix),
            )
        })
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(caption),
        )
        .child(action)
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

impl Render for ArchivedBoardsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return v_flex().child(crate::controls::empty_state_list(
                gpui_component::Icon::new(crate::icons::registry::UI_TEAM),
                crate::empty_state_copy::NO_TEAM_SELECTED,
                cx,
            ));
        };
        self.ensure_loaded(&team_id, cx);

        // Web parity: the archive glyph leads the band; no Refresh — the list
        // refetches on open and after every unarchive.
        let mut body = section(cx).child(crate::surface::glass_section_band(
            Some(
                Icon::from(registry::UI_ARCHIVE)
                    .xsmall()
                    .text_color(cx.theme().foreground.opacity(0.5))
                    .into_any_element(),
            ),
            "Archived boards",
            None,
            cx,
        ));

        match &self.load {
            Load::Idle | Load::Loading => {
                body = body.child(settings_skeleton("archived-boards-skeleton", cx));
            }
            // A failed load (FEED-79): the empty state + Retry, no toast.
            Load::Ready(Err(_)) => {
                body = body.child(crate::action_error::load_failed_state(
                    "archived-boards-retry",
                    gpui_component::Icon::new(crate::icons::registry::UI_ARCHIVE),
                    crate::empty_state_copy::ARCHIVED_BOARDS,
                    cx.listener(|this, _, _, cx| this.retry_load(cx)),
                    cx,
                ));
            }
            Load::Ready(Ok(rows)) if rows.is_empty() => {
                body = body.child(crate::controls::empty_state_list(
                    gpui_component::Icon::new(crate::icons::registry::UI_ARCHIVE),
                    crate::empty_state_copy::ARCHIVED_BOARDS,
                    cx,
                ));
            }
            Load::Ready(Ok(rows)) => {
                let rows = rows.clone();
                // EXP-1076: the gapless ladder under the band.
                body = body.child(v_flex().w_full().min_w_0().children(
                    rows.iter().map(|board| {
                        crate::surface::list_row(self.row(board, cx))
                    }),
                ));
            }
        }

        let mut page = v_flex().gap_6().child(body);
        if !self.trash.is_empty() {
            let rows = self.trash.clone();
            page = page.child(
                section(cx)
                    .child(crate::surface::glass_section_band(
                        Some(
                            Icon::from(registry::UI_DELETE)
                                .xsmall()
                                .text_color(cx.theme().foreground.opacity(0.5))
                                .into_any_element(),
                        ),
                        "Trash",
                        None,
                        cx,
                    ))
                    .child(
                        div()
                            .px_1()
                            .text_xs()
                            .text_color(cx.theme().foreground.opacity(0.5))
                            .child(TRASH_NOTE),
                    )
                    .child(v_flex().w_full().min_w_0().children(
                        rows.iter().map(|board| crate::surface::list_row(self.trash_row(board, cx))),
                    )),
            );
        }
        page
    }
}

/// Web `PendingDeletionCard`'s line under the Trash band.
const TRASH_NOTE: &str =
    "Deleted boards are kept for 48 hours, then permanently removed with all their issues.";

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
