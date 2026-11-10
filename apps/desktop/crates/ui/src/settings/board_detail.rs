//! Settings → one board's detail page (EXP-288 — the Boards group lists
//! every board as its own nav entry; this pane is the selected one's page).
//!
//! Web parity: `routes/t/$teamSlug/settings/boards/$boardId.tsx` (EXP-862
//! turned the settings dialog into a page) — Name (deferred
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

use std::rc::Rc;

use eui_gpui::{binding::Binding, host::HostPlugin, view::SurfaceView};
use gpui::{div, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};
use sync::Store;

use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;
use crate::repo_resolver::links_snapshot;
use crate::sidebar::RailShared;

use super::doc::{hex, path, IdeToasts, Ink, SM, VALUE_ALPHA, XS};
use super::repositories::{fetch_repositories, RepoRow};
use super::{spawn_trpc, SettingsSection};

/// Server fetch state for the per-board repository picker.
enum RepoLoad {
    Idle,
    Loading,
    Ready(Vec<RepoRow>),
    /// The list failed to load (a toast said so, FEED-79).
    Failed,
}

pub struct BoardDetailPane {
    nav: Entity<Navigation>,
    shared: Entity<RailShared>,
    /// The Name field's live text while it differs from the board's (its
    /// edits); `None` = untouched since the board was loaded.
    name_draft: Option<String>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
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
    /// FEED-79: the repo-list load in flight is the person's Retry press
    /// (its failure toasts); the load on open and every live re-list (a link
    /// change, the FEED-42 re-list) fail quietly into the empty picker.
    retry_pressed: bool,
    /// FEED-42: the linked repository id this pane already re-listed ONCE
    /// for — a load that came back without the board's repo (a server cache
    /// that lagged a connect on another client) gets one retry before the
    /// label settles on "Repository unavailable", never a fetch loop.
    relisted_unknown: Option<String>,
    /// EXP-712: `repositories.listBranches` for the board's repo, fetched
    /// when the Branch menu first opens and keyed by repository id so a
    /// retarget can never show the old repo's branches.
    branches: Option<(String, crate::board_form::BranchLoad)>,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    /// The bench's state (headless has no board, team or server).
    #[cfg(test)]
    shot: Option<BoardShot>,
    _subscriptions: Vec<Subscription>,
}

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) struct BoardShot {
    pub board: domain::rows::Board,
    /// The team's repositories (`repositories.list` rows); `None` = loading.
    pub repos: Option<serde_json::Value>,
    /// The list failed to load.
    pub failed: bool,
}

impl BoardDetailPane {
    pub fn new(
        nav: Entity<Navigation>,
        shared: Entity<RailShared>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |this: &mut Self, _, cx| {
                this.refresh_if_links_changed(cx);
                cx.notify();
            }),
        ];
        // Every action leaves the binding for the pane (the confirms and the
        // Repositories hand-off need its window).
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["repo", "branch", "branchOpen", "archive", "trash", "icon", "color"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let value = event.payload.as_ref().and_then(|p| p.get("value")).and_then(Value::as_str).unwrap_or_default().to_string();
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update and these write it.
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, value, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let plugin: Rc<dyn HostPlugin> = Rc::new(PanePlugin { pane: cx.weak_entity(), inner: super::doc::ide_plugin() });
        let view = super::doc::mount_with("settings-board", document(&Ink::of(cx), &Chrome::of(cx)), &binding, plugin, true, window, cx);
        Self {
            nav,
            shared,
            name_draft: None,
            binding,
            view,
            loaded_board: None,
            repos: RepoLoad::Idle,
            loaded_team: None,
            account_id: None,
            loaded_links: None,
            retry_pressed: false,
            relisted_unknown: None,
            branches: None,
            generation: 0,
            #[cfg(test)]
            shot: None,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = crate::navigation::nav_for_window(window, cx);
        let shared = crate::sidebar::rail_shared_for_window(window, cx);
        Self::new(nav, shared, window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: BoardShot, cx: &mut gpui::Context<Self>) {
        self.repos = match (&shot.repos, shot.failed) {
            (_, true) => RepoLoad::Failed,
            (None, false) => RepoLoad::Loading,
            (Some(rows), false) => RepoLoad::Ready(serde_json::from_value(rows.clone()).expect("repository rows")),
        };
        self.shot = Some(shot);
        cx.notify();
    }

    /// The pane's board (the bench's, under test).
    fn board(&self, cx: &gpui::App) -> Option<domain::rows::Board> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return Some(shot.board.clone());
        }
        self.selected_board_id(cx).and_then(|id| Store::global(cx).collections().boards.read(cx).get(&id).cloned())
    }

    /// The selected board id from the shared settings selection.
    fn selected_board_id(&self, cx: &gpui::App) -> Option<String> {
        match self.shared.read(cx).settings_section() {
            SettingsSection::Board(id) => Some(id),
            _ => None,
        }
    }

    /// Re-point the Name field when the selected board changes, flushing a
    /// pending rename for the PREVIOUS board first (blur doesn't fire on a
    /// nav-away — web parity with the dialog's on-close save).
    fn sync_selected_board(&mut self, board: &domain::rows::Board, cx: &mut gpui::Context<Self>) {
        if self.loaded_board.as_deref() == Some(board.id.as_str()) {
            return;
        }
        if self.loaded_board.is_some() {
            self.commit_rename(cx);
        }
        self.loaded_board = Some(board.id.clone());
        self.name_draft = None;
    }

    /// `boards.update({id, name})` when the drafted name is non-empty and
    /// differs from the synced row (keyed on the LOADED board).
    fn commit_rename(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(board_id) = self.loaded_board.clone() else {
            return;
        };
        let Some(drafted) = self.name_draft.as_deref().map(str::trim).map(str::to_string) else {
            return;
        };
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

    /// The Name field's edit (the plugin's): the live text, and on commit
    /// (blur / Enter: web parity, the dialog saves on blur) the rename.
    fn edited(&mut self, value: String, commit: bool, cx: &mut gpui::Context<Self>) {
        self.name_draft = Some(value);
        if commit {
            self.commit_rename(cx);
        }
    }

    /// Kick the `repositories.list` fetch backing the picker when the pane is
    /// first shown or the team / account changed (render-time — a hidden
    /// pane never fetches; mirror of the Repositories pane).
    fn ensure_repos(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        #[cfg(test)]
        if self.shot.is_some() {
            return;
        }
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
                    fetch_repositories(&trpc, &team_id).map_err(|err| err.user_message())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer fetch
                }
                let attended = std::mem::take(&mut this.retry_pressed);
                this.repos = match result {
                    Ok(repos) => RepoLoad::Ready(repos),
                    Err(message) => {
                        crate::action_error::failed_load(
                            "the repositories",
                            attended,
                            Some(message),
                            cx,
                        );
                        RepoLoad::Failed
                    }
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
            RepoLoad::Failed => true,
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

    /// FEED-42: the board's linked repository is missing from the FIRST load
    /// (the link snapshot matched, so [`Self::refresh_if_links_changed`]
    /// never fires) — re-list once, the placeholder bar meanwhile,
    /// before showing "Repository unavailable".
    fn relist_if_unknown(
        &mut self,
        team_id: &str,
        repository_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(repo_id) = repository_id else {
            return;
        };
        if !should_relist(repo_id, &self.repos, self.relisted_unknown.as_deref()) {
            return;
        }
        self.relisted_unknown = Some(repo_id.to_string());
        self.repos = RepoLoad::Idle;
        self.ensure_repos(team_id, cx);
    }

    /// The trash confirm: the prompts fixture's `trash-board` (EXP-1230; the
    /// server keeps a trashed board restorable from the WEB settings for 48h,
    /// then hard-deletes it with all its issues, EXP-288).
    fn open_trash_dialog(
        board_id: String,
        board_name: String,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let spec = AlertSpec::from_prompt(
            "Move board to trash",
            &domain::prompts::trash_board(&board_name),
        )
        .on_ok(move |_, cx| {
            let board_id = board_id.clone();
            spawn_trpc(cx, "boards.delete", move |trpc| {
                api::boards::boards_delete(trpc, &board_id)
            });
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// The archive confirm (EXP-500). Archiving is the non-purging sibling of
    /// the trash: the board and its issues leave every client's sync scope
    /// until an owner unarchives it from this settings section, and nothing is
    /// ever deleted. The nav clamps the now-stale `Board(id)` selection away
    /// on the next boards delta.
    fn open_archive_dialog(
        board_id: String,
        board_name: String,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let spec = AlertSpec::new(
            "Archive board",
            format!(
                "Archive {board_name}? It disappears for the whole team \u{2014} from the \
                 sidebar, search, pickers and every issue list \u{2014} along with all of \
                 its issues. Nothing is deleted, and owners can bring it back \
                 from Archived boards at any time."
            ),
            "Archive board",
        )
        .on_ok(move |_, cx| {
            let board_id = board_id.clone();
            spawn_trpc(cx, "boards.archive", move |trpc| {
                api::boards::boards_archive(trpc, &board_id)
            });
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// `boards.setRepository` off the foreground; success needs nothing
    /// (the synced board row's Electric echo re-labels every surface, incl.
    /// the trunk resolver), a rejection is the error toast with Retry
    /// (FEED-79 — a silently-dropped retarget would leave the user coding on
    /// the old repo).
    fn set_repository(
        &mut self,
        board_id: String,
        repository_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let retry = (board_id.clone(), repository_id.clone());
        // EXP-712: the server resets the board's branch pin on a retarget (it
        // belonged to the old repo) — drop the cached branch list with it.
        self.branches = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::boards::boards_set_repository(
                        &trpc,
                        &board_id,
                        repository_id.as_deref(),
                        None,
                    )
                })
                .await;
            if let Err(err) = result {
                log::warn!("[ui] boards.setRepository failed: {err}");
                let _ = this.update(cx, |_, cx| {
                    let pane = cx.entity().downgrade();
                    crate::action_error::failed_action_with_retry(
                        "change",
                        "the board's repository",
                        crate::action_error::action_error_description(&err),
                        move |_, cx| {
                            let (board_id, repository_id) = retry.clone();
                            let _ = pane.update(cx, |this, cx| {
                                this.set_repository(board_id, repository_id, cx)
                            });
                        },
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    /// A document action: a Repository or Branch pick, Archive, Move to
    /// trash.
    fn action(&mut self, name: &str, value: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(board) = self.board(cx) else { return };
        match name {
            // EXP-712: "No repository", a repo, the failed list's Retry, or
            // the hand-off to settings → Repositories (which also re-reads
            // the server list, so it doubles as the refresh a repo connected
            // on another client needs).
            "repo" => match value.as_str() {
                REPO_NONE => {
                    if board.repository_id.is_some() {
                        self.set_repository(board.id.clone(), None, cx);
                    }
                }
                REPO_RETRY => {
                    self.retry_pressed = true;
                    self.repos = RepoLoad::Idle;
                    cx.notify();
                }
                REPO_CONNECT => {
                    self.repos = RepoLoad::Idle;
                    cx.notify();
                    crate::sidebar::select_settings_section(window, cx, SettingsSection::Repositories);
                }
                "" => {}
                repo_id => {
                    if board.repository_id.as_deref() != Some(repo_id) {
                        self.set_repository(board.id.clone(), Some(repo_id.to_string()), cx);
                    }
                }
            },
            // EXP-712: picking the repo's own default means "follow the
            // repo" (an explicit `null`), never a pin on the same value.
            "branch" => {
                let Some((shown, repo_default)) = self.branch_value(&board) else { return };
                if value.is_empty() || value == shown {
                    return;
                }
                let pick = (repo_default.as_deref() != Some(value.as_str())).then_some(value);
                let board_id = board.id.clone();
                spawn_trpc(cx, "boards.update(defaultBranch)", move |trpc| {
                    let mut input = api::boards::BoardsUpdateInput::new(board_id);
                    // `Some(None)` is the explicit JSON `null` that clears the
                    // pin — a skipped field would keep the old one.
                    input.default_branch = Some(pick);
                    api::boards::boards_update(trpc, &input)
                });
            }
            // FEED-79: a failed branch list is fetched again as the menu
            // opens (today's rule); a loaded one never is.
            "branchOpen" => {
                if let Some(repository_id) = board.repository_id.clone() {
                    self.ensure_branches(repository_id, cx);
                }
            }
            // EXP-862: the icon and colour save IMMEDIATELY (web parity).
            "icon" if !value.is_empty() => {
                let board_id = board.id.clone();
                spawn_trpc(cx, "boards.update(icon)", move |trpc| {
                    let mut input = api::boards::BoardsUpdateInput::new(board_id);
                    input.icon = Some(value);
                    api::boards::boards_update(trpc, &input)
                });
            }
            "color" if !value.is_empty() => {
                let board_id = board.id.clone();
                spawn_trpc(cx, "boards.update(color)", move |trpc| {
                    let mut input = api::boards::BoardsUpdateInput::new(board_id);
                    input.color = Some(value);
                    api::boards::boards_update(trpc, &input)
                });
            }
            "archive" => Self::open_archive_dialog(board.id.clone(), board.name.clone(), window, cx),
            "trash" => Self::open_trash_dialog(board.id.clone(), board.name.clone(), window, cx),
            _ => {}
        }
    }

    /// The Branch row's value (the board's pin, else the repo's default)
    /// and the repo's own default; `None` = no repository, or neither is
    /// known (L30: never fabricate `main`).
    fn branch_value(&self, board: &domain::rows::Board) -> Option<(String, Option<String>)> {
        let repository_id = board.repository_id.as_deref()?;
        let repo_default = match &self.repos {
            RepoLoad::Ready(repos) => repos
                .iter()
                .find(|repo| repo.id == repository_id)
                .map(|repo| repo.default_branch.clone())
                .filter(|branch| !branch.is_empty()),
            _ => None,
        };
        let value = crate::repo_resolver::board_base_branch(board.default_branch.as_deref(), repo_default.as_deref())?;
        Some((value, repo_default))
    }

    /// The state as the document binds it.
    fn project(&self, board: &domain::rows::Board, cx: &gpui::App) -> Value {
        // EXP-712 — the Repository select (web `board-repo-field.tsx`): "No
        // repository", the team's connected repos, then "Connect another
        // repository…"; the label is FEED-32's never-blank rule.
        let label = repo_picker_label(board.repository_id.as_deref(), &self.repos);
        let mut options = vec![json!({"label": crate::board_form::NO_REPOSITORY, "value": REPO_NONE})];
        let mut has_repos = false;
        match &self.repos {
            RepoLoad::Ready(repos) => {
                has_repos = !repos.is_empty();
                options.extend(repos.iter().map(|repo| json!({"label": repo.full_name, "value": repo.id, "icon": "ui-github"})));
            }
            // A failed load is quiet (FEED-79): the picker's Retry.
            RepoLoad::Failed => options.push(json!({"label": crate::feedback::RETRY, "value": REPO_RETRY, "icon": "ui-refresh"})),
            RepoLoad::Idle | RepoLoad::Loading => {}
        }
        // A linked repo the list doesn't hold reads "Repository unavailable".
        if let (Some(repo_id), Some(label)) = (board.repository_id.as_deref(), label.as_ref()) {
            if !options.iter().any(|option| option["value"] == repo_id) {
                options.push(json!({"label": label.to_string(), "value": repo_id, "disabled": true}));
            }
        }
        options.push(json!({"label": crate::board_form::connect_repository_label(has_repos), "value": REPO_CONNECT, "icon": "ui-add"}));
        // EXP-712 — the Branch select: the shown branch always listed (a pin
        // whose branch GitHub no longer has), the repo default tagged.
        let branch = self.branch_value(board).map(|(value, repo_default)| {
            let mut names = vec![value.clone()];
            let load = self.branches.as_ref().filter(|(id, _)| board.repository_id.as_deref() == Some(id.as_str())).map(|(_, load)| load);
            if let Some(crate::board_form::BranchLoad::Ready(branches)) = load {
                names.extend(branches.iter().filter(|name| **name != value).cloned());
            }
            let failed = matches!(load, Some(crate::board_form::BranchLoad::Failed));
            let mut options: Vec<Value> = names
                .iter()
                .map(|name| match repo_default.as_deref() == Some(name.as_str()) {
                    // The trigger reads the bare name; the list tags it.
                    true => json!({"label": name, "value": name, "trailing": "\u{00b7} default"}),
                    false => json!({"label": name, "value": name}),
                })
                .collect();
            // FEED-79: a failed list is quiet; the open menu says so.
            if failed {
                options.push(json!({"label": crate::board_form::NO_BRANCHES, "value": "", "disabled": true}));
            }
            json!({"value": value, "options": options})
        });
        json!({
            "name": board.name,
            // A row from before the icon backfill carries no `icon`: the
            // board's fallback glyph (the rail's and web's rule), not the
            // create form's dashed "nothing picked" placeholder.
            "icon": board.icon.clone().unwrap_or_else(|| if board.repository_id.is_some() { "code" } else { "square-kanban" }.to_string()),
            "color": board.color.clone().unwrap_or_default(),
            // No colour yet: the swatch shows the muted fill (today's rule).
            "swatch": board.color.clone().filter(|color| super::parse_hex_color(color).is_some()).unwrap_or_else(|| hex(cx.theme().muted_foreground)[..7].to_string()),
            "prefix": board.prefix.clone().unwrap_or_default(),
            "repo": {"ready": label.is_some(), "loading": label.is_none(), "value": board.repository_id.clone().unwrap_or_else(|| REPO_NONE.to_string()), "options": options},
            "hasBranch": branch.is_some(),
            "branch": branch.unwrap_or_else(|| json!({"value": "", "options": []})),
        })
    }

    /// Lazy `repositories.listBranches` for the board's repo (EXP-712),
    /// kicked when the Branch menu opens and cached until the repo changes.
    fn ensure_branches(&mut self, repository_id: String, cx: &mut gpui::Context<Self>) {
        if crate::board_form::branches_settled_for(self.branches.as_ref(), &repository_id) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.branches = Some((
            repository_id.clone(),
            crate::board_form::BranchLoad::Loading,
        ));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let fetch_id = repository_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::repositories::list_branches(&trpc, &fetch_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let load = match result {
                    Ok(out) => crate::board_form::BranchLoad::Ready(out.branches),
                    Err(err) => crate::board_form::branches_failed(&err),
                };
                this.branches = Some((repository_id, load));
                cx.notify();
            });
        })
        .detach();
    }

}

impl Render for BoardDetailPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "BoardDetailPane").entered();
        let team_id = active_team_id(&self.nav, cx);
        if let Some(team_id) = team_id.as_deref() {
            self.ensure_repos(team_id, cx);
        }
        // The nav clamps a stale Board(id) selection away before this pane
        // shows, so a missing row is only a transient frame.
        let Some(board) = self.board(cx) else {
            return div().into_any_element();
        };
        self.sync_selected_board(&board, cx);
        if let Some(team_id) = team_id.as_deref() {
            self.relist_if_unknown(team_id, board.repository_id.as_deref(), cx);
        }
        // EXP-712: the Branch list loads with the page, ONCE per
        // repository (the same request the menu's open made); a failure
        // stays quiet (the open list says so) and the menu's next open
        // fetches again (`branchOpen`).
        if let Some(repository_id) = board.repository_id.clone() {
            if self.branches.as_ref().map(|(id, _)| id) != Some(&repository_id) {
                self.ensure_branches(repository_id, cx);
            }
        }
        let projection = self.project(&board, cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone()).into_any_element()
    }
}

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";
/// The Repository select's non-repo options.
const REPO_NONE: &str = "none";
const REPO_RETRY: &str = "retry";
const REPO_CONNECT: &str = "connect";

/// The pane's plugin: the IDE's, and the Name field's edits for the pane.
struct PanePlugin {
    pane: gpui::WeakEntity<BoardDetailPane>,
    inner: crate::exponential_ui_host::IdePlugin,
}

impl HostPlugin for PanePlugin {
    fn icon(&self, name: &str) -> Option<SharedString> {
        self.inner.icon(name)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.inner.on_unknown(node)
    }

    fn on_input(&self, event: &eui_gpui::host::InputEvent, cx: &mut gpui::App) {
        let commit = event.kind == eui_gpui::host::InputKind::Commit;
        let value = event.value.as_str().unwrap_or_default().to_string();
        let pane = self.pane.clone();
        cx.defer(move |cx| {
            let _ = pane.update(cx, |pane, cx| pane.edited(value, commit, cx));
        });
    }
}

/// The colours the document needs beyond [`Ink`]: the two `web_sm`
/// Buttons (the Default: the button fill, the input stroke; the danger).
struct Chrome {
    radius: f32,
    button_fg: String,
    button_fill: String,
    button_stroke: String,
    danger: String,
    danger_fg: String,
    danger_hover: String,
    /// `feedback::placeholder_bar`'s fill.
    skeleton: String,
}

impl Chrome {
    fn of(cx: &gpui::App) -> Self {
        let theme = cx.theme();
        Self {
            radius: f32::from(theme.radius),
            button_fg: hex(theme.button_foreground),
            button_fill: hex(theme.button),
            button_stroke: hex(theme.input),
            danger: hex(theme.button_danger),
            danger_fg: hex(theme.button_danger_foreground),
            danger_hover: hex(theme.button_danger_hover),
            skeleton: hex(theme.muted_foreground.opacity(0.18)),
        }
    }
}

/// EXP-862 (×4): the board's IDENTITY is ONE row — icon picker, colour
/// picker, name — on the 32px control rung (`gap_2`); the name types
/// straight into the row (saved on blur / Enter). Then the read-only Prefix
/// row (its value the mono readonly pill).
fn identity(ink: &Ink) -> Value {
    let icon = json!({"id": "icon_picker", "component": "IconPicker", "props": {"value": path("/pane/icon"), "color": path("/pane/color")}, "style": {"flexShrink": 0}, "on": {"change": {"event": {"name": "icon"}}}});
    let color = json!({"id": "color_picker", "component": "ColorPicker", "props": {"value": path("/pane/swatch")}, "style": {"flexShrink": 0}, "on": {"change": {"event": {"name": "color"}}}});
    let name = json!({"id": "name", "component": "Input", "props": {"variant": "bare", "label": "Name", "name": "name", "placeholder": "Board name", "value": path("/pane/name")},
        "style": {"flexGrow": 1, "minWidth": 0}});
    let mut row = ink.shell("identity", false, json!(true), vec![icon, color, name]);
    row["style"]["gap"] = json!(ink.r(0.5));
    let mut label = ink.text("prefix_label", json!("Prefix"), SM, &ink.foreground);
    label["style"]["flexGrow"] = json!(1);
    label["style"]["minWidth"] = json!(0);
    let prefix = ink.shell("prefix", false, json!(true), vec![label, ink.pill("prefix_pill", path("/pane/prefix"), true, None)]);
    ink.group("identity_group", vec![row, ink.divider("prefix_divider", json!(true)), prefix])
}

/// The Repository + Branch group (`row` Selects) with the note under it;
/// a linked repo the list hasn't resolved yet = the value's placeholder bar.
fn repository(ink: &Ink, chrome: &Chrome) -> Value {
    let select = json!({"id": "repo", "component": "Select", "visible": path("/pane/repo/ready"),
        "props": {"label": "Repository", "name": "repo", "triggerVariant": "row", "value": path("/pane/repo/value"), "options": path("/pane/repo/options")},
        "on": {"change": {"event": {"name": "repo"}}}});
    // `feedback::skeleton_value`: the 120 px pulsing bar, then the chevron.
    let glyph = ink.r(1.);
    let pending = ink.shell("repo_loading", false, path("/pane/repo/loading"), vec![
        json!({"id": "repo_loading_text", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexShrink": 0}, "children": [ink.text("repo_loading_label", json!("Repository"), SM, &ink.foreground)]}),
        json!({"id": "repo_loading_control", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "flex-end", "gap": ink.r(0.5), "flexGrow": 1, "minWidth": 200},
            "children": [
                {"id": "repo_loading_bar", "component": "Skeleton", "props": {"width": "120", "height": "8"}, "style": {"width": 120, "height": 8, "borderRadius": 4, "backgroundColor": chrome.skeleton}},
                {"id": "repo_loading_chevron", "component": "Icon", "props": {"name": "ui-chevron-right"}, "style": {"width": glyph, "height": glyph, "color": ink.value, "opacity": VALUE_ALPHA, "flexShrink": 0}},
            ]}),
    ]);
    let branch = json!({"id": "branch", "component": "Select",
        "props": {"label": "Branch", "name": "branch", "triggerVariant": "row", "value": path("/pane/branch/value"), "options": path("/pane/branch/options")},
        "on": {"change": {"event": {"name": "branch"}}, "open": {"event": {"name": "branchOpen"}}}});
    let branch = json!({"id": "branch_rows", "component": "Box", "visible": path("/pane/hasBranch"), "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [ink.divider("branch_divider", json!(true)), branch]});
    let group = ink.group("repo_group", vec![select, pending, branch]);
    let note = ink.text("repo_note", json!(crate::board_form::BOARD_REPO_NOTE), XS, &ink.muted);
    json!({"id": "repository", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0}, "children": [group, note]})
}

/// A `web_sm` Button with its glyph: 32 px, `px 12`, the theme radius, the
/// 12 px glyph (gpui-component's Small icon) and the `text_sm` label (line
/// height 1).
fn button(ink: &Ink, id: &str, icon: &str, label: &str, fill: &str, stroke: Option<&str>, ink_color: &str, hover: Option<&str>, event: &str, radius: f32) -> Value {
    let size = ink.r(SM);
    let mut button = json!({"id": id, "component": "Box", "props": {"pressable": true}, "accessibility": {"label": label}, "on": {"press": {"event": {"name": event}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": ink.r(0.25), "height": 32, "paddingHorizontal": 12, "borderRadius": radius, "backgroundColor": fill, "cursor": "pointer"},
        "children": [
            {"id": format!("{id}_icon"), "component": "Icon", "props": {"name": icon}, "style": {"width": 12, "height": 12, "color": ink_color, "flexShrink": 0}},
            {"id": format!("{id}_text"), "component": "Text", "props": {"text": label}, "style": {"fontSize": size, "lineHeight": size, "color": ink_color, "flexShrink": 0}},
        ]});
    if let Some(stroke) = stroke {
        button["style"]["borderWidth"] = json!(1);
        button["style"]["borderColor"] = json!(stroke);
    }
    if let Some(hover) = hover {
        button["style"][":hover"] = json!({"backgroundColor": hover});
    }
    button
}

/// The pane's tree (`gap_4`): the identity group, the repository block,
/// then Archive (NOT a danger button: it hides, it does not destroy) and
/// Move to trash (`pt_2`, `gap_2`); both confirm first.
fn document(ink: &Ink, chrome: &Chrome) -> Value {
    let actions = json!({"id": "actions", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "gap": ink.r(0.5), "paddingTop": ink.r(0.5)},
        "children": [
            button(ink, "archive", "ui-archive", "Archive board", &chrome.button_fill, Some(&chrome.button_stroke), &chrome.button_fg, None, "archive", chrome.radius),
            button(ink, "trash", "ui-delete", "Move to trash", &chrome.danger, None, &chrome.danger_fg, Some(&chrome.danger_hover), "trash", chrome.radius),
        ]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(1.), "width": "100%", "minWidth": 0},
        "children": [identity(ink), repository(ink, chrome), actions]})
}

/// FEED-32 (web `board-repo-field.tsx` `triggerLabel`): the Repository
/// select's label is NEVER blank and never claims "No repository" for a
/// linked board. A linked repo the list doesn't know is `None` (the value's
/// placeholder bar, D8) while the list is (re)fetching —
/// `refresh_if_links_changed` re-lists once per unknown id — and
/// "Repository unavailable" once the list came back (or failed) without it.
fn repo_picker_label(repository_id: Option<&str>, repos: &RepoLoad) -> Option<SharedString> {
    match (repository_id, repos) {
        (None, _) => Some(crate::board_form::NO_REPOSITORY.into()),
        (Some(repo_id), RepoLoad::Ready(repos)) => Some(
            repos
                .iter()
                .find(|repo| repo.id == repo_id)
                .map(|repo| SharedString::from(repo.full_name.clone()))
                .unwrap_or_else(|| "Repository unavailable".into()),
        ),
        (Some(_), RepoLoad::Failed) => Some("Repository unavailable".into()),
        (Some(_), RepoLoad::Idle | RepoLoad::Loading) => None,
    }
}

/// FEED-42: re-list when a READY list lacks the linked id and this id has
/// not been retried yet (one-shot per id).
fn should_relist(repository_id: &str, repos: &RepoLoad, relisted: Option<&str>) -> bool {
    match repos {
        RepoLoad::Ready(repos) => {
            relisted != Some(repository_id) && !repos.iter().any(|repo| repo.id == repository_id)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FEED-42: an unknown linked id re-lists exactly once; a known id, a
    /// load in flight or a failed load never do.
    #[test]
    fn unknown_linked_repo_relists_once() {
        let empty = RepoLoad::Ready(Vec::new());
        assert!(should_relist("repo-1", &empty, None));
        assert!(!should_relist("repo-1", &empty, Some("repo-1")));
        assert!(should_relist("repo-2", &empty, Some("repo-1")));
        assert!(!should_relist("repo-1", &RepoLoad::Loading, None));
        assert!(!should_relist("repo-1", &RepoLoad::Idle, None));
        assert!(!should_relist("repo-1", &RepoLoad::Failed, None));
    }

    #[test]
    fn unlinked_board_reads_no_repository_in_every_load_state() {
        for load in [
            RepoLoad::Idle,
            RepoLoad::Loading,
            RepoLoad::Ready(Vec::new()),
            RepoLoad::Failed,
        ] {
            assert_eq!(
                repo_picker_label(None, &load).as_deref(),
                Some(crate::board_form::NO_REPOSITORY)
            );
        }
    }

    /// FEED-32: a linked id the list can't resolve is never blank — loading
    /// copy while a fetch is in flight, "unavailable" once it came back
    /// (or failed) without the id.
    #[test]
    fn unknown_linked_repo_never_renders_blank() {
        assert_eq!(repo_picker_label(Some("repo-1"), &RepoLoad::Idle), None);
        assert_eq!(repo_picker_label(Some("repo-1"), &RepoLoad::Loading), None);
        assert_eq!(
            repo_picker_label(Some("repo-1"), &RepoLoad::Ready(Vec::new())).as_deref(),
            Some("Repository unavailable")
        );
        assert_eq!(
            repo_picker_label(Some("repo-1"), &RepoLoad::Failed).as_deref(),
            Some("Repository unavailable")
        );
    }
}
