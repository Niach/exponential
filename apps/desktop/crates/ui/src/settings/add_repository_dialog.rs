//! Settings → Repositories → "Add repository" (EXP-329, SLOP-7 one flow).
//!
//! Web parity: `components/github-repo-picker.tsx`. The desktop's direct
//! connect surface: it lists the repositories the VIEWER can push to, LIVE
//! off GitHub (`integrations.github.repos`), and `repositories.add` shares
//! the picked one with the team (tap adds, ×4).
//!
//! When a prerequisite is missing it says which one and offers the ONE fix —
//! not linked → Connect GitHub, expired → Reconnect GitHub (both the in-app
//! link hop, [`crate::github_connect::connect_github`]), not installed →
//! Install the app (GitHub's install page in the browser) — plus the
//! "I've done that" re-list. Everything reads the live server truth through
//! [`crate::github_connect::fetch_github_repos`]. The hops finish OUTSIDE the
//! app: the `oauth-return?linked=github` and `github-connected` deep links
//! re-list definitively, and this window regaining focus while the list is
//! unusable re-fetches with `refresh` as the fallback (the server caches
//! discovery per user for a minute).
//!
//! #EXP-1616: owners first pick the provider (`ChoiceTiles` GitHub |
//! Exponential, repo-page.json `copy.add`); Exponential is the create form
//! (name, default branch, the optional GitHub mirror) over
//! `repositories.create`; GitHub is the picker below, unchanged.
//!
//! #EXP-1629: the content is a document on the native binding (projection at
//! `/pane`); the window around it stays native. #EXP-1787: the lookup field
//! is a genUI `box` Input whose typed text takes the footer's muted ink
//! (`style.color`).

use std::rc::Rc;

use eui_gpui::{binding::Binding, view::SurfaceView};
use gpui::{div, px, size, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, WeakEntity, Window};
use gpui_component::{ActiveTheme as _, ElementExt as _};
use serde_json::{json, Value};

use crate::github_connect::{
    copy, fetch_github_repos, is_repo_full_name, lookup_repo, GithubRepo, GithubReposResult,
    Prerequisite,
};
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;

use super::api_keys::premultiplied_hex;
use super::doc::{failed_state, hex, mount_with, path, IdeToasts, Ink, PillPaint, SM, XS};
use super::repositories::RepositoriesPane;
use super::{open_url, row_stroke};

enum Load {
    Loading,
    Ready(GithubReposResult),
    Failed,
}

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) struct AddRepoShot {
    /// `None` = the first load; `Some(None)` = the failed load.
    pub load: Option<Option<GithubReposResult>>,
    pub query: &'static str,
    pub lookup: &'static str,
    pub lookup_busy: bool,
    pub lookup_error: Option<&'static str>,
    pub plan_limited: Option<&'static str>,
}

/// #EXP-1616: the Exponential form in a pinned state (`settings_bench`).
#[cfg(test)]
pub(crate) struct EcodeShot {
    pub name: &'static str,
    pub branch: &'static str,
    pub mirror: bool,
    pub mirror_url: &'static str,
}

/// The repo row's height (`py_1p5` around a `text_sm` line): the first
/// load's skeleton rows take it.
const REPO_ROW_H: f32 = 32.;

/// Open the dialog over the Repositories pane. `pane` is refreshed after a
/// successful add — `repositories.list` is a server read with no Electric
/// echo, so the new row only appears on a refetch.
pub(super) fn open(
    window: &mut Window,
    cx: &mut App,
    team_id: String,
    owner: bool,
    pane: WeakEntity<RepositoriesPane>,
) {
    // Same width as the other settings dialogs; the list owns the height, so
    // cap against the opener's viewport and let it scroll inside.
    let height = (window.viewport_size().height * 0.85).min(px(480.));
    let spec = DialogSpec::new(copy::PICKER_TITLE, size(px(416.), height));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| AddRepositoryDialogView::new(team_id, owner, pane, window, cx));
        let busy = view.clone();
        DialogContent::new(view)
            // The search input stays pinned while only the list scrolls.
            .self_scrolling()
            .can_close(move |cx| !busy.read(cx).adding)
    });
}

/// The picker when its list failed to load (FEED-79: the empty state +
/// Retry).
const NO_REPOSITORIES: &str = "No repositories";

pub struct AddRepositoryDialogView {
    team_id: String,
    /// #EXP-1616: owners pick the provider; members only connect GitHub.
    owner: bool,
    /// `github` | `ecode`.
    kind: String,
    /// The Exponential form: name, default branch, mirror on + its URL.
    ecode_name: String,
    ecode_branch: String,
    ecode_mirror: bool,
    ecode_mirror_url: String,
    creating: bool,
    /// The pane to refetch once a repo is connected.
    pane: WeakEntity<RepositoriesPane>,
    query: String,
    load: Load,
    /// Monotonic guard: a stale in-flight fetch must not clobber a newer one.
    generation: u64,
    adding: bool,
    /// FEED-79: the load in flight is the person's Refresh / Retry press
    /// (its failure toasts); the load on open, on window activation and
    /// after a connect hand-back fails quietly into the empty state.
    retry_pressed: bool,
    /// The add failed a plan cap — the server's own message, rendered with
    /// the neutral "Upgrade on the web" pointer (§4.9).
    plan_limited: Option<SharedString>,
    /// FEED-30: the footer's "Add by name" escape hatch — `owner/name`, looked
    /// up through `integrations.github.lookupRepo` (the connect path's own
    /// checks) and, on a hit, added exactly like a row pick.
    /// The lookup field's text (its bound value, like the search's `query`).
    lookup: String,
    lookup_busy: bool,
    /// The server's reason the typed name was refused — about the INPUT, so
    /// it stays under the field (FEED-79); offline is a toast instead.
    lookup_error: Option<SharedString>,
    /// The search field has the focus: its box wears the active stroke.
    query_focused: bool,
    /// The search takes the focus once, the first time it shows.
    focused_once: bool,
    /// The search field is to take the focus once the document has drawn
    /// it (it exists only after the paint that first shows it, so it is
    /// retried after every render).
    want_focus: bool,
    focus_tries: u8,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    chrome: Chrome,
    _subscriptions: Vec<Subscription>,
}

/// Renders a wanted focus is retried for.
const FOCUS_TRIES: u8 = 4;
/// The search `Input`'s text field (its `<id>.field` part).
const QUERY_FIELD: &str = "query.field";
/// The lookup `Input`'s text field (its `<id>.field` part).
const LOOKUP_FIELD: &str = "lookup.field";

impl AddRepositoryDialogView {
    fn new(
        team_id: String,
        owner: bool,
        pane: WeakEntity<RepositoriesPane>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        // Every action leaves the binding for the view (a pick's add and the
        // field writes need its window).
        let me = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new("/pane", Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["kind", "ecode_branch", "ecode_mirror", "create", "cancel", "edit", "focus_search", "clear", "lookup", "pick", "fix", "refresh", "recheck", "retry", "open"] {
                let me = me.clone();
                binding.on_action(name, move |_, event, cx| {
                    let context = |key: &str| event.context.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
                    let (field, key) = (context("field"), context("key"));
                    let value = event.payload.as_ref().and_then(|p| p.get("value").and_then(Value::as_str).map(str::to_string).or_else(|| p.get("checked").and_then(Value::as_bool).map(|on| on.to_string())));
                    let me = me.clone();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = me.update(cx, |view, cx| view.action(name, &field, &key, value, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let chrome = Chrome::of(cx);
        let plugin = Rc::new(super::doc::ide_plugin());
        let view = mount_with("add-repository", document(&Ink::of(cx), &chrome), &binding, plugin, true, window, cx);
        // The content owns the window's height: the rows scroll, the rest
        // stays (`DialogContent::self_scrolling`).
        view.update(cx, |view, cx| view.set_fill_height(true, cx));
        let subscriptions = vec![
            // Whether the search holds the focus: its box's stroke follows
            // it (the document fires no `focus` event).
            cx.observe(&view, |this, view, cx| {
                let surface = view.read(cx);
                let focused = surface.focused().is_some_and(|index| Some(index) == surface.index_of(QUERY_FIELD));
                if focused != this.query_focused {
                    this.query_focused = focused;
                    cx.notify();
                }
            }),
            // The connect/install hand-off completes in the browser — coming
            // back to this window refetches as a heuristic fallback; since
            // EXP-368 the definitive signal is the github-connected deep
            // link below.
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.refetch_after_connect(cx);
                }
            }),
            // EXP-368: the `exponential://github-connected` hand-back —
            // unconditional refresh on success (unlike the heuristic above,
            // the deep link is definitive), failure copy otherwise.
            cx.observe_global::<crate::github_connect::GithubConnectSignal>(|this, cx| {
                let Some(outcome) = cx
                    .try_global::<crate::github_connect::GithubConnectSignal>()
                    .and_then(|signal| signal.outcome.clone())
                else {
                    return;
                };
                match outcome {
                    crate::github_connect::GithubConnectOutcome::Connected => {
                        // Spinner only when there's no usable list on screen
                        // (the `show_loading` convention below).
                        let show_loading = !matches!(this.load, Load::Ready(_));
                        this.fetch(true, show_loading, cx);
                    }
                    crate::github_connect::GithubConnectOutcome::Failed(code) => {
                        this.connect_failed(crate::github_connect::connect_error_message(&code).into(), cx);
                    }
                }
            }),
            // SLOP-25: the Connect/Reconnect GitHub hop's own hand-back
            // (`oauth-return?linked=github`) — definitive, like the one above.
            cx.observe_global::<crate::oauth::SignInLinkOutcome>(|this, cx| {
                match crate::github_connect::github_link_outcome(cx) {
                    Some(Ok(())) => {
                        let show_loading = !matches!(this.load, Load::Ready(_));
                        this.fetch(true, show_loading, cx);
                    }
                    Some(Err(message)) => this.connect_failed(message.to_string(), cx),
                    None => {}
                }
            }),
        ];
        let mut this = Self {
            team_id,
            owner,
            kind: "github".into(),
            ecode_name: String::new(),
            ecode_branch: "main".into(),
            ecode_mirror: false,
            ecode_mirror_url: String::new(),
            creating: false,
            pane,
            query: String::new(),
            load: Load::Loading,
            generation: 0,
            adding: false,
            retry_pressed: false,
            plan_limited: None,
            lookup: String::new(),
            lookup_busy: false,
            lookup_error: None,
            query_focused: false,
            focused_once: false,
            want_focus: false,
            focus_tries: FOCUS_TRIES,
            binding,
            view,
            chrome,
            _subscriptions: subscriptions,
        };
        this.fetch(false, true, cx);
        this
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new("t".into(), true, WeakEntity::new_invalid(), window, cx)
    }

    /// #EXP-1616: the Exponential form, filled.
    #[cfg(test)]
    pub(crate) fn show_ecode(&mut self, shot: EcodeShot, cx: &mut gpui::Context<Self>) {
        self.kind = "ecode".into();
        self.ecode_name = shot.name.into();
        self.ecode_branch = shot.branch.into();
        self.ecode_mirror = shot.mirror;
        self.ecode_mirror_url = shot.mirror_url.into();
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: AddRepoShot, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.load = match shot.load {
            None => Load::Loading,
            Some(None) => Load::Failed,
            Some(Some(result)) => Load::Ready(result),
        };
        self.query = shot.query.to_string();
        self.lookup = shot.lookup.to_string();
        self.lookup_busy = shot.lookup_busy;
        self.lookup_error = shot.lookup_error.map(SharedString::from);
        self.plan_limited = shot.plan_limited.map(SharedString::from);
        cx.notify();
    }

    /// A document action: the search's edit and clear; the look-up; a row pick; the prerequisite's fix, the refreshes, Retry
    /// and the footer's links.
    fn action(&mut self, name: &str, field: &str, key: &str, value: Option<String>, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match name {
            "kind" => {
                if let Some(kind) = value.filter(|kind| kind == "github" || kind == "ecode") {
                    self.kind = kind;
                }
            }
            "edit" if field == "ecode_name" => self.ecode_name = value.unwrap_or_default(),
            "edit" if field == "ecode_mirror_url" => self.ecode_mirror_url = value.unwrap_or_default(),
            "ecode_branch" => self.ecode_branch = value.unwrap_or_else(|| "main".into()),
            "ecode_mirror" => self.ecode_mirror = value.as_deref() == Some("true"),
            "create" => self.create(window, cx),
            "cancel" => native_dialog::close_dialog_window(window, cx),
            "edit" if field == "query" => self.query = value.unwrap_or_default(),
            // Typing clears a previous lookup failure (web parity).
            "edit" if field == "lookup" => {
                self.lookup = value.unwrap_or_default();
                self.lookup_error = None;
            }
            "focus_search" => (self.want_focus, self.focus_tries) = (true, FOCUS_TRIES),
            // The clear puts the caret back: a focused field keeps what was
            // typed, so the write goes through a blur.
            "clear" => {
                self.view.update(cx, |view, cx| view.blur_field(QUERY_FIELD, window, cx));
                self.query.clear();
                (self.want_focus, self.focus_tries) = (true, FOCUS_TRIES);
            }
            "lookup" => self.lookup(window, cx),
            "pick" => {
                let repo = match &self.load {
                    Load::Ready(result) => result.repos.iter().find(|repo| repo.full_name == key).cloned(),
                    _ => None,
                };
                if let Some(repo) = repo {
                    self.add(&repo, window, cx);
                }
            }
            "fix" => {
                let Load::Ready(result) = &self.load else { return };
                match result.prerequisite() {
                    Prerequisite::NotInstalled => {
                        if let Some(url) = result.install_url.clone() {
                            open_url(cx, url);
                        }
                    }
                    _ => self.connect_github(cx),
                }
            }
            "refresh" => self.refresh_access(cx),
            "recheck" => self.fetch(true, true, cx),
            "retry" => self.retry_load(cx),
            "open" => open_url(cx, key.to_string()),
            _ => return,
        }
        cx.notify();
    }

    /// Refresh: a forced re-list (SLOP-7 lists LIVE off GitHub, so a repo
    /// granted since simply appears), keeping the current rows on screen.
    fn refresh_access(&mut self, cx: &mut gpui::Context<Self>) {
        self.retry_pressed = true;
        self.fetch(true, false, cx);
    }

    /// The failed list's Retry: the same forced re-list, attended.
    fn retry_load(&mut self, cx: &mut gpui::Context<Self>) {
        self.retry_pressed = true;
        self.fetch(true, true, cx);
    }

    /// Connect / Reconnect GitHub: the in-app link hop; the outcome lands as a
    /// [`crate::oauth::SignInLinkOutcome`] the constructor observes.
    fn connect_github(&mut self, cx: &mut gpui::Context<Self>) {
        cx.notify();
        crate::github_connect::connect_github(cx);
    }

    /// FEED-79: a failed connect hand-back is the error toast; Retry runs
    /// the hop again.
    fn connect_failed(&mut self, message: String, cx: &mut gpui::Context<Self>) {
        let dialog = cx.entity().downgrade();
        crate::action_error::failed_action_with_retry(
            "connect",
            "GitHub",
            Some(message),
            move |_, cx| {
                let _ = dialog.update(cx, |this, cx| this.connect_github(cx));
            },
            cx,
        );
        cx.notify();
    }

    /// FEED-30: `integrations.github.lookupRepo` for the typed `owner/name`;
    /// a hit is added exactly like a row pick, a miss shows the server's
    /// message inline (it names the real reason — grant it, connect that
    /// account, or reconnect).
    fn lookup(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let full_name = self.lookup.trim().to_string();
        if self.lookup_busy || !is_repo_full_name(&full_name) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            crate::action_error::failed_action("look up", "the repository", Some("Not signed in.".into()), cx);
            return;
        };
        self.lookup_busy = true;
        self.lookup_error = None;
        cx.notify();
        let team_id = self.team_id.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { lookup_repo(&trpc, &team_id, &full_name) })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.lookup_busy = false;
                match result {
                    Ok(repo) => {
                        // A focused field keeps what was typed: the clear
                        // goes through a blur.
                        this.view.update(cx, |view, cx| view.blur_field(LOOKUP_FIELD, window, cx));
                        this.lookup.clear();
                        this.add(&repo, window, cx);
                    }
                    // The server's reason is about the typed name (not
                    // installed there, no push access): the field's own
                    // line. Offline is a failed action (FEED-79).
                    Err(err @ api::ApiError::Transport { .. }) => {
                        let dialog = cx.entity().downgrade();
                        crate::action_error::failed_action_with_retry(
                            "look up",
                            "the repository",
                            crate::action_error::action_error_description(&err),
                            move |window, cx| {
                                let _ = dialog.update(cx, |this, cx| this.lookup(window, cx));
                            },
                            cx,
                        );
                    }
                    Err(err) => {
                        this.lookup_error = Some(err.user_message().into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Re-detect after a browser hand-off: only when the current list is
    /// unusable (a prerequisite missing, or nothing listed), and always with
    /// `refresh` so the server's discovery cache can't hide a just-completed
    /// hop.
    fn refetch_after_connect(&mut self, cx: &mut gpui::Context<Self>) {
        let Load::Ready(result) = &self.load else {
            return;
        };
        let repos_empty = result.repos.is_empty();
        let stale = result.prerequisite() != Prerequisite::Ready || repos_empty;
        if stale {
            self.fetch(true, repos_empty, cx);
        }
    }

    /// `show_loading` keeps a usable list on screen while a background
    /// re-detect runs (the reconnect-with-repos case) instead of flashing the
    /// spinner over it.
    fn fetch(&mut self, refresh: bool, show_loading: bool, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            self.load = Load::Failed;
            cx.notify();
            return;
        };
        self.generation += 1;
        let generation = self.generation;
        if show_loading {
            self.load = Load::Loading;
        }
        cx.notify();
        let team_id = self.team_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    fetch_github_repos(&trpc, &team_id, refresh).map_err(|err| err.user_message())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer fetch
                }
                let attended = std::mem::take(&mut this.retry_pressed);
                this.load = match result {
                    Ok(result) => Load::Ready(result),
                    Err(message) => {
                        // FEED-79: the empty state + Retry; only a Refresh /
                        // Retry press that fails toasts.
                        crate::action_error::failed_load(
                            "your GitHub repositories",
                            attended,
                            Some(message),
                            cx,
                        );
                        Load::Failed
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn add(&mut self, repo: &GithubRepo, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.adding {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            crate::action_error::failed_action("add", "the repository", Some("Not signed in.".into()), cx);
            return;
        };
        self.adding = true;
        self.plan_limited = None;
        cx.notify();
        let retry_repo = repo.clone();

        let team_id = self.team_id.clone();
        let pane = self.pane.clone();
        let full_name = repo.full_name.clone();
        let default_branch =
            (!repo.default_branch.is_empty()).then(|| repo.default_branch.clone());
        let private = repo.private;

        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    api::repositories::add(
                        &trpc,
                        &team_id,
                        &full_name,
                        default_branch.as_deref(),
                        Some(private),
                    )
                })
                .await;
            let _ = this.update_in(window, |this, window, cx| match result {
                Ok(_) => {
                    // `repositories.list` is a server read — the pane only
                    // shows the new row after a refetch.
                    native_dialog::close_then(window, cx, move |_, cx| {
                        let _ = pane.update(cx, |pane, cx| pane.refresh(cx));
                    });
                }
                Err(err) => {
                    this.adding = false;
                    // Grant-model FORBIDDEN first: `is_plan_limit` matches any
                    // 412/403-shaped cap, and a stale GitHub grant must not be
                    // misread as an upsell (create_board_dialog precedent).
                    // The dialog stays open on every failure (FEED-42).
                    // FEED-79: a failed add is the error toast — a stale
                    // grant's one action is the Reconnect GitHub hop, any
                    // other failure's is Retry.
                    let dialog = cx.entity().downgrade();
                    if crate::create_board_dialog::is_grant_forbidden(&err) {
                        let toast = crate::toast::Toast::error(crate::action_error::action_error_title(
                            "add",
                            "the repository",
                        ))
                        .description(copy::ADD_FORBIDDEN)
                        .action(copy::RECONNECT_GITHUB, move |_, cx| {
                            let _ = dialog.update(cx, |this, cx| this.connect_github(cx));
                        });
                        crate::toast::show(toast, window, cx);
                    } else if super::is_plan_limit(&err) {
                        this.plan_limited = Some(super::form_error(&err, &err.to_string()).into());
                    } else {
                        crate::action_error::failed_action_with_retry(
                            "add",
                            "the repository",
                            crate::action_error::action_error_description(&err),
                            move |window, cx| {
                                let repo = retry_repo.clone();
                                let _ = dialog.update(cx, |this, cx| this.add(&repo, window, cx));
                            },
                            cx,
                        );
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// #EXP-1616: `repositories.create` (owners) → close + refetch the pane;
    /// a refusal (a taken or invalid name) is the error toast, the dialog
    /// stays open.
    fn create(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let name = self.ecode_name.trim().to_string();
        if self.creating || !valid_ecode_name(&name) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            crate::action_error::failed_action("create", "the repository", Some("Not signed in.".into()), cx);
            return;
        };
        self.creating = true;
        cx.notify();
        let team_id = self.team_id.clone();
        let branch = self.ecode_branch.clone();
        let mirror = self.ecode_mirror.then(|| self.ecode_mirror_url.trim().to_string()).filter(|url| !url.is_empty());
        let pane = self.pane.clone();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::repositories::create(&trpc, &team_id, &name, Some(&branch), mirror.as_deref()) })
                .await;
            let _ = this.update_in(window, |this, window, cx| match result {
                Ok(_) => native_dialog::close_then(window, cx, move |_, cx| {
                    let _ = pane.update(cx, |pane, cx| pane.refresh(cx));
                }),
                Err(err) => {
                    this.creating = false;
                    crate::action_error::failed_action_err("create", "the repository", &err, cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The state as the document binds it.
    fn project(&self, _cx: &App) -> Value {
        let (loading, failed) = (matches!(self.load, Load::Loading), matches!(self.load, Load::Failed));
        let result = match &self.load {
            Load::Ready(result) => Some(result),
            _ => None,
        };
        let prerequisite = result.map(GithubReposResult::prerequisite);
        let not_configured = prerequisite == Some(Prerequisite::NotConfigured);
        let missing = prerequisite.is_some_and(|p| !matches!(p, Prerequisite::NotConfigured | Prerequisite::Ready));
        let ready = prerequisite == Some(Prerequisite::Ready);
        let sentence = match prerequisite {
            Some(Prerequisite::Expired) => copy::PICKER_RECONNECT_BANNER,
            Some(Prerequisite::NotInstalled) => copy::PICKER_NOT_INSTALLED,
            _ => copy::PICKER_NOT_LINKED,
        };
        let fix_label = match prerequisite {
            Some(Prerequisite::NotInstalled) => copy::INSTALL_APP,
            Some(Prerequisite::Expired) => copy::RECONNECT_GITHUB,
            _ => copy::CONNECT_GITHUB,
        };
        let install_url = result.and_then(|r| r.install_url.clone());
        let fix_off = prerequisite == Some(Prerequisite::NotInstalled) && install_url.is_none();

        let any_suspended = result.is_some_and(|r| r.installations.iter().any(|inst| inst.suspended));
        let repos: &[GithubRepo] = result.filter(|_| ready).map(|r| r.repos.as_slice()).unwrap_or(&[]);
        let filter = self.query.trim().to_lowercase();
        let visible: Vec<Value> = repos
            .iter()
            .filter(|repo| filter.is_empty() || repo.full_name.to_lowercase().contains(&filter))
            .map(|repo| json!({"key": repo.full_name, "name": repo.full_name, "private": repo.private}))
            .collect();
        let has_list = !visible.is_empty();

        let links: Vec<(String, String)> = result
            .filter(|_| ready)
            .map(|r| r.installations.iter().filter(|inst| !inst.manage_url.is_empty()).map(|inst| (inst.label(), inst.manage_url.clone())).collect())
            .unwrap_or_default();
        let last = links.len().saturating_sub(1);
        let links: Vec<Value> = links
            .into_iter()
            .enumerate()
            .map(|(index, (label, url))| json!({"key": url, "label": label, "url": url, "comma": index != last}))
            .collect();

        let lookup_valid = is_repo_full_name(self.lookup.trim());
        let ecode = self.owner && self.kind == "ecode";
        json!({
            "kinds": self.owner,
            "kind": if ecode { "ecode" } else { "github" },
            "isGithub": !ecode,
            "isEcode": ecode,
            "ecodeName": self.ecode_name,
            "ecodeBranch": self.ecode_branch,
            "ecodeMirror": self.ecode_mirror,
            "ecodeMirrorUrl": self.ecode_mirror_url,
            "ecodeBusy": self.creating,
            "loading": loading,
            "failed": failed,
            "notConfigured": not_configured,
            "missing": missing,
            "sentence": sentence,
            "fixLabel": fix_label,
            "fixOn": missing && !fix_off,
            "fixOff": missing && fix_off,
            "suspended": ready && any_suspended,
            "suspendedText": result.filter(|_| ready && any_suspended).map(|r| copy::picker_suspended_banner(&r.suspended_picker_labels())).unwrap_or_default(),
            "none": ready && repos.is_empty() && !any_suspended,
            "search": ready && !repos.is_empty(),
            "noMatch": ready && !repos.is_empty() && !has_list,
            "hasList": ready && has_list,
            "repos": visible,
            "hasQuery": !self.query.is_empty(),
            "query": self.query,
            "searchStroke": if self.query_focused { &self.chrome.field_active } else { &self.chrome.field_stroke },
            "footer": ready,
            "links": links,
            "capNote": result.filter(|_| ready).is_some_and(|r| r.has_more),
            "hasInstall": install_url.is_some(),
            "installUrl": install_url.clone().unwrap_or_default(),
            "lookup": self.lookup,
            "lookupOn": lookup_valid && !self.lookup_busy,
            "lookupDim": !lookup_valid && !self.lookup_busy,
            "lookupBusy": self.lookup_busy,
            "hasLookupError": self.lookup_error.is_some(),
            "lookupError": self.lookup_error.clone().map(|e| e.to_string()).unwrap_or_default(),
            "planLimited": self.plan_limited.is_some(),
            "planMessage": self.plan_limited.clone().map(|m| m.to_string()).unwrap_or_default(),
        })
    }

    /// The search shows (a ready list holding at least one repo).
    fn search_shown(&self) -> bool {
        matches!(&self.load, Load::Ready(result) if result.prerequisite() == Prerequisite::Ready && !result.repos.is_empty())
    }
}

impl Render for AddRepositoryDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AddRepositoryDialogView").entered();
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        // The search takes the focus the first time it shows (unless the
        // person has moved to the lookup field meanwhile).
        if !self.focused_once && self.search_shown() {
            self.focused_once = true;
            let surface = self.view.read(cx);
            let lookup_focused = surface.focused().is_some_and(|index| Some(index) == surface.index_of(LOOKUP_FIELD));
            if !lookup_focused {
                (self.want_focus, self.focus_tries) = (true, FOCUS_TRIES);
            }
        }
        if self.want_focus && self.focus_tries > 0 {
            self.focus_tries -= 1;
            let view = self.view.clone();
            cx.defer_in(window, move |this, window, cx| {
                // Focus once the field holds its bound text: a focused field
                // keeps its own text over a later write.
                let ready = view.read(cx).field_text(QUERY_FIELD, cx).as_ref() == Some(&this.query);
                if ready && view.update(cx, |view, cx| view.focus_field(QUERY_FIELD, window, cx)) {
                    this.want_focus = false;
                } else if this.focus_tries > 0 {
                    cx.notify();
                } else {
                    this.want_focus = false;
                }
            });
        }
        // The window owns the height: the document fills it.
        let surface = self.view.clone();
        div().size_full().min_w_0().on_prepaint(move |bounds, _, cx| {
            surface.update(cx, |view, cx| view.set_viewport_height(f32::from(bounds.size.height), cx));
        }).child(self.view.clone())
    }
}

/// The colours the document needs beyond [`Ink`].
struct Chrome {
    /// The theme radius (rows, dashed boxes, the notice, the `web_sm`
    /// button).
    radius: f32,
    danger: String,
    /// The suspended banner's destructive wash and stroke.
    alert_fill: String,
    alert_stroke: String,
    /// The plan notice's 40% primary stroke over its 5% primary wash.
    notice_stroke: String,
    notice_wash: String,
    link: String,
    /// The `row_stroke` hairline of the dashed boxes.
    dashed: String,
    /// `glass_input`'s fill and stroke, and its focus stroke.
    field_fill: String,
    field_stroke: String,
    field_active: String,
    /// A ghost `Button`'s hover wash (the search's clear).
    ghost_hover: String,
    skeleton: String,
}

impl Chrome {
    fn of(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            radius: f32::from(theme.radius),
            danger: hex(theme.danger),
            alert_fill: premultiplied_hex(theme.danger.opacity(0.1)),
            alert_stroke: premultiplied_hex(theme.danger.opacity(0.5)),
            notice_stroke: premultiplied_hex(theme.primary.opacity(0.4)),
            notice_wash: premultiplied_hex(theme.primary.opacity(0.05)),
            link: hex(theme.link),
            dashed: hex(row_stroke(cx)),
            field_fill: hex(theme.input_background()),
            field_stroke: hex(theme.input),
            field_active: hex(crate::controls::ide_chrome().stroke_active),
            ghost_hover: premultiplied_hex(theme.accent.opacity(0.5)),
            skeleton: hex(theme.muted_foreground.opacity(0.18)),
        }
    }
}

/// An `Icon` at `size`.
fn glyph(id: &str, name: Value, size: f32, color: &str) -> Value {
    json!({"id": id, "component": "Icon", "props": {"name": name}, "style": {"width": size, "height": size, "color": color, "flexShrink": 0}})
}

/// A text run taking the free width (`flex_1 min_w_0`), wrapping.
fn grow(mut text: Value) -> Value {
    text["style"]["flexGrow"] = json!(1);
    text["style"]["flexShrink"] = json!(1);
    text["style"]["flexBasis"] = json!(0);
    text["style"]["minWidth"] = json!(0);
    text
}

/// A pill whose label's line sits half a point lower (gpui centres a
/// Button's line lower than the flex centre at these pills' fractional y).
fn lowered(mut pill: Value) -> Value {
    let id = format!("{}_text", pill["id"].as_str().unwrap_or_default());
    for child in pill["children"].as_array_mut().expect("the pill's children") {
        if child["id"].as_str() == Some(id.as_str()) {
            child["style"]["paddingTop"] = json!(1);
        }
    }
    pill
}

/// A `Text` shown where `visible`.
fn shown(mut node: Value, visible: Value) -> Value {
    node["visible"] = visible;
    node
}

/// A full-state box (not configured / a missing prerequisite): the
/// `size_3p5` glyph, then the copy, in the dashed hairline.
fn state_box(ink: &Ink, chrome: &Chrome, id: &str, text: Value) -> Value {
    let icon = json!({"id": format!("{id}_glyph"), "component": "Box", "style": {"display": "flex", "flexShrink": 0, "paddingTop": ink.r(0.125)},
        "children": [glyph(&format!("{id}_icon"), json!("ui-github"), ink.r(0.875), &ink.muted)]});
    json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "flex-start", "width": "100%", "gap": ink.r(0.5), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.75), "borderRadius": chrome.radius, "borderWidth": 1, "borderStyle": "dashed", "borderColor": chrome.dashed},
        "children": [icon, grow(ink.text(&format!("{id}_text"), text, SM, &ink.muted))]})
}

/// The one fix's `web_sm` primary `Button` (32 px, `px 12`, `text_sm` on a
/// line of 1, the `size_3p5` glyph after `gap_1`, the theme radius). The
/// disabled face: the fill at half under the solid label.
fn fix_button(ink: &Ink, chrome: &Chrome) -> [Value; 2] {
    let size = ink.r(SM);
    let glyph_size = ink.r(0.875);
    let face = |id: &str, on: bool| {
        let text = json!({"id": format!("{id}_text"), "component": "Text", "props": {"text": path("/pane/fixLabel")}, "style": {"fontSize": size, "lineHeight": size, "color": ink.primary_button_fg, "flexShrink": 0}});
        let mut button = json!({"id": id, "component": "Box", "accessibility": {"label": path("/pane/fixLabel")},
            "style": {"position": "relative", "display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": ink.r(0.25), "height": 32, "paddingHorizontal": 12, "borderRadius": chrome.radius, "backgroundColor": if on { ink.primary_button_fill.as_str() } else { "#00000000" }},
            "children": [glyph(&format!("{id}_icon"), json!("ui-github"), glyph_size, &ink.primary_button_fg), text]});
        if on {
            button["props"] = json!({"pressable": true});
            button["on"] = json!({"press": {"event": {"name": "fix"}}});
            button["style"]["cursor"] = json!("pointer");
            button["style"][":hover"] = json!({"backgroundColor": ink.primary_button_hover});
        } else {
            button["children"].as_array_mut().expect("the button's children").insert(0, json!({"id": format!("{id}_fill"), "component": "Box",
                "style": {"position": "absolute", "top": 0, "left": 0, "right": 0, "bottom": 0, "borderRadius": chrome.radius, "backgroundColor": ink.primary_button_fill, "opacity": 0.5}}));
        }
        button
    };
    let mut on = face("fix", true);
    on["visible"] = path("/pane/fixOn");
    let mut off = face("fix_off", false);
    off["visible"] = path("/pane/fixOff");
    [on, off]
}

/// The search row (`search_field(.., Sm)`): the 28 px `glass_input` frame
/// with the glyph inside and the clear once there is text; Box, Icon and a
/// `bare` Input, since a `box` Input has neither the 28 px rung nor a glyph.
fn search_row(ink: &Ink, chrome: &Chrome) -> Value {
    let clear_box = json!({"id": "clear", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": "Clear search"}, "on": {"press": {"event": {"name": "clear"}}},
        "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "width": ink.r(1.25), "height": ink.r(1.25), "borderRadius": chrome.radius, "cursor": "pointer", ":hover": {"backgroundColor": chrome.ghost_hover}},
        "children": [glyph("clear_icon", json!("ui-clear"), ink.r(0.75), &ink.muted)]});
    let clear = json!({"id": "clear_tip", "component": "Tooltip", "visible": path("/pane/hasQuery"), "props": {"content": "Clear search"}, "children": [clear_box]});
    let field = json!({"id": "query", "component": "Input", "props": {"label": copy::SEARCH_PLACEHOLDER, "name": "query", "variant": "bare", "placeholder": copy::SEARCH_PLACEHOLDER, "value": path("/pane/query")},
        "style": {"flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0},
        "on": {
            "change": {"event": {"name": "edit", "context": {"field": "query"}}},
        }});
    json!({"id": "search_row", "component": "Box", "visible": path("/pane/search"), "props": {"pressable": true}, "accessibility": {"label": copy::SEARCH_PLACEHOLDER}, "on": {"press": {"event": {"name": "focus_search"}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0, "width": "100%", "height": 28, "paddingHorizontal": 8, "gap": 4, "borderRadius": ink.radius_lg, "borderWidth": 1, "borderColor": path("/pane/searchStroke"), "backgroundColor": chrome.field_fill},
        "children": [glyph("search_icon", json!("nav-search"), 14., &ink.muted), field, clear]})
}

/// One pickable repo row (`py_1p5`, `px_2`, `gap_2`, the theme radius, the
/// hover wash; the name ellipsizes, a private repo ends in its lock).
fn repo_row(ink: &Ink, chrome: &Chrome) -> Value {
    let glyph_size = ink.r(0.75);
    let mut name = grow(ink.text("row_name", path("name"), SM, &ink.foreground));
    name["props"]["lines"] = json!(1);
    let mut lock = glyph("row_lock", json!("ui-private"), glyph_size, &ink.muted);
    lock["visible"] = path("private");
    json!({"id": "row", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": path("name")}, "on": {"press": {"event": {"name": "pick", "context": {"key": path("key")}}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "flexShrink": 0, "gap": ink.r(0.5), "paddingHorizontal": ink.r(0.5), "paddingVertical": ink.r(0.375), "borderRadius": chrome.radius, "cursor": "pointer", ":hover": {"backgroundColor": ink.fill_row}},
        "children": [glyph("row_icon", json!("ui-github"), glyph_size, &ink.muted), name, lock]})
}

/// The scrolling list (`flex_1 min_h_0`, the footer stays).
fn repo_list(ink: &Ink, chrome: &Chrome) -> Value {
    let rows = json!({"id": "rows", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "template": {"component": "row_wrap", "path": "/pane/repos", "key": "key"},
        "children": [{"id": "row_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "flexShrink": 0}, "children": [repo_row(ink, chrome)]}]});
    json!({"id": "list", "component": "Box", "visible": path("/pane/hasList"), "style": {"flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minHeight": 0, "width": "100%", "overflowY": "auto"}, "children": [rows]})
}

/// The footer (a dashed `text_xs` box, `gap_2`): the sentence with one
/// Configure link per account, the cap note, Refresh and "Install on another
/// account", the lookup (a `box` `sm` Input and "Look up"), its error.
fn footer(ink: &Ink, chrome: &Chrome) -> Value {
    let xs = |id: &str, text: Value| ink.text(id, text, XS, &ink.muted);
    let mut explain = xs("explain", json!(copy::FOOTER_EXPLAIN));
    explain["style"]["width"] = json!("100%");
    let mut link_text = ink.text("link_text", path("label"), XS, &chrome.link);
    // A link Button's box is 1.5 less than its line of 1.25 (`text_xs`), and
    // gpui centres the line half a point lower than the flex centre.
    link_text["style"]["lineHeight"] = json!(ink.r(XS) * 1.25 - 1.5);
    link_text["style"]["textDecoration"] = json!("underline");
    link_text["style"]["flexShrink"] = json!(0);
    link_text["style"]["paddingTop"] = json!(1);
    let link = json!({"id": "link", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": path("label")}, "on": {"press": {"event": {"name": "open", "context": {"key": path("url")}}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": ink.r(0.25), "cursor": "pointer"},
        "children": [link_text, glyph("link_glyph", json!("ui-external-link"), crate::surface::PillSize::Sm.glyph(), &chrome.link)]});
    let comma = shown(xs("link_comma", json!(",")), path("comma"));
    let links = json!({"id": "link_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0}, "children": [link, comma]});
    let sentence = json!({"id": "sentence", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "flexWrap": "wrap", "alignItems": "center", "width": "100%", "columnGap": ink.r(0.25)},
        "template": {"component": "link_wrap", "path": "/pane/links", "key": "key"}, "children": [explain, links]});
    let cap = shown(xs("cap_note", json!(copy::CAP_NOTE)), path("/pane/capNote"));
    let [refresh, _] = ink.pill_button("refresh", Some("ui-refresh"), json!(copy::REFRESH), json!(false), json!({"name": "refresh"}));
    let [install, _] = ink.pill_button("install", Some("ui-add"), json!(copy::INSTALL_ANOTHER), json!(false), json!({"name": "open", "context": {"key": path("/pane/installUrl")}}));
    let actions = json!({"id": "footer_actions", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "flexWrap": "wrap", "alignItems": "center", "gap": ink.r(0.5)}, "children": [lowered(refresh), shown(lowered(install), path("/pane/hasInstall"))]});
    // #EXP-1787: the `box` `sm` Input (`glass_input(..).web_input_sm()`), its
    // typed text in the footer's muted ink.
    let input = json!({"id": "lookup", "component": "Input", "props": {"label": copy::LOOKUP_PLACEHOLDER, "name": "lookup", "variant": "box", "size": "sm", "placeholder": copy::LOOKUP_PLACEHOLDER, "value": path("/pane/lookup")},
        "style": {"flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0, "color": "$color.mutedForeground"},
        "on": {"change": {"event": {"name": "edit", "context": {"field": "lookup"}}}, "submit": {"event": {"name": "lookup"}}}});
    let [look, _] = ink.pill_button_with("look_up", PillPaint::Glass, None, None, json!(copy::LOOK_UP), json!(false), None, json!({"name": "lookup"}));
    let [_, dim] = ink.pill_button_with("look_up", PillPaint::Glass, None, None, json!(copy::LOOK_UP), json!(true), None, Value::Null);
    let [_, busy] = ink.pill_button_with("look_up_busy", PillPaint::Glass, Some(json!("ui-loading")), None, json!(copy::LOOK_UP), json!(true), Some(json!(true)), Value::Null);
    let lookup_row = json!({"id": "lookup_row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.5)},
        "children": [input, shown(lowered(look), path("/pane/lookupOn")), shown(lowered(dim), path("/pane/lookupDim")), shown(lowered(busy), path("/pane/lookupBusy"))]});
    let error = json!({"id": "lookup_error", "component": "Text", "visible": path("/pane/hasLookupError"), "props": {"text": path("/pane/lookupError")},
        "style": {"fontSize": crate::action_error::FIELD_ERROR_PX, "lineHeight": 20, "color": chrome.danger}});
    json!({"id": "footer", "component": "Box", "visible": path("/pane/footer"),
        "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "flexShrink": 0, "gap": ink.r(0.5), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": chrome.radius, "borderWidth": 1, "borderStyle": "dashed", "borderColor": chrome.dashed},
        "children": [sentence, cap, actions, lookup_row, error]})
}

/// The dialog's tree (`gap_3`): one child per state, the plan notice last.
fn document(ink: &Ink, chrome: &Chrome) -> Value {
    let sm = |id: &str, text: &str, visible: Value| shown(ink.text(id, json!(text), SM, &ink.muted), visible);
    let mut not_configured = state_box(ink, chrome, "not_configured", json!(copy::PICKER_NOT_CONFIGURED));
    not_configured["visible"] = path("/pane/notConfigured");

    let [fix, fix_off] = fix_button(ink, chrome);
    let [recheck, _] = ink.pill_button("recheck", Some("ui-refresh"), json!(copy::PICKER_CONNECTED_CHECK), json!(false), json!({"name": "recheck"}));
    let prereq_actions = json!({"id": "prereq_actions", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "flexWrap": "wrap", "alignItems": "center", "gap": ink.r(0.5)}, "children": [fix, fix_off, lowered(recheck)]});
    let prereq = json!({"id": "prereq", "component": "Box", "visible": path("/pane/missing"), "style": {"display": "flex", "flexDirection": "column", "width": "100%", "gap": ink.r(0.75)},
        "children": [state_box(ink, chrome, "prereq_box", path("/pane/sentence")), prereq_actions]});

    let banner = json!({"id": "banner", "component": "Box", "visible": path("/pane/suspended"),
        "style": {"display": "flex", "flexDirection": "row", "flexWrap": "wrap", "alignItems": "center", "width": "100%", "minWidth": 0, "flexShrink": 0, "gap": ink.r(0.5), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md, "borderWidth": 1, "borderColor": chrome.alert_stroke, "backgroundColor": chrome.alert_fill},
        "children": [glyph("banner_icon", json!("ui-warning"), ink.r(0.75), &chrome.danger), grow(ink.text("banner_text", path("/pane/suspendedText"), XS, &chrome.danger))]});

    let plan = json!({"id": "plan", "component": "Box", "visible": path("/pane/planLimited"),
        "style": {"display": "flex", "flexDirection": "column", "width": "100%", "flexShrink": 0, "gap": ink.r(0.25), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": chrome.radius, "borderWidth": 1, "borderColor": chrome.notice_stroke, "backgroundColor": chrome.notice_wash},
        "children": [ink.text("plan_message", path("/pane/planMessage"), SM, &ink.foreground), ink.text("plan_note", json!(copy::UPGRADE_ON_THE_WEB), XS, &ink.muted)]});

    let children = vec![
        ink.skeleton_rows_at("skeleton", &chrome.skeleton, REPO_ROW_H, path("/pane/loading")),
        failed_state(ink, "failed", "ui-github", NO_REPOSITORIES, path("/pane/failed")),
        not_configured,
        prereq,
        banner,
        sm("none", copy::NONE_PUSHABLE, path("/pane/none")),
        search_row(ink, chrome),
        sm("no_match", copy::NO_MATCH, path("/pane/noMatch")),
        repo_list(ink, chrome),
        footer(ink, chrome),
        plan,
    ];
    let github = json!({"id": "github", "component": "Box", "visible": path("/pane/isGithub"), "style": {"display": "flex", "flexDirection": "column", "width": "100%", "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minHeight": 0, "minWidth": 0, "gap": ink.r(0.75)}, "children": children});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "height": "100%", "minHeight": 0, "minWidth": 0, "gap": ink.r(0.75)}, "children": [kinds(), github, ecode_form()]})
}

/// #EXP-1616: a repo name of `[A-Za-z0-9._-]` (repo-page.json `copy.add.name`).
fn valid_ecode_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 100 && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// #EXP-1616: the provider pick (owners): `ChoiceTiles` GitHub | Exponential.
fn kinds() -> Value {
    let copy = &domain::repo_page::REPO_PAGE.copy.add;
    json!({"id": "kinds", "component": "ChoiceTiles", "visible": path("/pane/kinds"),
        "props": {"name": "kind", "value": path("/pane/kind"), "columns": 2, "options": [{"label": copy.github, "value": "github"}, {"label": copy.ecode, "value": "ecode"}]},
        "on": {"change": {"event": {"name": "kind"}}}})
}

/// #EXP-1616: the Exponential form: name, default branch, the mirror switch
/// revealing its URL, Cancel / Create.
fn ecode_form() -> Value {
    let copy = &domain::repo_page::REPO_PAGE.copy.add;
    let branches: Vec<Value> = ["main", "master", "develop"].iter().map(|branch| json!({"label": branch, "value": branch})).collect();
    json!({"id": "ecode", "component": "Form", "visible": path("/pane/isEcode"), "props": {"name": "ecode", "busy": path("/pane/ecodeBusy")},
        "on": {"submit": {"event": {"name": "create"}}},
        "children": [{"id": "ecode_fields", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "gap": 12}, "children": [
            {"id": "ecode_name", "component": "Input", "props": {"label": copy.name, "name": "name", "placeholder": "my-repo", "value": path("/pane/ecodeName")},
                "on": {"change": {"event": {"name": "edit", "context": {"field": "ecode_name"}}}}},
            {"id": "ecode_branch", "component": "Select", "props": {"label": copy.default_branch, "name": "defaultBranch", "value": path("/pane/ecodeBranch"), "options": branches},
                "on": {"change": {"event": {"name": "ecode_branch"}}}},
            {"id": "ecode_mirror_row", "component": "SettingsRow", "props": {"label": copy.mirror, "stackBelow": "never"}, "children": [
                {"id": "ecode_mirror", "component": "Switch", "props": {"label": "", "name": "mirror", "checked": path("/pane/ecodeMirror")}, "on": {"change": {"event": {"name": "ecode_mirror"}}}},
            ]},
            {"id": "ecode_mirror_url", "component": "Input", "visible": path("/pane/ecodeMirror"), "props": {"label": copy.mirror_url, "name": "mirrorUrl", "placeholder": "https://github.com/acme/repo.git", "value": path("/pane/ecodeMirrorUrl")},
                "on": {"change": {"event": {"name": "edit", "context": {"field": "ecode_mirror_url"}}}}},
            {"id": "ecode_actions", "component": "FormActions", "props": {"submit": copy.create, "cancel": true}, "on": {"cancel": {"event": {"name": "cancel"}}}},
        ]}]})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let ink = super::super::readiness::tests::test_ink();
        let chrome = Chrome {
            radius: 10.,
            danger: "#f87171ff".into(),
            alert_fill: "#f871711a".into(),
            alert_stroke: "#f8717180".into(),
            notice_stroke: "#fafafa66".into(),
            notice_wash: "#fafafa0d".into(),
            link: "#a1a1aaff".into(),
            dashed: "#ffffff14".into(),
            field_fill: "#ffffff08".into(),
            field_stroke: "#ffffff26".into(),
            field_active: "#ffffff29".into(),
            ghost_hover: "#ffffff0d".into(),
            skeleton: "#a1a1aa2e".into(),
        };
        let tree = document(&ink, &chrome);
        let bound = super::super::doc::bound_values(&tree);
        let mut surface = eui_core::surface::Surface::new("settings-add-repository", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
    }
}
