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

use gpui::{
    div, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    notification::Notification,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};
use sync::Store;

use api::boards::ArchivedBoard;

use crate::controls::WebControl as _;
use crate::icons::{board_icon_name_glyph, registry};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;

use super::{card_header, error_notice, section};

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
    /// The board being unarchived — disables its row's button.
    pending: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl ArchivedBoardsPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        Self {
            nav,
            load: Load::Idle,
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
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::boards::boards_list_archived(&trpc, &team_id)
                        .map_err(|err| err.to_string())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer fetch
                }
                this.load = Load::Ready(result);
                cx.notify();
            });
        })
        .detach();
    }

    /// Refetch (after an unarchive, or the Refresh button): drop the cached
    /// list; the next render re-fetches.
    fn refetch(&mut self, cx: &mut gpui::Context<Self>) {
        self.load = Load::Idle;
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
                // in which case it belongs to the web trash card now and must
                // drop off this list.
                this.refetch(cx);
                cx.notify();
            });
            if let Err(error) = result {
                log::warn!("[ui] boards.unarchive({board_id}) failed: {error}");
                let note = Notification::error(SharedString::from(format!(
                    "Could not unarchive {board_name}: {error}"
                )));
                let _ = handle.update(cx, |_, window, cx| {
                    window.push_notification(note, cx);
                });
            }
        })
        .detach();
    }

    fn row(&self, board: &ArchivedBoard, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let pending = self.pending.as_deref() == Some(board.id.as_str());
        let board_for_click = board.clone();
        // EXP-698: one row of an inset-grouped stack; `glass_group_rows` owns
        // the hairlines.
        crate::surface::glass_row_shell()
            .child(board_icon_name_glyph(board.icon.as_deref().unwrap_or_default()))
            .child(div().flex_1().min_w_0().text_sm().child(board.name.clone()))
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(board.prefix.clone().unwrap_or_default()),
            )
            .child(
                Button::new(SharedString::from(format!("unarchive-{}", board.id)))
                    .outline().cursor_pointer()
                    .web_xs()
                    .label(if pending { "Unarchiving\u{2026}" } else { "Unarchive" })
                    .disabled(pending)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.unarchive(&board_for_click, window, cx);
                    })),
            )
    }
}

impl Render for ArchivedBoardsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return v_flex().child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No team selected."),
            );
        };
        self.ensure_loaded(&team_id, cx);

        let mut body = section(cx).child(card_header(
            "Archived boards",
            "Archived boards and their issues are hidden from everyone in the team. \
             Nothing is deleted \u{2014} unarchive to bring a board back exactly as it was.",
            cx,
        ));

        let refresh = Button::new("archived-boards-refresh")
            .ghost().cursor_pointer()
            .web_sm()
            .label("Refresh")
            .loading(matches!(self.load, Load::Loading))
            .on_click(cx.listener(|this, _, _, cx| this.refetch(cx)));
        body = body.child(h_flex().w_full().justify_end().child(refresh));

        match &self.load {
            Load::Idle | Load::Loading => {
                body = body.child(
                    v_flex()
                        .gap_2()
                        .child(Skeleton::new().h_8().w_full())
                        .child(Skeleton::new().h_8().w_full()),
                );
            }
            Load::Ready(Err(message)) => {
                body = body.child(error_notice(SharedString::from(message.clone()), cx));
            }
            Load::Ready(Ok(rows)) if rows.is_empty() => {
                body = body.child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .py_2()
                        .child(
                            Icon::from(registry::UI_ARCHIVE)
                                .xsmall()
                                .text_color(cx.theme().muted_foreground),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("No archived boards."),
                        ),
                );
            }
            Load::Ready(Ok(rows)) => {
                let rows = rows.clone();
                body = body.child(crate::surface::glass_group_rows(
                    rows.iter().map(|board| self.row(board, cx)).collect(),
                ));
            }
        }

        v_flex().child(body)
    }
}
