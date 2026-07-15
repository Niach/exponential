//! Settings → Members (masterplan-v3 §4.2).
//!
//! Web parity: `components/workspace/members-section.tsx` — the member list
//! (avatar + name + role badge + per-row actions `DropdownMenu`: Make owner /
//! Make member / Remove member / Leave workspace) and the owner-only
//! `InviteControls` (Generate invite link → copy-to-clipboard, pending
//! invites with revoke).
//!
//! Reads are live: members/users/invites come from the synced collections
//! (the web reads the same shapes); role/remove/revoke are §4.1 un-gated
//! mutations reflected by the Electric echo. Invite creation is stateful
//! (spinner + the generated URL); its plan-cap failure surfaces as the §4.9
//! neutral "Upgrade on the web" notice — never an upgrade dialog.

use gpui::{
    div, prelude::FluentBuilder as _, App, ElementId, Entity, FontWeight, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    avatar::Avatar,
    button::{Button, ButtonVariants as _},
    clipboard::Clipboard,
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
};
use sync::Store;

use domain::board::format_short_date;
use domain::contract::WORKSPACE_ROLE_OWNER;
use domain::rows::{User, WorkspaceInvite, WorkspaceMember};

use crate::navigation::{active_workspace_id, Navigation};
use crate::queries;

use super::{
    card, card_header, error_notice, is_owner, is_plan_limit, show_workspace_chrome, spawn_trpc,
    upgrade_notice,
};

/// One joined member row (web `members` + `userMap`).
struct MemberRow {
    member: WorkspaceMember,
    user: Option<User>,
}

pub struct MembersPane {
    nav: Entity<Navigation>,
    invite_url: Option<SharedString>,
    generating: bool,
    error: Option<SharedString>,
    limit_notice: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl MembersPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |this, _, cx| {
                // Workspace switch: the generated URL belongs to the old one.
                this.invite_url = None;
                this.error = None;
                this.limit_notice = None;
                cx.notify();
            }),
            cx.observe(&collections.workspace_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.workspace_invites, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
        ];

        Self {
            nav,
            invite_url: None,
            generating: false,
            error: None,
            limit_notice: None,
            _subscriptions: subscriptions,
        }
    }

    /// Web: members joined with users, agent users filtered out
    /// (`!userMap.get(member.userId)?.isAgent`).
    fn member_rows(&self, workspace_id: &str, cx: &App) -> Vec<MemberRow> {
        let collections = Store::global(cx).collections();
        let users = collections.users.read(cx);
        let mut rows: Vec<MemberRow> = collections
            .workspace_members
            .read(cx)
            .iter()
            .filter(|member| member.workspace_id == workspace_id)
            .map(|member| MemberRow {
                member: member.clone(),
                user: users.get(&member.user_id).cloned(),
            })
            .filter(|row| {
                row.user
                    .as_ref()
                    .map(|user| user.is_agent != Some(true))
                    .unwrap_or(true)
            })
            .collect();
        rows.sort_by(|a, b| {
            display_name(a)
                .to_lowercase()
                .cmp(&display_name(b).to_lowercase())
        });
        rows
    }

    fn pending_invites(&self, workspace_id: &str, cx: &App) -> Vec<WorkspaceInvite> {
        let mut invites: Vec<WorkspaceInvite> = Store::global(cx)
            .collections()
            .workspace_invites
            .read(cx)
            .iter()
            .filter(|invite| {
                invite.workspace_id == workspace_id && invite.accepted_at.is_none()
            })
            .cloned()
            .collect();
        invites.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        invites
    }

    fn generate_invite(&mut self, workspace_id: String, cx: &mut gpui::Context<Self>) {
        if self.generating {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let Some(account) = queries::active_account(cx) else {
            return;
        };
        let base = account.instance_url;

        self.generating = true;
        self.error = None;
        self.limit_notice = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::workspaces::workspace_invites_create(
                        &trpc,
                        &workspace_id,
                        api::workspaces::WorkspaceRole::Member,
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.generating = false;
                match result {
                    Ok(out) => {
                        this.invite_url =
                            Some(format!("{base}/invite/{}", out.token).into());
                    }
                    Err(err) if is_plan_limit(&err) => {
                        this.limit_notice = Some(
                            "You've reached the maximum number of members for your plan."
                                .into(),
                        );
                    }
                    Err(err) => {
                        this.error = Some(format!("Couldn't create the invite: {err}").into());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn render_member_row(
        &self,
        row: &MemberRow,
        my_user_id: &str,
        i_am_owner: bool,
        owner_count: usize,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let member = &row.member;
        let is_self = member.user_id == my_user_id;
        let role = member
            .role
            .clone()
            .unwrap_or_else(|| "member".to_string());
        let is_owner_row = role == WORKSPACE_ROLE_OWNER;
        let name = display_name(row);
        let email = row.user.as_ref().and_then(|user| user.email.clone());

        // Web: sole remaining owner gets no self-actions.
        let show_actions =
            (i_am_owner || is_self) && !(is_self && is_owner_row && owner_count <= 1);

        let role_icon = if is_owner_row {
            IconName::Star
        } else {
            IconName::User
        };

        let mut identity = v_flex().gap_0p5().child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child(SharedString::from(if is_self {
                            format!("{name} (you)")
                        } else {
                            name.clone()
                        })),
                )
                .child(role_chip(role_icon, SharedString::from(role.clone()), cx)),
        );
        if let Some(email) = email {
            identity = identity.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(email)),
            );
        }

        h_flex()
            .justify_between()
            .items_center()
            .px_3()
            .py_2()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(Avatar::new().name(SharedString::from(name.clone())).small())
                    .child(identity),
            )
            .when(show_actions, |row_el| {
                row_el.child(member_actions_menu(
                    member.id.clone(),
                    &name,
                    is_self,
                    is_owner_row,
                    i_am_owner,
                ))
            })
    }
}

/// Web `DropdownMenu` per member row: role changes (owner, not self), then
/// Leave workspace (self) / Remove member (owner).
fn member_actions_menu(
    member_id: String,
    name: &str,
    is_self: bool,
    is_owner_row: bool,
    i_am_owner: bool,
) -> impl IntoElement {
    Button::new(row_id("member-actions", &member_id))
        .ghost()
        .xsmall()
        .icon(IconName::Ellipsis)
        .dropdown_menu({
            let member_id = member_id.clone();
            let name = name.to_string();
            move |mut menu, _, _| {
                if i_am_owner && !is_self {
                    for (label, role, icon, disabled) in [
                        (
                            "Make owner",
                            api::workspaces::WorkspaceRole::Owner,
                            IconName::Star,
                            is_owner_row,
                        ),
                        (
                            "Make member",
                            api::workspaces::WorkspaceRole::Member,
                            IconName::User,
                            !is_owner_row,
                        ),
                    ] {
                        let member_id = member_id.clone();
                        menu = menu.item(
                            PopupMenuItem::new(label)
                                .icon(Icon::new(icon))
                                .disabled(disabled)
                                .on_click(move |_, _, cx| {
                                    let member_id = member_id.clone();
                                    spawn_trpc(cx, "workspaceMembers.updateRole", move |trpc| {
                                        api::workspaces::workspace_members_update_role(
                                            trpc, &member_id, role,
                                        )
                                    });
                                }),
                        );
                    }
                }
                if is_self || i_am_owner {
                    let member_id = member_id.clone();
                    let label = if is_self {
                        "Leave team".to_string()
                    } else {
                        format!("Remove {name}")
                    };
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(label))
                            .icon(Icon::new(IconName::Close))
                            .on_click(move |_, _, cx| {
                                let member_id = member_id.clone();
                                spawn_trpc(cx, "workspaceMembers.remove", move |trpc| {
                                    api::workspaces::workspace_members_remove(trpc, &member_id)
                                });
                            }),
                    );
                }
                menu
            }
        })
}

impl Render for MembersPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(workspace_id) = active_workspace_id(&self.nav, cx) else {
            return v_flex().child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No team selected."),
            );
        };
        let my_user_id = queries::active_account(cx)
            .map(|account| account.user_id)
            .unwrap_or_default();
        let i_am_owner = is_owner(cx, &workspace_id);
        let solo = !show_workspace_chrome(cx, &workspace_id);

        let rows = self.member_rows(&workspace_id, cx);
        let owner_count = rows
            .iter()
            .filter(|row| row.member.role.as_deref() == Some(WORKSPACE_ROLE_OWNER))
            .count();

        let mut body = card(cx).child(card_header(
            if solo { "Invite teammates" } else { "Members" },
            if solo {
                "Invite someone to collaborate. Shared projects unlock team features.".to_string()
            } else {
                format!(
                    "{} member{} in this team",
                    rows.len(),
                    if rows.len() == 1 { "" } else { "s" }
                )
            },
            cx,
        ));

        let mut list = v_flex().gap_2();
        for row in &rows {
            list = list.child(self.render_member_row(row, &my_user_id, i_am_owner, owner_count, cx));
        }
        body = body.child(list);

        // InviteControls (web: owner-only `showInvite`).
        if i_am_owner {
            let mut invite_section = v_flex()
                .gap_3()
                .pt_3()
                .border_t_1()
                .border_color(cx.theme().border)
                .child(
                    v_flex()
                        .gap_0p5()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child("Invite Members"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    "Generate an invite link to share with people you want to add",
                                ),
                        ),
                );

            if let Some(url) = &self.invite_url {
                invite_section = invite_section.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .px_2()
                                .py_1()
                                .rounded(cx.theme().radius)
                                .border_1()
                                .border_color(cx.theme().border)
                                .text_xs()
                                .font_family(theme::terminal::FONT_FAMILY)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(url.clone()),
                        )
                        .child(
                            Clipboard::new("invite-copy")
                                .value(url.clone())
                                .tooltip("Copy invite URL"),
                        ),
                );
            }

            invite_section = invite_section.child(
                h_flex().child(
                    Button::new("invite-generate")
                        .primary()
                        .small()
                        .label("Generate invite link")
                        .icon(IconName::Plus)
                        .loading(self.generating)
                        .disabled(self.generating)
                        .on_click(cx.listener({
                            let workspace_id = workspace_id.clone();
                            move |this, _, _, cx| {
                                this.generate_invite(workspace_id.clone(), cx);
                            }
                        })),
                ),
            );

            if let Some(notice) = &self.limit_notice {
                invite_section = invite_section.child(upgrade_notice(notice.clone(), cx));
            }
            if let Some(error) = &self.error {
                invite_section = invite_section.child(error_notice(error.clone(), cx));
            }

            let invites = self.pending_invites(&workspace_id, cx);
            if !invites.is_empty() {
                let mut pending = v_flex().gap_2().child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(cx.theme().muted_foreground)
                        .child("Pending invites"),
                );
                for invite in invites {
                    let invite_id = invite.id.clone();
                    let role: SharedString = invite
                        .role
                        .clone()
                        .unwrap_or_else(|| "member".to_string())
                        .into();
                    let expires: SharedString = invite
                        .expires_at
                        .as_deref()
                        .map(|at| format!("Expires {}", format_short_date(at)))
                        .unwrap_or_else(|| "No expiry".to_string())
                        .into();
                    pending = pending.child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .px_3()
                            .py_2()
                            .rounded(cx.theme().radius)
                            .border_1()
                            .border_color(cx.theme().border)
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(role_chip(IconName::User, role, cx))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(expires),
                                    ),
                            )
                            .child(
                                Button::new(row_id("invite-revoke", &invite.id))
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::Delete)
                                    .on_click(move |_, _, cx| {
                                        let invite_id = invite_id.clone();
                                        spawn_trpc(cx, "workspaceInvites.revoke", move |trpc| {
                                            api::workspaces::workspace_invites_revoke(
                                                trpc, &invite_id,
                                            )
                                        });
                                    }),
                            ),
                    );
                }
                invite_section = invite_section.child(pending);
            }

            body = body.child(invite_section);
        }

        v_flex().child(body)
    }
}

fn display_name(row: &MemberRow) -> String {
    // Web: `user?.name ?? member.userId`.
    row.user
        .as_ref()
        .and_then(|user| user.name.clone())
        .unwrap_or_else(|| row.member.user_id.clone())
}

/// Web role `Badge`: secondary chip with the role icon.
fn role_chip(icon: IconName, label: SharedString, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_1()
        .px_1p5()
        .py_0p5()
        .rounded(cx.theme().radius)
        .bg(cx.theme().secondary)
        .text_xs()
        .text_color(cx.theme().secondary_foreground)
        .items_center()
        .child(Icon::new(icon).xsmall())
        .child(label)
}

fn row_id(kind: &str, id: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("{kind}-{id}")))
}
