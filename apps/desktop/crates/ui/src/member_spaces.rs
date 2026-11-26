//! #EXP-2161 (`fixtures/member-space.json`, UI ×4; web
//! `components/devices/member-spaces.tsx`): members' Assistants on a shared
//! device. The owner: under each shared team's switch in Device settings ›
//! Sharing the `copy.optIn` row (disabled with the doctor's detail while it
//! cannot), turning on confirms (`copy.optIn*`), off at once. A team owner:
//! the same switch, off only, on a device page they do not own. Both: one
//! `copy.members` row per member with a space here, each with `copy.remove`
//! ([`MemberSpacesList`]).

use std::sync::Arc;
use std::time::Duration;

use gpui::{div, prelude::*, px, App, Context, Div, SharedString, Task, Window};
use gpui_component::{button::Button, button::ButtonVariants as _, h_flex, v_flex, ActiveTheme as _, Disableable as _};

use coding::device_doctor::{DeviceDoctor, DoctorState};
use domain::member_space::{may_opt_in, MEMBER_SPACE};

use crate::controls::WebControl as _;
use crate::native_dialog::{self, AlertSpec};
use crate::surface;

#[cfg(test)]
thread_local! {
    /// The bench's members (headless has no server).
    pub(crate) static SHOT_MEMBERS: std::cell::RefCell<Vec<api::member_spaces::SpaceMember>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// How often the open list is read again (the spaces are not synced to it).
const MEMBERS_INTERVAL: Duration = Duration::from_secs(30);

/// The opt-in row's state (`optInCases`, web `optInRow`): off is always
/// allowed; `detail` = why the device cannot (its doctor item's, else
/// `Not available here`).
pub(crate) fn opt_in_row(checked: bool, kind: &str, doctor: Option<&DeviceDoctor>) -> (bool, Option<String>) {
    let item = doctor.and_then(|doctor| doctor.items.iter().find(|item| item.key == MEMBER_SPACE.doctor_key));
    let state = item.map(|item| if item.state == DoctorState::Ok { "ok" } else { "missing" });
    let able = may_opt_in("deviceOwner", true, kind, true, state);
    let detail = (!able).then(|| item.and_then(|item| item.detail.clone()).filter(|detail| !detail.is_empty()).unwrap_or_else(|| coding::device_doctor::UNAVAILABLE.to_string()));
    (!able && !checked, detail)
}

/// The indented `copy.optIn` switch row under a shared team's switch.
pub(crate) fn opt_in_toggle(id: SharedString, checked: bool, disabled: bool, detail: Option<String>, on_click: impl Fn(&bool, &mut Window, &mut App) + 'static, cx: &App) -> Div {
    surface::glass_toggle_row(
        MEMBER_SPACE.copy.opt_in.clone(),
        detail.filter(|_| disabled).map(SharedString::from),
        crate::controls::web_switch(id).checked(checked).disabled(disabled).on_click(on_click).into_any_element(),
        cx,
    )
    .pl(px(32.))
}

/// Turning the opt-in on: the confirm (`copy.optInTitle/Body/Confirm`,
/// web `memberSpaceOptInPrompt`).
pub(crate) fn opt_in_alert(on_allow: impl Fn(&mut App) + 'static) -> AlertSpec {
    let copy = &MEMBER_SPACE.copy;
    AlertSpec::new(copy.opt_in_title.clone(), copy.opt_in_body.clone(), copy.opt_in_confirm.clone()).on_ok(move |_, cx| {
        on_allow(cx);
        true
    })
}

/// [`opt_in_alert`] over `window`.
pub(crate) fn confirm_opt_in(on_allow: impl Fn(&mut App) + 'static, window: &mut Window, cx: &mut App) {
    native_dialog::open_alert(window, cx, opt_in_alert(on_allow));
}

/// Picking a team device on Runs on: the confirm (`copy.pick*`, `{device}`
/// and `{owner}` filled as web `memberSpacePickPrompt`).
pub(crate) fn pick_alert(device: &str, owner: &str, on_pick: impl Fn(&mut App) + 'static) -> AlertSpec {
    let (title, body) = domain::member_space::pick_prompt(device, owner);
    AlertSpec::new(title, body, MEMBER_SPACE.copy.pick_confirm.clone()).on_ok(move |_, cx| {
        on_pick(cx);
        true
    })
}

/// `devices.setAssistantTeam` off the foreground; a refusal is the
/// `Couldn't update sharing` toast. `done` runs once it landed or failed.
pub(crate) fn set_assistant_team(device_id: String, team_id: String, allowed: bool, done: impl FnOnce(&mut App) + 'static, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        done(cx);
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx.background_executor().spawn(async move { api::devices::set_assistant_team(&trpc, &device_id, &team_id, allowed) }).await;
        let _ = cx.update(|cx| {
            if let Err(err) = result {
                crate::action_error::failed_action_err("update", "sharing", &err, cx);
            }
            done(cx);
        });
    })
    .detach();
}

/// A team owner switches members' Assistants off on a teammate's device
/// (`devices.dropAssistantTeam {id, teamId}` on the ROW id).
pub(crate) fn drop_assistant_team(row_id: String, team_id: String, done: impl FnOnce(&mut App) + 'static, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        done(cx);
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx.background_executor().spawn(async move { api::devices::drop_assistant_team(&trpc, &row_id, &team_id) }).await;
        let _ = cx.update(|cx| {
            if let Err(err) = result {
                crate::action_error::failed_action_err("update", "sharing", &err, cx);
            }
            done(cx);
        });
    })
    .detach();
}

/// `copy.members`: who runs their Assistant on one device, each with
/// `copy.remove` (`memberSpaces.forDevice`, read again every
/// [`MEMBERS_INTERVAL`] and whenever `version` moves).
pub(crate) struct MemberSpacesList {
    row_id: Option<String>,
    version: String,
    members: Vec<api::member_spaces::SpaceMember>,
    removing: Option<String>,
    poll: Option<Task<()>>,
}

impl MemberSpacesList {
    pub(crate) fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self { row_id: None, version: String::new(), members: Vec::new(), removing: None, poll: None }
    }

    /// The device row (its `devices` id) and anything that may end a space
    /// on it (`version`, e.g. its opted-in teams).
    pub(crate) fn set_device(&mut self, row_id: Option<String>, version: String, cx: &mut Context<Self>) {
        if self.row_id == row_id && self.version == version && self.poll.is_some() {
            return;
        }
        if self.row_id != row_id {
            self.members.clear();
        }
        self.row_id = row_id;
        self.version = version;
        self.poll = None;
        let Some(row_id) = self.row_id.clone() else { return };
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        let trpc = Arc::new(trpc);
        self.poll = Some(cx.spawn(async move |this, cx| loop {
            let answer = cx
                .background_executor()
                .spawn({
                    let (trpc, row_id) = (trpc.clone(), row_id.clone());
                    async move { api::member_spaces::for_device(&trpc, &row_id) }
                })
                .await;
            let alive = this
                .update(cx, |this, cx| match answer {
                    Ok(members) if this.row_id.as_deref() == Some(row_id.as_str()) => {
                        if this.members != members {
                            this.members = members;
                            cx.notify();
                        }
                    }
                    Ok(_) => {}
                    Err(err) => log::debug!("[ui] memberSpaces.forDevice: {err}"),
                })
                .is_ok();
            if !alive {
                return;
            }
            cx.background_executor().timer(MEMBERS_INTERVAL).await;
        }));
    }

    fn remove(&mut self, space_id: String, cx: &mut Context<Self>) {
        if self.removing.is_some() {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        self.removing = Some(space_id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { api::member_spaces::remove(&trpc, &space_id) }).await;
            let _ = this.update(cx, |this, cx| {
                this.removing = None;
                match result {
                    Ok(()) => {
                        // Read again at once.
                        this.poll = None;
                        let (row_id, version) = (this.row_id.clone(), this.version.clone());
                        this.set_device(row_id, version, cx);
                    }
                    Err(err) => crate::action_error::failed_action_err("remove", "the member", &err, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for MemberSpacesList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        if self.members.is_empty() {
            self.members = SHOT_MEMBERS.with(|members| members.borrow().clone());
        }
        if self.members.is_empty() {
            return div().into_any_element();
        }
        let foreground = cx.theme().foreground;
        let rows: Vec<Div> = self
            .members
            .iter()
            .map(|member| {
                let space_id = member.space_id.clone();
                surface::glass_row_shell().py_2().child(
                    h_flex()
                        .id(SharedString::from(format!("member-space-{}", member.space_id)))
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap_3()
                        .child(div().flex_1().min_w_0().truncate().text_sm().text_color(foreground).child(member.name.clone()))
                        .child(
                            Button::new(SharedString::from(format!("member-space-remove-{}", member.space_id)))
                                .ghost()
                                .web_sm()
                                .cursor_pointer()
                                .label(MEMBER_SPACE.copy.remove.clone())
                                .loading(self.removing.as_deref() == Some(member.space_id.as_str()))
                                .disabled(self.removing.is_some())
                                .on_click(cx.listener(move |this, _, _, cx| this.remove(space_id.clone(), cx))),
                        ),
                )
            })
            .collect();
        v_flex()
            .id("member-spaces")
            .w_full()
            .min_w_0()
            .child(surface::glass_section_header(MEMBER_SPACE.copy.members.clone(), None, cx))
            .child(surface::glass_group_rows(rows))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doctor(state: Option<&str>, detail: Option<&str>) -> DeviceDoctor {
        let items: Vec<serde_json::Value> = state
            .map(|state| serde_json::json!({"key": "member_space", "group": "storage", "state": state, "detail": detail}))
            .into_iter()
            .collect();
        serde_json::from_value(serde_json::json!({"checkedAt": "x", "items": items})).unwrap()
    }

    /// `optInCases` as the row reads them: a ready server switches; a desktop,
    /// no namespaces or no report disable it with the reason; off stays
    /// reachable while checked.
    #[test]
    fn the_opt_in_row_follows_the_doctor() {
        assert_eq!(opt_in_row(false, "server", Some(&doctor(Some("ok"), Some("Ready")))), (false, None));
        assert_eq!(opt_in_row(false, "server", Some(&doctor(Some("missing"), Some("No user namespaces")))), (true, Some("No user namespaces".into())));
        assert_eq!(opt_in_row(false, "server", Some(&doctor(None, None))), (true, Some(coding::device_doctor::UNAVAILABLE.into())));
        assert_eq!(opt_in_row(false, "server", None), (true, Some(coding::device_doctor::UNAVAILABLE.into())));
        assert_eq!(opt_in_row(false, "desktop", Some(&doctor(Some("ok"), None))).0, true);
        assert_eq!(opt_in_row(true, "server", Some(&doctor(Some("missing"), Some("gone")))).0, false, "off is always allowed");
    }
}
