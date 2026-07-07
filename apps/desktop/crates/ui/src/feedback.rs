//! "Send Feedback" — join + open the PUBLIC feedback board in-app.
//!
//! Web parity: `/w/feedback` shows `WorkspaceJoinGate`
//! (`components/workspace/join-gate.tsx`) to a signed-in non-member — under
//! the v6 membership-only sync a public board never syncs until the user
//! explicitly joins via the self-service `workspaceMembers.join` (public-only
//! server-side). The desktop mirrors that flow instead of always bouncing to
//! the browser:
//!
//! 1. board already synced (member) → switch the window to it;
//! 2. otherwise resolve the bootstrap board via the public
//!    `workspaces.getBySlug` query (slug `feedback`, seeded by
//!    `bootstrap-cloud.ts`) — a member whose sync lags switches directly, a
//!    non-member gets a join-gate dialog (the web card's copy) whose Join
//!    button calls `workspaceMembers.join`, gates on the workspace appearing
//!    in the synced collection (§4.1), then switches;
//! 3. anything unavailable — signed out, a self-hosted instance without the
//!    public board (NOT_FOUND), a non-public slug squatter, transport errors
//!    — falls back to the cloud `/feedback` page in the system browser.
//!
//! Participant/anonymization semantics need no extra work here: once joined,
//! the synced `is_public`/`public_write_policy` fields drive the existing
//! permission gating, and public-board co-members render anonymized via
//! `domain::rows::member_fallback_label` — same as web/iOS/Android.

use gpui::{
    div, px, App, AppContext as _, AsyncWindowContext, IntoElement, ParentElement, Render,
    SharedString, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
};
use sync::Store;

use crate::actions::SendFeedback;
use crate::navigation::switch_workspace;
use crate::queries;

/// The bootstrap public feedback board's slug (web `/w/feedback`;
/// `bootstrap-cloud.ts` `PUBLIC_WORKSPACE_SLUG`).
const FEEDBACK_WORKSPACE_SLUG: &str = "feedback";

/// Register the App-global [`SendFeedback`] handler (call once from
/// `ui::init`).
pub fn init(cx: &mut App) {
    cx.on_action(|_: &SendFeedback, cx| open(cx));
}

/// Resolve the public feedback board and open it in-app; browser fallback
/// whenever that path is unavailable.
fn open(cx: &mut App) {
    if queries::active_account(cx).is_none() {
        // Signed out — the cloud page handles auth itself.
        browser_fallback(cx.background_executor());
        return;
    }
    crate::navigation::on_active_window(cx, |window, cx| {
        // Fast path: already a member — the board is in the synced set
        // (membership is exactly what makes it sync, v6).
        let synced = Store::global(cx)
            .collections()
            .workspaces
            .read(cx)
            .iter()
            .find(|workspace| {
                workspace.slug.as_deref() == Some(FEEDBACK_WORKSPACE_SLUG)
                    && workspace.is_public == Some(true)
            })
            .map(|workspace| workspace.id.clone());
        if let Some(workspace_id) = synced {
            switch_workspace(window, cx, workspace_id);
            return;
        }
        resolve_and_gate(window, cx);
    });
}

/// Non-member path: `workspaces.getBySlug` → member-but-lagging switch, or
/// the join-gate dialog, or the browser fallback.
fn resolve_and_gate(window: &mut Window, cx: &mut App) {
    let Some(trpc) = queries::trpc_client(cx) else {
        browser_fallback(cx.background_executor());
        return;
    };
    let workspaces = Store::global(cx).collections().workspaces.clone();
    window
        .spawn(cx, async move |window| {
            let resolved = window
                .background_executor()
                .spawn(async move {
                    api::workspaces::workspaces_get_by_slug(&trpc, FEEDBACK_WORKSPACE_SLUG)
                })
                .await;

            let workspace = match resolved {
                Ok(workspace) if workspace.is_public == Some(true) => workspace,
                Ok(_) => {
                    // A non-public workspace owns the slug (self-host oddity)
                    // — never self-join it.
                    log::info!(
                        "[ui] feedback: `{FEEDBACK_WORKSPACE_SLUG}` workspace is not public; opening the browser"
                    );
                    browser_fallback_async(window);
                    return;
                }
                Err(err) => {
                    // NOT_FOUND (instance without the bootstrap board) or a
                    // transport failure — the cloud page always works.
                    log::info!("[ui] feedback: board lookup failed ({err}); opening the browser");
                    browser_fallback_async(window);
                    return;
                }
            };

            if workspace.membership.is_some() {
                // Already a member, sync just hasn't caught up (the fast
                // path missed) — gate like the invite-accept flow, then
                // switch.
                let workspace_id = workspace.id.clone();
                queries::await_row_visible(&workspaces, &workspace_id, window).await;
                let _ = window.update(|window, cx| {
                    switch_workspace(window, cx, workspace_id);
                });
                return;
            }

            let _ = window.update(|window, cx| open_join_gate(window, cx, workspace));
        })
        .detach();
}

/// The explicit-consent gate (web `WorkspaceJoinGate` card): joining a public
/// board is never silent — it makes the user a visible (anonymized)
/// participant and starts syncing the board.
fn open_join_gate(window: &mut Window, cx: &mut App, workspace: api::workspaces::WorkspaceBySlugOut) {
    if window.has_active_dialog(cx) {
        return; // never stack over an open modal
    }
    let name = workspace
        .name
        .clone()
        .unwrap_or_else(|| "the feedback board".to_string());
    let title = SharedString::from(format!("Join {name}"));
    let view = cx.new(|_| FeedbackJoinGate {
        workspace_id: workspace.id,
        joining: false,
        error: None,
    });
    window.open_dialog(cx, move |dialog, _window, cx| {
        let busy = view.read(cx).joining;
        dialog
            .w(px(416.))
            .title(title.clone())
            .overlay_closable(!busy)
            .keyboard(!busy)
            .on_ok({
                let view = view.clone();
                move |_, window, cx| {
                    view.update(cx, |view, cx| view.join(window, cx));
                    false
                }
            })
            .child(view.clone())
    });
}

/// Open the cloud `/feedback` page in the system browser — the pre-v6
/// behavior, kept as the fallback (self-hosted instances have no public
/// board; their feedback lands on the cloud).
fn browser_fallback(executor: &gpui::BackgroundExecutor) {
    let url = format!(
        "{}/feedback",
        crate::login::cloud_instance().trim_end_matches('/')
    );
    executor
        .spawn(async move {
            if let Err(err) = api::opener::open_in_browser(&url) {
                log::warn!("[ui] feedback: browser open failed: {err}");
            }
        })
        .detach();
}

fn browser_fallback_async(window: &AsyncWindowContext) {
    browser_fallback(window.background_executor());
}

struct FeedbackJoinGate {
    workspace_id: String,
    joining: bool,
    error: Option<SharedString>,
}

impl FeedbackJoinGate {
    /// `workspaceMembers.join` → gate on the workspaces echo → switch (the
    /// same close-and-navigate flow as the invite accept).
    fn join(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.joining {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };
        let workspace_id = self.workspace_id.clone();
        self.joining = true;
        self.error = None;
        cx.notify();

        cx.spawn_in(window, async move |this, window| {
            let join_id = workspace_id.clone();
            let result = window
                .background_executor()
                .spawn(async move { api::workspaces::workspace_members_join(&trpc, &join_id) })
                .await;

            match result {
                Ok(_) => {
                    // §4.1 gated flow: membership changes the shape scope —
                    // wait for the board to appear before switching.
                    let workspaces = window
                        .update(|_, cx| Store::global(cx).collections().workspaces.clone())
                        .ok();
                    if let Some(workspaces) = workspaces {
                        queries::await_row_visible(&workspaces, &workspace_id, window).await;
                    }
                    let _ = this.update_in(window, |_, window, cx| {
                        window.close_dialog(cx);
                        switch_workspace(window, cx, workspace_id);
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, _, cx| {
                        this.joining = false;
                        this.error = Some(format!("{err}").into());
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }
}

impl Render for FeedbackJoinGate {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Web join-gate copy, verbatim.
        let mut body = v_flex().gap_3().child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(
                    "This is a public board. Join it to browse issues, follow discussions \
                     and share feedback. You can leave again anytime from the board settings.",
                ),
        );
        if let Some(error) = &self.error {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        body.child(
            h_flex()
                .justify_end()
                .gap_2()
                .child(
                    Button::new("feedback-join-cancel")
                        .outline()
                        .small()
                        .label("Cancel")
                        .disabled(self.joining)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.joining {
                                return;
                            }
                            window.close_dialog(cx);
                        })),
                )
                .child(
                    Button::new("feedback-join-primary")
                        .primary()
                        .small()
                        .label(if self.joining { "Joining…" } else { "Join board" })
                        .disabled(self.joining)
                        .loading(self.joining)
                        .on_click(cx.listener(|this, _, window, cx| this.join(window, cx))),
                ),
        )
    }
}
