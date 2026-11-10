use super::*;

#[test]
#[ignore]
fn settings_about() {
    shoot_pane("about", AboutPane::for_shots, |_, _| {});
}

#[test]
#[ignore]
fn settings_about_end() {
    shoot_pane("about-end", AboutPane::for_shots, |pane, cx| pane.show_end(cx));
}

/// The thumb faded out (gpui-component: 2 s idle, then a 1 s fade).
#[test]
#[ignore]
fn settings_about_end_idle() {
    shoot_pane_settled("about-end-idle", AboutPane::for_shots, |pane, cx| pane.show_end(cx), Some(std::time::Duration::from_millis(3500)));
}
