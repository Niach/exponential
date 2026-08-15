//! "Getting started" checklist page (EXP-470) — the desktop mirror of the
//! web checklist (`apps/web/src/components/getting-started/`), rendered as a
//! tab-less full-page center screen ([`crate::navigation::Screen::GettingStarted`],
//! the Actions page recipe) behind a conditional rail entry.
//!
//! Entries: github → board → coding → server → invite. Divergences from the
//! web model are deliberate:
//! * `desktop` ("Get the desktop app") is omitted — the IDE *is* a desktop
//!   device and self-registers on sign-in, so the step would always be done.
//! * `widget`/`helpdesk` are omitted — owner-only web settings surfaces with
//!   no desktop CTA target.
//! * `mcp` is omitted — the desktop auto-mints and wires the personal key
//!   (`api::users::ensure_personal_key`); there is nothing to set up.
//!
//! Signals: boards / coding sessions / members / invites are live synced
//! collections; the GitHub install state and the device registry are tRPC
//! one-shots on the MachinesSection render-liveness poll cadence (returning
//! from the browser connect flow flips the GitHub step within a tick).
//!
//! Dismissal is the web's one-way `users.dismissGettingStarted` flag,
//! mirrored locally in accounts.json exactly like the onboarding stamp (the
//! flag rides the session as a Better Auth additionalField, and warm starts
//! never re-fetch the session). The RAIL entry gates on dismissal only —
//! completion-gating would force tRPC polling from the rail; the page's
//! all-done state offers Dismiss as the exit.

use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, relative, App, Entity, FontWeight, IntoElement, ParentElement, Render, ScrollHandle,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, set_screen, Navigation};
use crate::queries;
use crate::session::AuthContext;

/// MachinesSection cadence: often enough that "I just connected GitHub /
/// ran the server one-liner" lands while the user is looking.
const POLL_INTERVAL: Duration = Duration::from_secs(15);
/// A poll tick with no render this recent means the page was left.
const VISIBLE_GRACE: Duration = Duration::from_secs(40);

// ---------------------------------------------------------------------------
// Pure model (mirrors web `deriveEntryStates` semantics; unit-tested)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryKey {
    Github,
    Board,
    Coding,
    Server,
    Invite,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryState {
    Done,
    Available,
    Locked,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Signals {
    pub github_installed: bool,
    pub has_board: bool,
    pub has_repo_board: bool,
    pub has_coding_session: bool,
    pub has_server_device: bool,
    /// members ≥ 2 or any invite row — proof the team was shared.
    pub invited: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub key: EntryKey,
    pub state: EntryState,
    /// For locked entries: the step whose completion unlocks this one.
    pub locked_by: Option<EntryKey>,
}

/// Static order github → board → coding → server → invite. Completion always
/// wins over locking (a coding session synced from before proves the prereqs
/// were satisfiable) — web parity.
pub(crate) fn derive_entries(signals: &Signals) -> Vec<Entry> {
    let simple = |key, done| Entry {
        key,
        state: if done {
            EntryState::Done
        } else {
            EntryState::Available
        },
        locked_by: None,
    };
    let coding = if signals.has_coding_session {
        simple(EntryKey::Coding, true)
    } else if signals.has_repo_board {
        simple(EntryKey::Coding, false)
    } else {
        Entry {
            key: EntryKey::Coding,
            state: EntryState::Locked,
            // Web parity minus the device feeder (this machine IS a device):
            // GitHub first — without it the board step can't attach a repo.
            locked_by: Some(if signals.github_installed {
                EntryKey::Board
            } else {
                EntryKey::Github
            }),
        }
    };
    vec![
        simple(EntryKey::Github, signals.github_installed),
        simple(EntryKey::Board, signals.has_board),
        coding,
        simple(EntryKey::Server, signals.has_server_device),
        simple(EntryKey::Invite, signals.invited),
    ]
}

// ---------------------------------------------------------------------------
// Dismissal (the onboarding-stamp pattern)
// ---------------------------------------------------------------------------

/// Whether the rail shows the Getting-started entry: signed in and not
/// dismissed. Deliberately NOT completion-based (module doc).
pub(crate) fn getting_started_visible(cx: &App) -> bool {
    queries::active_account(cx)
        .is_some_and(|account| account.getting_started_dismissed_at.is_none())
}

/// Mirror the dismissal into accounts.json (one-way; warm starts never
/// re-fetch the session). Safe to call repeatedly.
pub(crate) fn stamp_local_getting_started_dismissed(cx: &mut App) {
    let Some(account) = queries::active_account(cx) else {
        return;
    };
    if account.getting_started_dismissed_at.is_some() {
        return;
    }
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(auth) = cx.try_global::<AuthContext>() {
        auth.auth.set_getting_started_dismissed(&account.id, &now);
    }
}

// ---------------------------------------------------------------------------
// The page view
// ---------------------------------------------------------------------------

pub struct GettingStartedView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    /// GitHub install state keyed by the team it was asked for — a team
    /// switch reads as "not answered yet" (a stale `true` must never flash
    /// the new team's step done; the web hook has the identical guard).
    github: Option<(String, bool)>,
    /// Whether the signed-in user has a server-kind device registered;
    /// `None` until the first `devices.list` answer.
    has_server_device: Option<bool>,
    fetch_seq: u64,
    polling: bool,
    last_render: Instant,
    _subscriptions: Vec<Subscription>,
}

impl GettingStartedView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let store = Store::global(cx).clone();
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        // Coarse-grained on purpose: the page reads four collections
        // (boards, coding_sessions, team_members, team_invites) and renders
        // a handful of cards.
        subscriptions.extend(store.observe_collections(cx));

        Self {
            nav,
            scroll: ScrollHandle::new(),
            github: None,
            has_server_device: None,
            fetch_seq: 0,
            polling: false,
            last_render: Instant::now(),
            _subscriptions: subscriptions,
        }
    }

    /// Start the fetch + poll pair once (the MachinesSection render-liveness
    /// pattern); later renders only refresh `last_render`.
    fn ensure_polling(&mut self, cx: &mut gpui::Context<Self>) {
        if self.polling {
            return;
        }
        self.polling = true;
        self.fetch(cx);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_INTERVAL).await;
                let keep_going = this.update(cx, |this, cx| {
                    if this.last_render.elapsed() > VISIBLE_GRACE {
                        this.polling = false;
                        return false;
                    }
                    this.fetch(cx);
                    true
                });
                if !matches!(keep_going, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    /// One seq-guarded round: `devices.list` (user-level) + the active
    /// team's `integrations.github.status`. Failures keep the last answer.
    fn fetch(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let team_id = active_team_id(&self.nav, cx);
        self.fetch_seq += 1;
        let seq = self.fetch_seq;
        cx.spawn(async move |this, cx| {
            let fetch_team = team_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let devices = api::devices::list(&trpc);
                    let github = fetch_team
                        .as_deref()
                        .map(|team| crate::github_connect::fetch_github_status(&trpc, team));
                    (devices, github)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.fetch_seq != seq {
                    return;
                }
                let (devices, github) = result;
                match devices {
                    Ok(list) => {
                        this.has_server_device =
                            Some(list.devices.iter().any(api::devices::DeviceEntry::is_server));
                    }
                    Err(err) => log::warn!("[ui] getting-started devices.list failed: {err}"),
                }
                match (team_id, github) {
                    (Some(team), Some(Ok(status))) => {
                        this.github = Some((team, status.installed));
                    }
                    (Some(_), Some(Err(err))) => {
                        log::warn!("[ui] getting-started github.status failed: {err}");
                    }
                    _ => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        stamp_local_getting_started_dismissed(cx);
        if let Some(trpc) = queries::trpc_client(cx) {
            cx.background_executor()
                .spawn(async move {
                    if let Err(err) = api::users::dismiss_getting_started(&trpc) {
                        log::warn!(
                            "[ui] users.dismissGettingStarted failed (best-effort): {err}"
                        );
                    }
                })
                .detach();
        }
        // The rail entry is gone now — leave the page too.
        set_screen(window, cx, None);
        cx.notify();
    }

    fn signals(&self, team_id: &str, cx: &App) -> Signals {
        let collections = Store::global(cx).collections();
        let boards = collections.boards_in_team(team_id, cx);
        let has_coding_session = collections
            .coding_sessions
            .read(cx)
            .iter()
            .any(|session| session.team_id.as_deref() == Some(team_id));
        let members = collections
            .team_members
            .read(cx)
            .iter()
            .filter(|member| member.team_id == team_id)
            .count();
        let has_invite = collections
            .team_invites
            .read(cx)
            .iter()
            .any(|invite| invite.team_id == team_id);
        Signals {
            github_installed: matches!(&self.github, Some((team, true)) if team == team_id),
            has_board: !boards.is_empty(),
            has_repo_board: boards.iter().any(|board| board.repository_id.is_some()),
            has_coding_session,
            has_server_device: self.has_server_device.unwrap_or(false),
            invited: members >= 2 || has_invite,
        }
    }

    fn entry_card(
        &self,
        index: usize,
        entry: &Entry,
        team_id: &str,
        loading: bool,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let (icon, title, description) = match entry.key {
            EntryKey::Github => (
                registry::UI_GITHUB,
                "Connect a GitHub repo",
                "Link a GitHub account to your team so boards can attach repositories. \
                 Pull requests and coding sessions flow back into their issues.",
            ),
            EntryKey::Board => (
                registry::NAV_BOARDS,
                "Create a board",
                "Boards hold your issues. Connect a repository to code on a board; \
                 without one it works as a plain board.",
            ),
            EntryKey::Coding => (
                registry::NAV_TERMINAL,
                "Start coding with an agent",
                "\"Start coding\" on any issue hands it to your coding agent on this \
                 machine. It plans first, implements, then commits, pushes, and opens \
                 the pull request linked back to the issue.",
            ),
            EntryKey::Server => (
                registry::UI_SERVER,
                "Set up a server",
                "Run the headless agent daemon on an always-on machine. One command \
                 installs it; the server then shows up under My machines and can take \
                 remote \"Start coding\" requests.",
            ),
            EntryKey::Invite => (
                registry::UI_INVITE,
                "Invite your team",
                "Teammates share boards, reviews, and the support inbox. Send an \
                 invite by email or hand out an invite link.",
            ),
        };
        let locked = !loading && entry.state == EntryState::Locked;
        let hint: Option<&'static str> = match (entry.key, entry.locked_by) {
            _ if !locked => None,
            (EntryKey::Coding, Some(EntryKey::Github)) => {
                Some("Connect a GitHub repo first. Coding sessions need a repo-backed board.")
            }
            (EntryKey::Coding, Some(EntryKey::Board)) => {
                Some("Create a board with a repository first.")
            }
            _ => None,
        };

        // State glyph: green check / lock / bordered step number (the web
        // card's exact vocabulary; `loading` renders every card neutral).
        let glyph: gpui::AnyElement = if !loading && entry.state == EntryState::Done {
            Icon::new(registry::UI_SELECTED)
                .small()
                .text_color(theme::tokens::GREEN.to_hsla())
                .into_any_element()
        } else if locked {
            Icon::new(registry::UI_PRIVATE).small().text_color(muted).into_any_element()
        } else {
            div()
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(theme.border)
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(format!("{}", index + 1)))
                .into_any_element()
        };

        let cta: Option<gpui::AnyElement> = if locked || (!loading && entry.state == EntryState::Done)
        {
            None
        } else {
            let team = team_id.to_string();
            Some(match entry.key {
                EntryKey::Github => Button::new(("gs-cta-github", index))
                    .primary()
                    .small()
                    .label("Connect GitHub")
                    .on_click(|_, window, cx| {
                        crate::navigation::navigate(
                            window,
                            cx,
                            crate::navigation::Screen::Settings,
                        );
                        crate::sidebar::select_settings_section(
                            window,
                            cx,
                            crate::settings::SettingsSection::Repositories,
                        );
                    })
                    .into_any_element(),
                EntryKey::Board => Button::new(("gs-cta-board", index))
                    .primary()
                    .small()
                    .label("Create a board")
                    .on_click(move |_, window, cx| {
                        crate::create_board_dialog::open(window, cx, team.clone());
                    })
                    .into_any_element(),
                EntryKey::Coding => Button::new(("gs-cta-coding", index))
                    .primary()
                    .small()
                    .label("Start coding")
                    .on_click(move |_, window, cx| {
                        crate::start_coding_dialog::open_for_selection(
                            window,
                            cx,
                            team.clone(),
                            Vec::new(),
                            None,
                        );
                    })
                    .into_any_element(),
                EntryKey::Server => Button::new(("gs-cta-server", index))
                    .primary()
                    .small()
                    .label("Add a server")
                    .on_click(|_, window, cx| {
                        crate::machines::open_add_server_dialog(window, cx);
                    })
                    .into_any_element(),
                EntryKey::Invite => Button::new(("gs-cta-invite", index))
                    .primary()
                    .small()
                    .label("Invite in team settings")
                    .on_click(|_, window, cx| {
                        crate::navigation::navigate(
                            window,
                            cx,
                            crate::navigation::Screen::Settings,
                        );
                        crate::sidebar::select_settings_section(
                            window,
                            cx,
                            crate::settings::SettingsSection::Members,
                        );
                    })
                    .into_any_element(),
            })
        };

        crate::surface::glass_card()
            .w_full()
            .min_w_0()
            .p_4()
            .gap_2()
            .when(locked, |this| this.opacity(0.6))
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_shrink_0().child(glyph))
                    .child(
                        div()
                            .flex_shrink_0()
                            .child(Icon::new(icon).small().text_color(muted)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(title),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(hint.unwrap_or(description)),
            )
            .children(cta.map(|cta| h_flex().mt_1().child(cta)))
            .into_any_element()
    }
}

impl Render for GettingStartedView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Rendering IS the visibility signal (MachinesSection pattern).
        self.last_render = Instant::now();
        self.ensure_polling(cx);

        let muted = cx.theme().muted_foreground;
        let team_id = active_team_id(&self.nav, cx);

        let body: gpui::AnyElement = match team_id {
            None => div()
                .text_sm()
                .text_color(muted)
                .child("Waiting for your team to sync…")
                .into_any_element(),
            Some(team_id) => {
                let signals = self.signals(&team_id, cx);
                let entries = derive_entries(&signals);
                // Neutral until the one-shot signals answered — checks/locks
                // that pop in one by one read as state changes, not loading.
                let loading = self.has_server_device.is_none()
                    || !matches!(&self.github, Some((team, _)) if *team == team_id);
                let done = entries
                    .iter()
                    .filter(|entry| entry.state == EntryState::Done)
                    .count();
                let total = entries.len();

                let progress: gpui::AnyElement = if loading {
                    div().into_any_element()
                } else {
                    h_flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .text_sm()
                                .whitespace_nowrap()
                                .text_color(muted)
                                .child(SharedString::from(format!("{done}/{total} done"))),
                        )
                        .child(
                            div()
                                .h(px(6.))
                                .w(px(192.))
                                .rounded_full()
                                .bg(cx.theme().border)
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative((done as f32 / total.max(1) as f32).min(1.)))
                                        .rounded_full()
                                        .bg(cx.theme().primary),
                                ),
                        )
                        .into_any_element()
                };

                let cards: Vec<gpui::AnyElement> = entries
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| self.entry_card(index, entry, &team_id, loading, cx))
                    .collect();

                // NO `w_full` (EXP-508): a percent width on the centered
                // column's direct child resolves against the UNCLAMPED
                // ancestor available width at wide windows (the EXP-436
                // leak that broke the Actions page); auto width + the
                // column's flex-col stretch size it to the capped column.
                v_flex()
                    .min_w_0()
                    .gap_4()
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .child(
                                v_flex()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_xl()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Getting started"),
                                    )
                                    .child(div().text_sm().text_color(muted).child(
                                        if !loading && done == total {
                                            "All set — dismiss this page whenever you like."
                                        } else {
                                            "A few steps to get the most out of Exponential."
                                        },
                                    )),
                            )
                            .child(
                                v_flex()
                                    .items_end()
                                    .gap_2()
                                    .child(progress)
                                    .child(
                                        Button::new("gs-dismiss")
                                            .ghost()
                                            .small()
                                            .label("Dismiss")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.dismiss(window, cx);
                                            })),
                                    ),
                            ),
                    )
                    .children(cards)
                    .into_any_element()
            }
        };

        let column = v_flex().w_full().min_w_0().px_4().py_4().gap_4().child(body);

        v_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .text_color(cx.theme().foreground)
            .child(crate::scroll_pane::v_scroll_pane(
                "getting-started-scroll",
                &self.scroll,
                div()
                    .w_full()
                    .min_w_0()
                    .child(column.w_full().max_w(px(720.)).mx_auto()),
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_keep_the_static_order() {
        let entries = derive_entries(&Signals::default());
        let keys: Vec<EntryKey> = entries.iter().map(|entry| entry.key).collect();
        assert_eq!(
            keys,
            vec![
                EntryKey::Github,
                EntryKey::Board,
                EntryKey::Coding,
                EntryKey::Server,
                EntryKey::Invite,
            ]
        );
    }

    #[test]
    fn coding_lock_chain_walks_github_then_board() {
        let coding = |signals: &Signals| {
            derive_entries(signals)
                .into_iter()
                .find(|entry| entry.key == EntryKey::Coding)
                .unwrap()
        };

        let none = Signals::default();
        assert_eq!(coding(&none).state, EntryState::Locked);
        assert_eq!(coding(&none).locked_by, Some(EntryKey::Github));

        let github = Signals {
            github_installed: true,
            has_board: true,
            ..Signals::default()
        };
        assert_eq!(coding(&github).state, EntryState::Locked);
        assert_eq!(coding(&github).locked_by, Some(EntryKey::Board));

        let ready = Signals {
            github_installed: true,
            has_board: true,
            has_repo_board: true,
            ..Signals::default()
        };
        assert_eq!(coding(&ready).state, EntryState::Available);
        assert_eq!(coding(&ready).locked_by, None);
    }

    #[test]
    fn completion_beats_locking() {
        // A coding session synced from before (repo board since trashed)
        // must render done, never a lock — web parity.
        let signals = Signals {
            has_coding_session: true,
            ..Signals::default()
        };
        let coding = derive_entries(&signals)
            .into_iter()
            .find(|entry| entry.key == EntryKey::Coding)
            .unwrap();
        assert_eq!(coding.state, EntryState::Done);
        assert_eq!(coding.locked_by, None);
    }

    #[test]
    fn simple_entries_complete_from_their_signals() {
        let signals = Signals {
            github_installed: true,
            has_board: true,
            has_server_device: true,
            invited: true,
            ..Signals::default()
        };
        let entries = derive_entries(&signals);
        let state_of = |key| {
            entries
                .iter()
                .find(|entry| entry.key == key)
                .unwrap()
                .state
        };
        assert_eq!(state_of(EntryKey::Github), EntryState::Done);
        assert_eq!(state_of(EntryKey::Board), EntryState::Done);
        assert_eq!(state_of(EntryKey::Server), EntryState::Done);
        assert_eq!(state_of(EntryKey::Invite), EntryState::Done);
    }
}
