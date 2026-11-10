use super::*;

fn prefs(transport: bool, email: bool, daily: bool) -> EmailPrefs {
    EmailPrefs {
        email_enabled: email,
        type_prefs: HashMap::from([(domain::contract::NOTIFICATION_TYPE_ISSUE_COMMENT.to_string(), false)]),
        digest: Some(if daily { "daily" } else { "off" }.into()),
        digest_hour: 8,
        allow_agent_messages: true,
        transport_configured: transport,
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
