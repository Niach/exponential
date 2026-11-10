use super::*;

#[test]
#[ignore]
fn settings_sessions_unlimited() {
    shoot_pane("sessions-unlimited", SessionsPane::for_shots, |pane, cx| pane.show(None, cx));
}

#[test]
#[ignore]
fn settings_sessions_30_days() {
    shoot_pane("sessions-30-days", SessionsPane::for_shots, |pane, cx| pane.show(Some(30), cx));
}
