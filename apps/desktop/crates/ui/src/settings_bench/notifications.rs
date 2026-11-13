use super::*;

fn prefs(transport: bool, email: bool, daily: bool) -> EmailPrefs {
    EmailPrefs {
        email_enabled: email,
        type_prefs: HashMap::from([(domain::contract::NOTIFICATION_TYPE_ISSUE_COMMENT.to_string(), false)]),
        digest: Some(if daily { "daily" } else { "off" }.into()),
        digest_hour: 8,
        allow_agent_messages: true,
        transport_configured: transport,
        source_prefs: None,
    }
}

fn shoot(name: &str, load: ShotLoad) {
    shoot_pane(name, NotificationsPrefsPane::for_shots, move |pane, cx| pane.show(load, cx));
}

#[test]
#[ignore]
fn settings_notifications_no_transport() {
    shoot("notifications-no-transport", ShotLoad::Ready(prefs(false, false, false)));
}

#[test]
#[ignore]
fn settings_notifications_daily() {
    shoot("notifications-daily", ShotLoad::Ready(prefs(true, true, true)));
}

#[test]
#[ignore]
fn settings_notifications_hourly_email_off() {
    shoot("notifications-email-off", ShotLoad::Ready(prefs(true, false, false)));
}

#[test]
#[ignore]
fn settings_notifications_failed() {
    shoot("notifications-failed", ShotLoad::Failed);
}

/// #EXP-1815: the source switches over two teams' agents (one switched off).
#[test]
#[ignore]
fn settings_notifications_sources() {
    use domain::notification_delivery::{SettingsAgent, SettingsTeam, SourcePrefs};
    let mut ready = prefs(true, true, false);
    ready.source_prefs = Some(SourcePrefs { temporary: Some(false), agents: Some([("a3".to_string(), false)].into_iter().collect()), ..Default::default() });
    let team = |id: &str, name: &str| SettingsTeam { id: id.into(), name: name.into() };
    let agent = |id: &str, team: &str, name: &str, role: &str| SettingsAgent { id: id.into(), team_id: team.into(), name: name.into(), role: role.into() };
    let teams = vec![team("t1", "Acme"), team("t2", "Zeta")];
    let agents = vec![agent("a1", "t1", "Assistant", "assistant"), agent("a2", "t1", "Coordinator", "coordinator"), agent("a3", "t1", "Triage", "agent"), agent("a4", "t2", "Builder", "agent")];
    shoot_pane("notifications-sources", NotificationsPrefsPane::for_shots, move |pane, cx| {
        pane.show(ShotLoad::Ready(ready), cx);
        pane.show_sources(teams, agents, cx);
    });
}

/// #EXP-1815: THE toast a new notification raises while the app is focused
/// on another screen (kind info, the row's title + body, `Open`).
struct NotificationToastShot;

impl Render for NotificationToastShot {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let toast = crate::toast::Toast::info("Triage replied in Fix the login redirect")
            .description("The redirect loops because the session cookie is set after the 302.")
            .action(domain::notification_delivery::copy::TOAST_ACTION, |_, _| {});
        div().w(px(crate::toast::constants::WIDTH)).child(crate::toast::card(&toast, cx))
    }
}

#[test]
#[ignore]
fn settings_notifications_toast() {
    shoot_pane("notifications-toast", |_, _| NotificationToastShot, |_, _| {});
}
