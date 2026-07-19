//! One support ticket's conversation screen (EXP-180 — the Support inbox's
//! center tab, opened from the sidebar's Support tool window).
//!
//! Support threads are server-only tRPC data (never Electric-synced), so the
//! view owns a fetched [`api::helpdesk::SupportThreadDetail`] plus a 15s
//! poll that runs only while the tab is the window's active screen —
//! seq-guarded and entity-weak so a superseded/hidden view never leaks a
//! loop (the classic bug). Reactivating the tab restarts the poll via
//! [`SupportThreadView::set_thread`] (the screens panel re-points the shared
//! instance on every nav change).
//!
//! Surface: message bubbles (inbound reporter messages left, member replies
//! right-ish, internal notes amber-tinted with an "Internal" chip), a
//! Reply / Internal note composer (Cmd/Ctrl+Enter sends), Close/Reopen by
//! thread status, and the escalate flow — a board dropdown + confirm while
//! unlinked, the linked issue's chip (→ the issue tab) once escalated.

use std::collections::HashMap;
use std::time::Duration;

use gpui::{
    div, prelude::FluentBuilder as _, relative, App, AppContext as _, ClickEvent, Entity,
    FontWeight, InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    scroll::ScrollableElement as _,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _,
};
use sync::Store;

use crate::actions::OpenIssue;
use crate::comments;
use crate::icons::ExpIcon;
use crate::navigation::{active_team_id, nav_for_window, resolved_screen, Navigation, Screen};
use crate::queries;

// ---------------------------------------------------------------------------
// Thread-title memory (tab labels)
// ---------------------------------------------------------------------------

/// Thread titles are tRPC-only — remember every title we've seen (list rows
/// on click, detail fetches on land) so `screen_title` can label the center
/// tab without a fetch.
#[derive(Default)]
struct SupportTitles {
    by_thread: HashMap<String, String>,
}

impl gpui::Global for SupportTitles {}

/// Record a thread's title for tab labeling.
pub(crate) fn remember_title(cx: &mut App, thread_id: &str, title: &str) {
    cx.default_global::<SupportTitles>()
        .by_thread
        .insert(thread_id.to_string(), title.to_string());
}

/// The last-seen title of a thread, if any (`screen_title` falls back to
/// "Support ticket").
pub(crate) fn title_of(cx: &App, thread_id: &str) -> Option<String> {
    cx.try_global::<SupportTitles>()
        .and_then(|titles| titles.by_thread.get(thread_id).cloned())
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// Precomputed display data for one message bubble (resolved before element
/// building so collection reads never overlap `cx.listener` borrows).
struct MessageRow {
    id: String,
    author: String,
    time: String,
    body: String,
    inbound: bool,
    internal: bool,
}

pub struct SupportThreadView {
    nav: Entity<Navigation>,
    thread_id: Option<String>,
    detail: Option<api::helpdesk::SupportThreadDetail>,
    /// Bumped per fetch — a stale response checks it before landing.
    fetch_seq: u64,
    /// Bumped per poll spawn — a superseded loop sees the mismatch and dies.
    poll_seq: u64,
    composer: Entity<InputState>,
    /// `true` = the composer submits an internal note instead of a reply.
    note_mode: bool,
    sending: bool,
    /// Close/reopen in flight.
    acting: bool,
    escalating: bool,
    /// The escalate dropdown's picked board (id, name) — cleared on success.
    escalate_board: Option<(String, String)>,
    /// Last mutation failure — a caption under the header, cleared on the
    /// next attempt.
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl SupportThreadView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let composer = cx.new(|cx| {
            InputState::new(window, cx)
                .auto_grow(1, 8)
                .placeholder(REPLY_PLACEHOLDER)
        });
        let mut subscriptions = Vec::new();
        // Cmd/Ctrl+Enter sends (the comment composer's gesture).
        subscriptions.push(cx.subscribe_in(
            &composer,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { secondary: true, .. } => {
                    this.submit(window, cx);
                }
                // Send button's disabled state tracks the draft.
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        ));
        // The escalate dropdown lists synced boards; bubbles resolve member
        // names from the users shape.
        let collections = Store::global(cx).collections().clone();
        subscriptions.push(cx.observe(&collections.boards, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&collections.users, |_, _, cx| cx.notify()));

        Self {
            nav,
            thread_id: None,
            detail: None,
            fetch_seq: 0,
            poll_seq: 0,
            composer,
            note_mode: false,
            sending: false,
            acting: false,
            escalating: false,
            escalate_board: None,
            error: None,
            _subscriptions: subscriptions,
        }
    }

    /// Point the view at a thread. Local state resets per thread; re-pointing
    /// at the SAME thread (tab reactivation) only restarts the poll — the
    /// screens panel calls this on every nav change.
    pub fn set_thread(
        &mut self,
        thread_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.thread_id.as_deref() == Some(thread_id.as_str()) {
            self.ensure_poll(cx);
            return;
        }
        self.thread_id = Some(thread_id);
        self.detail = None;
        self.note_mode = false;
        self.sending = false;
        self.acting = false;
        self.escalating = false;
        self.escalate_board = None;
        self.error = None;
        self.composer.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.set_placeholder(REPLY_PLACEHOLDER, window, cx);
        });
        self.fetch(cx);
        self.ensure_poll(cx);
        cx.notify();
    }

    /// Whether this thread's tab is the window's active screen — the poll
    /// gate (a hidden/closed tab must not keep fetching).
    fn is_visible(&self, cx: &App) -> bool {
        matches!(
            resolved_screen(&self.nav, cx),
            Some(Screen::SupportThread { thread_id })
                if self.thread_id.as_deref() == Some(thread_id.as_str())
        )
    }

    /// One seq-guarded `helpdesk.getThread` fetch; the landing closure
    /// re-checks both the seq and the thread id so a stale response for a
    /// previous thread can never repaint the current one.
    fn fetch(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(thread_id) = self.thread_id.clone() else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.fetch_seq += 1;
        let seq = self.fetch_seq;
        cx.spawn(async move |this, cx| {
            let call_id = thread_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::helpdesk::helpdesk_get_thread(&trpc, &call_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.fetch_seq != seq
                    || this.thread_id.as_deref() != Some(thread_id.as_str())
                {
                    return;
                }
                match result {
                    Ok(detail) => {
                        remember_title(cx, &thread_id, &detail.thread.title);
                        this.detail = Some(detail);
                        cx.notify();
                    }
                    Err(err) => {
                        log::warn!("[ui] helpdesk.getThread({thread_id}) failed: {err}");
                    }
                }
            });
        })
        .detach();
    }

    /// (Re)start the 15s poll. Every spawn bumps `poll_seq`, so at most ONE
    /// loop is ever live; the loop is entity-weak (`this.update` failing ends
    /// it) and additionally dies when the thread changes or the tab stops
    /// being the active screen — reactivation respawns it via `set_thread`.
    fn ensure_poll(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(thread_id) = self.thread_id.clone() else {
            return;
        };
        self.poll_seq += 1;
        let generation = self.poll_seq;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_secs(15))
                    .await;
                let keep_going = this.update(cx, |this, cx| {
                    if this.poll_seq != generation
                        || this.thread_id.as_deref() != Some(thread_id.as_str())
                        || !this.is_visible(cx)
                    {
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

    // -- mutations ------------------------------------------------------------

    /// Send the composer draft as a public reply or an internal note
    /// (mode-dependent). Optimistic clear; the draft is restored on failure
    /// (the comment composer's contract).
    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.sending {
            return;
        }
        let Some(thread_id) = self.thread_id.clone() else {
            return;
        };
        let draft = self.composer.read(cx).value().trim().to_string();
        if draft.is_empty() {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            log::warn!("[ui] helpdesk send skipped: no signed-in account");
            return;
        };
        let note = self.note_mode;
        self.sending = true;
        self.error = None;
        self.composer
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();

        let body = draft.clone();
        cx.spawn_in(window, async move |this, cx| {
            let call_id = thread_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    if note {
                        api::helpdesk::helpdesk_note(&trpc, &call_id, &body)
                    } else {
                        api::helpdesk::helpdesk_reply(&trpc, &call_id, &body)
                    }
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.sending = false;
                match result {
                    Ok(_) => this.fetch(cx),
                    Err(err) => {
                        log::warn!("[ui] helpdesk send failed: {err}");
                        let message = match err {
                            api::ApiError::Http { message, .. } => message,
                            other => other.to_string(),
                        };
                        this.error = Some(message);
                        // Give the draft back (kept only on failure).
                        this.composer
                            .update(cx, |input, cx| input.set_value(draft, window, cx));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Close (resolve) or reopen the ticket, then refetch — the header
    /// action follows the thread's status.
    fn set_status(&mut self, close: bool, cx: &mut gpui::Context<Self>) {
        if self.acting {
            return;
        }
        let Some(thread_id) = self.thread_id.clone() else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.acting = true;
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let call_id = thread_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    if close {
                        api::helpdesk::helpdesk_close(&trpc, &call_id)
                    } else {
                        api::helpdesk::helpdesk_reopen(&trpc, &call_id)
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.acting = false;
                match result {
                    Ok(()) => this.fetch(cx),
                    Err(err) => {
                        log::warn!("[ui] helpdesk close/reopen({thread_id}) failed: {err}");
                        let message = match err {
                            api::ApiError::Http { message, .. } => message,
                            other => other.to_string(),
                        };
                        this.error = Some(message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// File the escalated issue on the picked board and link it (server
    /// rejects a second escalation), then refetch — the dropdown swaps into
    /// the linked-issue chip.
    fn escalate(&mut self, cx: &mut gpui::Context<Self>) {
        if self.escalating {
            return;
        }
        let Some(thread_id) = self.thread_id.clone() else {
            return;
        };
        let Some((board_id, _)) = self.escalate_board.clone() else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.escalating = true;
        self.error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let call_id = thread_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::helpdesk::helpdesk_escalate(&trpc, &call_id, &board_id, None)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.escalating = false;
                match result {
                    Ok(_) => {
                        this.escalate_board = None;
                        this.fetch(cx);
                    }
                    Err(err) => {
                        log::warn!("[ui] helpdesk.escalate({thread_id}) failed: {err}");
                        let message = match err {
                            api::ApiError::Http { message, .. } => message,
                            other => other.to_string(),
                        };
                        this.error = Some(message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Flip the composer between Reply and Internal-note mode.
    fn set_note_mode(&mut self, note: bool, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.note_mode == note {
            return;
        }
        self.note_mode = note;
        self.composer.update(cx, |input, cx| {
            input.set_placeholder(
                if note {
                    NOTE_PLACEHOLDER
                } else {
                    REPLY_PLACEHOLDER
                },
                window,
                cx,
            );
        });
        cx.notify();
    }
}

const REPLY_PLACEHOLDER: &str = "Reply to the reporter…";
const NOTE_PLACEHOLDER: &str = "Add an internal note (never emailed)…";

/// The reporter's display label: name, else email, else a generic.
fn reporter_label(name: Option<&str>, email: Option<&str>) -> String {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| email.map(str::to_string).filter(|email| !email.is_empty()))
        .unwrap_or_else(|| "Reporter".to_string())
}

impl Render for SupportThreadView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(detail) = self.detail.clone() else {
            // First fetch in flight — skeleton, never a wrong empty state.
            return v_flex()
                .size_full()
                .p_4()
                .gap_2()
                .child(Skeleton::new().h_4().w_64())
                .child(Skeleton::new().h_4().w_48())
                .child(Skeleton::new().h_4().w_56())
                .into_any_element();
        };

        // ---- precompute display data (collection reads before listeners) ---
        let reporter = reporter_label(
            detail.thread.reporter_name.as_deref(),
            detail.thread.reporter_email.as_deref(),
        );
        let resolved = detail.thread.status.as_deref() == Some("resolved");
        let team_id = detail
            .thread
            .team_id
            .clone()
            .or_else(|| active_team_id(&self.nav, cx));
        let boards: Vec<(String, String)> = team_id
            .as_deref()
            .map(|id| {
                Store::global(cx)
                    .collections()
                    .boards_in_team(id, cx)
                    .into_iter()
                    .map(|board| (board.id, board.name))
                    .collect()
            })
            .unwrap_or_default();

        let rows: Vec<MessageRow> = {
            let collections = Store::global(cx).collections().clone();
            let users = collections.users.read(cx);
            detail
                .messages
                .iter()
                .map(|message| {
                    let inbound = message.direction.as_deref() == Some("inbound");
                    let author = if inbound {
                        reporter.clone()
                    } else {
                        match message.author_user_id.as_deref() {
                            Some(id) => comments::user_label(id, users.get(id)),
                            None => "Member".to_string(),
                        }
                    };
                    MessageRow {
                        id: message.id.clone(),
                        author,
                        time: message
                            .created_at
                            .as_deref()
                            .map(crate::inbox::relative_time)
                            .unwrap_or_default(),
                        body: message.body.clone().unwrap_or_default(),
                        inbound,
                        internal: message.visibility.as_deref() == Some("internal"),
                    }
                })
                .collect()
        };

        let theme = cx.theme();
        let radius = theme.radius;
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let muted_bg = theme.muted;
        let accent = theme.accent;
        let warning = theme.warning;
        let danger = theme.danger;
        let border = theme.border;

        // ---- header ---------------------------------------------------------
        let status_button = {
            let mut button = Button::new("support-status").small().outline();
            button = if resolved {
                button.label("Reopen ticket")
            } else {
                button.label("Close ticket")
            };
            button
                .loading(self.acting)
                .disabled(self.acting)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_status(!resolved, cx);
                }))
        };

        let escalate_area: gpui::AnyElement = match &detail.linked_issue {
            Some(issue) => {
                // Already escalated: the issue chip opens the issue tab.
                let label = match issue.identifier.as_deref() {
                    Some(identifier) => format!(
                        "{identifier} {}",
                        issue.title.as_deref().unwrap_or_default()
                    ),
                    None => issue.title.clone().unwrap_or_else(|| "Issue".to_string()),
                };
                let issue_id = issue.id.clone();
                Button::new("support-linked-issue")
                    .xsmall()
                    .outline()
                    .icon(Icon::from(ExpIcon::CircleDot))
                    .label(SharedString::from(label.trim().to_string()))
                    .on_click(move |_: &ClickEvent, window, cx| {
                        window.dispatch_action(
                            Box::new(OpenIssue {
                                issue_id: issue_id.clone(),
                            }),
                            cx,
                        );
                    })
                    .into_any_element()
            }
            None => {
                // Unlinked: board dropdown + confirm.
                let picked = self.escalate_board.clone();
                let dropdown_label: SharedString = picked
                    .as_ref()
                    .map(|(_, name)| SharedString::from(name.clone()))
                    .unwrap_or_else(|| "Create issue on board…".into());
                let view = cx.entity().clone();
                let menu_boards = boards.clone();
                let picked_id = picked.as_ref().map(|(id, _)| id.clone());
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("support-escalate-board")
                            .xsmall()
                            .outline()
                            .label(dropdown_label)
                            .disabled(boards.is_empty())
                            .dropdown_menu(move |mut menu, _window, _cx| {
                                for (id, name) in &menu_boards {
                                    let view = view.clone();
                                    let choice = (id.clone(), name.clone());
                                    let checked = picked_id.as_deref() == Some(id.as_str());
                                    menu = menu.item(
                                        PopupMenuItem::new(SharedString::from(name.clone()))
                                            .checked(checked)
                                            .on_click(move |_, _, cx| {
                                                let choice = choice.clone();
                                                view.update(cx, |this, cx| {
                                                    this.escalate_board = Some(choice);
                                                    cx.notify();
                                                });
                                            }),
                                    );
                                }
                                menu
                            }),
                    )
                    .child(
                        Button::new("support-escalate")
                            .xsmall()
                            .primary()
                            .label("Escalate to issue")
                            .loading(self.escalating)
                            .disabled(self.escalating || picked.is_none())
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.escalate(cx);
                            })),
                    )
                    .into_any_element()
            }
        };

        let header = v_flex()
            .w_full()
            .flex_shrink_0()
            .px_4()
            .pt_4()
            .pb_3()
            .gap_2()
            .border_b_1()
            .border_color(border)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(SharedString::from(detail.thread.title.clone())),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .px_1p5()
                            .rounded(radius)
                            .text_xs()
                            .when(resolved, |this| {
                                this.bg(muted_bg).text_color(muted).child("Resolved")
                            })
                            .when(!resolved, |this| {
                                this.bg(theme::tokens::GREEN.to_hsla().opacity(0.15))
                                    .text_color(theme::tokens::GREEN.to_hsla())
                                    .child("Open")
                            }),
                    )
                    .child(status_button),
            )
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(reporter.clone())),
                    )
                    .child(div().flex_1())
                    .child(escalate_area),
            )
            .when_some(self.error.clone(), |this, message| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(danger)
                        .child(SharedString::from(message)),
                )
            });

        // ---- messages -------------------------------------------------------
        let bubbles: Vec<gpui::AnyElement> = rows
            .iter()
            .map(|row| {
                let (bubble_bg, bubble_border) = if row.internal {
                    (warning.opacity(0.12), Some(warning.opacity(0.4)))
                } else if row.inbound {
                    (muted_bg, None)
                } else {
                    (accent.opacity(0.5), None)
                };
                let mut bubble = v_flex()
                    .max_w(relative(0.78))
                    .min_w_0()
                    .px_3()
                    .py_2()
                    .gap_1()
                    .rounded(radius)
                    .bg(bubble_bg);
                if let Some(color) = bubble_border {
                    bubble = bubble.border_1().border_color(color);
                }
                bubble = bubble.child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .text_xs()
                        .text_color(muted)
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .child(SharedString::from(row.author.clone())),
                        )
                        .child(SharedString::from(row.time.clone()))
                        .when(row.internal, |this| {
                            this.child(
                                div()
                                    .px_1()
                                    .rounded(radius)
                                    .bg(warning.opacity(0.2))
                                    .text_color(warning)
                                    .child("Internal"),
                            )
                        }),
                );
                // Plain text bodies (support email content, not GFM):
                // newline-split so paragraphs survive; blank lines become
                // spacing.
                let mut body = v_flex().gap_0p5().text_sm().text_color(fg);
                for line in row.body.lines() {
                    if line.trim().is_empty() {
                        body = body.child(div().h_2());
                    } else {
                        body = body.child(div().child(SharedString::from(line.to_string())));
                    }
                }
                bubble = bubble.child(body);

                h_flex()
                    .id(SharedString::from(format!("support-msg-{}", row.id)))
                    .w_full()
                    .when(!row.inbound, |this| this.justify_end())
                    .child(bubble)
                    .into_any_element()
            })
            .collect();

        let messages: gpui::AnyElement = if bubbles.is_empty() {
            div()
                .p_4()
                .text_sm()
                .text_color(muted)
                .child("No messages yet.")
                .into_any_element()
        } else {
            div()
                .id("support-thread-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .child(v_flex().p_4().gap_3().children(bubbles))
                .into_any_element()
        };

        // ---- composer -------------------------------------------------------
        let has_draft = !self.composer.read(cx).value().trim().is_empty();
        let composer = v_flex()
            .w_full()
            .flex_shrink_0()
            .px_4()
            .py_3()
            .gap_2()
            .border_t_1()
            .border_color(border)
            .when(self.note_mode, |this| this.bg(warning.opacity(0.06)))
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        Button::new("support-mode-reply")
                            .ghost()
                            .xsmall()
                            .label("Reply")
                            .selected(!self.note_mode)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.set_note_mode(false, window, cx);
                            })),
                    )
                    .child(
                        Button::new("support-mode-note")
                            .ghost()
                            .xsmall()
                            .label("Internal note")
                            .selected(self.note_mode)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.set_note_mode(true, window, cx);
                            })),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_end()
                    .child(div().flex_1().min_w_0().child(Input::new(&self.composer).w_full()))
                    .child(
                        Button::new("support-send")
                            .primary()
                            .small()
                            .icon(Icon::from(ExpIcon::Send))
                            .loading(self.sending)
                            .disabled(self.sending || !has_draft)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.submit(window, cx);
                            })),
                    ),
            );

        v_flex()
            .size_full()
            .min_h_0()
            .child(header)
            .child(messages)
            .child(composer)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reporter_label_prefers_name_then_email() {
        assert_eq!(
            reporter_label(Some("Jane"), Some("jane@example.com")),
            "Jane"
        );
        assert_eq!(
            reporter_label(Some("  "), Some("jane@example.com")),
            "jane@example.com"
        );
        assert_eq!(reporter_label(None, Some("jane@example.com")), "jane@example.com");
        assert_eq!(reporter_label(None, None), "Reporter");
        assert_eq!(reporter_label(Some(""), Some("")), "Reporter");
    }
}
