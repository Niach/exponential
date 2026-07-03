//! Settings → General + Danger Zone (masterplan-v3 §4.2).
//!
//! Web parity: `components/workspace/general-section.tsx` (name `Input`,
//! isPublic `Switch`, publicWritePolicy select, dirty-gated Save) and the
//! Danger Zone card of `routes/w/$workspaceSlug/settings/index.tsx`
//! (type-the-name-to-confirm delete, gated owner + non-public + team-only).
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
    menu::DropdownMenu as _,
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _, IconName, Sizable as _, WindowExt as _,
};
use sync::Store;

use domain::contract::{PUBLIC_WRITE_POLICY_EVERYONE, PUBLIC_WRITE_POLICY_MEMBERS};

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
    is_public: bool,
    policy: String,
}

pub struct GeneralPane {
    nav: Entity<Navigation>,
    name_input: Entity<InputState>,
    delete_input: Entity<InputState>,
    is_public: bool,
    policy: String,
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
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Workspace name"));
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
            is_public: false,
            policy: PUBLIC_WRITE_POLICY_MEMBERS.to_string(),
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
            is_public: workspace.is_public == Some(true),
            policy: workspace
                .public_write_policy
                .clone()
                .unwrap_or_else(|| PUBLIC_WRITE_POLICY_MEMBERS.to_string()),
        };
        if self.snapshot.as_ref() == Some(&snapshot) {
            return;
        }
        self.name_input.update(cx, |state, cx| {
            state.set_value(snapshot.name.clone(), window, cx);
        });
        self.is_public = snapshot.is_public;
        self.policy = snapshot.policy.clone();
        self.snapshot = Some(snapshot);
        cx.notify();
    }

    fn dirty(&self, cx: &App) -> bool {
        let Some(snapshot) = &self.snapshot else {
            return false;
        };
        self.name_input.read(cx).value().as_ref() != snapshot.name
            || self.is_public != snapshot.is_public
            || self.policy != snapshot.policy
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
        input.is_public = Some(self.is_public);
        input.public_write_policy = Some(self.policy.clone());

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
                .title("Delete workspace")
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
                        .ok_text("Delete workspace")
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
                    .child("No workspace selected."),
            );
        };
        let solo = !show_workspace_chrome(cx, &workspace.id);
        let owner = is_owner(cx, &workspace.id);
        let dirty = self.dirty(cx);
        let saving = self.saving;
        let is_public = self.is_public;
        let policy = self.policy.clone();

        let mut general = card(cx).child(card_header(
            if solo { "Visibility" } else { "General" },
            if solo {
                "Visibility and contribution rules"
            } else {
                "Workspace name, visibility, and contribution rules"
            },
            cx,
        ));

        if !solo {
            general = general.child(
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
        }

        general = general.child(
            h_flex()
                .justify_between()
                .items_center()
                .gap_4()
                .p_3()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().border)
                .child(
                    v_flex()
                        .gap_0p5()
                        .child(div().text_sm().child("Public workspace"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    "Anyone can view this workspace, including people who \
                                     aren't signed in. Creating still requires login.",
                                ),
                        ),
                )
                .child(
                    Switch::new("workspace-public")
                        .checked(is_public)
                        .disabled(!owner)
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.is_public = *checked;
                            cx.notify();
                        })),
                ),
        );

        if is_public {
            let policy_label: SharedString = if policy == PUBLIC_WRITE_POLICY_EVERYONE {
                "Anyone signed in".into()
            } else {
                "Workspace members only".into()
            };
            let current = policy.clone();
            general = general.child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Who can create issues?"),
                    )
                    .child(
                        Button::new("workspace-policy")
                            .outline()
                            .small()
                            .label(policy_label)
                            .icon(IconName::ChevronDown)
                            .disabled(!owner)
                            .dropdown_menu({
                                let entity = cx.entity();
                                move |menu, _, _| {
                                    let mut menu = menu;
                                    for (value, label) in [
                                        (PUBLIC_WRITE_POLICY_MEMBERS, "Workspace members only"),
                                        (PUBLIC_WRITE_POLICY_EVERYONE, "Anyone signed in"),
                                    ] {
                                        let entity = entity.clone();
                                        menu = menu.item(
                                            gpui_component::menu::PopupMenuItem::new(label)
                                                .checked(current == value)
                                                .on_click(move |_, _, cx| {
                                                    entity.update(cx, |this, cx| {
                                                        this.policy = value.to_string();
                                                        cx.notify();
                                                    });
                                                }),
                                        );
                                    }
                                    menu
                                }
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                "Updating an issue is always limited to its creator, a \
                                 workspace member, or an admin.",
                            ),
                    ),
            );
        }

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

        // Danger Zone (web settings/index.tsx): owner + non-public + team-only.
        let synced_public = workspace.is_public == Some(true);
        if owner && !synced_public && !solo {
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
                                    .child("Permanently delete this workspace and all its data."),
                            ),
                    )
                    .child(
                        h_flex().child(
                            Button::new("workspace-delete")
                                .danger()
                                .small()
                                .label("Delete workspace")
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
                    .child("Only workspace owners can change these settings."),
            )
        })
    }
}
