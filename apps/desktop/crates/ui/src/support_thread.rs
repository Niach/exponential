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
    div, prelude::FluentBuilder as _, px, relative, App, AppContext as _, ClickEvent, Entity,
    FontWeight, InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputEvent, Textarea, TextareaState},
    menu::{DropdownMenu as _, PopupMenuItem},
    scroll::ScrollableElement as _,
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Selectable as _,
};

use crate::controls::WebControl as _;
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
    /// EXP-525: the widget submission behind the thread — the details rail's
    /// Context section (web `ThreadDetails`). Fetched once per thread.
    submission: Option<api::widgets::WidgetSubmission>,
    /// The reporter the reply placeholder was last synced for (render-time
    /// sync — the fetch landing has no `Window`).
    placeholder_reporter: Option<String>,
    /// Bumped per fetch — a stale response checks it before landing.
    fetch_seq: u64,
    /// Bumped per poll spawn — a superseded loop sees the mismatch and dies.
    poll_seq: u64,
    composer: Entity<TextareaState>,
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
            TextareaState::new(window, cx)
                .auto_grow(1, 8)
                .placeholder(reply_placeholder("the reporter"))
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
            submission: None,
            placeholder_reporter: None,
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
        // Same boot-order race as the PR diff: the screens panel points its
        // views from its CONSTRUCTOR, before the session finishes validating,
        // so `fetch` would find no client and drop the only load this thread
        // ever gets. Stay UNPOINTED instead — the Synced re-drive re-enters
        // here with a live client.
        if queries::trpc_client(cx).is_none() {
            return;
        }
        self.thread_id = Some(thread_id);
        self.detail = None;
        self.submission = None;
        self.placeholder_reporter = None;
        self.note_mode = false;
        self.sending = false;
        self.acting = false;
        self.escalating = false;
        self.escalate_board = None;
        self.error = None;
        self.composer.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.set_placeholder(reply_placeholder("the reporter"), window, cx);
        });
        self.fetch(cx);
        self.fetch_submission(cx);
        self.ensure_poll(cx);
        cx.notify();
    }

    /// EXP-525: the details rail's Context data — one fetch per thread
    /// (`widgets.submissionForThread`; `None` for mail-only threads).
    fn fetch_submission(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(thread_id) = self.thread_id.clone() else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let call_id = thread_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::widgets::submission_for_thread(&trpc, &call_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.thread_id.as_deref() != Some(thread_id.as_str()) {
                    return;
                }
                match result {
                    Ok(submission) => {
                        this.submission = submission;
                        cx.notify();
                    }
                    Err(err) => {
                        log::warn!("[ui] widgets.submissionForThread({thread_id}) failed: {err}");
                    }
                }
            });
        })
        .detach();
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
        let reporter = self
            .detail
            .as_ref()
            .map(|detail| {
                reporter_label(
                    detail.thread.reporter_name.as_deref(),
                    detail.thread.reporter_email.as_deref(),
                )
            })
            .unwrap_or_else(|| "the reporter".to_string());
        self.composer.update(cx, |input, cx| {
            input.set_placeholder(
                if note {
                    SharedString::from(NOTE_PLACEHOLDER)
                } else {
                    reply_placeholder(&reporter)
                },
                window,
                cx,
            );
        });
        cx.notify();
    }
}

/// Web composer placeholder, byte-for-byte
/// (`helpdesk/support-inbox.tsx`): `Reply to {reporter}… (emailed to them)`.
fn reply_placeholder(reporter: &str) -> SharedString {
    SharedString::from(format!("Reply to {reporter}… (emailed to them)"))
}

const NOTE_PLACEHOLDER: &str = "Add an internal note… (never sent to the reporter)";

/// The reporter's display label: name, else email, else a generic.
fn reporter_label(name: Option<&str>, email: Option<&str>) -> String {
    name.map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| email.map(str::to_string).filter(|email| !email.is_empty()))
        .unwrap_or_else(|| "Reporter".to_string())
}

impl Render for SupportThreadView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
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
        // Keep the reply placeholder addressed to the real reporter (the
        // fetch landing has no Window, so it syncs here — guarded, or the
        // notify would re-render forever).
        if !self.note_mode && self.placeholder_reporter.as_deref() != Some(reporter.as_str()) {
            self.placeholder_reporter = Some(reporter.clone());
            let placeholder = reply_placeholder(&reporter);
            self.composer.update(cx, |input, cx| {
                input.set_placeholder(placeholder, window, cx);
            });
        }
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
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let warning = theme.warning;
        let danger = theme.danger;

        // ---- header (web `support-inbox.tsx` conversation header) ----------
        let status_button = {
            let mut button = Button::new("support-status")
                .outline().cursor_pointer()
                .web_sm()
                .flex_shrink_0();
            button = if resolved {
                button
                    .icon(Icon::from(ExpIcon::Undo2).size_3())
                    .label("Reopen ticket")
            } else {
                button
                    .icon(Icon::from(ExpIcon::Check).size_3())
                    .label("Close ticket")
            };
            button
                .loading(self.acting)
                .disabled(self.acting)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_status(!resolved, cx);
                }))
        };

        let header = v_flex()
            .w_full()
            .flex_shrink_0()
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(SharedString::from(reporter.clone())),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .text_color(muted)
                                    .child(SharedString::from(detail.thread.title.clone())),
                            ),
                    )
                    .child(status_button),
            )
            .when_some(self.error.clone(), |this, message| {
                this.child(
                    div()
                        .px_4()
                        .py_1()
                        .text_xs()
                        .text_color(danger)
                        .child(SharedString::from(message)),
                )
            });

        // ---- details rail (web `ThreadDetails`, EXP-525 brings it back) ----
        // EXP-698: the shared glass section header, sentence case — the rail's
        // headings used to be a bespoke uppercase caption.
        let mut reporter_section = v_flex()
            .child(crate::surface::glass_section_header("Reporter", None, None, cx))
            .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .truncate()
                .child(SharedString::from(reporter.clone())),
        );
        if let Some(email) = detail
            .thread
            .reporter_email
            .clone()
            .filter(|email| !email.is_empty())
        {
            reporter_section = reporter_section.child(
                div()
                    .truncate()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(email)),
            );
        }
        if let Some(seen) = detail.thread.last_reporter_seen_at.as_deref() {
            reporter_section = reporter_section.child(
                div()
                    .mt_1()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(format!(
                        "Last seen {}",
                        crate::inbox::relative_time(seen)
                    ))),
            );
        }

        let context_section = self.submission.as_ref().and_then(|submission| {
            let mut section = v_flex()
                .child(crate::surface::glass_section_header("Context", None, None, cx));
            let mut any = false;
            if let Some(url) = submission.page_url.clone().filter(|url| !url.is_empty()) {
                any = true;
                section = section.child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(url)),
                );
            }
            if let Some(agent) = submission.user_agent.clone().filter(|ua| !ua.is_empty()) {
                any = true;
                section = section.child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(agent)),
                );
            }
            if let (Some(w), Some(h)) = (submission.viewport_width, submission.viewport_height) {
                any = true;
                section = section.child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(format!("Viewport {w}×{h}"))),
                );
            }
            any.then(|| section.into_any_element())
        });

        let escalate_section: gpui::AnyElement = match &detail.linked_issue {
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
                v_flex()
                    .child(crate::surface::glass_section_header("Linked issue", None, None, cx))
                    .child(
                        Button::new("support-linked-issue")
                            .outline().cursor_pointer()
                            .web_xs()
                            .icon(Icon::from(ExpIcon::CircleDot))
                            .label(SharedString::from(label.trim().to_string()))
                            .on_click(move |_: &ClickEvent, window, cx| {
                                window.dispatch_action(
                                    Box::new(OpenIssue {
                                        issue_id: issue_id.clone(),
                                    }),
                                    cx,
                                );
                            }),
                    )
                    .into_any_element()
            }
            None => {
                // Unlinked: web Escalate section — hint, board picker,
                // "Create issue".
                let picked = self.escalate_board.clone();
                let dropdown_label: SharedString = picked
                    .as_ref()
                    .map(|(_, name)| SharedString::from(name.clone()))
                    .unwrap_or_else(|| "Pick a board".into());
                let view = cx.entity().clone();
                let menu_boards = boards.clone();
                let picked_id = picked.as_ref().map(|(id, _)| id.clone());
                v_flex()
                    .gap_2()
                    .child(crate::surface::glass_section_header("Escalate", None, None, cx))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Create an issue from this ticket on one of the team's boards."),
                    )
                    .child(
                        Button::new("support-escalate-board")
                            .outline().cursor_pointer()
                            .web_input_sm()
                            .w_full()
                            .cursor_pointer()
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
                            .primary().cursor_pointer()
                            .web_sm()
                            .w_full()
                            .label("Create issue")
                            .loading(self.escalating)
                            .disabled(self.escalating || picked.is_none())
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.escalate(cx);
                            })),
                    )
                    .into_any_element()
            }
        };

        let details_rail = v_flex()
            .w(px(288.))
            .flex_shrink_0()
            .h_full()
            .gap_4()
            .px_4()
            .py_4()
            .border_l_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .overflow_hidden()
            .child(reporter_section)
            .children(context_section)
            .child(escalate_section);

        // ---- messages (web bubble colors: inbound muted, outbound primary,
        // internal amber; meta line UNDER the body) -------------------------
        let outbound = theme::tokens::PRIMARY.to_hsla();
        let outbound_fg = theme::tokens::PRIMARY_FOREGROUND.to_hsla();
        let bubbles: Vec<gpui::AnyElement> = rows
            .iter()
            .map(|row| {
                let (bubble_bg, bubble_border, body_color, meta_color) = if row.internal {
                    (
                        warning.opacity(0.12),
                        Some(warning.opacity(0.4)),
                        fg,
                        muted,
                    )
                } else if row.inbound {
                    (
                        theme::tokens::glass::FILL_CARD.to_hsla(),
                        None,
                        fg,
                        muted,
                    )
                } else {
                    (outbound, None, outbound_fg, outbound_fg.opacity(0.7))
                };
                let mut bubble = v_flex()
                    .max_w(relative(0.85))
                    .min_w_0()
                    .px_3p5()
                    .py_2p5()
                    .gap_1()
                    .rounded(px(theme::tokens::radius::XL))
                    .map(|bubble| {
                        // Web: the corner nearest the sender flattens
                        // (`rounded-bl-sm` / `rounded-br-sm`).
                        if row.inbound {
                            bubble.rounded_bl(px(4.))
                        } else {
                            bubble.rounded_br(px(4.))
                        }
                    })
                    .bg(bubble_bg);
                if let Some(color) = bubble_border {
                    bubble = bubble.border_1().border_color(color);
                }
                if row.internal {
                    bubble = bubble.child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .text_xs()
                            .text_color(warning)
                            .child(Icon::from(ExpIcon::FileText).size_3())
                            .child("Internal note"),
                    );
                }
                // Plain text bodies (support email content, not GFM):
                // newline-split so paragraphs survive; blank lines become
                // spacing.
                let mut body = v_flex().gap_0p5().text_sm().text_color(body_color);
                for line in row.body.lines() {
                    if line.trim().is_empty() {
                        body = body.child(div().h_2());
                    } else {
                        body = body.child(div().child(SharedString::from(line.to_string())));
                    }
                }
                bubble = bubble.child(body);
                // Web meta line: "{author} · {time}" under the body.
                let meta = if row.time.is_empty() {
                    row.author.clone()
                } else {
                    format!("{} · {}", row.author, row.time)
                };
                bubble = bubble.child(
                    div()
                        .text_xs()
                        .text_color(meta_color)
                        .child(SharedString::from(meta)),
                );

                h_flex()
                    .id(SharedString::from(format!("support-msg-{}", row.id)))
                    .w_full()
                    .when(!row.inbound, |this| this.justify_end())
                    .child(bubble)
                    .into_any_element()
            })
            .collect();

        let messages: gpui::AnyElement = if bubbles.is_empty() {
            crate::controls::empty_state(
                Icon::from(ExpIcon::LifeBuoy),
                "No messages yet",
                "The conversation with the reporter shows up here.",
                cx,
            )
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
        // EXP-277: the composer keeps ONE faint separator (the note-mode tint
        // needs an edge) — the glass row stroke, not the chrome border.
        let composer = v_flex()
            .w_full()
            .flex_shrink_0()
            .px_4()
            .py_3()
            .gap_2()
            .border_t_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .when(self.note_mode, |this| this.bg(warning.opacity(0.06)))
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        // Web mode pills: icon + label, `h-6 rounded-full`.
                        Button::new("support-mode-reply")
                            .ghost().cursor_pointer()
                            .web_xs()
                            .icon(Icon::from(ExpIcon::Mail).size_3())
                            .label("Reply")
                            .selected(!self.note_mode)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.set_note_mode(false, window, cx);
                            })),
                    )
                    .child(
                        Button::new("support-mode-note")
                            .ghost().cursor_pointer()
                            .web_xs()
                            .icon(Icon::from(ExpIcon::FileText).size_3())
                            .label("Internal note")
                            .selected(self.note_mode)
                            .when(self.note_mode, |button| button.text_color(warning))
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
                    .child(v_flex().flex_1().min_w_0().child(Textarea::new(&self.composer).w_full()))
                    .child(
                        Button::new("support-send")
                            .primary().cursor_pointer()
                            .web_icon_sm()
                            .icon(Icon::from(ExpIcon::Send))
                            .loading(self.sending)
                            .disabled(self.sending || !has_draft)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.submit(window, cx);
                            })),
                    ),
            );

        // EXP-525: conversation column + the details rail (web 3-pane's
        // right two panes; the thread list stays in the sidebar tool).
        h_flex()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .items_start()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .min_h_0()
                    .overflow_hidden()
                    .child(header)
                    .child(messages)
                    .child(composer),
            )
            .child(details_rail)
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
