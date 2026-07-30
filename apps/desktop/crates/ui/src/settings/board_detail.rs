//! Settings → one board's detail page (EXP-288 — the Boards group lists
//! every board as its own nav entry; this pane is the selected one's page).
//!
//! Web parity: `components/team/board-settings-dialog.tsx` — Name (deferred
//! save on blur/Enter), read-only Prefix with the "can't be changed" hint,
//! Icon + Color swatch grids saving immediately (`boards.update`), and the
//! per-board **repository picker** (`boards.setRepository` over the team's
//! connected registry repos — EXP-139); plus the list's "Move to trash"
//! action (`boards.delete` — a 48h SOFT delete server-side; restore lives in
//! the web settings' Pending-deletion card, deliberately not mirrored here).
//!
//! The pane is a singleton: it reads the selected `Board(id)` from the
//! window's shared settings selection and re-points itself on board
//! switches, flushing a pending rename for the previous board first (blur
//! doesn't fire on unmount — same caveat the web dialog has on close).

use gpui::{
    div, px, AppContext as _, ElementId, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;
use crate::repo_resolver::links_snapshot;
use crate::sidebar::RailShared;

use super::repositories::{fetch_repositories, RepoRow};
use super::{card_header, error_notice, row_stroke, section, spawn_trpc, SettingsSection};
use crate::icons::registry;

/// Server fetch state for the per-board repository picker.
enum RepoLoad {
    Idle,
    Loading,
    Ready(Vec<RepoRow>),
    Failed(SharedString),
}

pub struct BoardDetailPane {
    nav: Entity<Navigation>,
    shared: Entity<RailShared>,
    name_input: Entity<InputState>,
    /// The board the name input was last seeded from — a selection switch
    /// flushes the previous board's pending rename, then re-seeds.
    loaded_board: Option<String>,
    repos: RepoLoad,
    /// The team the current `repos` belongs to; a switch re-fetches.
    loaded_team: Option<String>,
    /// The account it was fetched as — a re-login must re-fetch.
    account_id: Option<String>,
    /// The synced (board → repository) links the current `repos` was
    /// fetched under (EXP-139) — a link to a repo this cache doesn't know yet
    /// (connected on another client) re-fetches exactly once per change.
    loaded_links: Option<Vec<(String, String)>>,
    /// The last `boards.setRepository` rejection, rendered inline (web
    /// parity: the ChangeRepositoryDialog error). Cleared on the next
    /// attempt / team switch.
    link_error: Option<SharedString>,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl BoardDetailPane {
    pub fn new(
        nav: Entity<Navigation>,
        shared: Entity<RailShared>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Board name"));
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |this: &mut Self, _, cx| {
                this.refresh_if_links_changed(cx);
                cx.notify();
            }),
            cx.subscribe(&name_input, |this, _, event: &InputEvent, cx| match event {
                // Web parity: the dialog saves the name on blur; Enter
                // commits too (both no-op on unchanged/empty).
                InputEvent::Blur | InputEvent::PressEnter { .. } => this.commit_rename(cx),
                _ => {}
            }),
        ];
        Self {
            nav,
            shared,
            name_input,
            loaded_board: None,
            repos: RepoLoad::Idle,
            loaded_team: None,
            account_id: None,
            loaded_links: None,
            link_error: None,
            generation: 0,
            _subscriptions: subscriptions,
        }
    }

    /// The selected board id from the shared settings selection.
    fn selected_board_id(&self, cx: &gpui::App) -> Option<String> {
        match self.shared.read(cx).settings_section() {
            SettingsSection::Board(id) => Some(id),
            _ => None,
        }
    }

    /// Re-seed the name input when the selected board changes, flushing a
    /// pending rename for the PREVIOUS board first (blur doesn't fire on a
    /// nav-away — web parity with the dialog's on-close save).
    fn sync_selected_board(
        &mut self,
        board: &domain::rows::Board,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.loaded_board.as_deref() == Some(board.id.as_str()) {
            return;
        }
        if self.loaded_board.is_some() {
            self.commit_rename(cx);
        }
        self.loaded_board = Some(board.id.clone());
        self.link_error = None;
        let name = board.name.clone();
        self.name_input
            .update(cx, |input, cx| input.set_value(name, window, cx));
    }

    /// `boards.update({id, name})` when the drafted name is non-empty and
    /// differs from the synced row (deliberately keyed on the LOADED board —
    /// remote renames while editing don't stomp the input, like the web's
    /// `useEffect([board?.id])`).
    fn commit_rename(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(board_id) = self.loaded_board.clone() else {
            return;
        };
        let drafted = self.name_input.read(cx).value().trim().to_string();
        if drafted.is_empty() {
            return;
        }
        let synced_name = Store::global(cx)
            .collections()
            .boards
            .read(cx)
            .get(&board_id)
            .map(|board| board.name.clone());
        if synced_name.as_deref() == Some(drafted.as_str()) {
            return;
        }
        spawn_trpc(cx, "boards.update(name)", move |trpc| {
            let mut input = api::boards::BoardsUpdateInput::new(board_id);
            input.name = Some(drafted);
            api::boards::boards_update(trpc, &input)
        });
    }

    /// Kick the `repositories.list` fetch backing the picker when the pane is
    /// first shown or the team / account changed (render-time — a hidden
    /// pane never fetches; mirror of the Repositories pane).
    fn ensure_repos(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        let account_id = Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.account_id {
            self.account_id = account_id;
            self.repos = RepoLoad::Idle;
        }
        let same_team = self.loaded_team.as_deref() == Some(team_id);
        if same_team && !matches!(self.repos, RepoLoad::Idle) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };

        if !same_team {
            self.link_error = None;
        }
        self.repos = RepoLoad::Loading;
        self.loaded_team = Some(team_id.to_string());
        self.loaded_links = Some(links_snapshot(team_id, cx));
        self.generation += 1;
        let generation = self.generation;
        let team_id = team_id.to_string();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    fetch_repositories(&trpc, &team_id).map_err(|err| err.to_string())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer fetch
                }
                this.repos = match result {
                    Ok(repos) => RepoLoad::Ready(repos),
                    Err(message) => RepoLoad::Failed(message.into()),
                };
                cx.notify();
                // A link that changed while this fetch was in flight still
                // lands: compare once more now that the load settled.
                this.refresh_if_links_changed(cx);
            });
        })
        .detach();
    }

    /// Re-fetch the repo list only when a link points at a repo this cache
    /// doesn't know — a repo connected + linked on another client must
    /// resolve to its name here without a restart (EXP-139). Snapshot-keyed
    /// so a repo the server genuinely doesn't list can't loop the fetch; a
    /// Failed load heals on any link change.
    fn refresh_if_links_changed(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.loaded_team.clone() else {
            return;
        };
        let links = links_snapshot(&team_id, cx);
        if self.loaded_links.as_ref() == Some(&links) {
            return;
        }
        let refetch = match &self.repos {
            // Idle refetches on the next render; Loading re-checks on
            // completion (which restores the loaded_links comparison base).
            RepoLoad::Idle | RepoLoad::Loading => return,
            RepoLoad::Failed(_) => true,
            RepoLoad::Ready(repos) => links
                .iter()
                .any(|(_, repo_id)| !repos.iter().any(|repo| &repo.id == repo_id)),
        };
        self.loaded_links = Some(links);
        if refetch {
            self.repos = RepoLoad::Idle;
            cx.notify();
        }
    }

    /// The trash confirm (EXP-288: honest 48h-soft-delete copy — the server
    /// keeps a trashed board restorable from the WEB settings for 48h, then
    /// hard-deletes it with all its issues).
    fn open_trash_dialog(
        board_id: String,
        board_name: String,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let spec = AlertSpec::new(
            "Move board to trash",
            format!(
                "Move {board_name} to the trash? It's kept for 48 hours — it can be \
                 restored from the team settings on the web — then permanently deleted \
                 with all its issues."
            ),
            "Move to trash",
        )
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            let board_id = board_id.clone();
            spawn_trpc(cx, "boards.delete", move |trpc| {
                api::boards::boards_delete(trpc, &board_id)
            });
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// `boards.setRepository` off the foreground; success needs nothing
    /// (the synced board row's Electric echo re-labels every surface, incl.
    /// the trunk resolver), a rejection surfaces inline (web parity — a
    /// silently-dropped retarget would leave the user coding on the old repo).
    fn set_repository(
        &mut self,
        board_id: String,
        repository_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.link_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::boards::boards_set_repository(&trpc, &board_id, &repository_id)
                })
                .await;
            if let Err(err) = result {
                log::warn!("[ui] boards.setRepository failed: {err}");
                let _ = this.update(cx, |this, cx| {
                    this.link_error = Some(format!("{err}").into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// The repository picker (EXP-139 — web parity: `ConnectedRepoPicker`):
    /// a dropdown of the team's connected registry repos, labeled with the
    /// board's CURRENT link from the synced row.
    fn repo_picker(
        &self,
        board: &domain::rows::Board,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let label: SharedString = match (&board.repository_id, &self.repos) {
            (Some(repo_id), RepoLoad::Ready(repos)) => repos
                .iter()
                .find(|repo| &repo.id == repo_id)
                .map(|repo| SharedString::from(repo.full_name.clone()))
                // Linked, but the list doesn't know it (yet) — never claim
                // "No repository" for a linked board.
                .unwrap_or_else(|| "Repository".into()),
            (Some(_), _) => "Repository".into(),
            (None, _) => "No repository".into(),
        };

        let button = Button::new(row_id("board-detail-repo", &board.id))
            .outline()
            .small()
            .max_w(px(320.))
            .icon(registry::UI_GITHUB)
            .label(label);

        let board_id = board.id.clone();
        let pane = cx.entity().clone();
        button
            .dropdown_menu(move |menu, _window, cx| {
                // Read LIVE state at open time — the component caches the
                // built menu until dismiss, so a render-time snapshot would
                // pin whatever the list looked like when the row last drew.
                let mut menu = menu.scrollable(true).max_h(px(320.));
                let current = Store::global(cx)
                    .collections()
                    .boards
                    .read(cx)
                    .get(&board_id)
                    .and_then(|board| board.repository_id.clone());
                match &pane.read(cx).repos {
                    RepoLoad::Idle | RepoLoad::Loading => {
                        menu = menu.label("Loading repositories\u{2026}");
                    }
                    RepoLoad::Failed(message) => {
                        menu = menu.label(SharedString::from(format!(
                            "Couldn't load repositories: {message}"
                        )));
                    }
                    RepoLoad::Ready(repos) if repos.is_empty() => {
                        menu = menu.label("No repositories connected yet.");
                    }
                    RepoLoad::Ready(repos) => {
                        for repo in repos {
                            let pane = pane.clone();
                            let board_id = board_id.clone();
                            let repo_id = repo.id.clone();
                            menu = menu.item(
                                PopupMenuItem::new(SharedString::from(repo.full_name.clone()))
                                    .icon(Icon::new(registry::UI_GITHUB))
                                    .checked(current.as_deref() == Some(repo.id.as_str()))
                                    .on_click(move |_, _, cx| {
                                        pane.update(cx, |this, cx| {
                                            this.set_repository(
                                                board_id.clone(),
                                                repo_id.clone(),
                                                cx,
                                            );
                                        });
                                    }),
                            );
                        }
                    }
                }
                // The list is a cached snapshot of the team's connected
                // repos — offer an explicit reload for repos connected on
                // another client (doubles as the Failed state's retry).
                let pane = pane.clone();
                menu.separator()
                    .item(PopupMenuItem::new("Refresh list").on_click(move |_, _, cx| {
                        pane.update(cx, |this, cx| {
                            this.repos = RepoLoad::Idle;
                            cx.notify();
                        });
                    }))
            })
            .into_any_element()
    }

    fn field_label(label: &'static str, cx: &gpui::App) -> impl IntoElement {
        div()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(label)
    }

    fn field_hint(hint: &'static str, cx: &gpui::App) -> impl IntoElement {
        div()
            .text_xs()
            .text_color(cx.theme().muted_foreground.opacity(0.7))
            .child(hint)
    }
}

impl Render for BoardDetailPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let team_id = active_team_id(&self.nav, cx);
        if let Some(team_id) = team_id.as_deref() {
            self.ensure_repos(team_id, cx);
        }
        let board = self.selected_board_id(cx).and_then(|id| {
            Store::global(cx).collections().boards.read(cx).get(&id).cloned()
        });
        // The nav clamps a stale Board(id) selection away before this pane
        // shows, so a missing row is only a transient frame.
        let Some(board) = board else {
            return v_flex().into_any_element();
        };
        self.sync_selected_board(&board, window, cx);

        let prefix: SharedString = board.prefix.clone().unwrap_or_default().into();

        let name_field = v_flex()
            .gap_1()
            .child(Self::field_label("Name", cx))
            .child(Input::new(&self.name_input).small());

        let prefix_field = v_flex()
            .gap_1()
            .child(Self::field_label("Prefix", cx))
            .child(
                h_flex().child(
                    div()
                        .px_2()
                        .py_1()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(row_stroke(cx))
                        .text_sm()
                        .font_family(theme::terminal::FONT_FAMILY)
                        .text_color(cx.theme().muted_foreground)
                        .child(prefix),
                ),
            )
            .child(Self::field_hint(
                "The prefix can't be changed after creation.",
                cx,
            ));

        // Icon + color save IMMEDIATELY (web parity — no Save button).
        let board_id = board.id.clone();
        let icon_field = v_flex()
            .gap_1()
            .child(Self::field_label("Icon", cx))
            .child(crate::board_form::icon_swatch_grid(
                "board-detail",
                board.icon.as_deref().unwrap_or_default(),
                move |name, _, cx| {
                    let board_id = board_id.clone();
                    spawn_trpc(cx, "boards.update(icon)", move |trpc| {
                        let mut input = api::boards::BoardsUpdateInput::new(board_id);
                        input.icon = Some(name.to_string());
                        api::boards::boards_update(trpc, &input)
                    });
                },
                cx,
            ));

        let board_id = board.id.clone();
        let color_field = v_flex()
            .gap_1()
            .child(Self::field_label("Color", cx))
            .child(crate::board_form::color_swatch_grid(
                "board-detail",
                board.color.as_deref().unwrap_or_default(),
                move |color, _, cx| {
                    let board_id = board_id.clone();
                    spawn_trpc(cx, "boards.update(color)", move |trpc| {
                        let mut input = api::boards::BoardsUpdateInput::new(board_id);
                        input.color = Some(color.to_string());
                        api::boards::boards_update(trpc, &input)
                    });
                },
                cx,
            ));

        let repo_field = v_flex()
            .gap_1()
            .child(Self::field_label("Repository", cx))
            .child(self.repo_picker(&board, cx))
            .child(Self::field_hint(
                "New \u{201c}Start coding\u{201d} launches use the selected repository.",
                cx,
            ));

        let mut body = section(cx)
            .child(card_header(
                format!("Board settings — {}", board.name),
                "Changes apply immediately; the name saves when you leave the field.",
                cx,
            ))
            .child(name_field)
            .child(prefix_field)
            .child(icon_field)
            .child(color_field)
            .child(repo_field);

        if let Some(error) = &self.link_error {
            body = body.child(error_notice(error.clone(), cx));
        }

        // Trash (owner-only pane already; the dialog confirms before it fires).
        let board_id = board.id.clone();
        let board_name = board.name.clone();
        body = body.child(
            h_flex().pt_2().child(
                Button::new(row_id("board-detail-trash", &board.id))
                    .danger()
                    .small()
                    .icon(registry::UI_DELETE)
                    .label("Move to trash")
                    .on_click(cx.listener(move |_, _, window, cx| {
                        Self::open_trash_dialog(board_id.clone(), board_name.clone(), window, cx);
                    })),
            ),
        );

        v_flex().child(body).into_any_element()
    }
}

fn row_id(kind: &str, id: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("{kind}-{id}")))
}
