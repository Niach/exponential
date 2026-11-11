//! #EXP-1629: Settings → Members in every state (an owner's roster with
//! owners, members, the three placeholder pills, a name-less email row and
//! a row with no synced user, plus the pending invites; a member's view;
//! the invite form typed, sending, adding, with its own notices; the link
//! path's seat-cap notice, generated link and generating; no team; a row's
//! ⋯ under the pointer).
use super::*;
use crate::settings::members::{MembersPane, MembersShot};
use serde_json::json;

fn members() -> Vec<domain::rows::TeamMember> {
    let m = |id: &str, user: &str, role: &str, at: &str| json!({"id": id, "team_id": "t", "user_id": user, "role": role, "created_at": at});
    serde_json::from_value(json!([
        m("m1", "u-me", "owner", "2026-01-01T00:00:00Z"),
        m("m2", "u-fox", "owner", "2026-01-02T00:00:00Z"),
        m("m3", "u-sam", "member", "2026-01-03T00:00:00Z"),
        m("m4", "u-ada", "member", "2026-11-01T00:00:00Z"),
        m("m5", "u-bob", "member", "2026-01-04T00:00:00Z"),
        m("m6", "u-cy", "member", "2026-02-01T00:00:00Z"),
        m("m7", "u-apple", "member", "2026-03-01T00:00:00Z"),
        m("m8", "u-gone-7f3a", "member", "2026-04-01T00:00:00Z"),
    ]))
    .expect("members")
}

fn users() -> Vec<domain::rows::User> {
    let u = |id: &str, name: &str, email: &str| json!({"id": id, "name": name, "email": email});
    serde_json::from_value(json!([
        u("u-me", "Dana Scully", "dana@acme.dev"),
        u("u-fox", "Fox Mulder", "fox@acme.dev"),
        u("u-sam", "Sam Rivera", "sam@acme.dev"),
        u("u-ada", "Ada Lovelace", "ada@example.com"),
        u("u-bob", "Bob Ross", "bob@example.com"),
        u("u-cy", "Cy Linear", "cy@example.com"),
        u("u-apple", "", "x7k2@privaterelay.appleid.com"),
    ]))
    .expect("users")
}

fn invites() -> Vec<domain::rows::TeamInvite> {
    serde_json::from_value(json!([
        {"id": "i1", "team_id": "t", "role": "member", "email": "ada@example.com", "placeholder_user_id": "u-ada", "sent_at": "2026-11-01T00:00:00Z", "expires_at": "2099-11-08T00:00:00Z", "created_at": "2026-11-01T00:00:00Z"},
        {"id": "i2", "team_id": "t", "role": "member", "email": "bob@example.com", "placeholder_user_id": "u-bob", "sent_at": "2026-01-04T00:00:00Z", "expires_at": "2026-01-11T00:00:00Z", "created_at": "2026-01-04T00:00:00Z"},
        {"id": "i3", "team_id": "t", "role": "member", "email": "cy@example.com", "placeholder_user_id": "u-cy", "expires_at": "2026-02-01T00:00:00Z", "created_at": "2026-02-01T00:00:00Z"},
        {"id": "i4", "team_id": "t", "role": "member", "sent_at": "2026-11-05T00:00:00Z", "expires_at": "2099-12-01T00:00:00Z", "created_at": "2026-11-05T00:00:00Z"},
        {"id": "i5", "team_id": "t", "role": "owner", "email": "eve@other.dev", "sent_at": "2026-11-03T00:00:00Z", "expires_at": "2099-11-20T00:00:00Z", "created_at": "2026-11-03T00:00:00Z"},
    ]))
    .expect("invites")
}

const LINK: &str = "https://exponential.at/invite/9f2c4e7a1b3d5f6e8a0c2e4b6d8f0a1c3e5b7d9f";

fn shot() -> MembersShot {
    MembersShot {
        team: Some("t".into()),
        owner: true,
        me: "u-me".into(),
        members: members(),
        users: users(),
        invites: invites(),
        name: String::new(),
        email: String::new(),
        sending: false,
        adding: false,
        form_out_of_seats: false,
        form_link: None,
        email_error: None,
        link: None,
        out_of_seats: false,
        generating: false,
    }
}

fn typed() -> MembersShot {
    MembersShot { name: "Grace Hopper".into(), email: "grace@example.com".into(), ..shot() }
}

fn members_shot(name: &str, shot: MembersShot) {
    members_hover(name, shot, None);
}

/// The pane, then the form filled; `hover` = the pointer moved there
/// (window points) after it.
fn members_hover(name: &str, shot: MembersShot, hover: Option<(f32, f32)>) {
    shoot_pane_acted(name, MembersPane::for_shots, move |pane, cx| pane.show(shot, cx), None, &[], move |pane, window, cx| {
        pane.shot_act(window, cx);
        if let Some((x, y)) = hover {
            // After the pane's update: the move repaints it.
            window.defer(cx, move |window, cx| {
                let moved = gpui::MouseMoveEvent { position: gpui::point(px(x), px(y)), pressed_button: None, modifiers: Default::default() };
                window.dispatch_event(gpui::PlatformInput::MouseMove(moved), cx);
            });
        }
    });
}

#[test]
#[ignore]
fn settings_members_owner() {
    members_shot("members-owner", shot());
}

#[test]
#[ignore]
fn settings_members_member() {
    members_shot("members-member", MembersShot { owner: false, me: "u-sam".into(), ..shot() });
}

#[test]
#[ignore]
fn settings_members_typed() {
    members_shot("members-typed", typed());
}

#[test]
#[ignore]
fn settings_members_sending() {
    members_shot("members-sending", MembersShot { sending: true, ..typed() });
}

#[test]
#[ignore]
fn settings_members_adding() {
    members_shot("members-adding", MembersShot { adding: true, ..typed() });
}

#[test]
#[ignore]
fn settings_members_out_of_seats() {
    members_shot("members-out-of-seats", MembersShot { out_of_seats: true, ..shot() });
}

#[test]
#[ignore]
fn settings_members_link() {
    members_shot("members-link", MembersShot { link: Some(LINK.into()), ..shot() });
}

#[test]
#[ignore]
fn settings_members_generating() {
    members_shot("members-generating", MembersShot { generating: true, ..typed() });
}

#[test]
#[ignore]
fn settings_members_form_notices() {
    members_shot("members-form-notices", MembersShot { form_out_of_seats: true, form_link: Some(LINK.into()), email_error: Some("Enter a full email address".into()), ..typed() });
}

#[test]
#[ignore]
fn settings_members_no_pending() {
    members_shot("members-no-pending", MembersShot { invites: Vec::new(), ..shot() });
}

#[test]
#[ignore]
fn settings_members_no_team() {
    members_shot("members-no-team", MembersShot { team: None, ..shot() });
}

/// The pointer over the second row's ⋯ (`EXP_MEMBERS_HOVER=x,y`
/// overrides [`MENU`]).
#[test]
#[ignore]
fn settings_members_menu_hover() {
    let point = std::env::var("EXP_MEMBERS_HOVER").ok().and_then(|v| v.split_once(',').and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)))).unwrap_or(MENU);
    members_hover("members-menu-hover", shot(), Some(point));
}

/// #EXP-1629: the second row's ⋯ open (the genUI Menu).
#[test]
#[ignore]
fn settings_members_menu_open() {
    let shot = shot();
    shoot_pane_acted("members-menu-open", MembersPane::for_shots, move |pane, cx| pane.show(shot, cx), None, &[], |pane, window, cx| pane.press_for_shots("member_menu_more.m2", window, cx));
}

/// The second row's ⋯ (window points), read off `members-owner`.
const MENU: (f32, f32) = (700., 140.);
