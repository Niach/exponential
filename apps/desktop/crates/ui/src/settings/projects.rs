//! Settings → Projects (masterplan-v3 §4.2).
//!
//! Web parity: `components/workspace/projects-section.tsx` — the visible
//! (non-archived) project list with color dot + name + prefix badge and a
//! per-project **Delete** behind a confirm dialog (`projects.delete`).
//!
//! The §7.3 run-targets editor (DB `run_configs`) plugs into this pane when
//! the IDE track lands it — its CRUD + Trust gate is §07-owned; this file
//! deliberately does not stub it.

use gpui::{
    div, ElementId, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    h_flex, v_flex, ActiveTheme as _, Disableable as _, IconName, Sizable as _, WindowExt as _,
};
use sync::Store;

use crate::navigation::{active_workspace_id, Navigation};

use super::{card, card_header, parse_hex_color, spawn_trpc};

pub struct ProjectsPane {
    nav: Entity<Navigation>,
    _subscriptions: Vec<Subscription>,
}

impl ProjectsPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&collections.projects, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            _subscriptions: subscriptions,
        }
    }

    fn open_delete_dialog(
        project_id: String,
        project_name: String,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        window.open_dialog(cx, move |dialog, _, _| {
            let name = project_name.clone();
            let project_id = project_id.clone();
            dialog
                .title("Delete project")
                .content(move |content, _, cx| {
                    content.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(format!(
                                "This will permanently delete {name} and all its issues. \
                                 This cannot be undone."
                            ))),
                    )
                })
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("Delete project")
                        .ok_variant(ButtonVariant::Danger)
                        .show_cancel(true)
                        .on_ok({
                            let project_id = project_id.clone();
                            move |_, _, cx| {
                                let project_id = project_id.clone();
                                spawn_trpc(cx, "projects.delete", move |trpc| {
                                    api::projects::projects_delete(trpc, &project_id)
                                });
                                true
                            }
                        }),
                )
        });
    }
}

impl Render for ProjectsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let projects = active_workspace_id(&self.nav, cx)
            .map(|workspace_id| {
                Store::global(cx)
                    .collections()
                    .projects_in_workspace(&workspace_id, cx)
            })
            .unwrap_or_default();

        let mut body = card(cx).child(card_header(
            format!("Projects · {}", projects.len()),
            "Manage projects in this team.",
            cx,
        ));

        if projects.is_empty() {
            body = body.child(
                div()
                    .px_3()
                    .py_2()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No projects in this team yet."),
            );
        } else {
            let mut list = v_flex().gap_2();
            for project in &projects {
                let color = project
                    .color
                    .as_deref()
                    .and_then(parse_hex_color)
                    .unwrap_or(cx.theme().muted_foreground);
                let prefix: SharedString = project.prefix.clone().unwrap_or_default().into();
                let project_id = project.id.clone();
                let project_name = project.name.clone();
                // Protected projects (the bootstrap dogfood board) are
                // non-deletable — the server refuses, so grey out the
                // affordance from the synced flag like the other clients.
                let protected = project.is_protected.unwrap_or(false);
                let delete_button = Button::new(row_id("project-delete", &project.id))
                    .ghost()
                    .xsmall()
                    .icon(IconName::Delete);
                let delete_button = if protected {
                    delete_button.disabled(true)
                } else {
                    delete_button.on_click(cx.listener(move |_, _, window, cx| {
                        Self::open_delete_dialog(
                            project_id.clone(),
                            project_name.clone(),
                            window,
                            cx,
                        );
                    }))
                };

                list = list.child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .px_3()
                        .py_2()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().border)
                        .child(div().size_2p5().rounded_full().flex_shrink_0().bg(color))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(SharedString::from(project.name.clone())),
                        )
                        .child(
                            div()
                                .px_1p5()
                                .py_0p5()
                                .rounded(cx.theme().radius)
                                .border_1()
                                .border_color(cx.theme().border)
                                .text_xs()
                                .font_family(theme::terminal::FONT_FAMILY)
                                .text_color(cx.theme().muted_foreground)
                                .child(prefix),
                        )
                        .child(delete_button),
                );
            }
            body = body.child(list);
        }

        v_flex().child(body)
    }
}

fn row_id(kind: &str, id: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("{kind}-{id}")))
}
