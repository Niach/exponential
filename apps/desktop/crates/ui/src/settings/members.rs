//! Settings → Members (masterplan-v3 §4.2).
//!
//! Web parity: `components/team/members-section.tsx` — the member list
//! (avatar + name + role badge + per-row actions `DropdownMenu`: Make owner /
//! Make member / Remove member / Leave team) and the owner-only
//! `InviteControls` (optional invitee email → server mails the link,
//! Generate invite link → copy-to-clipboard, pending invites with revoke).
//!
//! Reads are live: members/users/invites come from the synced collections
//! (the web reads the same shapes); role/remove/revoke are §4.1 un-gated
//! mutations reflected by the Electric echo. Invite creation is stateful
//! (spinner + the generated URL); its plan-cap failure surfaces as the §4.9
//! neutral "Upgrade on the web" notice — never an upgrade dialog.

use gpui::{
    div, prelude::FluentBuilder as _, App, AppContext as _, ElementId, Entity, FontWeight,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    avatar::Avatar,
    button::{Button, ButtonVariants as _},
    clipboard::Clipboard,
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
};
use sync::Store;

use domain::board::format_short_date;
use domain::contract::TEAM_ROLE_OWNER;
use domain::rows::{User, TeamInvite, TeamMember};

use crate::navigation::{active_team_id, Navigation};
use crate::queries;

use super::{
    card, card_header, error_notice, is_owner, is_plan_limit, show_team_chrome, spawn_trpc,
    upgrade_notice,
};

/// One joined member row (web `members` + `userMap`).
struct MemberRow {
    member: TeamMember,
    user: Option<User>,
}

pub struct MembersPane {
    nav: Entity<Navigation>,
    /// Optional invitee email (EXP-188 invite-by-email) — empty = link-only.
    email_input: Entity<InputState>,
    invite_url: Option<SharedString>,
    generating: bool,
    error: Option<SharedString>,
    limit_notice: Option<SharedString>,
    /// "Invite sent to X" after a delivered email invite.
    sent_notice: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl MembersPane {
    pub fn new(
        nav: Entity<Navigation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let collections = Store::global(cx).collections().clone();
        let email_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Email (optional)"));
        let subscriptions = vec![
            cx.observe(&nav, |this, _, cx| {
                // Team switch: the generated URL belongs to the old one.
                this.invite_url = None;
                this.error = None;
                this.limit_notice = None;
                this.sent_notice = None;
                cx.notify();
            }),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_invites, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
            // The button label switches with the email field's emptiness.
            cx.subscribe(&email_input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];

        Self {
            nav,
            email_input,
            invite_url: None,
            generating: false,
            error: None,
            limit_notice: None,
            sent_notice: None,
            _subscriptions: subscriptions,
        }
    }

    /// Web: members joined with users, agent users filtered out
    /// (`!userMap.get(member.userId)?.isAgent`).
    fn member_rows(&self, team_id: &str, cx: &App) -> Vec<MemberRow> {
        let collections = Store::global(cx).collections();
        let users = collections.users.read(cx);
        let mut rows: Vec<MemberRow> = collections
            .team_members
            .read(cx)
            .iter()
            .filter(|member| member.team_id == team_id)
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

    fn pending_invites(&self, team_id: &str, cx: &App) -> Vec<TeamInvite> {
        let mut invites: Vec<TeamInvite> = Store::global(cx)
            .collections()
            .team_invites
            .read(cx)
            .iter()
            .filter(|invite| {
                invite.team_id == team_id && invite.accepted_at.is_none()
            })
            .cloned()
            .collect();
        invites.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        invites
    }

    fn generate_invite(
        &mut self,
        team_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
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
        let email = {
            let value = self.email_input.read(cx).value().trim().to_string();
            (!value.is_empty()).then_some(value)
        };

        self.generating = true;
        self.error = None;
        self.limit_notice = None;
        self.sent_notice = None;
        cx.notify();

        cx.spawn_in(window, async move |this, window| {
            let request_email = email.clone();
            let result = window
                .background_executor()
                .spawn(async move {
                    api::teams::team_invites_create(
                        &trpc,
                        &team_id,
                        api::teams::TeamRole::Member,
                        request_email.as_deref(),
                    )
                })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.generating = false;
                match result {
                    Ok(out) => {
                        let url: SharedString =
                            format!("{base}/invite/{}", out.token).into();
                        match (&email, out.email_delivered) {
                            (Some(sent_to), Some(true)) => {
                                // Delivered — clear the field and confirm; the
                                // link stays available as a manual fallback.
                                this.sent_notice =
                                    Some(format!("Invite sent to {sent_to}").into());
                                this.email_input.update(cx, |state, cx| {
                                    state.set_value("", window, cx);
                                });
                            }
                            (Some(_), _) => {
                                // Requested but not delivered (transport down
                                // or unconfigured) — fall back to the link.
                                this.error = Some(
                                    "Couldn't email the invite — copy the link and share it instead."
                                        .into(),
                                );
                            }
                            (None, _) => {}
                        }
                        this.invite_url = Some(url);
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
        let is_owner_row = role == TEAM_ROLE_OWNER;
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
        // Skip the email sub-line when the resolved name already IS the email
        // (the name-less Apple-ID case above), so it never shows twice.
        if let Some(email) = email.filter(|email| *email != name) {
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
/// Leave team (self) / Remove member (owner).
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
                    // Only the applicable role change renders — demoting another
                    // owner is always safe (I stay an owner). The no-op variant
                    // is HIDDEN, not a disabled dead item (EXP-228).
                    let role_item = if is_owner_row {
                        ("Make member", api::teams::TeamRole::Member, IconName::User)
                    } else {
                        ("Make owner", api::teams::TeamRole::Owner, IconName::Star)
                    };
                    let (label, role, icon) = role_item;
                    let member_id = member_id.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .icon(Icon::new(icon))
                            .on_click(move |_, _, cx| {
                                let member_id = member_id.clone();
                                spawn_trpc(cx, "teamMembers.updateRole", move |trpc| {
                                    api::teams::team_members_update_role(trpc, &member_id, role)
                                });
                            }),
                    );
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
                                spawn_trpc(cx, "teamMembers.remove", move |trpc| {
                                    api::teams::team_members_remove(trpc, &member_id)
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
        let Some(team_id) = active_team_id(&self.nav, cx) else {
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
        let i_am_owner = is_owner(cx, &team_id);
        let solo = !show_team_chrome(cx, &team_id);

        let rows = self.member_rows(&team_id, cx);
        let owner_count = rows
            .iter()
            .filter(|row| row.member.role.as_deref() == Some(TEAM_ROLE_OWNER))
            .count();

        let mut body = card(cx).child(card_header(
            if solo { "Invite teammates" } else { "Members" },
            if solo {
                "Invite someone to collaborate. Shared boards unlock team features.".to_string()
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
                                    "Enter an email to send the invite directly, or generate a link to share",
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

            let email_empty = self
                .email_input
                .read(cx)
                .value()
                .trim()
                .is_empty();
            invite_section = invite_section.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.email_input).small()),
                    )
                    .child(
                        Button::new("invite-generate")
                            .primary()
                            .small()
                            .label(if email_empty {
                                "Generate invite link"
                            } else {
                                "Send invite"
                            })
                            .icon(IconName::Plus)
                            .loading(self.generating)
                            .disabled(self.generating)
                            .on_click(cx.listener({
                                let team_id = team_id.clone();
                                move |this, _, window, cx| {
                                    this.generate_invite(team_id.clone(), window, cx);
                                }
                            })),
                    ),
            );

            if let Some(notice) = &self.sent_notice {
                invite_section = invite_section.child(sent_notice(notice.clone(), cx));
            }
            if let Some(notice) = &self.limit_notice {
                invite_section = invite_section.child(upgrade_notice(notice.clone(), cx));
            }
            if let Some(error) = &self.error {
                invite_section = invite_section.child(error_notice(error.clone(), cx));
            }

            let invites = self.pending_invites(&team_id, cx);
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
                    // Emailed invites show who they went to as the primary
                    // text; link-only invites keep the chip-first row.
                    let mut invite_identity = h_flex().gap_2().items_center();
                    if let Some(email) = invite.email.clone() {
                        invite_identity = invite_identity.child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(SharedString::from(email)),
                        );
                    }
                    invite_identity = invite_identity
                        .child(role_chip(IconName::User, role, cx))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(expires),
                        );
                    pending = pending.child(
                        h_flex()
                            .justify_between()
                            .items_center()
                            .px_3()
                            .py_2()
                            .rounded(cx.theme().radius)
                            .border_1()
                            .border_color(cx.theme().border)
                            .child(invite_identity)
                            .child(
                                Button::new(row_id("invite-revoke", &invite.id))
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::Delete)
                                    .on_click(move |_, _, cx| {
                                        let invite_id = invite_id.clone();
                                        spawn_trpc(cx, "teamInvites.revoke", move |trpc| {
                                            api::teams::team_invites_revoke(
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
    // Mirror `comments::author_label`: name (non-empty), else email
    // (non-empty), else the `Member <LAST4>` fallback. A blank name is the
    // Apple-ID case (Better Auth stores `name = ""` when Apple omits it), so a
    // truthy filter is what makes the email show through instead of an empty
    // label (EXP-228).
    let user = row.user.as_ref();
    user.and_then(|user| user.name.clone())
        .filter(|name| !name.is_empty())
        .or_else(|| {
            user.and_then(|user| user.email.clone())
                .filter(|email| !email.is_empty())
        })
        .unwrap_or_else(|| domain::member_fallback_label(&row.member.user_id))
}

/// "Invite sent to X" confirmation (EXP-188 invite-by-email).
fn sent_notice(message: SharedString, cx: &App) -> impl IntoElement {
    div()
        .px_3()
        .py_2()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(theme::tokens::GREEN.to_hsla().opacity(0.5))
        .bg(theme::tokens::GREEN.to_hsla().opacity(0.1))
        .text_sm()
        .text_color(theme::tokens::GREEN.to_hsla())
        .child(message)
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
