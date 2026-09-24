//! Settings → Members (masterplan-v3 §4.2).
//!
//! Web parity: `components/team/members-section.tsx` — the member list
//! (avatar + name + role badge + per-row actions `DropdownMenu`: Make owner /
//! Make member / Remove member / Leave team) and the owner-only
//! `InviteControls`.
//!
//! EXP-771 took the web's invite UX verbatim: an email field beside a PRIMARY
//! "Send invite", then the generated link (read-only + copy) whenever there is
//! one, then an OUTLINE "Generate invite link" for the share-it-yourself path,
//! then the pending invites with revoke. It used to be ONE button whose label
//! flipped with the field's emptiness — two intents on one control, and the
//! link path was unreachable while an address was typed.
//!
//! EXP-630 placeholder members: an invite by email puts the person on the
//! ROSTER at once (name + email, assignable before they sign in), so the send
//! control is [`super::invite_form::InviteForm`] — the web's shared form, Name
//! beside the address — and a member row that has not joined yet wears a muted
//! "Invited" / "Invite expired" pill (the [`domain::placeholder_status`] rule
//! over the synced invites) with "Resend invite" in its overflow menu. The
//! pending list is what is left: unaccepted AND unexpired links.
//!
//! Reads are live: members/users/invites come from the synced collections
//! (the web reads the same shapes); role/remove/revoke are §4.1 un-gated
//! mutations reflected by the Electric echo, except remove/leave — those
//! confirm first, in the web's words. Invite creation is stateful (spinner +
//! the generated URL); its seat-cap failure surfaces as the web's "Out of
//! seats" copy, rendered as a §4.9 inline notice — never an upgrade dialog.

use gpui::{
    div, prelude::FluentBuilder as _, App, AppContext as _, ElementId, Entity, FontWeight,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;

use domain::board::format_short_date;
use domain::contract::TEAM_ROLE_OWNER;
use domain::placeholder_status::{invite_is_pending, placeholder_statuses, PlaceholderStatus};
use domain::rows::{User, TeamInvite, TeamMember};

use crate::controls::WebControl as _;
use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;

use super::invite_form::{
    invite_link_row, out_of_seats_notice, InviteForm, InviteFormLayout, ResendTarget,
};
use super::{section, error_notice, is_owner, is_plan_limit, spawn_trpc};
use crate::icons::registry;

/// The web's helper line under "Invite members" — an invite by email is a
/// ROSTER action now (EXP-630), and the copy says so. Byte-identical to
/// `members-section.tsx`.
const INVITE_HELP: &str = "Send an invite by email — they join the team right \
     away and can be assigned work before they sign in — or generate a link to \
     share yourself";

/// One joined member row (web `members` + `userMap`).
struct MemberRow {
    member: TeamMember,
    user: Option<User>,
}

pub struct MembersPane {
    nav: Entity<Navigation>,
    /// EXP-630: the shared invite-by-email form (name + address + "Send
    /// invite"), the same view the "Resend invite" dialog renders.
    invite_form: Entity<InviteForm>,
    /// The link minted by the OUTLINE "Generate invite link" (the form owns
    /// the one its own failed delivery falls back to).
    invite_url: Option<SharedString>,
    /// Web `generating` — the OUTLINE "Generate invite link" is in flight.
    generating: bool,
    error: Option<SharedString>,
    /// The seat cap rejected the invite — the web's "Out of seats" notice.
    out_of_seats: bool,
    _subscriptions: Vec<Subscription>,
}

impl MembersPane {
    pub fn new(
        nav: Entity<Navigation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let collections = Store::global(cx).collections().clone();
        let team_id = active_team_id(&nav, cx).unwrap_or_default();
        let invite_form =
            cx.new(|cx| InviteForm::new(team_id, None, InviteFormLayout::Row, window, cx));
        let subscriptions = vec![
            cx.observe(&nav, |this, _, cx| {
                // Team switch: the generated URL belongs to the old one, and so
                // does everything the form has on screen.
                this.invite_url = None;
                this.error = None;
                this.out_of_seats = false;
                let team_id = active_team_id(&this.nav, cx).unwrap_or_default();
                this.invite_form
                    .update(cx, |form, cx| form.reset_for_team(team_id, cx));
                cx.notify();
            }),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_invites, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
            // The form's own spinner gates this pane's link button too.
            cx.observe(&invite_form, |_, _, cx| cx.notify()),
        ];

        Self {
            nav,
            invite_form,
            invite_url: None,
            generating: false,
            error: None,
            out_of_seats: false,
            _subscriptions: subscriptions,
        }
    }

    /// Web: members joined with users.
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
            .collect();
        rows.sort_by(|a, b| {
            display_name(a)
                .to_lowercase()
                .cmp(&display_name(b).to_lowercase())
        });
        rows
    }

    /// Every synced invite of the team, newest first — the input both the
    /// member-row badges and the pending list read.
    fn team_invites(&self, team_id: &str, cx: &App) -> Vec<TeamInvite> {
        let mut invites: Vec<TeamInvite> = Store::global(cx)
            .collections()
            .team_invites
            .read(cx)
            .iter()
            .filter(|invite| invite.team_id == team_id)
            .cloned()
            .collect();
        invites.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        invites
    }

    /// `teamInvites.create` with NO address — the web's second intent:
    /// "Generate invite link", the share-it-yourself path. It mints no
    /// placeholder member (nobody was named), so it only ever produces a link.
    /// The email intent lives in [`InviteForm`].
    fn generate_link(
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

        self.generating = true;
        self.error = None;
        self.out_of_seats = false;
        cx.notify();

        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    api::teams::team_invites_create(
                        &trpc,
                        &team_id,
                        api::teams::TeamRole::Member,
                        None,
                        api::teams::InviteExtras::default(),
                    )
                })
                .await;
            let _ = this.update_in(window, |this, _, cx| {
                this.generating = false;
                match result {
                    Ok(out) => {
                        this.invite_url = Some(format!("{base}/invite/{}", out.token).into());
                    }
                    Err(err) if is_plan_limit(&err) => {
                        this.out_of_seats = true;
                    }
                    Err(err) => {
                        this.error =
                            Some(super::form_error(&err, "Couldn't create the invite.").into());
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
        team_id: &str,
        placeholder: Option<PlaceholderStatus>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
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
            registry::UI_OWNER
        } else {
            registry::UI_MEMBER
        };

        let mut identity = v_flex().gap_0p5().child(
            h_flex()
                // EXP-698: 12px from the name to the role pill, the same
                // rhythm the row keeps to its trailing icon button.
                .gap_3()
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
                .child(role_chip(
                    row_id("member-role", &member.id),
                    role_icon,
                    SharedString::from(role.clone()),
                    cx,
                ))
                // EXP-630: the roster row of somebody who has not joined yet —
                // MUTED, right after the role, so the list reads "who is here"
                // with "and who is still on the way" as a footnote.
                .when_some(placeholder, |identity_row, status| {
                    identity_row.child(placeholder_chip(
                        row_id("member-placeholder", &member.id),
                        status,
                        cx,
                    ))
                }),
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

        // EXP-721: a member is an OBJECT, not a field of a form — so every
        // team-settings entity list wears the gapped `glass_row_card` ladder
        // (the labels list's idiom, now the rule on all four clients) instead
        // of fusing into one inset-grouped block.
        crate::surface::flat_row()
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .justify_between()
            .px_3()
            .py_2()
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    // EXP-547: picture + initials fallback (web
                    // `MembersSection` renders `AvatarImage`).
                    .child(crate::user_avatar::user_avatar(
                        &member.user_id,
                        &name,
                        row.user.as_ref().and_then(|user| user.image.as_deref()),
                        gpui_component::Size::Small,
                        cx,
                    ))
                    .child(identity),
            )
            .when(show_actions, |row_el| {
                row_el.child(member_actions_menu(
                    member.id.clone(),
                    &name,
                    is_self,
                    is_owner_row,
                    i_am_owner,
                    // EXP-630: a placeholder member gets "Resend invite" —
                    // the dialog needs the team plus the row's own prefill.
                    placeholder.map(|_| ResendContext {
                        team_id: team_id.to_string(),
                        display_name: name.clone(),
                        target: ResendTarget {
                            user_id: member.user_id.clone(),
                            name: row
                                .user
                                .as_ref()
                                .and_then(|user| user.name.clone())
                                .unwrap_or_default(),
                            email: row
                                .user
                                .as_ref()
                                .and_then(|user| user.email.clone())
                                .unwrap_or_default(),
                        },
                    }),
                    cx,
                ))
            })
    }
}

/// What the row's "Resend invite" needs to open its dialog (EXP-630) —
/// `None` on a member who has joined.
#[derive(Clone, Debug)]
struct ResendContext {
    team_id: String,
    /// The row's resolved label — it leads the dialog's description.
    display_name: String,
    target: ResendTarget,
}

/// Web `DropdownMenu` per member row: Resend invite (owner, not self, still
/// invited), role changes (owner, not self), then Leave team (self) / Remove
/// member (owner).
fn member_actions_menu(
    member_id: String,
    name: &str,
    is_self: bool,
    is_owner_row: bool,
    i_am_owner: bool,
    resend: Option<ResendContext>,
    cx: &gpui::App,
) -> impl IntoElement {
    // EXP-862: a row's "..." is a GHOST glyph, never a circle.
    crate::controls::ghost_icon_button(
        row_id("member-actions", &member_id),
        Icon::new(registry::UI_MORE),
        cx,
    )
        .dropdown_menu({
            let member_id = member_id.clone();
            let name = name.to_string();
            move |mut menu, _, _| {
                // EXP-630: first item — the invite is the thing that is
                // unfinished about this row.
                if let Some(resend) = resend.clone().filter(|_| i_am_owner && !is_self) {
                    menu = menu.item(
                        PopupMenuItem::new("Resend invite")
                            .icon(Icon::new(registry::UI_MAIL))
                            .on_click(move |_, window, cx| {
                                let resend = resend.clone();
                                super::resend_invite_dialog::open(
                                    window,
                                    cx,
                                    resend.team_id,
                                    resend.display_name,
                                    resend.target,
                                );
                            }),
                    );
                }
                if i_am_owner && !is_self {
                    // Only the applicable role change renders — demoting another
                    // owner is always safe (I stay an owner). The no-op variant
                    // is HIDDEN, not a disabled dead item (EXP-228).
                    let role_item = if is_owner_row {
                        ("Make member", api::teams::TeamRole::Member, registry::UI_MEMBER)
                    } else {
                        ("Make owner", api::teams::TeamRole::Owner, registry::UI_OWNER)
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
                    let name = name.clone();
                    // EXP-687: leaving is a sign-out, removing someone is a
                    // user-minus — the web draws the same two concepts.
                    let (label, icon) = if is_self {
                        ("Leave team", registry::NAV_SIGN_OUT)
                    } else {
                        ("Remove member", registry::UI_REMOVE_MEMBER)
                    };
                    menu = menu.item(
                        PopupMenuItem::new(label)
                            .icon(Icon::new(icon))
                            // EXP-771: losing team access is instant and has
                            // no undo — it confirms first, in the web's words.
                            .on_click(move |_, window, cx| {
                                open_remove_member_dialog(
                                    member_id.clone(),
                                    name.clone(),
                                    is_self,
                                    window,
                                    cx,
                                );
                            }),
                    );
                }
                menu
            }
        })
}

/// The web's remove/leave confirm (`members-section.tsx`), word for word:
/// losing access to a team is immediate and cannot be undone, and the item sits
/// right under the role toggles — so it asks first, like every other
/// destructive settings action.
fn open_remove_member_dialog(
    member_id: String,
    name: String,
    is_self: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let (title, description, ok_text) = if is_self {
        (
            "Leave team",
            "Leave this team? You lose access to its boards and issues \
             immediately and need a new invite to rejoin."
                .to_string(),
            "Leave team",
        )
    } else {
        (
            "Remove member",
            format!(
                "Remove {name} from the team? They lose access to its boards \
                 and issues immediately."
            ),
            "Remove",
        )
    };
    let spec = AlertSpec::new(title, description, ok_text)
        .ok_variant(ButtonVariant::Danger)
        .on_ok(move |_, cx| {
            let member_id = member_id.clone();
            spawn_trpc(cx, "teamMembers.remove", move |trpc| {
                api::teams::team_members_remove(trpc, &member_id)
            });
            true
        });
    native_dialog::open_alert(window, cx, spec);
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

        let rows = self.member_rows(&team_id, cx);
        let owner_count = rows
            .iter()
            .filter(|row| row.member.role.as_deref() == Some(TEAM_ROLE_OWNER))
            .count();
        // EXP-630: who is on the roster without having joined — read off the
        // SYNCED invites, by the same rule the web and the natives apply.
        let invites = self.team_invites(&team_id, cx);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let placeholders = placeholder_statuses(&invites, now_ms);

        // EXP-771: the web header — a bare "Members" label, no count subline
        // (EXP-698 retired header counts on every client, and the desktop's
        // "N members in this team" was the last one left in the panes).
        let mut body = section(cx).child(crate::surface::glass_section_header(
            "Members",
            None,
            cx,
        ));

        // EXP-994: ONE table — a hairline between rows, no gap, no card
        // around them (see [`Self::render_member_row`]).
        let mut list = v_flex().w_full().min_w_0();
        for (index, row) in rows.iter().enumerate() {
            list = list.child(crate::surface::list_row(
                self.render_member_row(
                    row,
                    &my_user_id,
                    i_am_owner,
                    owner_count,
                    &team_id,
                    placeholders.get(&row.member.user_id).copied(),
                    cx,
                ),
                index,
            ));
        }
        body = body.child(list);

        // InviteControls (web: owner-only `showInvite`).
        if i_am_owner {
            let busy = self.generating || self.invite_form.read(cx).sending();
            // Web order, top to bottom: the heading pair, the invite form, the
            // generated link, the "Generate invite link" button, the pending
            // list. The heading is SENTENCE case here — the web's "Invite
            // Members" is the last title-cased heading in the settings pages.
            let mut invite_section = v_flex()
                .gap_3()
                .pt_3()
                .border_t_1()
                .border_color(super::row_stroke(cx))
                .child(
                    v_flex()
                        .gap_0p5()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child("Invite members"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(INVITE_HELP),
                        ),
                );

            // EXP-630: THE shared form (name + address + "Send invite") — it
            // owns its own spinner, notices and fallback link.
            invite_section = invite_section.child(self.invite_form.clone());

            // The notices below belong to the LINK path only (the form renders
            // its own).
            if self.out_of_seats {
                invite_section = invite_section.child(out_of_seats_notice(cx));
            }
            if let Some(error) = &self.error {
                invite_section = invite_section.child(error_notice(error.clone(), cx));
            }

            if let Some(url) = &self.invite_url {
                invite_section =
                    invite_section.child(invite_link_row("invite-link-copy", url.clone(), cx));
            }

            invite_section = invite_section.child(
                h_flex().child(
                    Button::new("invite-generate")
                        .outline()
                        .web_sm()
                        .label("Generate invite link")
                        .icon(registry::UI_LINK)
                        .loading(self.generating)
                        .disabled(busy)
                        .on_click(cx.listener({
                            let team_id = team_id.clone();
                            move |this, _, window, cx| {
                                this.generate_link(team_id.clone(), window, cx);
                            }
                        })),
                ),
            );

            // EXP-630: PENDING = unaccepted AND unexpired. An expired
            // placeholder invite is a badge on its member row above, not a
            // live link in this list.
            let pending: Vec<&TeamInvite> = invites
                .iter()
                .filter(|invite| invite_is_pending(invite, now_ms))
                .collect();
            if !pending.is_empty() {
                let mut pending_rows = v_flex().w_full().min_w_0();
                for (invite_index, invite) in pending.iter().enumerate() {
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
                    match invite.email.clone() {
                        Some(email) => {
                            invite_identity = invite_identity.child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(SharedString::from(email)),
                            );
                        }
                        // EXP-698: a link-only invite went to nobody in
                        // particular — the address slot says so (muted)
                        // instead of collapsing, so the role chip and the
                        // expiry stay in the same column on every row.
                        None => {
                            invite_identity = invite_identity.child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Link invite"),
                            );
                        }
                    }
                    invite_identity = invite_identity
                        .child(role_chip(
                            row_id("invite-role", &invite.id),
                            registry::UI_MEMBER,
                            role,
                            cx,
                        ))
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(expires),
                        );
                    // EXP-721: a pending invite is an entity too — the same
                    // gapped row card the member rows above it wear, instead
                    // of the hand-rolled bordered box.
                    pending_rows = pending_rows.child(crate::surface::list_row(
                        crate::surface::flat_row()
                            .flex()
                            .w_full()
                            .min_w_0()
                            .justify_between()
                            .items_center()
                            .px_3()
                            .py_2()
                            .child(invite_identity)
                            .child(
                                Button::new(row_id("invite-revoke", &invite.id))
                                    .ghost()
                                    .web_icon_xs()
                                    .icon(registry::UI_DELETE)
                                    .on_click(move |_, _, cx| {
                                        let invite_id = invite_id.clone();
                                        spawn_trpc(cx, "teamInvites.revoke", move |trpc| {
                                            api::teams::team_invites_revoke(
                                                trpc, &invite_id,
                                            )
                                        });
                                    }),
                            ),
                        invite_index,
                    ));
                }
                // EXP-771: the web's `GlassSectionHeader` — the same heading
                // the Members list above wears. NO gap on the wrapper: the
                // header's own `pb_2` IS the 8px to the rows (EXP-697).
                invite_section = invite_section.child(
                    v_flex()
                        .child(crate::surface::glass_section_header(
                            "Pending invites",
                            None,
                            cx,
                        ))
                        .child(pending_rows),
                );
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

/// EXP-698: the role badge IS the shared small READONLY pill, with the role
/// glyph leading. (It was a bespoke `role_chip` recipe — the last chip shape
/// in the settings panes.)
fn role_chip(
    id: impl Into<ElementId>,
    icon: crate::icons::ExpIcon,
    label: SharedString,
    cx: &App,
) -> impl IntoElement {
    use crate::surface::{PillMode, PillSize};
    crate::surface::glass_pill(id, PillSize::Sm, PillMode::Readonly, cx)
    .child(Icon::new(icon).with_size(gpui::px(PillSize::Sm.glyph())))
    .child(label)
}

/// EXP-630: the "invited, not joined" badge — the same small readonly pill the
/// role wears, MUTED and led by the mail concept, so it reads as a footnote on
/// the row rather than a second role.
fn placeholder_chip(
    id: impl Into<ElementId>,
    status: PlaceholderStatus,
    cx: &App,
) -> impl IntoElement {
    use crate::surface::{PillMode, PillSize};
    crate::surface::glass_pill(id, PillSize::Sm, PillMode::Readonly, cx)
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(registry::UI_MAIL).with_size(gpui::px(PillSize::Sm.glyph())))
        .child(status.label())
}

fn row_id(kind: &str, id: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("{kind}-{id}")))
}
