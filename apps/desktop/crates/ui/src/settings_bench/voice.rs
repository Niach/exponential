use super::*;

fn voice_settings(ptt: &str) -> etalk::voice::VoiceSettings {
    etalk::voice::VoiceSettings {
        voice_device_id: Some("d-mint".into()),
        agent_id: Some("a1".into()),
        ptt_shortcut: ptt.into(),
        language: "en".into(),
        ..Default::default()
    }
}

fn voice(name: &str, settings: etalk::voice::VoiceSettings, capture: Option<etalk::voice::CaptureState>) {
    shoot_pane(name, VoicePrefsPane::for_shots, move |pane, cx| pane.show(VoiceShot { settings, capture }, cx));
}

#[test]
#[ignore]
fn settings_voice_unbound() {
    voice("voice-unbound", voice_settings(""), None);
}

#[test]
#[ignore]
fn settings_voice_bound() {
    let settings = etalk::voice::VoiceSettings {
        wake_word: true,
        wake_phrase: "Hey Studio".into(),
        voice_file: "~/voices/me.wav".into(),
        ..voice_settings("ctrl+shift+space")
    };
    voice("voice-bound", settings, None);
}

#[test]
#[ignore]
fn settings_voice_recording_pending() {
    use etalk::voice::{capture_start, capture_step, CaptureEvent};
    let capture = [CaptureEvent::Down("ctrl".into()), CaptureEvent::Down("shift".into())].iter().fold(capture_start("ctrl+space"), |c, e| capture_step(&c, e));
    voice("voice-recording-pending", voice_settings("ctrl+space"), Some(capture));
}

#[test]
#[ignore]
fn settings_voice_error() {
    let capture = etalk::voice::CaptureState {
        status: etalk::voice::CaptureStatus::Recording,
        value: "ctrl+space".into(),
        held: Vec::new(),
        error: Some(etalk::voice::VOICE.ptt_copy.shortcut_needs_modifier.clone()),
    };
    voice("voice-error", voice_settings("ctrl+space"), Some(capture));
}

/// #NEW-57: Desktop audio picks apps on a per-app device: the Apps row.
fn voice_apps_settings() -> etalk::voice::VoiceSettings {
    etalk::voice::VoiceSettings {
        desktop_audio: etalk::meeting::DesktopAudio::Include,
        desktop_audio_apps: vec![etalk::meeting::AudioApp { id: "us.zoom.xos".into(), name: "Zoom".into() }],
        ..voice_settings("ctrl+space")
    }
}

fn voice_clicked(name: &str, settings: etalk::voice::VoiceSettings, clicks: &'static [(f32, f32)]) {
    crate::meeting_mode::SHOT_AUDIO.with(|shot| {
        *shot.borrow_mut() = Some((
            etalk::meeting::DesktopAudioCapability::PerApp,
            vec![
                etalk::meeting::AudioApp { id: "us.zoom.xos".into(), name: "Zoom".into() },
                etalk::meeting::AudioApp { id: "com.tinyspeck.slackmacgap".into(), name: "Slack".into() },
            ],
        ))
    });
    let points: Vec<(f32, f32)> = std::env::var("EXP_VOICE_CLICKS").ok().map(|v| v.split(';').filter_map(|p| p.split_once(',')).filter_map(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?))).collect()).unwrap_or_else(|| clicks.to_vec());
    // The eUI pane presses node ids (a headless click doesn't reach its popups);
    // the pointer still moves to the first point, as a click would leave it.
    let presses: Vec<String> = std::env::var("EXP_VOICE_PRESS").ok().map(|v| v.split(';').map(str::to_string).collect()).unwrap_or_default();
    let points = if presses.is_empty() { points } else { Vec::new() };
    let hover = std::env::var("EXP_VOICE_HOVER").ok().and_then(|v| v.split_once(',').and_then(|(x, y)| Some((x.parse::<f32>().ok()?, y.parse::<f32>().ok()?))));
    shoot_pane_acted(name, VoicePrefsPane::for_shots, move |pane, cx| pane.show(VoiceShot { settings, capture: None }, cx), None, &points, move |pane, window, cx| {
        if let Some((x, y)) = hover {
            let moved = gpui::MouseMoveEvent { position: gpui::point(px(x), px(y)), pressed_button: None, modifiers: Default::default() };
            window.dispatch_event(gpui::PlatformInput::MouseMove(moved), cx);
        }
        pane.press_for_shots(&presses, window, cx)
    });
}

#[test]
#[ignore]
fn settings_voice_apps() {
    voice_clicked("voice-apps", voice_apps_settings(), &[]);
}

#[test]
#[ignore]
fn settings_voice_apps_open() {
    voice_clicked("voice-apps-open", voice_apps_settings(), &[]);
}
