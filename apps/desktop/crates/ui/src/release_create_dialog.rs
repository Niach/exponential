//! Release creation dialog (EXP-62): name + the same multi-select issue
//! picker as the detail's "Add issues" dialog — a release only comes into
//! existence WITH its issues (Create stays disabled until ≥1 is checked; an
//! empty release is useless). Two entry points: the Releases tool header's
//! "+" (empty preselection) and the bulk bar's "New release" (pre-seeded with
//! the selection — preselected rows stay offered even when closed, so an
//! explicit bulk pick is never silently dropped). One `releases.create` call
//! attaches up to 200 issues in the same server transaction; larger
//! selections chunk the rest through `releases.addIssues`. Success gates on
//! the Electric echo, then lands on the new release's detail (the Releases
//! tool drill-down).

use std::collections::HashSet;

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, Entity,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement as _,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};
use sync::Store;

use domain::rows::Issue;
use domain::IssueStatus;

use crate::icons::ExpIcon;
use crate::queries;

/// Open the creation dialog for `workspace_id`, optionally pre-seeded with a
/// bulk selection.
pub fn open(window: &mut Window, cx: &mut App, workspace_id: String, preselected: Vec<String>) {
    let view = cx.new(|cx| ReleaseCreateDialogView::new(workspace_id, preselected, window, cx));
    window.open_dialog(cx, move |dialog, _window, cx| {
        let busy = view.read(cx).submitting;
        dialog
            .w(px(480.))
            .title("New release")
            .overlay_closable(!busy)
            .keyboard(!busy)
            .child(view.clone())
    });
}

pub struct ReleaseCreateDialogView {
    workspace_id: String,
    name: Entity<InputState>,
    search: Entity<InputState>,
    checked: HashSet<String>,
    /// Bulk-bar seed: these ids stay in the candidate pool even when their
    /// status is closed (the user picked them explicitly).
    preselected: HashSet<String>,
    submitting: bool,
    error: Option<SharedString>,
    focused_once: bool,
    _subscriptions: Vec<Subscription>,
}

impl ReleaseCreateDialogView {
    fn new(
        workspace_id: String,
        preselected: Vec<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let name =
            cx.new(|cx| InputState::new(window, cx).placeholder("Release name (optional)"));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search issues…"));
        let issues = Store::global(cx).collections().issues.clone();
        let subscriptions = vec![
            cx.subscribe(&name, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            // Candidates are a live collection read — echoes re-render.
            cx.observe(&issues, |_, _, cx| cx.notify()),
        ];

        let preselected: HashSet<String> = preselected.into_iter().collect();
        Self {
            workspace_id,
            name,
            search,
            checked: preselected.clone(),
            preselected,
            submitting: false,
            error: None,
            focused_once: false,
            _subscriptions: subscriptions,
        }
    }

    /// The full candidate pool (no search filter — the prune's reference so
    /// a narrowing search never wipes checks): workspace issues that are
    /// still actionable (not `done`/`cancelled`/`duplicate`, not archived),
    /// plus any preselected rows regardless of status.
    fn pool(&self, cx: &App) -> Vec<Issue> {
        queries::workspace_issues(cx, &self.workspace_id)
            .into_iter()
            .filter(|issue| {
                issue.archived_at.is_none()
                    && (self.preselected.contains(&issue.id)
                        || !matches!(
                            issue.status,
                            IssueStatus::Done | IssueStatus::Cancelled | IssueStatus::Duplicate
                        ))
            })
            .collect()
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.submitting || self.checked.is_empty() {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };

        self.error = None;
        self.submitting = true;
        cx.notify();

        let workspace_id = self.workspace_id.clone();
        let name_value = self.name.read(cx).value().trim().to_string();
        let name = (!name_value.is_empty()).then_some(name_value);
        let ids: Vec<String> = self.checked.iter().cloned().collect();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    // create caps issueIds at 200 — the first chunk rides the
                    // create transaction, any overflow chunks through
                    // addIssues (the shared bulk wire contract).
                    let (first, rest) = ids.split_at(ids.len().min(200));
                    let created =
                        api::releases::create(&trpc, &workspace_id, name.as_deref(), first)?;
                    let release_id = created.release.id.clone();
                    for chunk in rest.chunks(200) {
                        api::releases::add_issues(&trpc, &release_id, chunk)?;
                    }
                    Ok::<String, api::ApiError>(release_id)
                })
                .await;

            match result {
                Ok(release_id) => {
                    // Gate on the echo so the drill-down renders from the
                    // synced row (the Releases detail self-heals to the list
                    // while the row is missing — a too-early selection would
                    // be silently wiped).
                    let releases = window
                        .update(|_, cx| Store::global(cx).collections().releases.clone())
                        .ok();
                    if let Some(releases) = releases {
                        queries::await_row_visible(&releases, &release_id, window).await;
                    }
                    let _ = this.update_in(window, |_, window, cx| {
                        window.close_dialog(cx);
                        crate::sidebar::open_release(window, cx, release_id);
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, _window, cx| {
                        this.error = Some(format!("{err}").into());
                        this.submitting = false;
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// One candidate row: checkbox + identifier + title (+ the OTHER
    /// release's name when creating would move the issue over).
    fn issue_row(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let is_checked = self.checked.contains(&issue.id);
        let toggle_id = issue.id.clone();
        let other_release: Option<SharedString> = issue.release_id.as_deref().map(|release_id| {
            Store::global(cx)
                .collections()
                .releases
                .read(cx)
                .get(release_id)
                .and_then(|release| release.name.clone())
                .unwrap_or_else(|| "another release".to_string())
                .into()
        });

        h_flex()
            .w_full()
            .items_center()
            .gap_2()
            .child(
                Checkbox::new(SharedString::from(format!("create-release-issue-{}", issue.id)))
                    .checked(is_checked)
                    .on_click(cx.listener(move |this, on: &bool, _, cx| {
                        if *on {
                            this.checked.insert(toggle_id.clone());
                        } else {
                            this.checked.remove(&toggle_id);
                        }
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(SharedString::from(issue.identifier.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .truncate()
                    .text_color(theme.foreground)
                    .child(SharedString::from(issue.title.clone())),
            )
            .when_some(other_release, |this, name| {
                this.child(
                    h_flex()
                        .flex_shrink_0()
                        .gap_1()
                        .items_center()
                        .text_xs()
                        .text_color(muted)
                        .child(Icon::from(ExpIcon::Rocket).xsmall().text_color(muted))
                        .child(name),
                )
            })
            .into_any_element()
    }
}

impl Render for ReleaseCreateDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            self.name.update(cx, |state, cx| state.focus(window, cx));
        }

        let pool = self.pool(cx);
        // Prune checks whose rows left the pool entirely (delete / archive /
        // a status flip on a non-preselected row) — NOT rows merely hidden by
        // the current search, so narrowing a search never wipes picks.
        let present: HashSet<&str> = pool.iter().map(|issue| issue.id.as_str()).collect();
        self.checked.retain(|id| present.contains(id.as_str()));
        let checked_count = self.checked.len();

        let needle = self.search.read(cx).value().trim().to_lowercase();
        let candidates: Vec<&Issue> = pool
            .iter()
            .filter(|issue| {
                needle.is_empty()
                    || issue.identifier.to_lowercase().contains(&needle)
                    || issue.title.to_lowercase().contains(&needle)
            })
            .collect();

        let muted = cx.theme().muted_foreground;
        let rows: Vec<gpui::AnyElement> = candidates
            .iter()
            .map(|issue| self.issue_row(issue, cx))
            .collect();

        let list: gpui::AnyElement = if rows.is_empty() {
            div()
                .py_4()
                .text_sm()
                .text_color(muted)
                .child(if needle.is_empty() {
                    "No open issues to add."
                } else {
                    "No issues match your search."
                })
                .into_any_element()
        } else {
            div()
                .id("create-release-scroll")
                .max_h(px(300.))
                .overflow_y_scrollbar()
                .child(v_flex().gap_1().children(rows))
                .into_any_element()
        };

        let mut body = v_flex()
            .gap_3()
            .child(Input::new(&self.name).small())
            .child(Input::new(&self.search).small())
            .child(list);

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
                    Button::new("create-release-cancel")
                        .outline()
                        .small()
                        .label("Cancel")
                        .disabled(self.submitting)
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.submitting {
                                return;
                            }
                            window.close_dialog(cx);
                        })),
                )
                .child(
                    Button::new("create-release-submit")
                        .primary()
                        .small()
                        .label(if self.submitting {
                            "Creating…".to_string()
                        } else if checked_count == 0 {
                            "Create release".to_string()
                        } else if checked_count == 1 {
                            "Create with 1 issue".to_string()
                        } else {
                            format!("Create with {checked_count} issues")
                        })
                        .disabled(checked_count == 0 || self.submitting)
                        .loading(self.submitting)
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                ),
        )
    }
}
