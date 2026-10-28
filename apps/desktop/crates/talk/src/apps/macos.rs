//! NEW-57: macOS desktop audio (meeting.json CAPABILITY + CHANNELS): Core
//! Audio process taps, macOS 14.2+ (`per_app`; older = `none`).
//!
//! - APPS: Core Audio's process objects (`kAudioHardwarePropertyProcessObjectList`)
//!   grouped into apps: a process inside an `.app` belongs to the OUTERMOST
//!   bundle on its path (Chrome's helpers join Google Chrome); a system XPC
//!   service outside any bundle (Safari's `com.apple.WebKit.GPU`) belongs to
//!   the app macOS holds responsible for it; anything else is its own app by
//!   bundle id or executable name. An app plays now (= [`live`]) while one
//!   of its processes has `kAudioProcessPropertyIsRunningOutput`. This
//!   process's own app (the IDE and its speech worker) is never listed.
//! - CAPTURE ([`open`]): one thread scans every second; a wanted app that
//!   plays gets ONE tap over all its processes ([`tap::Tap`]), rebuilt when
//!   its process set changes and closed [`GRACE`] after it stops playing.
//!   Every 20 ms each tap's audio is resampled to 16 kHz mono and sent as a
//!   [`Chunk`] on the app's channel.
//! - PERMISSION: the "Audio Recording" TCC service (`kTCCServiceAudioCapture`,
//!   System Settings → Privacy & Security → Screen & System Audio Recording →
//!   System Audio Recording Only). macOS has no public preflight, and probing
//!   by creating a tap would itself prompt, so [`permission`] reads the
//!   private `TCCAccessPreflight` (never prompts) and [`request_permission`]
//!   calls `TCCAccessRequest` (prompts while not asked yet). The full Screen
//!   Recording grant (`kTCCServiceScreenCapture`, screen memory's) covers
//!   system audio too: measured on macOS 27, a tap delivers real audio with
//!   Audio Recording never asked while Screen Recording is granted. Without
//!   the private symbols the permission reads as granted and the first tap
//!   asks.

mod resample;
mod sys;
mod tap;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::{class, msg_send};
use objc2_core_audio::{
    kAudioHardwarePropertyProcessObjectList, kAudioObjectSystemObject, kAudioProcessPropertyBundleID,
    kAudioProcessPropertyIsRunningOutput, kAudioProcessPropertyPID, AudioObjectID,
};
use objc2_foundation::NSString;

use super::{chunk_queue, AppCapture, AudioApp, Capability, Chunk, ChunkSender, Selection, CAPTURE_QUEUE, SAMPLE_RATE};
use resample::Resampler;
use tap::Tap;

/// How often the apps are rescanned.
const SCAN: Duration = Duration::from_secs(1);
/// How often captured audio is drained into chunks.
const DRAIN: Duration = Duration::from_millis(20);
/// An app that stopped playing keeps its channel this long (pauses between
/// sentences never tear the tap down).
const GRACE: Duration = Duration::from_secs(3);
/// A tap that failed to open is retried after this long.
const RETRY: Duration = Duration::from_secs(10);
/// How long [`request_permission`] waits for the person's answer.
const ASK_TIMEOUT: Duration = Duration::from_secs(120);

pub(super) fn capability() -> Capability {
    let new_enough = sys::os_version().is_some_and(|version| version >= (14, 2));
    if new_enough && sys::tap_functions().is_some() && AnyClass::get(c"CATapDescription").is_some() {
        Capability::PerApp
    } else {
        Capability::None
    }
}

pub(super) fn live() -> Vec<AudioApp> {
    if capability() != Capability::PerApp {
        return Vec::new();
    }
    let mut apps: Vec<AudioApp> =
        Scanner::default().scan().into_iter().filter(|group| group.playing).map(|group| group.app).collect();
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
    apps
}

pub(super) fn permission() -> Option<bool> {
    if capability() != Capability::PerApp {
        return None;
    }
    Some(sys::Tcc::load().is_none_or(|tcc| state(&tcc) == State::Granted))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    Granted,
    Denied,
    NotAsked,
}

/// Audio Recording, or the Screen Recording grant that includes it.
fn state(tcc: &sys::Tcc) -> State {
    match tcc.preflight(sys::AUDIO_CAPTURE) {
        0 => State::Granted,
        _ if tcc.preflight(sys::SCREEN_CAPTURE) == 0 => State::Granted,
        1 => State::Denied,
        _ => State::NotAsked,
    }
}

pub(super) fn request_permission() -> Option<bool> {
    if capability() != Capability::PerApp {
        return None;
    }
    let Some(tcc) = sys::Tcc::load() else {
        return Some(true);
    };
    match state(&tcc) {
        State::Granted => return Some(true),
        // Denied once: macOS never asks again (System Settings only).
        State::Denied => return Some(false),
        State::NotAsked => {}
    }
    // TCC refuses (or kills) a request without the usage string in the main
    // bundle: an unbundled dev build leaves the asking to the first tap.
    if !has_usage_description() {
        return Some(false);
    }
    let (tx, rx) = channel();
    tcc.request(sys::AUDIO_CAPTURE, move |granted| {
        let _ = tx.send(granted);
    });
    Some(rx.recv_timeout(ASK_TIMEOUT).unwrap_or_else(|_| state(&tcc) == State::Granted))
}

pub(super) fn open(selection: Selection) -> Result<AppCapture, String> {
    if capability() != Capability::PerApp {
        return Err("Per-app audio needs macOS 14.2 or later.".to_string());
    }
    if sys::Tcc::load().is_some_and(|tcc| state(&tcc) == State::Denied) {
        return Err("The Audio Recording permission is off (System Settings → Privacy & Security → Screen & System Audio Recording).".to_string());
    }
    let (tx, rx) = chunk_queue(CAPTURE_QUEUE);
    let stop = Arc::new(AtomicBool::new(false));
    let thread = std::thread::Builder::new()
        .name("talk-app-audio".to_string())
        .spawn({
            let stop = stop.clone();
            move || run(selection, tx, &stop)
        })
        .map_err(|error| format!("Could not start the desktop audio thread: {error}"))?;
    Ok(AppCapture::new(rx, Box::new(Guard { stop, thread: Some(thread) })))
}

struct Guard {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One app's running channel.
struct Channel {
    label: String,
    objects: Vec<AudioObjectID>,
    tap: Tap,
    resampler: Resampler,
    quiet_since: Option<Instant>,
}

fn run(selection: Selection, tx: ChunkSender, stop: &AtomicBool) {
    let mut scanner = Scanner::default();
    let mut channels: HashMap<String, Channel> = HashMap::new();
    let mut failed: HashMap<String, Instant> = HashMap::new();
    let mut next_scan = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        let now = Instant::now();
        if now >= next_scan {
            next_scan = now + SCAN;
            let groups = scanner.scan();
            let present: HashSet<&str> = groups.iter().map(|group| group.app.id.as_str()).collect();
            channels.retain(|id, _| present.contains(id.as_str()));
            for group in groups.iter().filter(|group| selection.wants(&group.app.id)) {
                let id = &group.app.id;
                if !group.playing {
                    if let Some(channel) = channels.get_mut(id) {
                        let since = *channel.quiet_since.get_or_insert(now);
                        if now.duration_since(since) >= GRACE {
                            channels.remove(id);
                        }
                    }
                    continue;
                }
                if let Some(channel) = channels.get_mut(id) {
                    channel.quiet_since = None;
                    if channel.objects == group.objects {
                        let rate = channel.tap.rate();
                        if rate != channel.resampler.from() {
                            channel.resampler = Resampler::new(rate, f64::from(SAMPLE_RATE));
                        }
                        continue;
                    }
                    // A process joined or left the app: retap all of them.
                    channels.remove(id);
                }
                if failed.get(id).is_some_and(|at| now.duration_since(*at) < RETRY) {
                    continue;
                }
                match Tap::open(&group.objects, &group.app.name) {
                    Ok(tap) => {
                        failed.remove(id);
                        let resampler = Resampler::new(tap.rate(), f64::from(SAMPLE_RATE));
                        channels.insert(
                            id.clone(),
                            Channel {
                                label: group.app.name.clone(),
                                objects: group.objects.clone(),
                                tap,
                                resampler,
                                quiet_since: None,
                            },
                        );
                    }
                    Err(error) => {
                        log::warn!("talk: no tap for {id}: {error}");
                        failed.insert(id.clone(), now);
                    }
                }
            }
        }
        for (id, channel) in channels.iter_mut() {
            let mut samples = Vec::new();
            let mut captured_ms = 0;
            for (cycle, at) in channel.tap.rx.try_iter() {
                channel.resampler.process(&cycle, &mut samples);
                captured_ms = at;
            }
            if samples.is_empty() {
                continue;
            }
            // Dated by the IO callback that delivered its last cycle.
            let chunk = Chunk { channel: id.clone(), label: channel.label.clone(), samples, captured_ms };
            if !tx.send(chunk) {
                return;
            }
        }
        std::thread::sleep(DRAIN);
    }
}

/// One app as Core Audio sees it now.
struct Group {
    app: AudioApp,
    /// Its process objects, sorted.
    objects: Vec<AudioObjectID>,
    playing: bool,
}

/// Groups process objects into apps, remembering each pid's app.
#[derive(Default)]
struct Scanner {
    apps: HashMap<i32, AudioApp>,
    own: Option<AudioApp>,
}

impl Scanner {
    fn scan(&mut self) -> Vec<Group> {
        autoreleasepool(|_| {
            let own_pid = std::process::id() as i32;
            if self.own.is_none() {
                self.own = Some(identify(own_pid, None));
            }
            let own_id = self.own.as_ref().map(|app| app.id.clone()).unwrap_or_default();
            let mut groups: Vec<Group> = Vec::new();
            let mut seen = HashSet::new();
            for object in sys::get_ids(kAudioObjectSystemObject as AudioObjectID, kAudioHardwarePropertyProcessObjectList) {
                let Some(pid) = sys::get::<i32>(object, kAudioProcessPropertyPID) else {
                    continue;
                };
                if pid <= 0 || pid == own_pid {
                    continue;
                }
                seen.insert(pid);
                let app = self.apps.entry(pid).or_insert_with(|| identify(pid, Some(object))).clone();
                if app.id == own_id {
                    continue;
                }
                let playing = sys::get::<u32>(object, kAudioProcessPropertyIsRunningOutput).unwrap_or(0) != 0;
                match groups.iter_mut().find(|group| group.app.id == app.id) {
                    Some(group) => {
                        group.objects.push(object);
                        group.playing |= playing;
                    }
                    None => groups.push(Group { app, objects: vec![object], playing }),
                }
            }
            self.apps.retain(|pid, _| seen.contains(pid));
            for group in &mut groups {
                group.objects.sort_unstable();
            }
            groups
        })
    }
}

/// The app `pid` belongs to (module docs, APPS).
fn identify(pid: i32, object: Option<AudioObjectID>) -> AudioApp {
    let path = sys::pid_path(pid);
    let mut bundle = path.as_deref().and_then(outermost_app);
    if bundle.is_none() && path.as_deref().is_some_and(|path| path.contains(".xpc/")) {
        bundle = sys::responsible_pid(pid)
            .filter(|responsible| *responsible != pid)
            .and_then(sys::pid_path)
            .as_deref()
            .and_then(outermost_app);
    }
    if let Some(bundle) = bundle {
        if let Some(app) = bundle_app(&bundle) {
            return app;
        }
    }
    let executable = path.as_deref().and_then(|path| path.rsplit('/').next()).filter(|name| !name.is_empty());
    let bundle_id = object.and_then(|object| sys::get_string(object, kAudioProcessPropertyBundleID));
    let id = bundle_id.clone().or_else(|| executable.map(str::to_string)).unwrap_or_else(|| format!("pid-{pid}"));
    let name = executable.map(str::to_string).unwrap_or_else(|| id.clone());
    AudioApp { id, name }
}

/// `/Applications/Google Chrome.app/Contents/Frameworks/…Helper.app/…` →
/// `/Applications/Google Chrome.app`.
fn outermost_app(path: &str) -> Option<String> {
    let end = path.find(".app/")?;
    Some(path[..end + 4].to_string())
}

/// An `.app` bundle's id and display name.
fn bundle_app(path: &str) -> Option<AudioApp> {
    let ns_path = NSString::from_str(path);
    let bundle: Option<Retained<AnyObject>> = unsafe { msg_send![class!(NSBundle), bundleWithPath: &*ns_path] };
    let stem = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".app").to_string();
    let Some(bundle) = bundle else {
        return (!stem.is_empty()).then(|| AudioApp { id: stem.clone(), name: stem });
    };
    let id: Option<Retained<NSString>> = unsafe { msg_send![&*bundle, bundleIdentifier] };
    let name = ["CFBundleDisplayName", "CFBundleName"]
        .into_iter()
        .find_map(|key| info_string(&bundle, key))
        .unwrap_or_else(|| stem.clone());
    let id = id.map(|id| id.to_string()).filter(|id| !id.is_empty()).unwrap_or(stem);
    Some(AudioApp { id, name })
}

fn info_string(bundle: &AnyObject, key: &str) -> Option<String> {
    let key = NSString::from_str(key);
    let value: Option<Retained<AnyObject>> = unsafe { msg_send![bundle, objectForInfoDictionaryKey: &*key] };
    let value = value?;
    let is_string: Bool = unsafe { msg_send![&*value, isKindOfClass: class!(NSString)] };
    if !is_string.as_bool() {
        return None;
    }
    let value: Retained<NSString> = unsafe { Retained::cast_unchecked(value) };
    let text = value.to_string();
    (!text.trim().is_empty()).then_some(text)
}

/// TCC aborts a request without `NSAudioCaptureUsageDescription` in the
/// main bundle's Info.plist (the IDE's `assets/packaging/Info.plist`).
fn has_usage_description() -> bool {
    autoreleasepool(|_| {
        let bundle: Option<Retained<AnyObject>> = unsafe { msg_send![class!(NSBundle), mainBundle] };
        bundle.is_some_and(|bundle| info_string(&bundle, "NSAudioCaptureUsageDescription").is_some())
    })
}

#[cfg(test)]
mod tests {
    use super::outermost_app;

    #[test]
    fn helpers_belong_to_the_outermost_app() {
        assert_eq!(
            outermost_app("/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Helpers/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper"),
            Some("/Applications/Google Chrome.app".to_string())
        );
        assert_eq!(outermost_app("/usr/bin/say"), None);
    }
}
