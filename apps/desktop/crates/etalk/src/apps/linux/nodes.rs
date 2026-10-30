//! Which PipeWire node is which app (pure: the property lookups are passed
//! in, so the rules are unit-tested without a daemon).

#![allow(dead_code)]

use super::super::AudioApp;
use super::pcm;

/// The media class of an app's playback stream.
pub(super) const STREAM_OUTPUT: &str = "Stream/Output/Audio";

/// Who we are: the IDE and its speech worker (the same executable re-run)
/// are never captured.
#[derive(Clone, Debug)]
pub(super) struct Own {
    pub pid: u32,
    pub stem: Option<String>,
}

impl Own {
    pub(super) fn current() -> Self {
        Own { pid: std::process::id(), stem: pcm::own_stem() }
    }
}

/// The app a node plays for, from its properties: `None` for anything that
/// is not an app's playback stream, and for this executable's own streams.
pub(super) fn app_of<'a>(get: impl Fn(&str) -> Option<&'a str>, own: &Own) -> Option<AudioApp> {
    if get("media.class") != Some(STREAM_OUTPUT) {
        return None;
    }
    let clean = |key: &str| get(key).map(str::trim).filter(|value| !value.is_empty());
    let binary = clean("application.process.binary");
    let name = clean("application.name");
    if get("application.process.id").and_then(|pid| pid.trim().parse::<u32>().ok()) == Some(own.pid) {
        return None;
    }
    if let (Some(binary), Some(stem)) = (binary, own.stem.as_deref()) {
        if pcm::same_binary(stem, binary) {
            return None;
        }
    }
    let id = binary.or(name)?;
    let label = name.or(binary)?;
    Some(AudioApp { id: id.to_string(), name: label.to_string() })
}

/// Apps by id (several nodes of one app = one app), sorted by name.
pub(super) fn group(apps: impl IntoIterator<Item = AudioApp>) -> Vec<AudioApp> {
    let mut out: Vec<AudioApp> = Vec::new();
    for app in apps {
        if !out.iter().any(|seen| seen.id == app.id) {
            out.push(app);
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
    out
}

#[cfg(test)]
mod tests {
    use super::super::super::Selection;
    use super::*;

    fn props<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<&'a str> + 'a {
        move |key| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }

    fn own() -> Own {
        Own { pid: 4242, stem: Some("exponential".to_string()) }
    }

    #[test]
    fn app_by_binary_named_by_application_name() {
        let p = [
            ("media.class", STREAM_OUTPUT),
            ("application.process.binary", "firefox"),
            ("application.name", "Firefox"),
            ("application.process.id", "100"),
        ];
        assert_eq!(app_of(props(&p), &own()), Some(AudioApp { id: "firefox".into(), name: "Firefox".into() }));
    }

    #[test]
    fn app_falls_back_to_the_other_key() {
        let only_name = [("media.class", STREAM_OUTPUT), ("application.name", "Zoom")];
        assert_eq!(app_of(props(&only_name), &own()), Some(AudioApp { id: "Zoom".into(), name: "Zoom".into() }));
        let only_binary = [("media.class", STREAM_OUTPUT), ("application.process.binary", "mpv")];
        assert_eq!(app_of(props(&only_binary), &own()), Some(AudioApp { id: "mpv".into(), name: "mpv".into() }));
        let neither = [("media.class", STREAM_OUTPUT), ("application.name", "  ")];
        assert_eq!(app_of(props(&neither), &own()), None);
    }

    #[test]
    fn only_playback_streams_count() {
        for class in ["Stream/Input/Audio", "Audio/Sink", "Audio/Source", "Video/Source"] {
            let p = [("media.class", class), ("application.name", "Firefox")];
            assert_eq!(app_of(props(&p), &own()), None, "{class}");
        }
    }

    #[test]
    fn never_this_executable() {
        let by_pid = [("media.class", STREAM_OUTPUT), ("application.name", "Whatever"), ("application.process.id", "4242")];
        assert_eq!(app_of(props(&by_pid), &own()), None);
        // The speech worker: another pid, the same executable.
        let worker = [
            ("media.class", STREAM_OUTPUT),
            ("application.process.binary", "exponential"),
            ("application.process.id", "4243"),
        ];
        assert_eq!(app_of(props(&worker), &own()), None);
    }

    #[test]
    fn group_dedupes_by_id_and_sorts_by_name() {
        let app = |id: &str, name: &str| AudioApp { id: id.into(), name: name.into() };
        let grouped = group([
            app("zoom", "Zoom"),
            app("firefox", "Firefox"),
            app("firefox", "Firefox"),
            app("chrome", "Google Chrome"),
            app("aplay", "aplay"),
        ]);
        let ids: Vec<&str> = grouped.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, vec!["aplay", "firefox", "chrome", "zoom"]);
    }

    #[test]
    fn selection_picks_by_app_id() {
        let picked = Selection::Include(vec!["zoom".into()]);
        assert!(picked.wants("zoom"));
        assert!(!picked.wants("firefox"));
        let but = Selection::Exclude(vec!["spotify".into()]);
        assert!(but.wants("zoom"));
        assert!(!but.wants("spotify"));
        assert!(Selection::All.wants("anything"));
    }
}
