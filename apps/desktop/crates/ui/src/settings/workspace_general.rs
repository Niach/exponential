//! Settings → General + Danger Zone (masterplan-v3 §4.2).
//!
//! Web parity: `components/workspace/general-section.tsx` (name `Input`,
//! dirty-gated Save; workspaces are always private — there is no visibility
//! setting) and the Danger Zone card of
//! `routes/w/$workspaceSlug/settings/index.tsx` (type-the-name-to-confirm
//! delete, gated owner + team-only).
//!
//! Local state mirrors the web's `useState` + resync-on-workspace-change
//! `useEffect`: an Electric echo that changes the synced row overwrites the
//! local draft (which is exactly how the post-save echo clears `dirty`).

use gpui::{
    div, prelude::FluentBuilder as _, App, AppContext as _, Entity, IntoElement, ParentElement,
    Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
};
use sync::Store;

use crate::navigation::Navigation;

use super::{
    active_workspace, card, card_header, is_owner, show_workspace_chrome, spawn_trpc,
};

/// Snapshot of the synced fields the pane mirrors — resync happens whenever
/// this differs from the live row (the web `useEffect` dep list).
#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    workspace_id: String,
    name: String,
}

pub struct GeneralPane {
    nav: Entity<Navigation>,
    name_input: Entity<InputState>,
    delete_input: Entity<InputState>,
    snapshot: Option<Snapshot>,
    saving: bool,
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl GeneralPane {
    pub fn new(
        nav: Entity<Navigation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Team name"));
        let delete_input = cx.new(|cx| InputState::new(window, cx));

        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            // Resync needs the window (set_value) — window-aware observers.
            cx.observe_in(&nav, window, |this, _, window, cx| {
                this.resync(window, cx);
            }),
            cx.observe_in(&collections.workspaces, window, |this, _, window, cx| {
                this.resync(window, cx);
            }),
            cx.observe(&collections.workspace_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
            // Live dirty tracking: typing enables/disables Save.
            cx.subscribe(&name_input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];

        let mut this = Self {
            nav,
            name_input,
            delete_input,
            snapshot: None,
            saving: false,
            error: None,
            _subscriptions: subscriptions,
        };
        this.resync(window, cx);
        this
    }

    /// Mirror the web `useEffect`: whenever the synced row (or the active
    /// workspace) changes, replace the local draft.
    fn resync(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(workspace) = active_workspace(cx, &self.nav) else {
            return;
        };
        let snapshot = Snapshot {
            workspace_id: workspace.id.clone(),
            name: workspace.name.clone(),
        };
        if self.snapshot.as_ref() == Some(&snapshot) {
            return;
        }
        self.name_input.update(cx, |state, cx| {
            state.set_value(snapshot.name.clone(), window, cx);
        });
        self.snapshot = Some(snapshot);
        cx.notify();
    }

    fn dirty(&self, cx: &App) -> bool {
        let Some(snapshot) = &self.snapshot else {
            return false;
        };
        self.name_input.read(cx).value().as_ref() != snapshot.name
    }

    fn save(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(snapshot) = self.snapshot.clone() else {
            return;
        };
        if !self.dirty(cx) || self.saving {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };

        let typed = self.name_input.read(cx).value().trim().to_string();
        // Web: `name.trim() || workspace.name` — an emptied field falls back.
        let name = if typed.is_empty() {
            snapshot.name.clone()
        } else {
            typed
        };
        let mut input = api::workspaces::WorkspacesUpdateInput::new(snapshot.workspace_id.clone());
        input.name = Some(name);

        self.saving = true;
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::workspaces::workspaces_update(&trpc, &input) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                if let Err(err) = result {
                    this.error = Some(format!("Failed to save changes: {err}").into());
                }
                // Success needs no action: the Electric echo resyncs the
                // snapshot, which clears `dirty`.
                cx.notify();
            });
        })
        .detach();
    }

    fn open_delete_dialog(
        &mut self,
        workspace_id: String,
        workspace_name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.delete_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        let delete_input = self.delete_input.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            let name = workspace_name.clone();
            let confirm_name = workspace_name.clone();
            let workspace_id = workspace_id.clone();
            let content_input = delete_input.clone();
            let ok_input = delete_input.clone();
            dialog
                .title("Delete team")
                .content(move |content, _, cx| {
                    content
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(SharedString::from(format!(
                                    "This will permanently delete {name} and all its projects, \
                                     issues, and data. This cannot be undone."
                                ))),
                        )
                        .child(
                            v_flex()
                                .gap_1()
                                .mt_2()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(SharedString::from(format!(
                                            "Type {name} to confirm"
                                        ))),
                                )
                                .child(Input::new(&content_input).small()),
                        )
                })
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("Delete team")
                        .ok_variant(ButtonVariant::Danger)
                        .show_cancel(true)
                        .on_ok({
                            let delete_input = ok_input.clone();
                            move |_, _, cx| {
                                let typed = delete_input.read(cx).value().trim().to_string();
                                if typed != confirm_name {
                                    // Mismatch keeps the dialog open (web
                                    // disables the button until it matches).
                                    return false;
                                }
                                let workspace_id = workspace_id.clone();
                                spawn_trpc(cx, "workspaces.delete", move |trpc| {
                                    api::workspaces::workspaces_delete(trpc, &workspace_id)
                                });
                                true
                            }
                        }),
                )
        });
    }
}

impl Render for GeneralPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(workspace) = active_workspace(cx, &self.nav) else {
            return v_flex().child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No team selected."),
            );
        };
        let solo = !show_workspace_chrome(cx, &workspace.id);
        // Web parity: `if (solo) return null` — solo users don't see the
        // "workspace" concept, so the name card (a name nobody else sees) is
        // hidden. Visibility is deliberately not configurable (v6).
        if solo {
            return v_flex();
        }
        let owner = is_owner(cx, &workspace.id);
        let dirty = self.dirty(cx);
        let saving = self.saving;

        let mut general = card(cx)
            .child(card_header("General", "Team name", cx))
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Name"),
                    )
                    .child(Input::new(&self.name_input).small().disabled(!owner)),
            );

        if let Some(error) = &self.error {
            general = general.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }

        general = general.child(
            h_flex().justify_end().child(
                Button::new("workspace-save")
                    .primary()
                    .small()
                    .label(if saving { "Saving…" } else { "Save changes" })
                    .disabled(!owner || !dirty || saving)
                    .loading(saving)
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            ),
        );

        let mut pane = v_flex().gap_4().child(general);

        // Danger Zone (web settings/index.tsx): owner + team-only (the solo
        // case already returned above).
        if owner {
            let workspace_id = workspace.id.clone();
            let workspace_name = workspace.name.clone();
            pane = pane.child(
                v_flex()
                    .w_full()
                    .gap_3()
                    .p_4()
                    .border_1()
                    .border_color(cx.theme().danger.opacity(0.5))
                    .rounded(cx.theme().radius_lg)
                    .bg(cx.theme().colors.list_head)
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().danger)
                                    .child("Danger Zone"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Permanently delete this team and all its data."),
                            ),
                    )
                    .child(
                        h_flex().child(
                            Button::new("workspace-delete")
                                .danger()
                                .small()
                                .label("Delete team")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_delete_dialog(
                                        workspace_id.clone(),
                                        workspace_name.clone(),
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                    ),
            );
        }

        pane.when(!owner, |pane| {
            pane.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Only team owners can change these settings."),
            )
        })
    }
}
