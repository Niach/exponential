//! "+ Add issues" picker of the release detail (release rework Phase 7):
//! search + checkbox rows over the workspace's synced issues, footer
//! "Add N issues" → `releases.addIssues` (its first real call site — the
//! wrapper predates this dialog). Candidates exclude `done`/`cancelled`/
//! `duplicate` and issues already in THIS release; issues bundled into OTHER
//! releases stay listed with their release's name (adding MOVES them —
//! `releases.addIssues` semantics, matching the web picker). Rows appear in
//! the detail via the issues-shape Electric echo after close.

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

/// Open the picker for `release_id`. A no-op when the release row isn't
/// synced (racing a delete).
pub fn open(window: &mut Window, cx: &mut App, release_id: String) {
    let Some(release) = Store::global(cx)
        .collections()
        .releases
        .read(cx)
        .get(&release_id)
        .cloned()
    else {
        log::warn!("[ui] add-issues dialog for unknown release {release_id}");
        return;
    };
    let Some(workspace_id) = release.workspace_id.clone() else {
        log::warn!("[ui] add-issues dialog: release {release_id} has no workspace yet");
        return;
    };
    let view = cx.new(|cx| AddIssuesDialogView::new(release_id, workspace_id, window, cx));
    window.open_dialog(cx, move |dialog, _window, cx| {
        let busy = view.read(cx).submitting;
        dialog
            .w(px(480.))
            .title("Add issues")
            .overlay_closable(!busy)
            .keyboard(!busy)
            .child(view.clone())
    });
}

pub struct AddIssuesDialogView {
    release_id: String,
    workspace_id: String,
    search: Entity<InputState>,
    checked: HashSet<String>,
    submitting: bool,
    error: Option<SharedString>,
    focused_once: bool,
    _subscriptions: Vec<Subscription>,
}

impl AddIssuesDialogView {
    fn new(
        release_id: String,
        workspace_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search issues…"));
        let issues = Store::global(cx).collections().issues.clone();
        let subscriptions = vec![
            cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            // Candidates are a live collection read — echoes re-render.
            cx.observe(&issues, |_, _, cx| cx.notify()),
        ];

        Self {
            release_id,
            workspace_id,
            search,
            checked: HashSet::new(),
            submitting: false,
            error: None,
            focused_once: false,
            _subscriptions: subscriptions,
        }
    }

    /// The candidate pool: workspace issues that are neither closed-out
    /// (`done`/`cancelled`/`duplicate`) nor archived nor already in this
    /// release, filtered by the search text (identifier + title substring,
    /// case-insensitive).
    fn candidates(&self, cx: &App) -> Vec<Issue> {
        let needle = self.search.read(cx).value().trim().to_lowercase();
        queries::workspace_issues(cx, &self.workspace_id)
            .into_iter()
            .filter(|issue| {
                !matches!(
                    issue.status,
                    IssueStatus::Done | IssueStatus::Cancelled | IssueStatus::Duplicate
                ) && issue.archived_at.is_none()
                    && issue.release_id.as_deref() != Some(self.release_id.as_str())
                    && (needle.is_empty()
                        || issue.identifier.to_lowercase().contains(&needle)
                        || issue.title.to_lowercase().contains(&needle))
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

        let release_id = self.release_id.clone();
        let ids: Vec<String> = self.checked.iter().cloned().collect();
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    // addIssues caps at 200 ids — chunk sequentially (the
                    // shared bulk wire contract).
                    let mut last = None;
                    for chunk in ids.chunks(200) {
                        last = Some(api::releases::add_issues(&trpc, &release_id, chunk)?);
                    }
                    Ok::<_, api::ApiError>(last)
                })
                .await;

            let _ = this.update_in(window, |this, window, cx| {
                match result {
                    // Close on Ok — the detail's rows land via the Electric
                    // echo (§4.1 un-gated form).
                    Ok(_) => window.close_dialog(cx),
                    Err(err) => {
                        this.error = Some(format!("{err}").into());
                        this.submitting = false;
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// One candidate row: checkbox + identifier + title (+ the OTHER
    /// release's name when adding would move the issue).
    fn issue_row(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let is_checked = self.checked.contains(&issue.id);
        let toggle_id = issue.id.clone();
        // An issue already bundled elsewhere: show where it would move FROM.
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
                Checkbox::new(SharedString::from(format!("add-issue-{}", issue.id)))
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

impl Render for AddIssuesDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            self.search.update(cx, |state, cx| state.focus(window, cx));
        }

        let candidates = self.candidates(cx);
        // Prune picks whose rows left the pool (status flip / delete / a
        // concurrent add elsewhere) so the footer count never lies.
        let present: HashSet<&str> = candidates.iter().map(|issue| issue.id.as_str()).collect();
        self.checked.retain(|id| present.contains(id.as_str()));
        let checked_count = self.checked.len();

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
                .child(if self.search.read(cx).value().trim().is_empty() {
                    "No open issues to add."
                } else {
                    "No issues match your search."
                })
                .into_any_element()
        } else {
            div()
                .id("add-issues-scroll")
                .max_h(px(320.))
                .overflow_y_scrollbar()
                .child(v_flex().gap_1().children(rows))
                .into_any_element()
        };

        let mut body = v_flex()
            .gap_3()
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
                    Button::new("add-issues-cancel")
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
                    Button::new("add-issues-submit")
                        .primary()
                        .small()
                        .label(if self.submitting {
                            "Adding…".to_string()
                        } else if checked_count == 1 {
                            "Add 1 issue".to_string()
                        } else {
                            format!("Add {checked_count} issues")
                        })
                        .disabled(checked_count == 0 || self.submitting)
                        .loading(self.submitting)
                        .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                ),
        )
    }
}
