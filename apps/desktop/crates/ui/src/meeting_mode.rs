//! NEW-57: meeting mode's IDE surfaces (`fixtures/meeting.json`) beside the
//! host (`etalk_gpui::meeting`, which listens, flushes and grabs the pause
//! hotkey): THE consent dialog (Settings → Voice's toggle and the tray's
//! item open it; Start = `voice.startMeeting {consent: true}`), stopping at
//! once (`voice.stopMeeting`) and the voice device's desktop audio as its
//! doctor reports it (`doctor.desktopAudio`).

use gpui::{App, Window};

use etalk::meeting::{consent_body, AudioApp, DesktopAudioCapability, MEETING, RETENTION_DAYS};
use etalk::voice::{VoiceSettings, VOICE};

/// The voice device's desktop audio as its synced doctor reports it
/// (`doctor.desktopAudio`; absent = `none` and `[]`).
pub(crate) fn desktop_audio_of(settings: &VoiceSettings, cx: &App) -> (DesktopAudioCapability, Vec<AudioApp>) {
    let Some(device_id) = settings.voice_device_id.as_deref() else { return Default::default() };
    crate::talk_host::desktop_audio(device_id, cx)
}

/// THE consent dialog (`copy.consentTitle`, `consentBody`, Cancel / Start).
pub(crate) fn consent_alert(
    who: &str,
    agent: &str,
    on_start: impl Fn(&mut Window, &mut App) -> bool + 'static,
) -> crate::native_dialog::AlertSpec {
    let copy = &MEETING.copy;
    crate::native_dialog::AlertSpec::new(copy.consent_title.clone(), consent_body(who, agent, RETENTION_DAYS), copy.start.clone())
        .on_ok(on_start)
}

/// Open the consent dialog over `window` for the member's current settings.
pub(crate) fn open_consent(window: &mut Window, cx: &mut App) {
    let settings = etalk_gpui::speak_replies::settings(cx);
    let who = etalk_gpui::meeting::who_of(&settings, cx);
    let agent = etalk_gpui::voice_tray::agent_label(&settings, cx).unwrap_or_else(|| VOICE.copy.agent.clone());
    let spec = consent_alert(&who, &agent, |_, cx| {
        start(cx);
        true
    });
    crate::native_dialog::open_alert(window, cx, spec);
}

/// Settings → Voice's toggle turned on: the consent dialog over the shell
/// window (deferred: the click still holds the window).
pub(crate) fn prompt_consent(cx: &mut App) {
    cx.defer(|cx| {
        if let Some(window) = crate::os_notifications::shell_window(cx) {
            let _ = window.update(cx, |_, window, cx| open_consent(window, cx));
        }
    });
}

/// How long the tray waits for an IDE window to come up.
const WINDOW_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// The tray's Meeting mode checked: an IDE window on Settings → Voice with
/// the consent dialog open.
pub(crate) fn request_consent(cx: &mut App) {
    crate::os_notifications::open_settings(crate::settings::SettingsSection::Voice, cx);
    cx.spawn(async move |cx| {
        let started = std::time::Instant::now();
        while started.elapsed() < WINDOW_WAIT {
            let opened = cx.update(|cx| match crate::os_notifications::shell_window(cx) {
                Some(window) => window.update(cx, |_, window, cx| open_consent(window, cx)).is_ok(),
                None => false,
            });
            if opened {
                return;
            }
            cx.background_executor().timer(std::time::Duration::from_millis(100)).await;
        }
        log::warn!("[meeting] no IDE window came up for the consent dialog");
    })
    .detach();
}

/// Start = `voice.startMeeting {consent: true}`; the stored settings go to
/// the speaker (the host follows them).
pub(crate) fn start(cx: &mut App) {
    etalk_gpui::meeting::write(etalk_gpui::VoiceWrite::StartMeeting, cx);
}

/// Stop at once (`voice.stopMeeting`, no dialog). #FEED-163's automatic
/// stops (`{reason}`) are the meeting host's own (`etalk_gpui::meeting`).
pub(crate) fn stop(cx: &mut App) {
    etalk_gpui::meeting::write(etalk_gpui::VoiceWrite::StopMeeting(None), cx);
}

/// AUTO-STOP said on the feedback path: an info toast in the active IDE
/// window and, with OS notifications on for this machine, a system
/// notification (the IDE may live in the tray with no window) that opens
/// Settings → Voice.
pub(crate) fn notify_auto_stopped(hours: i64, cx: &mut App) {
    let line = etalk::meeting::auto_stopped(hours);
    crate::toast::show_in_active_window(crate::toast::Toast::info(line.clone()), cx);
    crate::os_notifications::raise_settings(crate::settings::SettingsSection::Voice, &MEETING.copy.meeting, &line, cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_consent_dialog_reads_the_fixture() {
        let spec = consent_alert("You, Zoom and Slack", "Ops", |_, _| true);
        let (question, body, answers) = spec.words();
        let _ = question;
        assert_eq!(
            body,
            "You, Zoom and Slack will be transcribed. Everyone in the meeting has to agree first. Audio is never stored. The transcript goes to Ops: what it hears is deleted from the server after 7 days, but its replies and anything it creates stay."
        );
        assert!(answers.iter().any(|answer| answer == &MEETING.copy.start), "{answers:?}");
    }
}
