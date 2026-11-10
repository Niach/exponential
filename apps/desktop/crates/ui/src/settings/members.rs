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
//! control is the web's shared invite form — Name beside the address — and a
//! member row that has not joined yet wears a muted "Invited" / "Invite
//! expired" pill (the [`domain::placeholder_status`] rule over the synced
//! invites) with "Resend invite" in its overflow menu. EXP-1076: a row the
//! Linear import seated without ever mailing (`sent_at` NULL) wears "Not
//! invited" and the menu says "Send invite" — the same re-invite call
//! underneath. The pending list is what is left: unaccepted AND unexpired
//! links.
//!
//! Reads are live: members/users/invites come from the synced collections
//! (the web reads the same shapes); role/remove/revoke are §4.1 un-gated
//! mutations reflected by the Electric echo, except remove/leave — those
//! confirm first, in the web's words. Invite creation is stateful (spinner +
//! the generated URL); its seat-cap failure surfaces as the web's "Out of
//! seats" copy, rendered as a §4.9 inline notice — never an upgrade dialog.
//!
//! #EXP-1629: the pane is a document on the native binding (projection at
//! `/pane`): the bands, the roster rows (avatar, name + "(you)", the role
//! and placeholder pills, the email), the invite form (genUI `box` Inputs,
//! the primary "Send invite" with its pending face, the outline "Add without
//! inviting"), the notices, the link rows with their copy buttons, "Generate
//! invite link", the pending invites with revoke, and each row's ⋯ (a
//! genUI `Menu`); the confirms and the Resend invite dialog stay native.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    div, App, AppContext as _, ClipboardItem, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{ActiveTheme as _, Colorize as _, IconNamed as _};
use serde_json::{json, Value};
use sync::Store;

use domain::board::format_short_date;
use domain::contract::TEAM_ROLE_OWNER;
use domain::placeholder_status::{invite_is_pending, placeholder_statuses, PlaceholderStatus};
use domain::rows::{TeamInvite, TeamMember, User};
use eui_gpui::binding::Binding;
use eui_gpui::host::HostPlugin;
use eui_gpui::view::SurfaceView;

use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, Navigation};
use crate::queries;

use super::doc::{and, empty_state, hex, mount_with, not, path, IdeToasts, Ink, SM, VALUE_ALPHA, XS};
use super::invite_form::{invite_outcome, InviteOutcome, ResendTarget, INVALID_EMAIL, OUT_OF_SEATS_MESSAGE, OUT_OF_SEATS_NOTE, OUT_OF_SEATS_TITLE};
use super::{is_owner, is_plan_limit, spawn_trpc};
use crate::icons::registry;

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

/// One joined member row (web `members` + `userMap`).
struct MemberRow {
    member: TeamMember,
    user: Option<User>,
}

/// Which link a copy button copies (the form's fallback or the generated
/// one).
#[derive(Clone, Copy, PartialEq, Eq)]
enum LinkSlot {
    Form,
    Pane,
}

pub struct MembersPane {
    nav: Entity<Navigation>,
    /// The invite form (web `InviteMemberForm`, `allowAddWithoutInvite`):
    /// the typed Name and address.
    name: String,
    email: String,
    /// "Send invite" is in flight; EXP-1259 D8: its pending face
    /// ("Inviting").
    sending: bool,
    pending: crate::feedback::Pending,
    /// FEED-108: `teamMembers.addPlaceholder` is in flight — "Add without
    /// inviting" spins, no word (web parity: no `pendingLabel`).
    adding: bool,
    /// FEED-79: the address the server refused — INVALID INPUT, so it stays
    /// under the field until the address changes; every other failure is a
    /// toast.
    email_error: Option<String>,
    /// The form's seat-cap notice and its minted link (the manual fallback
    /// when delivery failed).
    form_out_of_seats: bool,
    form_url: Option<String>,
    /// The link minted by the OUTLINE "Generate invite link".
    invite_url: Option<String>,
    /// Web `generating` — the OUTLINE "Generate invite link" is in flight.
    generating: bool,
    /// The seat cap rejected the link — the web's "Out of seats" notice.
    out_of_seats: bool,
    /// The copy button that shows its check (for two seconds after a copy).
    copied: Option<LinkSlot>,
    copy_generation: u64,
    #[cfg(test)]
    shot: Option<MembersShot>,
    /// What each row's ⋯ opens, by member id (refilled by every
    /// projection).
    menus: RefCell<HashMap<String, MenuRow>>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    _subscriptions: Vec<Subscription>,
}

/// A state the pictures show (`settings_bench`): the headless store syncs
/// no team, so the roster, the invites and the form are pinned.
#[cfg(test)]
pub(crate) struct MembersShot {
    /// `None` = no team selected.
    pub team: Option<String>,
    pub owner: bool,
    pub me: String,
    pub members: Vec<TeamMember>,
    pub users: Vec<User>,
    pub invites: Vec<TeamInvite>,
    /// The form's typed Name and address.
    pub name: String,
    pub email: String,
    /// "Send invite" / "Add without inviting" in flight.
    pub sending: bool,
    pub adding: bool,
    /// The form's own seat-cap notice, fallback link and refused address.
    pub form_out_of_seats: bool,
    pub form_link: Option<String>,
    pub email_error: Option<String>,
    /// The link path: the generated link, its seat-cap notice, in flight.
    pub link: Option<String>,
    pub out_of_seats: bool,
    pub generating: bool,
}

impl MembersPane {
    pub fn new(
        nav: Entity<Navigation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |this, _, cx| {
                // Team switch: the minted links belong to the old one, and so
                // does everything the form has on screen.
                this.invite_url = None;
                this.out_of_seats = false;
                this.form_url = None;
                this.form_out_of_seats = false;
                this.email_error = None;
                cx.notify();
            }),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_invites, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
        ];
        // The actions leave the binding for the pane: the form's edits and
        // sends, the copies, Generate and Revoke.
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["edit", "send", "add", "generate", "copy", "revoke", "menu"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let context = event.context.clone();
                    let payload = event.payload.clone().unwrap_or(Value::Null);
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update and these write it.
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| pane.action(name, &context, &payload, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let tints = Tints::of(cx);
        let plugin = Rc::new(MembersHost { ide: super::doc::ide_plugin() });
        let view = mount_with("settings-members", document(&Ink::of(cx), &tints), &binding, plugin, false, window, cx);
        Self {
            nav,
            name: String::new(),
            email: String::new(),
            sending: false,
            pending: crate::feedback::Pending::default(),
            adding: false,
            email_error: None,
            form_out_of_seats: false,
            form_url: None,
            invite_url: None,
            generating: false,
            out_of_seats: false,
            copied: None,
            copy_generation: 0,
            #[cfg(test)]
            shot: None,
            menus: RefCell::default(),
            binding,
            view,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(crate::navigation::nav_for_window(window, cx), window, cx)
    }

    /// The pane in a pinned state; [`Self::shot_act`] fills the form.
    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: MembersShot, cx: &mut gpui::Context<Self>) {
        self.invite_url = shot.link.clone();
        self.out_of_seats = shot.out_of_seats;
        self.generating = shot.generating;
        self.shot = Some(shot);
        cx.notify();
    }

    /// The bench's press on a node by id (a row's ⋯), its popup's fade run
    /// out (the headless clock stands still).
    #[cfg(test)]
    pub(crate) fn press_for_shots(&mut self, id: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.view.update(cx, |view, cx| {
            let index = view.index_of(id).expect("the pressed node");
            view.press(index, window, cx);
            view.advance_clock(std::time::Duration::from_millis(500), cx);
        });
    }

    /// The form's typed text and its request in flight, after the first
    /// frames.
    #[cfg(test)]
    pub(crate) fn shot_act(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(shot) = &self.shot else { return };
        self.name = shot.name.clone();
        self.email = shot.email.clone();
        if shot.sending {
            self.sending = true;
            self.pending.start(cx);
        }
        self.adding = shot.adding;
        self.form_out_of_seats = shot.form_out_of_seats;
        self.form_url = shot.form_link.clone();
        self.email_error = shot.email_error.clone();
        cx.notify();
    }

    fn pane_team(&self, cx: &App) -> Option<String> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.team.clone();
        }
        active_team_id(&self.nav, cx)
    }

    fn pane_owner(&self, team_id: &str, cx: &App) -> bool {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.owner;
        }
        is_owner(cx, team_id)
    }

    fn my_user_id(&self, cx: &App) -> String {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.me.clone();
        }
        queries::active_account(cx).map(|account| account.user_id).unwrap_or_default()
    }

    /// Web: members joined with users.
    fn member_rows(&self, team_id: &str, cx: &App) -> Vec<MemberRow> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            let mut rows: Vec<MemberRow> = shot.members.iter().map(|member| MemberRow { member: member.clone(), user: shot.users.iter().find(|user| user.id == member.user_id).cloned() }).collect();
            sort_join_order(&mut rows);
            return rows;
        }
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
        sort_join_order(&mut rows);
        rows
    }

    /// Every synced invite of the team, newest first — the input both the
    /// member-row badges and the pending list read.
    fn team_invites(&self, team_id: &str, cx: &App) -> Vec<TeamInvite> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            let mut invites = shot.invites.clone();
            invites.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            return invites;
        }
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

    /// A request is in flight (send or add): the form's other button and
    /// "Generate invite link" wait for it.
    fn form_busy(&self) -> bool {
        self.sending || self.adding
    }

    /// A document action: the form's `edit` (field in the context, text in
    /// the payload), `send` / `add`, `generate`, `copy` (which link) and
    /// `revoke` (the invite `key`).
    fn action(&mut self, name: &str, context: &Value, payload: &Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.pane_team(cx) else { return };
        match name {
            "edit" => {
                let text = payload.get("value").and_then(Value::as_str).unwrap_or_default().to_string();
                match context.get("field").and_then(Value::as_str) {
                    Some("name") => self.name = text,
                    Some("email") => {
                        if text != self.email {
                            self.email_error = None;
                        }
                        self.email = text;
                    }
                    _ => return,
                }
                cx.notify();
            }
            // Web: Enter in the address field sends (the empty-address and
            // in-flight guards are the button's).
            "send" => self.send(team_id, window, cx),
            "add" => self.add(team_id, window, cx),
            "generate" => self.generate_link(team_id, window, cx),
            "copy" => {
                let slot = match context.get("link").and_then(Value::as_str) {
                    Some("form") => LinkSlot::Form,
                    _ => LinkSlot::Pane,
                };
                self.copy(slot, cx);
            }
            "revoke" => {
                let invite_id = context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
                if invite_id.is_empty() {
                    return;
                }
                spawn_trpc(cx, "teamInvites.revoke", move |trpc| {
                    api::teams::team_invites_revoke(trpc, &invite_id)
                });
            }
            // A row's ⋯ pick: its confirm or the Resend invite dialog.
            "menu" => {
                let key = context.get("key").and_then(Value::as_str).unwrap_or_default();
                let Some(row) = self.menus.borrow().get(key).cloned() else { return };
                match payload.get("value").and_then(Value::as_str) {
                    // dialog stays native: its own window around the shared gpui `InviteForm`.
                    Some("resend") => {
                        let Some(resend) = row.resend.filter(|_| row.i_am_owner && !row.is_self) else { return };
                        super::resend_invite_dialog::open(window, cx, resend.team_id, resend.display_name, resend.status, resend.target);
                    }
                    // EXP-1230: a role change confirms with the prompts
                    // fixture's make-owner / make-member.
                    Some("role") if row.i_am_owner && !row.is_self => {
                        let role = if row.is_owner_row { api::teams::TeamRole::Member } else { api::teams::TeamRole::Owner };
                        open_role_dialog(row.member_id, &row.name, role, window, cx);
                    }
                    // EXP-771: losing team access is instant and has no
                    // undo — it confirms first, in the web's words.
                    Some("remove") if row.is_self || row.i_am_owner => {
                        open_remove_member_dialog(row.member_id, &row.team_id, &row.name, row.is_self, window, cx);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// gpui-component's `Clipboard`: the link to the clipboard, the button's
    /// check for two seconds (no second copy meanwhile).
    fn copy(&mut self, slot: LinkSlot, cx: &mut gpui::Context<Self>) {
        if self.copied == Some(slot) {
            return;
        }
        let url = match slot {
            LinkSlot::Form => self.form_url.clone(),
            LinkSlot::Pane => self.invite_url.clone(),
        };
        let Some(url) = url else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(url));
        self.copied = Some(slot);
        self.copy_generation += 1;
        let generation = self.copy_generation;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_secs(2)).await;
            let _ = this.update(cx, |this, cx| {
                if this.copy_generation == generation {
                    this.copied = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// `teamInvites.create` with an address: the server mails the link, puts
    /// the invitee on the roster as a placeholder member and reports delivery
    /// ([`invite_outcome`], the shared form's reading). Delivery is
    /// best-effort — with no transport configured (or on a failed send) the
    /// link is shown so the owner can share it by hand.
    fn send(&mut self, team_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.form_busy() {
            return;
        }
        let email = self.email.trim().to_string();
        if email.is_empty() {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let Some(account) = queries::active_account(cx) else {
            return;
        };
        let base = account.instance_url;
        let name = self.name.trim().to_string();

        self.sending = true;
        self.pending.start(cx);
        self.email_error = None;
        self.form_out_of_seats = false;
        cx.notify();

        cx.spawn_in(window, async move |this, window| {
            let request = (email.clone(), name);
            let result = window
                .background_executor()
                .spawn(async move {
                    let (email, name) = request;
                    api::teams::team_invites_create(
                        &trpc,
                        &team_id,
                        api::teams::TeamRole::Member,
                        Some(&email),
                        api::teams::InviteExtras { name: Some(name.as_str()), placeholder_user_id: None },
                    )
                })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.sending = false;
                this.pending.finish();
                match invite_outcome(result, &base, &email, window, cx) {
                    // Delivered: the form empties for the next invite; the
                    // link stays as a manual fallback.
                    InviteOutcome::Delivered(url) => {
                        this.form_url = Some(url);
                        this.name.clear();
                        this.email.clear();
                    }
                    InviteOutcome::Undelivered(url) => this.form_url = Some(url),
                    InviteOutcome::OutOfSeats => this.form_out_of_seats = true,
                    InviteOutcome::Refused(message) => this.email_error = Some(message),
                    InviteOutcome::Failed => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// FEED-108: `teamMembers.addPlaceholder` — the roster row WITHOUT a link.
    /// Nothing is mailed, so there is no link to fall back to; a refused
    /// address (already a member, or an account that must be invited) stays
    /// under the field like the invite's, everything else toasts. Web
    /// `handleAdd`.
    fn add(&mut self, team_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.form_busy() {
            return;
        }
        let email = self.email.trim().to_string();
        if email.is_empty() {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let name = self.name.trim().to_string();

        self.adding = true;
        self.email_error = None;
        self.form_out_of_seats = false;
        cx.notify();

        cx.spawn_in(window, async move |this, window| {
            let request = (email, name);
            let result = window
                .background_executor()
                .spawn(async move {
                    let (email, name) = request;
                    api::teams::team_members_add_placeholder(&trpc, &team_id, &email, Some(name.as_str()))
                })
                .await;
            let _ = this.update_in(window, |this, window, cx| {
                this.adding = false;
                match result {
                    Ok(member) => {
                        // Web: `Added <name> to the team`, then an empty form
                        // for the next person. The roster row arrives over
                        // sync and reads "Not invited".
                        let added = format!("Added {} to the team", member.name);
                        crate::navigation::defer_in_result_window(window, cx, move |w, cx| {
                            crate::toast::success(added, w, cx);
                        });
                        this.name.clear();
                        this.email.clear();
                        this.form_url = None;
                    }
                    // The refusal stays under its field (feedback-states.json's
                    // invalid-input rule), exactly like a refused invite address.
                    Err(api::ApiError::Http { status: 400, message }) => {
                        this.email_error = Some(crate::action_error::description_of(&message).unwrap_or_else(|| INVALID_EMAIL.to_string()));
                    }
                    Err(err) => crate::action_error::failed_action_err("add", "the member", &err, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `teamInvites.create` with NO address — the web's second intent:
    /// "Generate invite link", the share-it-yourself path. It mints no
    /// placeholder member (nobody was named), so it only ever produces a link.
    fn generate_link(
        &mut self,
        team_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.generating || self.form_busy() {
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
        self.out_of_seats = false;
        cx.notify();
        let retry_team = team_id.clone();

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
                        this.invite_url = Some(format!("{base}/invite/{}", out.token));
                    }
                    Err(err) if is_plan_limit(&err) => {
                        this.out_of_seats = true;
                    }
                    // FEED-79: the error toast; Retry mints again.
                    Err(err) => {
                        let pane = cx.entity().downgrade();
                        crate::action_error::failed_action_with_retry(
                            "create",
                            "the invite link",
                            crate::action_error::action_error_description(&err),
                            move |window, cx| {
                                let team_id = retry_team.clone();
                                let _ = pane.update(cx, |this, cx| this.generate_link(team_id, window, cx));
                            },
                            cx,
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The pane's state as the document reads it; what the rows' ⋯ menus
    /// open is kept for their picks at the same time.
    fn project(&self, cx: &App) -> Value {
        let mut menus = HashMap::new();
        let Some(team_id) = self.pane_team(cx) else {
            *self.menus.borrow_mut() = menus;
            return json!({"noTeam": true, "owner": false, "rows": [], "pending": []});
        };
        let my_user_id = self.my_user_id(cx);
        let i_am_owner = self.pane_owner(&team_id, cx);
        let rows = self.member_rows(&team_id, cx);
        let owner_count = rows.iter().filter(|row| row.member.role.as_deref() == Some(TEAM_ROLE_OWNER)).count();
        // EXP-630: who is on the roster without having joined — read off the
        // SYNCED invites, by the same rule the web and the natives apply.
        let invites = self.team_invites(&team_id, cx);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let placeholders = placeholder_statuses(&invites, now_ms);

        let rows: Vec<Value> = rows
            .iter()
            .map(|row| {
                let member = &row.member;
                let is_self = member.user_id == my_user_id;
                let role = member.role.clone().unwrap_or_else(|| "member".to_string());
                let is_owner_row = role == TEAM_ROLE_OWNER;
                let name = display_name(row);
                let user = row.user.as_ref();
                // Skip the email sub-line when the resolved name already IS
                // the email (the name-less Apple-ID case), so it never shows
                // twice.
                let email = user.and_then(|user| user.email.clone()).filter(|email| *email != name);
                let placeholder = placeholders.get(&member.user_id).copied();
                // Web: sole remaining owner gets no self-actions.
                let menu = (i_am_owner || is_self) && !(is_self && is_owner_row && owner_count <= 1);
                let items = if menu { menu_items(is_self, is_owner_row, i_am_owner, placeholder) } else { Vec::new() };
                if menu {
                    menus.insert(member.id.clone(), MenuRow {
                        member_id: member.id.clone(),
                        team_id: team_id.clone(),
                        name: name.clone(),
                        is_self,
                        is_owner_row,
                        i_am_owner,
                        // EXP-630: a placeholder member gets "Resend invite" —
                        // the dialog needs the team plus the row's own
                        // prefill. EXP-1076: the status picks the verb.
                        resend: placeholder.map(|status| ResendContext {
                            team_id: team_id.clone(),
                            display_name: name.clone(),
                            status,
                            target: ResendTarget {
                                user_id: member.user_id.clone(),
                                name: user.and_then(|user| user.name.clone()).unwrap_or_default(),
                                email: user.and_then(|user| user.email.clone()).unwrap_or_default(),
                            },
                        }),
                    });
                }
                // EXP-547: picture + initials fallback; EXP-698: the hue is
                // keyed on the USER ID.
                let picture = user.and_then(|user| user.image.clone()).filter(|url| picture_src(url));
                json!({
                    "key": member.id,
                    "name": name,
                    "you": is_self,
                    "role": role,
                    "roleIcon": if is_owner_row { "ui-owner" } else { "ui-member" },
                    "hasPlaceholder": placeholder.is_some(),
                    "placeholder": placeholder.map(PlaceholderStatus::label).unwrap_or_default(),
                    "hasEmail": email.is_some(),
                    "email": email.unwrap_or_default(),
                    "initials": crate::user_avatar::initials(&name),
                    "hue": crate::user_avatar::avatar_hue_index(&member.user_id),
                    "hasPicture": picture.is_some(),
                    "picture": picture.unwrap_or_default(),
                    "seed": member.user_id,
                    "menu": menu,
                    "menuLabel": format!("Member actions for {name}"),
                    "items": items,
                })
            })
            .collect();
        *self.menus.borrow_mut() = menus;

        // EXP-630: PENDING = unaccepted AND unexpired. An expired placeholder
        // invite is a badge on its member row above, not a live link in this
        // list.
        let pending: Vec<Value> = invites
            .iter()
            .filter(|invite| invite_is_pending(invite, now_ms))
            .map(|invite| {
                let role = invite.role.clone().unwrap_or_else(|| "member".to_string());
                let expires = invite
                    .expires_at
                    .as_deref()
                    .map(|at| format!("Expires {}", format_short_date(at)))
                    .unwrap_or_else(|| "No expiry".to_string());
                json!({
                    "key": invite.id,
                    // A leading muted mail glyph (link glyph for a link
                    // invite) · the address or "Link invite" · the role pill
                    // · "Expires Mon D".
                    "kindIcon": if invite.email.is_some() { "ui-mail" } else { "ui-link" },
                    "hasEmail": invite.email.is_some(),
                    "email": invite.email.clone().unwrap_or_default(),
                    "roleIcon": if role == TEAM_ROLE_OWNER { "ui-owner" } else { "ui-member" },
                    "role": role,
                    "expires": expires,
                })
            })
            .collect();

        let email_empty = self.email.trim().is_empty();
        let busy = self.generating || self.form_busy();
        json!({
            "noTeam": false,
            "owner": i_am_owner,
            "rows": rows,
            "form": {
                "name": self.name,
                "email": self.email,
                // The send button's faces: disabled while the address is
                // empty or an add runs; pending while it sends (the word
                // after `PENDING_DELAY`).
                "sendDisabled": email_empty || self.adding,
                "sending": self.sending,
                "sendShown": self.pending.shown(),
                "sendLabel": if self.pending.shown() { crate::feedback::PENDING_INVITE } else { "Send invite" },
                "addDisabled": email_empty || self.sending,
                "adding": self.adding,
                "hasError": self.email_error.is_some(),
                "error": self.email_error.clone().unwrap_or_default(),
                "outOfSeats": self.form_out_of_seats,
                "hasLink": self.form_url.is_some(),
                "link": self.form_url.clone().unwrap_or_default(),
                "copyIcon": copy_icon(self.copied == Some(LinkSlot::Form)),
            },
            "outOfSeats": self.out_of_seats,
            "hasLink": self.invite_url.is_some(),
            "link": self.invite_url.clone().unwrap_or_default(),
            "copyIcon": copy_icon(self.copied == Some(LinkSlot::Pane)),
            "generating": self.generating,
            "busy": busy,
            "hasPending": !pending.is_empty(),
            "pending": pending,
        })
    }
}

/// The copy button's glyph (gpui-component `Clipboard`: the check while
/// copied).
fn copy_icon(copied: bool) -> &'static str {
    if copied {
        CHECK_ICON
    } else {
        COPY_ICON
    }
}

/// gpui-component's own Copy / Check glyphs, by name (the host resolves
/// them to their asset paths).
const COPY_ICON: &str = "ide-copy";
const CHECK_ICON: &str = "ide-check";

/// A profile picture's src: an absolute http(s) URL (Better Auth `image`).
fn picture_src(url: &str) -> bool {
    let url = url.trim();
    url.starts_with("https://") || url.starts_with("http://")
}

/// What the row's "Resend invite" needs to open its dialog (EXP-630) —
/// `None` on a member who has joined.
#[derive(Clone, Debug)]
struct ResendContext {
    team_id: String,
    /// The row's resolved label — it leads the dialog's description.
    display_name: String,
    /// Which invite state the row is in — "Send invite" (never invited) or
    /// "Resend invite", in the menu AND the dialog it opens.
    status: PlaceholderStatus,
    target: ResendTarget,
}

/// What a row's ⋯ pick needs.
#[derive(Clone)]
struct MenuRow {
    member_id: String,
    team_id: String,
    name: String,
    is_self: bool,
    is_owner_row: bool,
    i_am_owner: bool,
    resend: Option<ResendContext>,
}

/// The pane's host: the IDE's plugin, gpui-component's copy glyphs and the
/// members' pictures.
struct MembersHost {
    ide: crate::exponential_ui_host::IdePlugin,
}

impl HostPlugin for MembersHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        match name {
            // The registry's copy / check (the same Lucide paths gpui-component's
            // `Clipboard` draws).
            COPY_ICON => Some(registry::UI_COPY.path()),
            CHECK_ICON => Some(registry::UI_CHECK.path()),
            _ => self.ide.icon(name),
        }
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.ide.on_unknown(node)
    }

    /// `users.image` is an absolute third-party URL (Google/GitHub), fetched
    /// without the instance's headers.
    fn media_allowed(&self, src: &str) -> bool {
        picture_src(src) || self.ide.media_allowed(src)
    }
}

/// Web `DropdownMenu` per member row: Resend invite / Send invite (owner, not
/// self, still a placeholder), the applicable role change (owner, not self),
/// then Leave team (self) / Remove member (owner).
fn menu_items(is_self: bool, is_owner_row: bool, i_am_owner: bool, placeholder: Option<PlaceholderStatus>) -> Vec<Value> {
    let mut items = Vec::new();
    // EXP-630: first item — the invite is the thing that is unfinished
    // about this row. EXP-1076: the status picks the verb.
    if let Some(status) = placeholder.filter(|_| i_am_owner && !is_self) {
        items.push(json!({"label": status.invite_verb(), "value": "resend", "icon": "ui-mail"}));
    }
    if i_am_owner && !is_self {
        // Only the applicable role change renders — demoting another owner
        // is always safe (I stay an owner). The no-op variant is HIDDEN,
        // not a disabled dead item (EXP-228).
        let (label, icon) = if is_owner_row { ("Make member", "ui-member") } else { ("Make owner", "ui-owner") };
        items.push(json!({"label": label, "value": "role", "icon": icon}));
    }
    if is_self || i_am_owner {
        // EXP-687: leaving is a sign-out, removing someone is a user-minus —
        // the web draws the same two concepts.
        let (label, icon) = if is_self { ("Leave team", "nav-sign-out") } else { ("Remove member", "ui-remove-member") };
        items.push(json!({"label": label, "value": "remove", "icon": icon}));
    }
    items
}

/// The remove/leave confirm: the prompts fixture's `remove-member` /
/// `leave-team` (EXP-1230). Losing access to a team is immediate and cannot
/// be undone, and the item sits right under the role toggles, so it asks
/// first, like every other destructive settings action.
fn open_remove_member_dialog(
    member_id: String,
    team_id: &str,
    name: &str,
    is_self: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let spec = if is_self {
        let team_name = Store::global(cx)
            .collections()
            .teams
            .read(cx)
            .get(team_id)
            .map(|team| team.name.clone())
            .unwrap_or_default();
        AlertSpec::from_prompt("Leave team", &domain::prompts::leave_team(&team_name))
    } else {
        AlertSpec::from_prompt("Remove member", &domain::prompts::remove_member(name))
    }
    .on_ok(move |_, cx| {
        let member_id = member_id.clone();
        spawn_trpc(cx, "teamMembers.remove", move |trpc| {
            api::teams::team_members_remove(trpc, &member_id)
        });
        true
    });
    // dialog stays native: a destructive confirm is the OS alert (prompts fixture).
    native_dialog::open_alert(window, cx, spec);
}

/// The role-change confirm: `make-owner` / `make-member` (EXP-1230).
fn open_role_dialog(
    member_id: String,
    name: &str,
    role: api::teams::TeamRole,
    window: &mut Window,
    cx: &mut App,
) {
    let spec = match role {
        api::teams::TeamRole::Owner => {
            AlertSpec::from_prompt("Make owner", &domain::prompts::make_owner(name))
        }
        api::teams::TeamRole::Member => {
            AlertSpec::from_prompt("Make member", &domain::prompts::make_member(name))
        }
    }
    .on_ok(move |_, cx| {
        let member_id = member_id.clone();
        spawn_trpc(cx, "teamMembers.updateRole", move |trpc| {
            api::teams::team_members_update_role(trpc, &member_id, role)
        });
        true
    });
    // dialog stays native: a role change confirms in the OS alert (prompts fixture).
    native_dialog::open_alert(window, cx, spec);
}

impl Render for MembersPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "MembersPane").entered();
        // The bench: an open popup's enter fade run out by hand (the
        // headless clock stands still).
        #[cfg(test)]
        if self.shot.is_some() && !self.view.read(cx).layers().is_empty() {
            self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
        }
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// Join order (web `useTeamUsers`): the member row's `created_at`, then its
/// id — never the name, so the roster reads the same on every client.
fn sort_join_order(rows: &mut [MemberRow]) {
    rows.sort_by(|a, b| join_order_key(&a.member).cmp(&join_order_key(&b.member)));
}

fn join_order_key(member: &TeamMember) -> (&str, &str) {
    (member.created_at.as_deref().unwrap_or(""), member.id.as_str())
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

// ---------------------------------------------------------------------------
// The document
// ---------------------------------------------------------------------------

/// The colours the document needs beyond [`Ink`].
struct Tints {
    /// The theme radius (the text buttons, the notice, the link box).
    radius: f32,
    danger: String,
    /// The link box's hairline (`row_stroke`).
    stroke: String,
    /// The seat notice's 40% primary stroke over its 5% primary wash.
    notice_stroke: String,
    notice_wash: String,
    /// gpui-component's outline `Button`: the input wash and stroke, the
    /// button ink; disabled = half of each, the half-muted ink.
    outline_fill: String,
    outline_stroke: String,
    outline_fg: String,
    outline_hover: String,
    outline_fill_dim: String,
    outline_stroke_dim: String,
    /// A ghost `Button`'s ink and hover wash (half the accent).
    ghost_fg: String,
    /// The ghost ⋯'s hover wash.
    list_hover: String,
    ghost_hover: String,
    /// The contract avatar hues: the 20% fill and the solid initials.
    hues: Vec<(String, String)>,
}

impl Tints {
    fn of(cx: &App) -> Self {
        use super::api_keys::premultiplied_hex;
        let theme = cx.theme();
        Self {
            radius: f32::from(theme.radius),
            danger: hex(theme.danger),
            stroke: hex(super::row_stroke(cx)),
            notice_stroke: premultiplied_hex(theme.primary.opacity(0.4)),
            notice_wash: premultiplied_hex(theme.primary.opacity(0.05)),
            outline_fill: hex(theme.input_background()),
            outline_stroke: hex(theme.input),
            outline_fg: hex(theme.button_foreground),
            outline_hover: premultiplied_hex(theme.input.mix_oklab(theme.transparent, 0.5)),
            outline_fill_dim: premultiplied_hex(theme.input_background().opacity(0.5)),
            outline_stroke_dim: hex(theme.input.opacity(0.5)),
            ghost_fg: hex(theme.secondary_foreground),
            list_hover: hex(theme.list_hover),
            ghost_hover: premultiplied_hex(theme.accent.opacity(0.5)),
            hues: theme::tokens::avatar::HUES
                .iter()
                .map(|hue| {
                    let hue = hue.to_hsla();
                    (hex(hue.opacity(0.2)), hex(hue))
                })
                .collect(),
        }
    }
}

/// A `Text` of `text` cut to one line.
fn line(ink: &Ink, id: &str, text: Value, rems: f32, color: &str) -> Value {
    let mut t = ink.text(id, text, rems, color);
    t["props"]["lines"] = json!(1);
    t["style"]["flexShrink"] = json!(0);
    t
}

/// The role pill (`glass_pill` Sm Readonly with the role glyph leading at
/// `PillSize::Sm.glyph()`); `muted` = the placeholder pill, all in the muted
/// ink.
fn glyph_pill(ink: &Ink, id: &str, icon: Value, label: Value, muted: bool) -> Value {
    let mut pill = ink.pill(id, label, false, None);
    let (color, opacity) = if muted { (ink.muted.as_str(), 1.0) } else { (ink.foreground.as_str(), VALUE_ALPHA) };
    if muted {
        pill["children"][0]["style"]["color"] = json!(ink.muted);
        pill["children"][0]["style"]["opacity"] = json!(1.0);
    }
    let glyph = eui_gpui::controls::PillSize::Sm.glyph();
    pill["children"].as_array_mut().expect("the pill's children").insert(0, json!({"id": format!("{id}_icon"), "component": "Icon", "props": {"name": icon},
        "style": {"width": glyph, "height": glyph, "color": color, "opacity": opacity, "flexShrink": 0}}));
    pill
}

/// The roster avatar (`user_avatar` at `Size::Small`): the picture where
/// there is one (genUI `Avatar` sm), else the initials (`text_xs`) on the
/// row's contract hue at 20%: eight discs, `hue` picks.
fn avatar(ink: &Ink, tints: &Tints) -> Value {
    let size = 24.;
    let mut children: Vec<Value> = tints
        .hues
        .iter()
        .enumerate()
        .map(|(ix, (fill, solid))| {
            let mut initials = ink.text(&format!("row_initials_{ix}"), path("initials"), XS, solid);
            initials["style"]["flexShrink"] = json!(0);
            json!({"id": format!("row_disc_{ix}"), "component": "Box", "visible": and(not(path("hasPicture")), super::doc::eq(path("hue"), json!(ix))),
                "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "width": size, "height": size, "flexShrink": 0, "borderRadius": 9999, "overflow": "hidden", "backgroundColor": fill},
                "children": [initials]})
        })
        .collect();
    children.push(json!({"id": "row_picture", "component": "Avatar", "visible": path("hasPicture"), "props": {"name": path("name"), "src": path("picture"), "seed": path("seed"), "size": "sm"}, "style": {"flexShrink": 0}}));
    json!({"id": "row_avatar", "component": "Box", "style": {"display": "flex", "flexShrink": 0}, "children": children})
}

/// One member row (`flat_row`, `px_3` / `py_2`): the avatar and the
/// identity (`gap_3`) — the name with its muted "(you)", the role pill and
/// the placeholder pill over the `text_xs` muted email — then the ⋯.
fn member_row(ink: &Ink, tints: &Tints) -> Value {
    let mut name = line(ink, "row_name", path("name"), SM, &ink.foreground);
    name["style"]["fontWeight"] = json!(500);
    let mut you = line(ink, "row_you", json!("(you)"), SM, &ink.muted);
    you["visible"] = path("you");
    let mut placeholder = glyph_pill(ink, "row_placeholder", json!("ui-mail"), path("placeholder"), true);
    placeholder["visible"] = path("hasPlaceholder");
    let mut email = line(ink, "row_email", path("email"), XS, &ink.muted);
    email["visible"] = path("hasEmail");
    // EXP-862: a row's "..." is a GHOST glyph, never a circle.
    let menu = ink.row_menu("member_menu", path("menuLabel"), path("menu"), &tints.list_hover, None, None);
    json!({"id": "row", "component": "Box",
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md},
        "children": [
            {"id": "row_lead", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.75)},
                "children": [
                    avatar(ink, tints),
                    {"id": "row_identity", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.125)},
                        "children": [
                            {"id": "row_head", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.75)},
                                "children": [
                                    {"id": "row_names", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.25)}, "children": [name, you]},
                                    glyph_pill(ink, "row_role", path("roleIcon"), path("role"), false),
                                    placeholder,
                                ]},
                            email,
                        ]},
                ]},
            {"id": "row_spacer", "component": "Box", "style": {"flexGrow": 1}},
            menu,
        ]})
}

/// One pending invite (`flat_row`, `px_3` / `py_2`): the 16 px muted kind
/// glyph, the address (`text_sm` MEDIUM) or the muted "Link invite", the
/// role pill and the `text_xs` muted expiry (`gap_3`), then the ghost
/// revoke (`web_icon_xs`: a 24 px circle, the `size_3` trash).
fn pending_row(ink: &Ink, tints: &Tints) -> Value {
    let mut email = line(ink, "invite_email", path("email"), SM, &ink.foreground);
    email["style"]["fontWeight"] = json!(500);
    email["visible"] = path("hasEmail");
    let mut link = line(ink, "invite_link", json!("Link invite"), SM, &ink.muted);
    link["visible"] = not(path("hasEmail"));
    let glyph = ink.r(XS);
    let revoke = json!({"id": "invite_revoke", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": "Revoke invite"}, "on": {"press": {"event": {"name": "revoke", "context": {"key": path("key")}}}},
        "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "width": 24, "height": 24, "borderRadius": 9999, "cursor": "pointer", ":hover": {"backgroundColor": tints.ghost_hover}},
        "children": [{"id": "invite_revoke_icon", "component": "Icon", "props": {"name": "ui-delete"}, "style": {"width": glyph, "height": glyph, "color": tints.ghost_fg, "flexShrink": 0}}]});
    json!({"id": "invite", "component": "Box",
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md},
        "children": [
            {"id": "invite_lead", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.75)},
                "children": [
                    {"id": "invite_kind", "component": "Icon", "props": {"name": path("kindIcon")}, "style": {"width": 16, "height": 16, "color": ink.muted, "flexShrink": 0}},
                    email,
                    link,
                    glyph_pill(ink, "invite_role", path("roleIcon"), path("role"), false),
                    line(ink, "invite_expires", path("expires"), XS, &ink.muted),
                ]},
            {"id": "invite_spacer", "component": "Box", "style": {"flexGrow": 1}},
            revoke,
        ]})
}

/// How a [`text_button`] paints: gpui-component's primary or outline
/// `Button`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ButtonPaint {
    Primary,
    Outline,
}

/// A gpui-component `Button` at `web_sm` (32 px, `px 12`, the theme
/// radius) with its `size_3p5` glyph `gap_1` before the `text_sm` label
/// (line height 1, which gpui centres half a point lower: `paddingTop: 1`).
/// Three faces, the flags pick: idle (pressable, the hover fill), busy
/// (`loading` and enabled: the whole button at 80%, the glyph slot the
/// spinner where `spin`, else blank) and disabled (the variant's disabled
/// paint; a loading one keeps its spinner). `min_label` holds the button
/// at least as wide as its idle face with that label (`feedback::Pending`'s
/// idle width).
#[allow(clippy::too_many_arguments)]
fn text_button(ink: &Ink, tints: &Tints, id: &str, paint: ButtonPaint, icon: &str, label: Value, disabled: Value, loading: Value, spin: Value, event: Value, min_label: Option<&str>) -> Value {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Face {
        Idle,
        Busy,
        Dim,
        Ghost,
    }
    let primary = paint == ButtonPaint::Primary;
    let size = ink.r(SM);
    let glyph = ink.r(0.875);
    let face = |face: Face| {
        let id = match face {
            Face::Idle => id.to_string(),
            Face::Busy => format!("{id}_busy"),
            Face::Dim => format!("{id}_dim"),
            Face::Ghost => format!("{id}_ghost"),
        };
        let dim = face == Face::Dim;
        let (fg, alpha) = match (primary, dim) {
            (true, _) => (ink.primary_button_fg.as_str(), 1.0),
            (false, true) => (ink.muted.as_str(), 0.5),
            (false, false) => (tints.outline_fg.as_str(), 1.0),
        };
        let mut slot = vec![json!({"id": format!("{id}_icon"), "component": "Icon", "props": {"name": icon}, "style": {"width": glyph, "height": glyph, "color": fg, "opacity": alpha, "flexShrink": 0}})];
        if face != Face::Ghost {
            slot[0]["visible"] = not(loading.clone());
            slot.push(json!({"id": format!("{id}_spin"), "component": "Spinner", "visible": and(loading.clone(), spin.clone()), "style": {"width": glyph, "height": glyph, "color": fg, "opacity": alpha, "flexShrink": 0}}));
            slot.push(json!({"id": format!("{id}_blank"), "component": "Box", "visible": and(loading.clone(), not(spin.clone())), "style": {"width": glyph, "height": glyph, "flexShrink": 0}}));
        }
        let text = match face {
            Face::Ghost => json!(min_label.unwrap_or_default()),
            _ => label.clone(),
        };
        let text = json!({"id": format!("{id}_text"), "component": "Text", "props": {"text": text, "lines": 1}, "style": {"fontSize": size, "lineHeight": size * 1.25, "color": fg, "opacity": alpha, "flexShrink": 0}});
        let (fill, stroke) = match (primary, dim) {
            (true, false) => (ink.primary_button_fill.as_str(), None),
            (true, true) => ("#00000000", None),
            (false, false) => (tints.outline_fill.as_str(), Some(tints.outline_stroke.as_str())),
            (false, true) => (tints.outline_fill_dim.as_str(), Some(tints.outline_stroke_dim.as_str())),
        };
        let visible = match face {
            Face::Idle => and(not(disabled.clone()), not(loading.clone())),
            Face::Busy => and(not(disabled.clone()), loading.clone()),
            Face::Dim => disabled.clone(),
            Face::Ghost => json!(true),
        };
        let mut button = json!({"id": id, "component": "Box", "visible": visible,
            "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "gap": ink.r(0.25), "height": 32, "paddingHorizontal": 12, "borderRadius": tints.radius, "backgroundColor": fill},
            "children": slot.into_iter().chain([text]).collect::<Vec<_>>()});
        if let Some(stroke) = stroke {
            button["style"]["borderWidth"] = json!(1);
            button["style"]["borderColor"] = json!(stroke);
        }
        if primary && dim {
            // The half fill as a layer at half opacity: a `#rrggbbaa` alpha
            // byte can't say 0.5.
            button["children"].as_array_mut().expect("the button's children").insert(0, json!({"id": format!("{id}_fill"), "component": "Box",
                "style": {"position": "absolute", "top": 0, "left": 0, "right": 0, "bottom": 0, "borderRadius": tints.radius, "backgroundColor": ink.primary_button_fill, "opacity": 0.5}}));
        }
        match face {
            Face::Idle => {
                button["props"] = json!({"pressable": true});
                button["on"] = json!({"press": {"event": event.clone()}});
                button["style"]["cursor"] = json!("pointer");
                button["style"][":hover"] = json!({"backgroundColor": if primary { &ink.primary_button_hover } else { &tints.outline_hover }});
            }
            Face::Busy => button["style"]["opacity"] = json!(0.8),
            Face::Ghost => {
                button["style"]["height"] = json!(0);
                button["style"]["overflow"] = json!("hidden");
                button["style"]["opacity"] = json!(0);
                button["style"]["borderWidth"] = json!(0);
            }
            Face::Dim => {}
        }
        button
    };
    let mut faces = Vec::new();
    if min_label.is_some() {
        faces.push(face(Face::Ghost));
    }
    faces.extend([face(Face::Idle), face(Face::Busy), face(Face::Dim)]);
    json!({"id": format!("{id}_slot"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexShrink": 0}, "children": faces})
}

/// `invite_form::out_of_seats_notice`: the web's seat-cap `UpgradeDialog`,
/// inline (`px_3` / `py_2`, `gap_1`, the theme radius in a 40% primary
/// stroke over a 5% primary wash).
fn seats_notice(ink: &Ink, tints: &Tints, id: &str, visible: Value) -> Value {
    let mut title = ink.text(&format!("{id}_title"), json!(OUT_OF_SEATS_TITLE), SM, &ink.foreground);
    title["style"]["fontWeight"] = json!(500);
    json!({"id": id, "component": "Box", "visible": visible,
        "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.25), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": tints.radius, "borderWidth": 1, "borderColor": tints.notice_stroke, "backgroundColor": tints.notice_wash},
        "children": [
            title,
            ink.text(&format!("{id}_message"), json!(OUT_OF_SEATS_MESSAGE), SM, &ink.foreground),
            ink.text(&format!("{id}_note"), json!(OUT_OF_SEATS_NOTE), XS, &ink.muted),
        ]})
}

/// `invite_form::invite_link_row`: the minted link, read-only (`px_2` /
/// `py_1`, the theme radius, the row hairline, `text_xs` mono, one line),
/// with gpui-component's `Clipboard` (a ghost `XSmall` icon button: 20
/// rems-quarter square, the theme radius, the `size_3` glyph) and its
/// "Copy invite URL" tooltip; the copy is the `copy` action.
fn link_row(ink: &Ink, tints: &Tints, id: &str, base: &str, link: &str, visible: Value) -> Value {
    let mut url = ink.text(&format!("{id}_url"), path(&format!("{base}/link")), XS, &ink.foreground);
    url["props"]["lines"] = json!(1);
    url["style"]["fontFamily"] = json!(ink.mono);
    url["style"]["minWidth"] = json!(0);
    url["style"]["flexShrink"] = json!(1);
    let side = ink.r(1.25);
    let glyph = ink.r(XS);
    let copy = json!({"id": format!("{id}_copy"), "component": "Box", "props": {"pressable": true}, "accessibility": {"label": "Copy"}, "on": {"press": {"event": {"name": "copy", "context": {"link": link}}}},
        "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "width": side, "height": side, "borderRadius": tints.radius, "cursor": "pointer", ":hover": {"backgroundColor": tints.ghost_hover}},
        "children": [{"id": format!("{id}_copy_icon"), "component": "Icon", "props": {"name": path(&format!("{base}/copyIcon"))}, "style": {"width": glyph, "height": glyph, "color": tints.ghost_fg, "flexShrink": 0}}]});
    json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [
            {"id": format!("{id}_box"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0, "paddingHorizontal": ink.r(0.5), "paddingVertical": ink.r(0.25), "borderRadius": tints.radius, "borderWidth": 1, "borderColor": tints.stroke}, "children": [url]},
            {"id": format!("{id}_tip"), "component": "Tooltip", "props": {"content": "Copy invite URL"}, "children": [copy]},
        ]})
}

/// The invite form (web `InviteMemberForm`, `layout="row"`): the 176 px
/// Name and the address (genUI `box` Inputs at `sm`, the IDE's
/// `glass_input(..).web_input_sm()`) beside "Send invite" and "Add without
/// inviting" (`gap_2`), the refused address under them, then the form's
/// own seat notice and fallback link.
fn form(ink: &Ink, tints: &Tints) -> Value {
    let input = |id: &str, label: &str, placeholder: &str, style: Value, submit: bool| {
        let mut node = json!({"id": id, "component": "Input", "props": {"label": label, "name": id, "variant": "box", "size": "sm", "placeholder": placeholder, "value": path(&format!("/pane/form/{id}"))},
            "style": style, "on": {"change": {"event": {"name": "edit", "context": {"field": id}}}}});
        if submit {
            node["on"]["submit"] = json!({"event": {"name": "send"}});
        }
        node
    };
    let send = text_button(ink, tints, "send", ButtonPaint::Primary, "ui-mail", path("/pane/form/sendLabel"), path("/pane/form/sendDisabled"), path("/pane/form/sending"), path("/pane/form/sendShown"), json!({"name": "send"}), Some("Send invite"));
    let add = text_button(ink, tints, "add", ButtonPaint::Outline, "ui-invite", json!("Add without inviting"), path("/pane/form/addDisabled"), path("/pane/form/adding"), path("/pane/form/adding"), json!({"name": "add"}), None);
    let error = json!({"id": "form_error", "component": "Text", "visible": path("/pane/form/hasError"), "props": {"text": path("/pane/form/error")},
        "style": {"fontSize": crate::action_error::FIELD_ERROR_PX, "lineHeight": 20, "color": tints.danger}});
    json!({"id": "form", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [
            {"id": "form_row", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
                "children": [
                    input("name", "Name", "Name", json!({"width": 176, "flexShrink": 0}), false),
                    input("email", "Email", "teammate@example.com", json!({"flexGrow": 1, "flexShrink": 1, "minWidth": 0}), true),
                    {"id": "form_buttons", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5), "flexShrink": 0}, "children": [send, add]},
                ]},
            error,
            seats_notice(ink, tints, "form_seats", path("/pane/form/outOfSeats")),
            link_row(ink, tints, "form_link", "/pane/form", "form", path("/pane/form/hasLink")),
        ]})
}

/// The pane's tree: the Members band over the roster; for an owner the
/// invite section (`gap_3`, `pt_3`): its band, the form, the link path's
/// notice and link, "Generate invite link", the pending invites under their
/// band. No team = its empty state alone.
fn document(ink: &Ink, tints: &Tints) -> Value {
    let list = |id: &str, template: &str, data: &str, row: Value| json!({"id": id, "component": "List", "props": {"gap": "none"},
        "template": {"component": template, "path": data, "key": "key"},
        "children": [{"id": template, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [row]}]});
    let generate = text_button(ink, tints, "generate", ButtonPaint::Outline, "ui-link", json!("Generate invite link"), path("/pane/busy"), path("/pane/generating"), path("/pane/generating"), json!({"name": "generate"}), None);
    let pending = json!({"id": "pending", "component": "Box", "visible": path("/pane/hasPending"), "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [ink.band("pending_band", "Pending invites", None), list("pending_rows", "pending_wrap", "/pane/pending", pending_row(ink, tints))]});
    let invite = json!({"id": "invite_section", "component": "Box", "visible": path("/pane/owner"), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.75), "paddingTop": ink.r(0.75), "width": "100%", "minWidth": 0},
        "children": [
            ink.band("invite_band", "Invite members", None),
            form(ink, tints),
            seats_notice(ink, tints, "seats", path("/pane/outOfSeats")),
            link_row(ink, tints, "link", "/pane", "pane", path("/pane/hasLink")),
            {"id": "generate_row", "component": "Box", "style": {"display": "flex", "flexDirection": "row"}, "children": [generate]},
            pending,
        ]});
    let body = json!({"id": "body", "component": "Box", "visible": not(path("/pane/noTeam")), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [
            ink.band("band", "Members", None),
            list("rows", "row_wrap", "/pane/rows", member_row(ink, tints)),
            invite,
        ]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [body, empty_state(ink, "no_team", "ui-team", crate::empty_state_copy::NO_TEAM_SELECTED, path("/pane/noTeam"))]})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, created_at: Option<&str>) -> MemberRow {
        MemberRow {
            member: TeamMember {
                id: id.to_string(),
                team_id: "t".to_string(),
                user_id: format!("u-{id}"),
                role: None,
                created_at: created_at.map(str::to_string),
                updated_at: None,
            },
            user: None,
        }
    }

    #[test]
    fn roster_is_join_order_then_id() {
        let mut rows = vec![
            member("b", Some("2026-03-01T00:00:00Z")),
            member("c", Some("2026-01-01T00:00:00Z")),
            member("a", Some("2026-03-01T00:00:00Z")),
        ];
        sort_join_order(&mut rows);
        let ids: Vec<&str> = rows.iter().map(|row| row.member.id.as_str()).collect();
        assert_eq!(ids, ["c", "a", "b"]);
    }
}
