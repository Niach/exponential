//! Create-project dialog (masterplan-v3 §4.2 — mirror of
//! `apps/web/src/components/create-project-dialog.tsx`).
//!
//! Name `Input` + an **auto-derived-but-editable prefix** `Input`
//! (`derivePrefix`, uppercased, max 10) + the `ColorSwatchGrid` — **no slug
//! field** (server-derived). Submit → `projects.create`; the close is gated
//! on the new project appearing in the synced collection (§4.1 create flows),
//! so the sidebar row is there the moment the dialog is gone. A plan-cap
//! FORBIDDEN surfaces as the neutral "Upgrade on the web" notification
//! (§4.9) — never an in-app purchase UI.
//!
//! Opened by the sidebar's Projects `+` (EXP-1 #2) via the [`NewProject`]
//! action; [`init`] owns the handler.

use gpui::{
    div, px, App, AppContext as _, Entity, InteractiveElement as _, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    notification::Notification,
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
};
use sync::Store;

use crate::actions::NewProject;
use crate::create_issue_dialog::parse_hex_color;
use crate::navigation::{active_workspace_id, nav_for_window, navigate, Screen};
use crate::queries;

/// Web `LABEL_COLORS` (`lib/label-colors.ts`) — the swatch palette shared by
/// project + label colors (fixed hex literals on web too).
pub(crate) const SWATCH_COLORS: [&str; 20] = [
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308", "#84cc16", "#22c55e", "#10b981",
    "#14b8a6", "#06b6d4", "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7", "#ec4899",
    "#f43f5e", "#78716c", "#64748b", "#a3a3a3",
];

/// Web default project color (`create-project-dialog.tsx`).
const DEFAULT_COLOR: &str = "#6366f1";

/// Register the App-global [`NewProject`] handler (call once from `ui::init`).
pub fn init(cx: &mut App) {
    cx.on_action(|_: &NewProject, cx| {
        crate::navigation::on_active_window(cx, |window, cx| {
            let nav = nav_for_window(window, cx);
            let Some(workspace_id) = active_workspace_id(&nav, cx) else {
                return;
            };
            open(window, cx, workspace_id);
        });
    });
}

/// Open the dialog for a workspace.
pub fn open(window: &mut Window, cx: &mut App, workspace_id: String) {
    let view = cx.new(|cx| CreateProjectDialogView::new(workspace_id, window, cx));
    window.open_dialog(cx, move |dialog, _window, cx| {
        let busy = view.read(cx).submitting;
        dialog
            .w(px(416.)) // web sm:max-w-[26rem]
            .title("Create project")
            .overlay_closable(!busy)
            .keyboard(!busy)
            .on_ok({
                let view = view.clone();
                move |_, window, cx| {
                    view.update(cx, |view, cx| view.submit(window, cx));
                    false
                }
            })
            .child(view.clone())
    });
}

pub struct CreateProjectDialogView {
    workspace_id: String,
    name: Entity<InputState>,
    prefix: Entity<InputState>,
    color: String,
    submitting: bool,
    error: Option<SharedString>,
    focused_once: bool,
    _subscriptions: Vec<Subscription>,
}

impl CreateProjectDialogView {
    fn new(workspace_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. Backend API"));
        let prefix = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. API"));

        let mut subscriptions = Vec::new();
        // Web `handleNameChange`: every name edit re-derives the prefix.
        subscriptions.push(cx.subscribe_in(
            &name,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let derived = derive_prefix(&this.name.read(cx).value());
                    this.prefix
                        .update(cx, |state, cx| state.set_value(derived, window, cx));
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                _ => {}
            },
        ));
        // Web prefix input: uppercased, maxLength 10.
        subscriptions.push(cx.subscribe_in(
            &prefix,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    let value = this.prefix.read(cx).value().to_string();
                    let normalized: String = value.to_uppercase().chars().take(10).collect();
                    if normalized != value {
                        this.prefix
                            .update(cx, |state, cx| state.set_value(normalized, window, cx));
                    }
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.submit(window, cx),
                _ => {}
            },
        ));

        Self {
            workspace_id,
            name,
            prefix,
            color: DEFAULT_COLOR.to_string(),
            submitting: false,
            error: None,
            focused_once: false,
            _subscriptions: subscriptions,
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let name = self.name.read(cx).value().trim().to_string();
        let prefix = self.prefix.read(cx).value().trim().to_string();
        if name.is_empty() || prefix.is_empty() || self.submitting {
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

        let input = api::projects::ProjectsCreateInput {
            workspace_id: self.workspace_id.clone(),
            name,
            prefix,
            color: Some(self.color.clone()),
        };

        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::projects::projects_create(&trpc, &input) })
                .await;

            match result {
                Ok(output) => {
                    // Gate the close on the Electric echo (§4.1) so the
                    // sidebar shows the project the moment the dialog closes,
                    // then open its (empty) board.
                    let project_id = output.project.id.clone();
                    let projects = window
                        .update(|_, cx| Store::global(cx).collections().projects.clone())
                        .ok();
                    if let Some(projects) = projects {
                        queries::await_row_visible(&projects, &project_id, window).await;
                    }
                    let _ = this.update_in(window, |_, window, cx| {
                        window.close_dialog(cx);
                        navigate(window, cx, Screen::Board { project_id });
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, window, cx| {
                        if is_plan_limit(&err) {
                            // §4.9: neutral hand-off, never an upgrade dialog.
                            window.close_dialog(cx);
                            window.push_notification(
                                Notification::warning(
                                    "Project limit reached — upgrade on the web to create more.",
                                ),
                                cx,
                            );
                            return;
                        }
                        this.error = Some(format!("{err}").into());
                        this.submitting = false;
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }
}

impl Render for CreateProjectDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if !self.focused_once {
            self.focused_once = true;
            self.name.update(cx, |state, cx| state.focus(window, cx));
        }

        let name_empty = self.name.read(cx).value().trim().is_empty();
        let prefix_empty = self.prefix.read(cx).value().trim().is_empty();
        let disabled = name_empty || prefix_empty || self.submitting;

        let mut form = v_flex()
            .gap_4()
            .child(labeled(cx, "Name", Input::new(&self.name).small()))
            .child(labeled(cx, "Prefix", Input::new(&self.prefix).small()))
            .child(
                v_flex()
                    .gap_2()
                    .child(field_label(cx, "Color"))
                    .child(color_swatch_grid(&self.color, cx.entity().clone(), cx)),
            );

        if let Some(error) = &self.error {
            form = form.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }

        form.child(
            h_flex().justify_end().child(
                Button::new("create-project-submit")
                    .primary()
                    .small()
                    .label(if self.submitting {
                        "Creating..."
                    } else {
                        "Create project"
                    })
                    .disabled(disabled)
                    .loading(self.submitting)
                    .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
            ),
        )
    }
}

/// Web `ColorSwatchGrid`: a wrapping row of h-5 w-5 rounded-full swatches;
/// the selected one carries a ring (approximated as a padded border ring).
fn color_swatch_grid(
    selected: &str,
    view: Entity<CreateProjectDialogView>,
    cx: &App,
) -> impl IntoElement {
    let mut grid = h_flex().flex_wrap().gap_1p5();
    for color in SWATCH_COLORS {
        let fill = parse_hex_color(color).unwrap_or(cx.theme().muted_foreground);
        let is_selected = color == selected;
        let view = view.clone();
        grid = grid.child(
            div()
                .id(SharedString::from(format!("swatch-{color}")))
                .size(px(24.))
                .rounded_full()
                .p(px(2.))
                .border_1()
                .border_color(if is_selected {
                    cx.theme().foreground
                } else {
                    gpui::transparent_black()
                })
                .cursor_pointer()
                .child(div().size_full().rounded_full().bg(fill))
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.color = color.to_string();
                        cx.notify();
                    });
                }),
        );
    }
    grid
}

fn field_label(cx: &App, label: &'static str) -> impl IntoElement {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(label)
}

fn labeled(cx: &App, label: &'static str, input: Input) -> impl IntoElement {
    v_flex().gap_2().child(field_label(cx, label)).child(input)
}

/// Web `derivePrefix` (`lib/project.ts`): first letter of each
/// space/dash/underscore-separated word, uppercased, max 5.
pub(crate) fn derive_prefix(name: &str) -> String {
    name.split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter_map(|word| word.chars().next())
        .collect::<String>()
        .to_uppercase()
        .chars()
        .take(5)
        .collect()
}

/// The web `isPlanLimitError` analog: plan caps surface as tRPC FORBIDDEN
/// (HTTP 403).
pub(crate) fn is_plan_limit(err: &api::ApiError) -> bool {
    matches!(err, api::ApiError::Http { status: 403, .. })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_prefix_matches_web() {
        assert_eq!(derive_prefix("My Project"), "MP");
        assert_eq!(derive_prefix("backend-api"), "BA");
        assert_eq!(derive_prefix("a_b_c_d_e_f_g"), "ABCDE");
        assert_eq!(derive_prefix(""), "");
        assert_eq!(derive_prefix("  spaced   out  "), "SO");
    }
}
