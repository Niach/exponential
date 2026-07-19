//! The JetBrains-style run widget (masterplan-v3 §7.5) — a run-config
//! select + play button on the board screen's top-right toolbar.
//!
//! The select lists the active board's `runConfigs.list` (last selection
//! persisted per board in the per-account [`api::TrustStore`]); the play
//! button launches the selected config as a `TabKind::Run` tab in the bottom
//! terminal dock and **becomes a stop button** while that tab's child is
//! alive (stop = SIGTERM → §7.5 grace → SIGKILL; the §6.7 exit edge flips
//! the button back and the dock shows the exit-code strip).
//!
//! **Trust & Run (§7.3.5) — the security boundary, not a nicety.** Run
//! configs are DB-stored argv executed locally; before ANY launch the bar
//! compares `command_set_hash` over the full fetched set against the hash
//! this device last trusted for the board ([`api::TrustStore`]). Untrusted
//! (fresh device, or ANY content change — add/edit/foreign author) blocks the
//! launch behind a modal listing the exact argv/cwd/env of every config in
//! the set; confirming records the new hash and only then spawns. A broken
//! trust store fails CLOSED (treated as untrusted). Spawns are argv-direct
//! via [`coding::run_spawn_spec`] — never a shell.
//!
//! The select's trailing "Edit configurations…" opens the owner-only CRUD
//! dialog (§7.3.4 desktop parity: name + one monospace command line backed by
//! `parse_argv_line`/`format_argv_line` + cwd + env rows + reorder), driving
//! the `runConfigs` router. Members see the list read-only (server enforces;
//! the UI hides the write affordances).
//!
//! Scope rule (§7.5): the widget follows the window's navigation — a board
//! board or an issue detail resolves to that board; other screens render
//! nothing (the screens chrome additionally only mounts it on the board).
//! The clone-root rule is §7.4's: launches resolve cwd against
//! `<repos_root>/<owner>/<name>` of the board's primary repo.

use std::collections::BTreeMap;
use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, AppContext as _, ClickEvent, Entity, FontWeight, IntoElement, ParentElement,
    Render, SharedString, Styled, Subscription, WeakEntity, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    dock::{DockArea, DockItem},
    h_flex,
    input::{Input, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, WindowExt as _,
};
use sync::Store;

use api::run_configs::{command_set_hash, RunConfig, RunConfigUpdate};
use api::TrustStore;
use coding::run_launch::{
    self, cwd_error, format_argv_line, format_env_lines, parse_argv_line, parse_env_lines,
    play_state, sort_order_after_move, PlayState,
};
use terminal::{ExitHook, TabKind, TerminalManager, TerminalManagerEvent};

use crate::icons::ExpIcon;
use crate::navigation::{self, Navigation};
use crate::queries;
use crate::repo_resolver::{repo_resolver_for_window, RepoLookup, RepoResolver};
use crate::session::AuthContext;
use crate::terminal_dock::TerminalDockPanel;

enum Load {
    Idle,
    Loading,
    Ready,
}

/// Background fetch result for one board scope.
struct Fetched {
    configs: Result<Vec<RunConfig>, String>,
    persisted_selection: Option<String>,
}

pub struct RunBar {
    nav: Entity<Navigation>,
    /// The shared per-window repo resolver (§4.2) — the §7.4 clone-root repo
    /// comes from here instead of a per-bar `repositories.list` call.
    repo_resolver: Entity<RepoResolver>,
    dock_area: WeakEntity<DockArea>,
    /// The board scope the loaded state below belongs to.
    board_id: Option<String>,
    load: Load,
    configs: Vec<RunConfig>,
    error: Option<SharedString>,
    /// Primary (or sole) linked repo of the scope board — the §7.4 clone
    /// root. `None` = no repo linked (launch surfaces the link-a-repo helper).
    repo_full_name: Option<String>,
    /// Selected run-config id (persisted per board, §7.5).
    selected: Option<String>,
    /// Stale-fetch guard.
    generation: u64,
    /// This window's terminal manager (found by walking the bottom dock) +
    /// the event subscription that re-renders play↔stop on exit edges.
    manager: Option<Entity<TerminalManager>>,
    _manager_sub: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl RunBar {
    pub fn new(
        dock_area: WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let nav = navigation::nav_for_window(window, cx);
        let repo_resolver = repo_resolver_for_window(window, cx);
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            // Scope follows navigation (board/issue-detail → board).
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // The issue→board join and the default-board resolution both
            // read the synced collections.
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            // Re-render when the shared repo resolution lands / changes.
            cx.observe(&repo_resolver, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            repo_resolver,
            dock_area,
            board_id: None,
            load: Load::Idle,
            configs: Vec::new(),
            error: None,
            repo_full_name: None,
            selected: None,
            generation: 0,
            manager: None,
            _manager_sub: None,
            _subscriptions: subscriptions,
        }
    }

    /// The §7.5 scope: the window's active board (screen scope with the
    /// last-board fallback).
    fn scope_board_id(&self, cx: &App) -> Option<String> {
        navigation::active_board_id(&self.nav, cx)
    }

    /// Render-time load gate (mirror of the settings panes): scope change
    /// resets, `Idle` kicks one background fetch of configs + repo link +
    /// persisted selection.
    fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        // Drive the shared window resolver (idempotent — one fetch per
        // team, shared by all five trunk/IDE surfaces).
        self.repo_resolver
            .update(cx, |resolver, cx| resolver.ensure_loaded(cx));

        let scope = self.scope_board_id(cx);
        if scope != self.board_id {
            self.board_id = scope;
            self.load = Load::Idle;
            self.configs.clear();
            self.error = None;
            self.repo_full_name = None;
            self.selected = None;
        }
        if !matches!(self.load, Load::Idle) {
            return;
        }
        let Some(board_id) = self.board_id.clone() else {
            return;
        };
        // The §7.4 clone-root repo comes from the shared resolver; wait until it
        // has resolved (its observer re-renders us), then a missing/failed repo
        // is simply `None` (the play button surfaces the link-a-repo helper).
        let repo_full_name = match self.repo_resolver.read(cx).lookup_board(&board_id) {
            RepoLookup::Loading => return,
            RepoLookup::Found(repo) => Some(repo.full_name),
            RepoLookup::NotFound | RepoLookup::Error(_) => None,
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let Some(account) = queries::active_account(cx) else {
            return;
        };
        let data_dir = AuthContext::global(cx).data_dir.clone();

        self.load = Load::Loading;
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let board = board_id.clone();
            let fetched = cx
                .background_executor()
                .spawn(async move {
                    let configs =
                        api::run_configs::list(&trpc, &board).map_err(|err| err.to_string());
                    let persisted_selection =
                        TrustStore::open(&TrustStore::default_path(&data_dir, &account.id))
                            .ok()
                            .and_then(|store| {
                                store.selected_run_config(&board).ok().flatten()
                            });
                    Fetched {
                        configs,
                        persisted_selection,
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation
                    || this.board_id.as_deref() != Some(board_id.as_str())
                {
                    return; // superseded
                }
                match fetched.configs {
                    Ok(configs) => {
                        this.configs = configs;
                        this.error = None;
                    }
                    Err(err) => {
                        log::warn!("[ui] run bar: runConfigs.list failed: {err}");
                        this.configs = Vec::new();
                        this.error = Some(err.into());
                    }
                }
                this.repo_full_name = repo_full_name;
                // Persisted selection wins while it still exists; else the
                // first config (server order: sortOrder, name).
                this.selected = fetched
                    .persisted_selection
                    .filter(|id| this.configs.iter().any(|config| &config.id == id))
                    .or_else(|| this.configs.first().map(|config| config.id.clone()));
                this.load = Load::Ready;
                cx.notify();
            });
        })
        .detach();
    }

    /// The editor dialog calls this after any CRUD mutation — refetch on the
    /// next render (also re-arms the §7.3.5 hash comparison).
    pub(crate) fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        self.load = Load::Idle;
        cx.notify();
    }

    /// Dropdown selection: remember per board (§7.5), persisted off-thread.
    fn select(&mut self, config_id: String, cx: &mut gpui::Context<Self>) {
        if self.selected.as_deref() == Some(config_id.as_str()) {
            return;
        }
        self.selected = Some(config_id.clone());
        if let (Some(board_id), Some(account)) =
            (self.board_id.clone(), queries::active_account(cx))
        {
            let data_dir = AuthContext::global(cx).data_dir.clone();
            cx.background_executor()
                .spawn(async move {
                    let result =
                        TrustStore::open(&TrustStore::default_path(&data_dir, &account.id))
                            .and_then(|store| {
                                store.set_selected_run_config(&board_id, &config_id)
                            });
                    if let Err(err) = result {
                        log::warn!("[ui] run bar: persisting selection failed: {err}");
                    }
                })
                .detach();
        }
        cx.notify();
    }

    // -- terminal manager seam (§7.4: the run bar only *requests* spawns) --

    /// This window's [`TerminalManager`], found by walking the bottom dock to
    /// the [`TerminalDockPanel`]. Cached + subscribed (exit edges flip
    /// play↔stop).
    fn manager(&mut self, cx: &mut gpui::Context<Self>) -> Option<Entity<TerminalManager>> {
        if let Some(manager) = &self.manager {
            return Some(manager.clone());
        }
        let dock_area = self.dock_area.upgrade()?;
        let dock = dock_area.read(cx).bottom_dock().cloned()?;
        let panel = find_terminal_panel(dock.read(cx).panel())?;
        let manager = panel.read(cx).manager().clone();
        self._manager_sub = Some(cx.subscribe(
            &manager,
            |_, _, _: &TerminalManagerEvent, cx| cx.notify(),
        ));
        self.manager = Some(manager.clone());
        Some(manager)
    }

    /// `(run_config_id, is_running)` for every `Run` tab in this window —
    /// the input of the §7.5 play/stop state machine.
    fn run_tab_snapshot(&mut self, cx: &mut gpui::Context<Self>) -> Vec<(String, bool)> {
        let Some(manager) = self.manager(cx) else {
            return Vec::new();
        };
        manager
            .read(cx)
            .tabs()
            .iter()
            .filter_map(|tab| match &tab.kind {
                TabKind::Run(id) => Some((id.clone(), tab.is_running())),
                _ => None,
            })
            .collect()
    }

    // ------------------------- play / stop --------------------------------

    fn on_play_clicked(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(selected) = self.selected.clone() else {
            return;
        };
        let snapshot = self.run_tab_snapshot(cx);
        match play_state(Some(&selected), &snapshot) {
            PlayState::Stop => self.stop(&selected, cx),
            PlayState::Play => self.launch_gated(&selected, window, cx),
            PlayState::Disabled => {}
        }
    }

    /// §7.3.5: the trust gate in front of EVERY spawn. Fail closed — a
    /// broken/unreadable trust store means "untrusted", never "run anyway".
    fn launch_gated(
        &mut self,
        config_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(board_id) = self.board_id.clone() else {
            return;
        };
        let Some(config) = self
            .configs
            .iter()
            .find(|config| config.id == config_id)
            .cloned()
        else {
            return;
        };
        let Some(account) = queries::active_account(cx) else {
            return;
        };
        let data_dir = AuthContext::global(cx).data_dir.clone();
        let hash = command_set_hash(&self.configs);
        let device = api::device_id(&data_dir);
        let store_path = TrustStore::default_path(&data_dir, &account.id);

        let trusted = TrustStore::open(&store_path)
            .and_then(|store| store.is_trusted(&device, &board_id, &hash))
            .unwrap_or(false);
        if trusted {
            if let Err(message) = self.spawn_run_tab(&config, cx) {
                window.push_notification(Notification::error(message), cx);
            }
        } else {
            self.open_trust_dialog(config, board_id, hash, device, store_path, window, cx);
        }
    }

    /// The Trust & Run modal (§7.3.5): the exact argv/cwd/env of the FULL
    /// fetched set (the hash covers the set), explicit confirm required.
    #[allow(clippy::too_many_arguments)]
    fn open_trust_dialog(
        &mut self,
        config: RunConfig,
        board_id: String,
        hash: String,
        device_id: String,
        store_path: PathBuf,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let configs = self.configs.clone();
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let configs = configs.clone();
            let config = config.clone();
            let board_id = board_id.clone();
            let hash = hash.clone();
            let device_id = device_id.clone();
            let store_path = store_path.clone();
            let this = this.clone();
            dialog
                .title("Trust & Run")
                .w(px(560.))
                .content(move |content, _, cx| {
                    content.child(trust_dialog_body(&configs, cx))
                })
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("Trust & Run")
                        .ok_variant(ButtonVariant::Primary)
                        .show_cancel(true)
                        .on_ok(move |_, window, cx| {
                            let record = TrustStore::open(&store_path).and_then(|store| {
                                store.trust(&device_id, &board_id, &hash)
                            });
                            if let Err(err) = record {
                                // Still run (the user JUST reviewed the set),
                                // but the next launch will re-prompt.
                                log::warn!("[ui] run bar: recording trust failed: {err}");
                            }
                            if let Some(this) = this.upgrade() {
                                let result = this
                                    .update(cx, |this, cx| this.spawn_run_tab(&config, cx));
                                if let Err(message) = result {
                                    window.push_notification(
                                        Notification::error(message),
                                        cx,
                                    );
                                }
                            }
                            true
                        }),
                )
        });
    }

    /// §7.4: build the argv-direct SpawnSpec and hand it to this window's
    /// manager as a `Run` tab (the dock expands + focuses on `TabOpened`).
    /// An exited tab for the same config is closed first — §7.5 "re-uses/
    /// re-runs its existing tab".
    fn spawn_run_tab(
        &mut self,
        config: &RunConfig,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), SharedString> {
        let Some(repo) = self.repo_full_name.clone() else {
            return Err(
                "Link a repository to this board in team settings to run configurations."
                    .into(),
            );
        };
        let data_dir = AuthContext::global(cx).data_dir.clone();
        let settings = coding::Settings::load(&coding::Settings::default_path(&data_dir));
        let root = run_launch::run_root(&settings.repos_root_path(), &repo);
        if !root.is_dir() {
            return Err(SharedString::from(format!(
                "Repository not cloned yet — start coding once to clone {repo}, or clone it to {}.",
                root.display()
            )));
        }
        let spec = run_launch::run_spawn_spec(config, &root)
            .map_err(SharedString::from)?;
        let manager = self
            .manager(cx)
            .ok_or_else(|| SharedString::from("Terminal dock is not available."))?;

        let config_id = config.id.clone();
        let title = config.name.clone();
        manager
            .update(cx, |manager, cx| {
                if let Some(existing) = manager
                    .tabs()
                    .iter()
                    .find(|tab| tab.kind == TabKind::Run(config_id.clone()) && !tab.is_running())
                {
                    let id = existing.id;
                    manager.close_tab(id, cx);
                }
                manager.open_tab(TabKind::Run(config_id.clone()), title, None, &spec, None, cx)
            })
            .map(|_| ())
            .map_err(|err| SharedString::from(format!("Launch failed: {err}")))
    }

    /// §7.5 stop: SIGTERM the tab's child now, SIGKILL after the grace period
    /// if it is still alive. The §6.7 exit edge flips the button back and
    /// paints the exit-code strip.
    fn stop(&mut self, config_id: &str, cx: &mut gpui::Context<Self>) {
        let Some(manager) = self.manager(cx) else {
            return;
        };
        let tab = manager.read(cx).tabs().iter().find_map(|tab| {
            (tab.kind == TabKind::Run(config_id.to_string()) && tab.is_running())
                .then(|| (tab.id, tab.view.clone()))
        });
        let Some((tab_id, view)) = tab else {
            return;
        };
        let Some(pid) = view.read(cx).session().borrow().process_id() else {
            // No pid to signal — fall back to the manager's kill path.
            view.read(cx).session().borrow().kill();
            return;
        };
        run_launch::terminate(pid);
        let manager = manager.downgrade();
        cx.spawn(async move |_, cx| {
            cx.background_executor().timer(run_launch::STOP_GRACE).await;
            cx.update(|cx| {
                let Some(manager) = manager.upgrade() else {
                    return;
                };
                let still_running = manager
                    .read(cx)
                    .tab(tab_id)
                    .is_some_and(|tab| tab.is_running());
                if still_running {
                    run_launch::force_kill(pid);
                }
            });
        })
        .detach();
    }

    // --------------------- create configs with Claude --------------------

    /// §7.3 / L24: the ONE MCP-enabled Claude task. Spawns
    /// `coding::claude_task` at the board's trunk clone with a **scoped
    /// `.exp-mcp.json`** (the same expu_ key as a coding session), so Claude
    /// can inspect the repo and create run configs via the
    /// `exponential_run_configs_*` MCP tools. No worktree, no
    /// `coding_sessions` row — the tab is a plain `ClaudeTask`. The
    /// `.exp-mcp.json` is git-excluded, `0600`, and removed on task exit; the
    /// run bar refetches its configs at that point too.
    fn create_configs_with_claude(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(board_id) = self.board_id.clone() else {
            return;
        };
        let Some(repo) = self.repo_full_name.clone() else {
            window.push_notification(
                Notification::error(
                    "Link a repository to this board in team settings first.",
                ),
                cx,
            );
            return;
        };
        let Some(account) = queries::active_account(cx) else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let Some(manager) = self.manager(cx) else {
            window.push_notification(Notification::error("Terminal dock is not available."), cx);
            return;
        };
        let data_dir = AuthContext::global(cx).data_dir.clone();
        let settings = coding::Settings::load(&coding::Settings::default_path(&data_dir));
        let root = run_launch::run_root(&settings.repos_root_path(), &repo);
        if !root.is_dir() {
            window.push_notification(
                Notification::error(SharedString::from(format!(
                    "Repository not cloned yet — start coding once to clone {repo}.",
                ))),
                cx,
            );
            return;
        }
        // Only reclaim a `.exp-mcp.json` we created (never clobber-then-delete
        // a file the repo already carried).
        let created_marker = !root.join(coding::MCP_JSON_FILE).exists();
        cx.spawn(async move |this, cx| {
            let prep_root = root.clone();
            let prep = cx
                .background_executor()
                .spawn(async move {
                    let store = api::token_store::TokenStore::new(data_dir);
                    let key = api::users::ensure_personal_key(&trpc, &store, &account.id)
                        .map_err(|err| err.to_string())?;
                    coding::write_mcp_json(&prep_root, trpc.base_url(), &key)
                        .map_err(|err| format!("write .exp-mcp.json: {err}"))?;
                    // Token-leak guard (the file carries the raw expu_ key).
                    let _ = coding::git_worktree::ensure_local_excludes(
                        &prep_root,
                        &[coding::MCP_JSON_FILE],
                    );
                    Ok::<(), String>(())
                })
                .await;
            let _ = this.update_in(cx, |_this, window, cx| {
                if let Err(message) = prep {
                    window.push_notification(Notification::error(SharedString::from(message)), cx);
                    return;
                }
                let prompt = coding::create_run_configs_prompt(&board_id);
                let task =
                    coding::claude_task_with_mcp(&settings, &root, &prompt, "Create run configs");
                let cleanup_path = created_marker.then(|| root.join(coding::MCP_JSON_FILE));
                let run_bar = cx.entity().downgrade();
                let on_exit: ExitHook = Box::new(move |_id, _exit, cx| {
                    if let Some(path) = cleanup_path {
                        let _ = std::fs::remove_file(path);
                    }
                    if let Some(run_bar) = run_bar.upgrade() {
                        run_bar.update(cx, |run_bar, cx| run_bar.mark_stale(cx));
                    }
                });
                let result = manager.update(cx, |manager, cx| {
                    manager.open_tab(
                        TabKind::ClaudeTask,
                        task.tab_title.clone(),
                        None,
                        &task.spawn,
                        Some(on_exit),
                        cx,
                    )
                });
                match result {
                    Ok(tab_id) => {
                        // Claude creates configs through the server while its
                        // interactive session stays open — the exit hook alone
                        // would only refresh after the tab dies. Poll the list
                        // every few seconds while the task is alive so new
                        // configs appear in the select immediately.
                        let manager = manager.downgrade();
                        let this = cx.entity().downgrade();
                        cx.spawn(async move |_, cx| {
                            loop {
                                cx.background_executor()
                                    .timer(std::time::Duration::from_secs(3))
                                    .await;
                                let alive = cx.update(|cx| {
                                    let Some(this) = this.upgrade() else {
                                        return false;
                                    };
                                    this.update(cx, |this, cx| this.mark_stale(cx));
                                    manager.upgrade().is_some_and(|manager| {
                                        manager
                                            .read(cx)
                                            .tab(tab_id)
                                            .is_some_and(|tab| tab.is_running())
                                    })
                                });
                                if !alive {
                                    break;
                                }
                            }
                        })
                        .detach();
                    }
                    Err(err) => {
                        // The tab never opened, so its exit hook won't run —
                        // clean up the just-written key file here.
                        if created_marker {
                            let _ = std::fs::remove_file(root.join(coding::MCP_JSON_FILE));
                        }
                        window.push_notification(
                            Notification::error(SharedString::from(format!(
                                "Could not start Claude: {err}"
                            ))),
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }

    // ------------------------------ render --------------------------------

    fn open_editor(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(board_id) = self.board_id.clone() else {
            return;
        };
        run_configs_editor::open(board_id, cx.entity().downgrade(), window, cx);
    }

    /// The JetBrains-style run-config select: current selection as the label,
    /// the config list + trailing "Edit configurations…" in the dropdown.
    fn render_dropdown(&mut self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let selected_name: Option<SharedString> = self
            .selected
            .as_ref()
            .and_then(|id| self.configs.iter().find(|config| &config.id == id))
            .map(|config| SharedString::from(config.name.clone()));
        let placeholder: SharedString = if matches!(self.load, Load::Ready) {
            if self.error.is_some() {
                "Run configs unavailable".into()
            } else {
                "No run configurations".into()
            }
        } else {
            "Loading…".into()
        };
        let label = selected_name.unwrap_or(placeholder);
        let configs: Vec<(String, SharedString)> = self
            .configs
            .iter()
            .map(|config| (config.id.clone(), SharedString::from(config.name.clone())))
            .collect();
        let selected = self.selected.clone();
        let this = cx.entity().downgrade();

        Button::new("run-config-select")
            .ghost()
            .xsmall()
            .label(label)
            .dropdown_caret(true)
            .dropdown_menu(move |mut menu, _window, _cx| {
                for (id, name) in &configs {
                    let id = id.clone();
                    let this = this.clone();
                    menu = menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(selected.as_deref() == Some(id.as_str()))
                            .on_click(move |_, _, cx| {
                                let id = id.clone();
                                if let Some(this) = this.upgrade() {
                                    this.update(cx, |this, cx| this.select(id, cx));
                                }
                            }),
                    );
                }
                if !configs.is_empty() {
                    menu = menu.item(PopupMenuItem::separator());
                }
                let this = this.clone();
                menu.item(
                    PopupMenuItem::new("Edit configurations…").on_click(move |_, window, cx| {
                        if let Some(this) = this.upgrade() {
                            this.update(cx, |this, cx| this.open_editor(window, cx));
                        }
                    }),
                )
            })
    }

    fn render_play_button(
        &mut self,
        state: PlayState,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let (icon, tooltip): (Icon, SharedString) = match state {
            PlayState::Stop => (
                Icon::from(ExpIcon::Square),
                "Stop (SIGTERM, then SIGKILL)".into(),
            ),
            _ => (Icon::new(IconName::Play), "Run selected configuration".into()),
        };
        let color = match state {
            PlayState::Stop => cx.theme().danger,
            PlayState::Play => cx.theme().success,
            PlayState::Disabled => cx.theme().muted_foreground,
        };
        Button::new("run-config-play")
            .ghost()
            .xsmall()
            .icon(icon.text_color(color))
            .tooltip(tooltip)
            .disabled(matches!(state, PlayState::Disabled))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.on_play_clicked(window, cx);
            }))
    }
}

impl Render for RunBar {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_loaded(cx);

        // Run controls only when a board scope resolves (the screens chrome
        // additionally mounts this widget on the board only).
        if self.board_id.is_none() {
            return div().into_any_element();
        }
        let snapshot = self.run_tab_snapshot(cx);
        let state = play_state(self.selected.as_deref(), &snapshot);
        h_flex()
            .gap_1()
            .items_center()
            .child(self.render_dropdown(cx))
            .child(self.render_play_button(state, cx))
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Walk a `DockItem` tree to the window's [`TerminalDockPanel`].
fn find_terminal_panel(item: &DockItem) -> Option<Entity<TerminalDockPanel>> {
    match item {
        DockItem::Tabs { items, .. } => items
            .iter()
            .find_map(|panel| panel.view().downcast::<TerminalDockPanel>().ok()),
        DockItem::Panel { view, .. } => view.view().downcast::<TerminalDockPanel>().ok(),
        DockItem::Split { items, .. } => items.iter().find_map(find_terminal_panel),
        _ => None,
    }
}

/// The Trust & Run dialog body: warning + the exact argv/cwd/env of every
/// config in the fetched set (§7.3.5 "shows the exact argv/cwd/env").
fn trust_dialog_body(configs: &[RunConfig], cx: &App) -> impl IntoElement {
    let mono = cx.theme().mono_font_family.clone();
    v_flex()
        .gap_3()
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(
                    "Run configurations are stored in the team and will execute \
                     as local processes on this machine. Review the exact commands \
                     below — trusting runs them and remembers this set on this device; \
                     any change will ask again.",
                ),
        )
        .children(configs.iter().map(|config| {
            let mono = mono.clone();
            v_flex()
                .gap_0p5()
                .p_2()
                .rounded(px(4.))
                .border_1()
                .border_color(cx.theme().border)
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(config.name.clone())),
                )
                .child(
                    div()
                        .text_xs()
                        .font_family(mono.clone())
                        .child(SharedString::from(format_argv_line(&config.argv))),
                )
                .when_some(config.cwd.clone(), |this, cwd| {
                    this.child(
                        div()
                            .text_xs()
                            .font_family(mono.clone())
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(format!("cwd: {cwd}"))),
                    )
                })
                .when(!config.env.is_empty(), |this| {
                    this.child(
                        div()
                            .text_xs()
                            .font_family(mono.clone())
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(
                                config
                                    .env
                                    .iter()
                                    .map(|(key, value)| format!("{key}={value}"))
                                    .collect::<Vec<_>>()
                                    .join(" "),
                            )),
                    )
                })
        }))
}

// ---------------------------------------------------------------------------
// §7.3.4 desktop editor dialog — "Edit configurations…"
// ---------------------------------------------------------------------------

mod run_configs_editor {
    use super::*;

    /// Open the CRUD dialog for `board_id`. Owner-only for writes (server
    /// enforces; the UI hides write affordances for members).
    pub(super) fn open(
        board_id: String,
        run_bar: WeakEntity<RunBar>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let view = cx.new(|cx| Editor::new(board_id, run_bar, window, cx));
        window.open_dialog(cx, move |dialog, window, _| {
            let height = (window.viewport_size().height * 0.85).min(px(520.));
            dialog
                .title("Run configurations")
                .w(px(640.))
                .h(height)
                .overlay_closable(true)
                .button_props(DialogButtonProps::default().ok_text("Done"))
                .child(view.clone())
        });
    }

    enum EditTarget {
        New,
        Existing(String),
    }

    /// A validated run-config form — `(name, argv, cwd, env)` (§7.3.3). Aliased
    /// so `read_form`'s return type stays under clippy's `type_complexity` bar.
    type ValidatedForm = (String, Vec<String>, Option<String>, BTreeMap<String, String>);

    pub(super) struct Editor {
        board_id: String,
        run_bar: WeakEntity<RunBar>,
        configs: Vec<RunConfig>,
        loaded: bool,
        is_owner: bool,
        editing: Option<EditTarget>,
        name: Entity<InputState>,
        command: Entity<InputState>,
        cwd: Entity<InputState>,
        env: Entity<InputState>,
        busy: bool,
        error: Option<SharedString>,
        generation: u64,
    }

    impl Editor {
        fn new(
            board_id: String,
            run_bar: WeakEntity<RunBar>,
            window: &mut Window,
            cx: &mut gpui::Context<Self>,
        ) -> Self {
            let name = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
            let command = cx.new(|cx| {
                InputState::new(window, cx).placeholder("bun run dev --port 5173")
            });
            let cwd = cx.new(|cx| {
                InputState::new(window, cx).placeholder("Working directory (repo-relative, optional)")
            });
            let env = cx.new(|cx| {
                InputState::new(window, cx)
                    .multi_line(true)
                    .rows(3)
                    .placeholder("Environment — one KEY=value per line (optional)")
            });
            // Owner gate (web §7.3.4: owner-only editor) from the synced
            // membership rows.
            let is_owner = Store::global(cx)
                .collections()
                .boards
                .read(cx)
                .get(&board_id)
                .map(|board| board.team_id.clone())
                .is_some_and(|team_id| is_team_owner(cx, &team_id));

            let mut editor = Self {
                board_id,
                run_bar,
                configs: Vec::new(),
                loaded: false,
                is_owner,
                editing: None,
                name,
                command,
                cwd,
                env,
                busy: false,
                error: None,
                generation: 0,
            };
            editor.reload(cx);
            editor
        }

        /// Refetch the server list (source of truth after every mutation) and
        /// mark the run bar stale so its dropdown + trust hash follow.
        fn reload(&mut self, cx: &mut gpui::Context<Self>) {
            let Some(trpc) = queries::trpc_client(cx) else {
                return;
            };
            let board_id = self.board_id.clone();
            self.generation += 1;
            let generation = self.generation;
            let run_bar = self.run_bar.clone();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api::run_configs::list(&trpc, &board_id) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.generation != generation {
                        return;
                    }
                    match result {
                        Ok(configs) => {
                            this.configs = configs;
                            this.loaded = true;
                        }
                        Err(err) => this.error = Some(format!("{err}").into()),
                    }
                    this.busy = false;
                    cx.notify();
                });
                cx.update(|cx| {
                    if let Some(run_bar) = run_bar.upgrade() {
                        run_bar.update(cx, |run_bar, cx| run_bar.mark_stale(cx));
                    }
                });
            })
            .detach();
        }

        fn start_create(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
            self.editing = Some(EditTarget::New);
            self.error = None;
            self.set_form("", "", "", "", window, cx);
        }

        fn start_edit(&mut self, config_id: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
            let Some(config) = self.configs.iter().find(|c| c.id == config_id).cloned() else {
                return;
            };
            self.editing = Some(EditTarget::Existing(config.id.clone()));
            self.error = None;
            self.set_form(
                &config.name,
                &format_argv_line(&config.argv),
                config.cwd.as_deref().unwrap_or(""),
                &format_env_lines(&config.env),
                window,
                cx,
            );
        }

        fn set_form(
            &mut self,
            name: &str,
            command: &str,
            cwd: &str,
            env: &str,
            window: &mut Window,
            cx: &mut gpui::Context<Self>,
        ) {
            let values = [
                (&self.name, name),
                (&self.command, command),
                (&self.cwd, cwd),
                (&self.env, env),
            ];
            for (input, value) in values {
                let value = value.to_string();
                input.update(cx, |state, cx| state.set_value(value, window, cx));
            }
            cx.notify();
        }

        /// Validate the form into `(name, argv, cwd, env)` using the same
        /// pure rules the server applies (§7.3.3).
        fn read_form(&self, cx: &App) -> Result<ValidatedForm, String> {
            let name = self.name.read(cx).value().trim().to_string();
            if name.is_empty() {
                return Err("Name is required".to_string());
            }
            let argv = parse_argv_line(self.command.read(cx).value().as_ref());
            if argv.first().map(|p| p.trim().is_empty()).unwrap_or(true) {
                return Err("Command is required".to_string());
            }
            let cwd_raw = self.cwd.read(cx).value().trim().to_string();
            let cwd = if cwd_raw.is_empty() {
                None
            } else {
                if let Some(error) = cwd_error(&cwd_raw) {
                    return Err(error.to_string());
                }
                Some(cwd_raw)
            };
            let env = parse_env_lines(self.env.read(cx).value().as_ref())?;
            Ok((name, argv, cwd, env))
        }

        fn save(&mut self, cx: &mut gpui::Context<Self>) {
            if self.busy {
                return;
            }
            let (name, argv, cwd, env) = match self.read_form(cx) {
                Ok(parsed) => parsed,
                Err(message) => {
                    self.error = Some(message.into());
                    cx.notify();
                    return;
                }
            };
            let Some(trpc) = queries::trpc_client(cx) else {
                return;
            };
            let board_id = self.board_id.clone();
            let target = match &self.editing {
                Some(EditTarget::Existing(id)) => Some(id.clone()),
                _ => None,
            };
            self.busy = true;
            self.error = None;
            cx.notify();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        match target {
                            Some(id) => {
                                let mut input = RunConfigUpdate::new(id);
                                input.name = Some(name);
                                input.argv = Some(argv);
                                input.cwd = api::Patch::set_or_null(cwd);
                                input.env = Some(env);
                                api::run_configs::update(&trpc, &input).map(|_| ())
                            }
                            None => api::run_configs::create(
                                &trpc,
                                &board_id,
                                &name,
                                &argv,
                                cwd.as_deref(),
                                Some(&env),
                            )
                            .map(|_| ()),
                        }
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    match result {
                        Ok(()) => {
                            this.editing = None;
                            this.reload(cx);
                        }
                        Err(err) => {
                            this.busy = false;
                            this.error = Some(format!("{err}").into());
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }

        fn delete(&mut self, config_id: String, cx: &mut gpui::Context<Self>) {
            if self.busy {
                return;
            }
            let Some(trpc) = queries::trpc_client(cx) else {
                return;
            };
            self.busy = true;
            cx.notify();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api::run_configs::delete(&trpc, &config_id) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    match result {
                        Ok(()) => this.reload(cx),
                        Err(err) => {
                            this.busy = false;
                            this.error = Some(format!("{err}").into());
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }

        /// §7.3.2 reorder: one `update.sortOrder` with the fractional index
        /// for the move. `delta` = ±1 row.
        fn move_config(&mut self, index: usize, delta: isize, cx: &mut gpui::Context<Self>) {
            if self.busy {
                return;
            }
            let to = index as isize + delta;
            if to < 0 || to as usize >= self.configs.len() {
                return;
            }
            let orders: Vec<f64> = self.configs.iter().map(|c| c.sort_order).collect();
            let Some(new_order) = sort_order_after_move(&orders, index, to as usize) else {
                return;
            };
            let Some(trpc) = queries::trpc_client(cx) else {
                return;
            };
            let id = self.configs[index].id.clone();
            self.busy = true;
            cx.notify();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let mut input = RunConfigUpdate::new(id);
                        input.sort_order = Some(new_order);
                        api::run_configs::update(&trpc, &input).map(|_| ())
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    match result {
                        Ok(()) => this.reload(cx),
                        Err(err) => {
                            this.busy = false;
                            this.error = Some(format!("{err}").into());
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }

        fn render_row(
            &self,
            index: usize,
            config: &RunConfig,
            cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            let mono = cx.theme().mono_font_family.clone();
            let id = config.id.clone();
            let id_for_edit = id.clone();
            let id_for_delete = id.clone();
            h_flex()
                .gap_2()
                .px_2()
                .py_1()
                .items_center()
                .rounded(px(4.))
                .border_1()
                .border_color(cx.theme().border)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(SharedString::from(config.name.clone())),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_family(mono)
                                .text_color(cx.theme().muted_foreground)
                                .truncate()
                                .child(SharedString::from(format_argv_line(&config.argv))),
                        ),
                )
                .when(self.is_owner, |this| {
                    this.child(
                        Button::new(("run-config-up", index))
                            .ghost()
                            .xsmall()
                            .icon(IconName::ArrowUp)
                            .disabled(self.busy || index == 0)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.move_config(index, -1, cx);
                            })),
                    )
                    .child(
                        Button::new(("run-config-down", index))
                            .ghost()
                            .xsmall()
                            .icon(IconName::ArrowDown)
                            .disabled(self.busy || index + 1 == self.configs.len())
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.move_config(index, 1, cx);
                            })),
                    )
                    .child(
                        Button::new(("run-config-edit", index))
                            .ghost()
                            .xsmall()
                            .icon(Icon::from(ExpIcon::Pencil))
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.start_edit(&id_for_edit, window, cx);
                            })),
                    )
                    .child(
                        Button::new(("run-config-delete", index))
                            .ghost()
                            .xsmall()
                            .icon(
                                Icon::new(IconName::Close)
                                    .text_color(cx.theme().danger),
                            )
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.delete(id_for_delete.clone(), cx);
                            })),
                    )
                })
        }

        fn render_form(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
            let mono = cx.theme().mono_font_family.clone();
            v_flex()
                .gap_2()
                .p_2()
                .rounded(px(4.))
                .border_1()
                .border_color(cx.theme().border)
                .child(Input::new(&self.name).small())
                .child(
                    div()
                        .font_family(mono)
                        .child(Input::new(&self.command).small()),
                )
                .child(Input::new(&self.cwd).small())
                .child(Input::new(&self.env).small())
                .child(
                    h_flex()
                        .gap_2()
                        .justify_end()
                        .child(
                            Button::new("run-config-cancel")
                                .ghost()
                                .small()
                                .label("Cancel")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.editing = None;
                                    this.error = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("run-config-save")
                                .primary()
                                .small()
                                .label(match self.editing {
                                    Some(EditTarget::New) => "Create",
                                    _ => "Save",
                                })
                                .loading(self.busy)
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.save(cx);
                                })),
                        ),
                )
        }
    }

    impl Render for Editor {
        fn render(
            &mut self,
            _window: &mut Window,
            cx: &mut gpui::Context<Self>,
        ) -> impl IntoElement {
            let rows: Vec<_> = self
                .configs
                .iter()
                .cloned()
                .enumerate()
                .collect();

            v_flex()
                .gap_2()
                .size_full()
                .overflow_hidden()
                .when(!self.loaded, |this| {
                    this.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Loading…"),
                    )
                })
                .when(self.loaded && rows.is_empty(), |this| {
                    this.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(if self.is_owner {
                                "No run configurations yet — add one to launch it from the board's run widget."
                            } else {
                                "No run configurations yet. Team owners can add them."
                            }),
                    )
                })
                .children(
                    rows.iter()
                        .map(|(index, config)| self.render_row(*index, config, cx)),
                )
                .when_some(self.error.clone(), |this, error| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().danger)
                            .child(error),
                    )
                })
                .when(self.editing.is_some(), |this| this.child(self.render_form(cx)))
                .when(self.editing.is_none() && self.is_owner, |this| {
                    this.child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("run-config-add")
                                    .outline()
                                    .small()
                                    .icon(IconName::Plus)
                                    .label("Add configuration")
                                    .disabled(self.busy || !self.loaded)
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        this.start_create(window, cx);
                                    })),
                            )
                            .child(
                                // L24: hand the empty/unsure board to Claude —
                                // it inspects the repo and creates configs via the
                                // run-config MCP tools (the ONE MCP-enabled task).
                                Button::new("run-config-claude")
                                    .ghost()
                                    .small()
                                    .icon(Icon::from(ExpIcon::Sparkles))
                                    .label("Create with Claude")
                                    .disabled(self.busy || !self.loaded)
                                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                        if let Some(run_bar) = this.run_bar.upgrade() {
                                            run_bar.update(cx, |run_bar, cx| {
                                                run_bar.create_configs_with_claude(window, cx);
                                            });
                                        }
                                        window.close_dialog(cx);
                                    })),
                            ),
                    )
                })
        }
    }

    /// `currentMember?.role === 'owner'` over the synced membership rows
    /// (same rule as the settings panes).
    fn is_team_owner(cx: &App, team_id: &str) -> bool {
        let Some(me) = queries::active_account(cx) else {
            return false;
        };
        Store::global(cx)
            .collections()
            .team_members
            .read(cx)
            .iter()
            .any(|member| {
                member.team_id == team_id
                    && member.user_id == me.user_id
                    && member.role.as_deref() == Some(domain::contract::TEAM_ROLE_OWNER)
            })
    }
}
