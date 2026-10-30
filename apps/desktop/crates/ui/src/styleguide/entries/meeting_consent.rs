//! NEW-57: `meeting-consent` (3 Special components): THE consent dialog
//! Meeting mode opens before a meeting starts (`fixtures/meeting.json`):
//! Settings → Voice's Meeting mode row and the tray's check item never
//! start one by themselves. The IDE's dialogs are native alert windows, so
//! the demo draws the alert's REAL view (`native_dialog::alert_specimen`)
//! over the SAME spec builder the product opens
//! (`meeting_mode::consent_alert`): "You, Zoom and Slack" to Ops, kept 7
//! days on the server. Start = `voice.startMeeting {consent: true}`.

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use etalk::meeting::{who, AudioApp, DesktopAudio, DesktopAudioCapability};

pub(crate) const ID: &str = "meeting-consent";
pub(crate) const OWNER: &str = "NEW-57";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let apps = [AudioApp { id: "z".into(), name: "Zoom".into() }, AudioApp { id: "s".into(), name: "Slack".into() }];
    let who = who(DesktopAudio::Include, &apps, DesktopAudioCapability::PerApp);
    let consent = crate::meeting_mode::consent_alert(&who, "Ops", |_, _| true);
    div().flex().flex_wrap().gap_4().child(crate::native_dialog::alert_specimen("sg-meeting-consent", consent, window, cx))
}
