//! NEW-57: meeting mode's IDE surfaces (`fixtures/meeting.json`) beside the
//! host (`crate::meeting`, which listens and flushes): THE consent dialog
//! (Settings → Voice's toggle and the tray's item open it; Start =
//! `voice.startMeeting {consent: true}`), stopping at once
//! (`voice.stopMeeting`), the voice device's desktop audio as its doctor
//! reports it (`doctor.desktopAudio`), and the pause hotkey
//! (`pause.shortcut`), grabbed like push to talk's keys whenever a meeting
//! runs or screen memory is on.

use gpui::{App, Global, Window};

use etalk::meeting::{consent_body, who, AudioApp, DesktopAudioCapability, MeetingStopReason, MEETING, RETENTION_DAYS};
use etalk::voice::{VoiceSettings, VOICE};

use crate::push_to_talk::hotkey::{self, HotkeyEvent};

/// The voice device's desktop audio as its synced doctor reports it
/// (`doctor.desktopAudio`; absent = `none` and `[]`).
pub(crate) fn desktop_audio_of(settings: &VoiceSettings, cx: &App) -> (DesktopAudioCapability, Vec<AudioApp>) {
    let Some(device_id) = settings.voice_device_id.as_deref() else { return Default::default() };
    let Some(store) = sync::Store::try_global(cx) else { return Default::default() };
    let devices = store.collections().devices.read(cx);
    let audio = devices
        .iter()
        .find(|row| row.device_id.as_deref() == Some(device_id))
        .and_then(|row| row.doctor.as_ref())
        .and_then(|doctor| doctor.get("desktopAudio"))
        .and_then(|audio| serde_json::from_value::<coding::device_doctor::DoctorDesktopAudio>(audio.clone()).ok());
    audio.map(|audio| (audio.capability, audio.apps)).unwrap_or_default()
}

/// `meeting::who` of the member's settings, with the voice device's capability.
pub(crate) fn who_of(settings: &VoiceSettings, cx: &App) -> String {
    let (capability, _) = desktop_audio_of(settings, cx);
    who(settings.desktop_audio, &settings.desktop_audio_apps, capability)
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
    let settings = crate::speak_replies::settings(cx);
    let who = who_of(&settings, cx);
    let agent = crate::voice_tray::agent_label(&settings, cx).unwrap_or_else(|| VOICE.copy.agent.clone());
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
    write(true, None, cx);
}

/// Stop at once (`voice.stopMeeting`, no dialog).
pub(crate) fn stop(cx: &mut App) {
    write(false, None, cx);
}

/// #FEED-163: an automatic stop (`voice.stopMeeting {reason}`; the server
/// posts the conversation note).
pub(crate) fn stop_for(reason: MeetingStopReason, cx: &mut App) {
    write(false, Some(reason), cx);
}

fn write(on: bool, reason: Option<MeetingStopReason>, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else { return };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move {
                if on { api::voice::voice_start_meeting(&trpc) } else { api::voice::voice_stop_meeting(&trpc, reason) }
            })
            .await;
        let _ = cx.update(|cx| match result {
            Ok(stored) => crate::speak_replies::set_settings(stored, cx),
            Err(err) => crate::action_error::failed_action_err(if on { "start" } else { "stop" }, "meeting mode", &err, cx),
        });
    })
    .detach();
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

/// The portal shortcut id the pause hotkey binds.
const PAUSE_ID: &str = "pause-listening";

/// The live pause grab (dropping it lets the keys go).
#[derive(Default)]
struct PauseHotkey(Option<hotkey::Grab>);

impl Global for PauseHotkey {}

/// Grab `pause.shortcut` while `wanted` (the voice device, a meeting runs or
/// screen memory is on); each press toggles `meeting::set_paused`. Letting
/// it go resumes. Main thread (the native grab lives there).
pub(crate) fn sync_pause_hotkey(wanted: bool, cx: &mut App) {
    let held = cx.try_global::<PauseHotkey>().is_some_and(|pause| pause.0.is_some());
    if wanted == held {
        return;
    }
    if !wanted {
        cx.set_global(PauseHotkey(None));
        if crate::meeting::state(cx).paused {
            crate::meeting::set_paused(false, cx);
        }
        return;
    }
    let (tx, rx) = flume::unbounded::<HotkeyEvent>();
    let grab = hotkey::grab_as(&MEETING.pause.shortcut, PAUSE_ID, &MEETING.copy.pause, tx);
    cx.set_global(PauseHotkey(Some(grab)));
    cx.spawn(async move |cx| {
        while let Ok(event) = rx.recv_async().await {
            match event {
                HotkeyEvent::Down => cx.update(|cx| {
                    let paused = crate::meeting::state(cx).paused;
                    crate::meeting::set_paused(!paused, cx);
                }),
                HotkeyEvent::Status(status) => {
                    if let Some(reason) = status.reason() {
                        log::info!("[meeting] the pause hotkey is not held system-wide: {reason:?}");
                    }
                }
                HotkeyEvent::Up => {}
            }
        }
    })
    .detach();
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
