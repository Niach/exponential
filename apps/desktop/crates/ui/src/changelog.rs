//! The in-app changelog (EXP-723) — the desktop half of EXP-164's "What's
//! new".
//!
//! The web app keeps the whole history in `apps/web/src/lib/changelog.ts` and
//! surfaces the HEAD entry as a dismissable card in its sidebar footer. The
//! desktop shows the same card in the rail footer, so it needs the same head
//! entry — mirrored here as a const rather than fetched, because the app must
//! be able to say what is new in the build the user is actually running (an
//! older desktop pinned to an older release must not advertise a newer web
//! entry).
//!
//! **The mirror is a contract**: [`LATEST`] is a verbatim copy of
//! `CHANGELOG[0]` — id, date, title, summary and body — and
//! `apps/web/src/lib/changelog-desktop-mirror.test.ts` reads this file and
//! fails the web suite when the ids or titles drift. Prepending a web entry
//! means updating this const in the same commit.
//!
//! Dismissal state is the per-install `settings.json` key `changelogSeenId`
//! (the desktop analog of the web's per-device `changelog-seen.ts`): the card
//! renders while the stored id differs from [`LATEST`]'s, and both the ✕ and
//! opening the dialog store it.

use gpui::{
    div, px, size, App, AppContext as _, IntoElement, ParentElement as _, Render, SharedString,
    Styled as _, Window,
};
use gpui_component::{text::TextView, v_flex, ActiveTheme as _};

use crate::coding_flow;
use crate::native_dialog::{self, DialogContent, DialogSpec};

/// One changelog entry — the `ChangelogEntry` interface of
/// `apps/web/src/lib/changelog.ts`, field for field.
pub(crate) struct ChangelogEntry {
    /// Stable slug and dismissal key. Never reused.
    pub id: &'static str,
    /// ISO date, display only.
    pub date: &'static str,
    pub title: &'static str,
    /// One-line card preview (rendered truncated).
    pub summary: &'static str,
    /// GFM markdown, rendered read-only in the dialog.
    pub body: &'static str,
}

/// The head entry of the web `CHANGELOG` — see the module docs: this is a
/// verbatim mirror, gated by a web-side test.
pub(crate) const LATEST: ChangelogEntry = ChangelogEntry {
    id: "2026-10-28-voice-calls",
    date: "2026-10-28",
    title: "Calls like a phone call",
    summary: "Calls ring first, your agent answers with Hello?, and the call screen looks like a phone call.",
    body: r#"- **It rings first**: a call rings for 3 seconds while your voice device loads speech and the voice, then answers with Hello?; a voice that is still downloading keeps ringing until it is ready.
- **A real call screen**: the agent's avatar, its name, the call's timer and what is being said, with round mute, speaker and red end buttons on web, iOS, Android and the desktop.
- **Floating pill**: while the desktop app is minimized a small call pill stays on top; drag it anywhere, it remembers where you put it, and Esc or the tray's End call ends the call.
- **Replies are always spoken**: Speak replies is gone; whatever you say by voice is answered by voice, typed messages never are.
- **Your own push-to-talk key**: press any key or combination in Settings → Voice to bind it, like in a game; Backspace clears it.
- **Wake word**: it now hears its phrase anywhere in what you say, and Settings → Voice tells you why it isn't listening."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_109: ChangelogEntry = ChangelogEntry {
    id: "2026-10-28-meeting-mode",
    date: "2026-10-28",
    title: "Meeting mode",
    summary: "Your voice agent follows a meeting you start, with everyone's consent.",
    body: r#"- **Meeting mode**: start it in Settings → Voice or the tray; after you confirm that everyone agrees, your voice device transcribes you and the apps you pick (Zoom, Slack…) on separate channels and hands the transcript to your voice agent in short batches.
- **Nothing kept longer than needed**: audio is never stored, the transcript is deleted from the server after 7 days and from your device after 30.
- **Pause listening**: Ctrl + Alt + P pauses the meeting and screen memory at once.
- **Never forgotten on**: a meeting stops by itself after 4 hours and tells you so, and while it listens the tray icon carries the microphone dot."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_108: ChangelogEntry = ChangelogEntry {
    id: "2026-10-28-screen-memory",
    date: "2026-10-28",
    title: "Screen memory",
    summary: "Your voice agent can look at the last 10 minutes of your screen.",
    body: r#"- **Screen memory**: on your voice device, the tray's Screen memory switch keeps the last 10 minutes of your screen (deduplicated, on this device only). Your voice agent can look at up to 4 screenshots from a moment you talk about.
- **Paused for private windows**: password managers, private browser windows and online banking are never captured; the tray says Paused while one is focused.
- **Private windows stay private**: a password manager or banking window behind other windows is blacked out too, and on desktops without a window list (Wayland) the switch doesn't show."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_107: ChangelogEntry = ChangelogEntry {
    id: "2026-10-28-work-screen",
    date: "2026-10-28",
    title: "Work screen",
    summary: "Pick the display agents drive with computer use, and fullscreen apps stay yours.",
    body: r#"- **Work screen**: Device settings → Computer use has a Work screen row: Auto (every display without a fullscreen app) or one display. Agents only see and drive that display.
- **Fullscreen stays yours**: while you are in a fullscreen app (a game, a video) on another display, agents don't click or type; the voice overlay never opens over a fullscreen app.
- **Follows its monitor**: the picked work screen is remembered by the monitor itself, so plugging in or reordering displays never moves agents to another screen."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_106: ChangelogEntry = ChangelogEntry {
    id: "2026-10-28-sessions-never-stall",
    date: "2026-10-28",
    title: "Sessions that never stall, and a disk check on every device",
    summary: "A new thread always starts its first turn, a killed or restarted run says so and resumes, the model picker works on a session that hit its limit, and devices clean up old worktrees and warn when the disk runs low.",
    body: r#"- **First turns**: a new thread whose first turn failed or never ran starts again by itself instead of waiting for a nudge.
- **Interrupted runs**: a run killed by its machine (out of memory, a crash, a restart) reads interrupted and resumes; on Linux each run gets its own memory scope, so one heavy run no longer takes the others down.
- **Model limits**: picking a model on a session that hit its limit switches it at once.
- **Automerge**: a thread can no longer stay locked, a change arriving mid-merge is picked up, and draft PRs follow GitHub.
- **Disk**: devices remove build output from finished worktrees, list the worktrees they kept and why, and warn when less than 10 GB is free.
- **Live view**: a reconnecting run is never replaced by its replay, and a session without a transcript answers at once instead of spinning."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_105: ChangelogEntry = ChangelogEntry {
    id: "2026-10-27-release-train",
    date: "2026-10-27",
    title: "Release 2026-10-27: call your agent (beta), with the train's fixes",
    summary: "This release adds voice calls to your agent from the phone, the web and the IDE, the wake word and voice proposal cards, all in beta. Interrupted sessions now resume by themselves, queued sessions wait for their device, and chats can be selected and archived.",
    body: r#"- **Calls (beta)**: call your voice device's agent from iPhone, Android, the web or the IDE; a call on the desktop's own speakers no longer interrupts the agent's voice.
- **Wake word (beta)**: say the wake phrase on your voice device instead of holding push to talk; a red dot shows while the microphone listens, Mute is remembered, and a sidebar voice button stands in where no tray shows.
- **Spoken replies (beta)**: German now starts speaking as fast as English.
- **Proposal cards (beta)**: a voice agent proposes changes on a card and only acts after your yes.
- **Sessions**: interruptions resume by themselves, queued sessions wait for their device, an idle session's transcript opens on every device, and plan and question cards reach the team agent's conversation and show inline on Android.
- **Merges**: a refused merge lands in the thread with Retry, and automerge also runs when a turn or session ends or a PR opens.
- **Chats and Devices**: select chats to mark read, archive (just for you), stop or delete them; every check is the square checkbox; Devices shows one line per machine, and team owners can remove a shared machine.
- **Privacy**: a private chat's text never shows up in a conversation that may not see it.
- **Store builds**: the IDE and CLI 0.14.80 (an older 0.14.79 daemon cannot resume interrupted runs, please update), Android 0.14.72; iOS 0.14.70 is in TestFlight."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_104: ChangelogEntry = ChangelogEntry {
    id: "2026-10-27-devices-one-line",
    date: "2026-10-27",
    title: "Devices: one line per machine, warnings behind one icon",
    summary: "The Devices page lists each machine on one line with its logins beside it. Limits show when you hover a login, and anything that needs a fix sits behind one warning icon.",
    body: r#"- **One line**: each device shows its name, version and status, then one chip per login. Nothing folds any more.
- **Limits on hover**: hover a login to see its 5h, week and model limits. Click or tap it for Sign in, Sign out and Remove account, with the same limits on top.
- **One warning icon**: a login that needs signing in again, a signed-out login or an available update show behind a single icon, each with its own button.
- **No false "Signed out"**: a coding agent you installed but never signed in to no longer marks the device as signed out. A device without any login says No account.
- **Offline last**: offline devices sit under an Offline label at the end, dimmed."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_103: ChangelogEntry = ChangelogEntry {
    id: "2026-10-27-rail-voice-button",
    date: "2026-10-27",
    title: "Wake word: a voice button in the sidebar when there's no tray",
    summary: "On a Linux desktop that shows no tray icons, such as GNOME without AppIndicator, the sidebar gets a microphone button while the wake word is on. It shows a red dot while the microphone listens and opens the tray's menu, including Mute.",
    body: r#"- **Where**: next to the Computer button at the bottom of the sidebar, only on your voice device, only with the wake word on, and only while nothing shows the tray icon.
- **What it says**: hover it for the phrase it listens for, or Microphone muted. The red dot is the same one the tray icon shows.
- **Its menu**: Speak replies, Push to talk, Wake word, Mute microphone and Voice settings…, working exactly as in the tray."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_102: ChangelogEntry = ChangelogEntry {
    id: "2026-10-27-chat-selection-archive",
    date: "2026-10-27",
    title: "Select chats, mark them read and archive them",
    summary: "Select several chats at once to mark them read or unread, archive, stop or delete them, and keep your Chats list tidy with an Archived page of your own.",
    body: r#"- **Select**: on a phone, long-press a chat, or an issue, to select it straight away. Nothing moves aside: a selected chat gets a highlight and a check on its icon. On a computer tick the checkbox that shows when you hover a chat, or pick Select from its right-click menu; cmd or ctrl click adds a chat, shift click adds a range, cmd or ctrl A selects them all. Tapping a chat while selecting toggles it.
- **The bar**: the same bar as the issue list's selection: Mark read or Mark unread, Archive, Stop while a chat runs and Delete when you may delete every selected chat.
- **Archived**: archiving is yours alone, your teammates keep their list. Archived chats move to the Archived row at the top of Chats, and a chat that needs you comes back on its own.
- **The Chats dot**: an archived chat no longer lights it."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_101: ChangelogEntry = ChangelogEntry {
    id: "2026-10-27-wake-word-follow-ups",
    date: "2026-10-27",
    title: "Wake word: a pause after each turn, Listening in Settings, Mute remembered",
    summary: "After a wake turn the wake word waits a moment before it listens again, so the end of your sentence can't start a second turn. Settings → Voice shows Listening next to Wake word on your voice device, and Mute microphone stays on after a restart.",
    body: r#"- **A short pause**: for a second and a half after a wake turn ends, the wake word ignores what it hears, so a trailing phrase or an echo doesn't open another turn.
- **Listening in Settings**: on your voice device, the Wake word row in Settings → Voice reads Listening while the microphone listens for your phrase, or Microphone muted. This helps where no tray icon shows, such as GNOME without AppIndicator.
- **Mute remembered**: Mute microphone in the tray stays on after you restart the app on that computer.
- **Tray**: on Linux the app notices right away when the panel showing its tray icon goes away."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_100: ChangelogEntry = ChangelogEntry {
    id: "2026-10-27-merge-refusals-in-the-thread",
    date: "2026-10-27",
    title: "A refused merge goes to the thread",
    summary: "When a merge is refused, the thread's session gets GitHub's reason and takes it from there, so the Fix conflicts run is gone.",
    body: r#"- **Into the thread**: a refused merge (checks, conflicts, anything else) appears as Merge of PR #N refused in the thread and wakes its session with the reason and what to do; the error says Sent to the thread.
- **Retry everywhere**: every refused merge offers Retry, and the error names what failed: Couldn't merge the PR or Couldn't merge the stack.
- **Fix conflicts is gone** on web, the IDE, iPhone and Android: the session rebases and resolves conflicts itself."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_99: ChangelogEntry = ChangelogEntry {
    id: "2026-10-27-wake-word",
    date: "2026-10-27",
    title: "Wake word on your voice device",
    summary: "Turn on Wake word in the tray or in Settings → Voice and say your phrase (Hey and your agent's name, unless you set your own) to talk without a shortcut. The tray icon shows a dot while the microphone listens; Mute turns it off.",
    body: r#"- **Wake word**: on the desktop you picked as your voice device, say the phrase from Settings → Voice (Hey and your agent's name when you leave it empty) and what follows; the capsule opens like push to talk and the message goes to your agent when you stop talking. It runs on the speech recognition push to talk already uses, so there is no extra download.
- **Tray**: the icon carries a dot while the microphone listens for the wake word, and Mute microphone turns it off too.
- **Tray fixes**: a double-click on the Windows tray icon opens one window, and on Linux the tray watches for a panel only while this computer is your voice device."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_98: ChangelogEntry = ChangelogEntry {
    id: "2026-10-27-voice-proposal-cards",
    date: "2026-10-27",
    title: "Voice: your agent asks before it changes things",
    summary: "In your voice conversation the agent now proposes every change as a card in the Dock instead of making it. Reads run at once; you answer a card by tapping it or by voice (\"take two\", \"reject one\", \"accept all\", \"two, but priority high\"), and deletes, merges and messages to others are read back and need a spoken yes.",
    body: r#"- **Proposal cards**: when you talk to your voice agent, creating or updating an issue, commenting, adding a label or a relation, or starting a subagent shows a card with what would change and what you said. Allow runs it, Deny (or "no") drops it, and a typed or spoken change ("yes, but priority high") is applied in one go, for that same change only. A card waits 15 minutes; a newer card for the same issue replaces the older one.
- **Say yes for the big ones**: deleting, merging or closing a pull request, messaging another session, running a command or editing a file is read aloud and needs an explicit yes. Team settings, people, device sign-ins and scripts are never run from a voice conversation, and only you can answer its cards.
- **Answer by voice**: with push to talk, "take two", "reject one", "accept all", "yes" or "no" answer the waiting cards instead of being sent as a message.
- **Only your voice agent**: every other session, thread and agent conversation works exactly as before.
- **Update the voice device**: the cards need desktop app 0.14.80 or later; an older desktop keeps running your voice conversation without asking."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_97: ChangelogEntry = ChangelogEntry {
    id: "2026-10-27-voice-calls-phones",
    date: "2026-10-27",
    title: "Call your agent from your phone (beta)",
    summary: "The telephone in an agent's chat now calls from the iPhone and Android apps too: the call streams to your voice device, which listens, sends what you say and speaks every reply.",
    body: r#"- **Calls from your phone**: tap the telephone in an agent chat's header. The call fills the screen with Mute and End call, and runs on your voice device, so it needs that device online.
- **Talk over it**: speaking while a reply plays stops the voice and listens again; your phone's echo cancelling keeps the reply out of the microphone.
- **When it can't call**: the telephone stays visible but dimmed; long-press it to see why (no voice device, Speak replies off, the device offline, its voice still getting ready, or no usable GPU).
- **Lock the screen**: a call keeps running with the screen locked or another app in front; only End call ends it. A phone call coming in drops it.
- **Microphone**: the apps ask for the microphone on your first call. On Android a notification shows while a call runs."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_96: ChangelogEntry = ChangelogEntry {
    id: "2026-10-27-voice-calls-web",
    date: "2026-10-27",
    title: "Call your agent from the browser (beta)",
    summary: "The telephone in an agent's chat on web calls it through your voice device: talk hands-free in the browser, every reply is spoken, and talking over a reply stops it.",
    body: r#"- **Calls on web**: the telephone in an agent chat's header opens a call card over the chat. Your microphone streams to your voice device, which listens and speaks the replies back into the browser.
- **Mute and End call**: the card shows what was heard, what is queued and what is said, with End call and Mute.
- **When it can't call**: the telephone stays visible but dimmed, and its tooltip says why (no voice device, Speak replies off, the device offline, its voice still getting ready, or no usable GPU)."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_95: ChangelogEntry = ChangelogEntry {
    id: "2026-10-27-voice-calls-ide",
    date: "2026-10-27",
    title: "Call your agent from the IDE (beta)",
    summary: "On your voice device, the telephone in an agent's chat starts a call: talk hands-free, every reply is spoken, and talking over a reply stops it. Calling from your phone and the browser comes next.",
    body: r#"- **Calls**: the telephone in an agent chat's header, or Call in the tray, opens a call on your voice device. The microphone stays open until End call, each thing you say goes to the agent as a message, and every reply is read aloud.
- **Talk over it**: speaking while a reply plays stops the voice and listens again. What you say while the agent is still working waits in the chat's queue, like a typed message.
- **Mute**: the call's bar has Mute and End call.
- **When it can't call**: the telephone stays visible but dimmed, and its tooltip says why (no voice device, Speak replies off, or no usable GPU to speak with).
- **Devices**: the voice device's readiness now has a Speech row that says whether it can speak."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_94: ChangelogEntry = ChangelogEntry {
    id: "2026-10-27-device-share-remove",
    date: "2026-10-27",
    title: "Team owners can remove a shared machine",
    summary: "A team owner can now remove a teammate's shared machine from the team, and when two accounts claim the same machine, the account that registered it first wins.",
    body: r#"- **Remove from this team**: on Devices, a team owner sees Remove on a teammate's shared machine. It stops sharing that machine with the team, and sessions teammates started on it end.
- **Who holds it**: if a machine is already shared with the team from another account, a team owner sees whose account shares it.
- **First registration wins**: a machine shared from an account that registered it later is replaced when the account that registered it first shares it.
- **One account per machine and team**: two accounts signed in on one machine can each share it, but only into different teams."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_93: ChangelogEntry = ChangelogEntry {
    id: "2026-10-26-release-train",
    date: "2026-10-26",
    title: "Release 2026-10-26: Talk to your agents (beta), with the train's fixes",
    summary: "This release ships voice to the IDE as a beta: push to talk and spoken replies in English and German, set up in Settings → Voice on every device. Calling your agent from the phone and the wake word are coming. The release review also fixed how Settings → Voice saves, and push to talk now stops listening when the IDE window loses focus.",
    body: r#"- **Talk (beta)**: on the desktop you pick as your voice device, hold the push to talk shortcut, speak and let go; with Speak replies on, the answer is read aloud in English or German. Calling your agent from the phone and the wake word are coming.
- **Settings → Voice**: on web, the IDE, iPhone and Android, quick changes no longer undo each other, a failed save only resets the field it was about, and typed text is kept when you leave the page.
- **Push to talk**: when the shortcut only works inside an Exponential window, switching away now ends the hold instead of listening on.
- **Speak replies**: a failed first download is retried on its own, and Linux finds the audio player on more setups.
- **Sessions**: a long turn no longer shows needs input by mistake, and a machine shared into a team is listed under the right host.
- **Store builds**: Android 0.14.71 and the IDE 0.14.79 are live; iOS 0.14.69 is in TestFlight."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_92: ChangelogEntry = ChangelogEntry {
    id: "2026-10-26-small-fixes",
    date: "2026-10-26",
    title: "Mention pills in chats, and fixes",
    summary: "People and agents you mention in a chat show as pills on every platform; the computer-use banner shows its whole label, and the voice switches stay off after their agent is deleted.",
    body: r#"- **Mention pills**: an @mention in a chat message shows as a pill with the person's or agent's name on web, the IDE, iPhone and Android.
- **Computer-use banner**: the banner window is exactly the size of its bar, so a long label is no longer cut off.
- **Voice**: if your voice agent is deleted, Speak replies, Push to talk and Wake word stay off until you pick another agent; a wake phrase with control characters is refused."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_91: ChangelogEntry = ChangelogEntry {
    id: "2026-10-26-push-to-talk",
    date: "2026-10-26",
    title: "Push to talk on your voice device",
    summary: "Hold a shortcut anywhere on your voice device, speak, and let go: what you said goes to your agent as a plain message, and a small overlay shows it listening, thinking and speaking the reply.",
    body: r#"- **Hold to talk**: on the desktop picked as your voice device, with Push to talk on, hold the shortcut from Settings → Voice (Ctrl + Space by default) in any app, speak, and release. The words land in your chat with the voice agent; with Speak replies on, the answer is read aloud.
- **The overlay**: a capsule at the bottom of the screen under your pointer shows Listening, Heard, Thinking and Speaking, with Stop while a reply is read. Talking over a reply stops it.
- **Mute**: the tray gains Mute microphone; while it is on, the shortcut shows Microphone muted and the microphone never opens.
- **Never silent**: if the shortcut can't be held system-wide (another app has it, or the desktop has no global shortcuts), the tray item says it is unavailable, Settings → Voice says why, and it still works while an Exponential window is focused. On Wayland, KDE, GNOME 48+ and Hyprland ask you to confirm the shortcut once.
- **Mouse buttons**: Mouse button 4 and 5 work on Linux with X11 and on Windows; macOS and Wayland don't offer them.
- **Not sent**: if a message can't be sent, the overlay says so and the words wait in the agent's chat composer.
- **Windows**: speech recognition needs the Microsoft Visual C++ runtime; the voice device's Microphone row offers to install it when it is missing.
- **First use** downloads the speech recognition model (about 670 MB)."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_90: ChangelogEntry = ChangelogEntry {
    id: "2026-10-26-voice-german",
    date: "2026-10-26",
    title: "Speak replies in German and in your own voice",
    summary: "The IDE now speaks replies in English or German with one voice that runs on your GPU, cloned from your own clip or a built-in voice; without a usable GPU, replies are shown, not spoken.",
    body: r#"- **German and English**: the IDE reads your agent's replies aloud in the language you pick in Settings → Voice, with one voice for both.
- **Your own voice**: with a Voice file and its transcript set, the voice is cloned from that clip on your voice device; without one it uses a built-in voice. The file must be a local .wav of at most 10 MB, and nothing is uploaded.
- **Needs a GPU**: the voice runs on your desktop's GPU (NVIDIA on Linux, Apple Silicon on macOS, a Vulkan GPU on Windows). Without a usable GPU, replies in either language are shown, not spoken; this also ends English speaking on a desktop without one, and Settings → Voice says so next to each language.
- **One download on first use**: about 3.0 GB on Linux, 2.2 GB on macOS and 2.3 GB on Windows, checked before it is used; the Speak replies row shows the progress. The old English voice (about 350 MB) is removed.
- **Marked as AI**: every spoken reply carries an inaudible AI-content watermark."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_89: ChangelogEntry = ChangelogEntry {
    id: "2026-10-26-voice-tray",
    date: "2026-10-26",
    title: "The IDE in your system tray",
    summary: "The desktop you picked as your voice device now sits in the system tray: open the IDE and switch Speak replies, push to talk and the wake word from there, even with every window closed.",
    body: r#"- **Tray menu**: Open Exponential, Speak replies, Push to talk (with its shortcut) and Wake word (with its phrase), plus Voice settings… and Quit. Mute microphone joins once the IDE listens.
- **Only on your voice device**: the tray shows on the desktop picked in Settings → Voice and disappears when you pick another.
- **Stays running**: on Linux and Windows, closing the last window keeps the IDE in the tray so replies keep being spoken; clicking the tray icon or launching the IDE again brings a window back, and Quit in the tray menu ends it.
- **Linux**: the tray uses StatusNotifierItem, shown by KDE, most other desktops and GNOME with the AppIndicator extension."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_88: ChangelogEntry = ChangelogEntry {
    id: "2026-10-26-voice-language",
    date: "2026-10-26",
    title: "Settings → Voice: German and your own voice",
    summary: "Settings → Voice gains a Language row (English or Deutsch) and a Voice file with its transcript, so a reply can soon be spoken in German or in a voice cloned from your own clip.",
    body: r#"- **Language**: pick English or Deutsch for spoken replies. German is spoken on a desktop with a GPU; without one, German replies are shown, not spoken.
- **Your own voice**: enter the path of a short reference clip on your voice device and what it says. Nothing is uploaded, and there is no training: the German voice will clone it from that one clip.
- **How to make a good clip**: the new guide at exponential.at/skills/voice-reference-clip/SKILL.md walks through picking, cutting and transcribing it, and the consent rule.
- **Coming next**: the German voice and cloning arrive in the IDE in the next update; until then English keeps speaking as before."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_87: ChangelogEntry = ChangelogEntry {
    id: "2026-10-25-voice-settings",
    date: "2026-10-25",
    title: "Settings → Voice: one voice device, one agent, for you",
    summary: "Voice moved out of Device settings into its own Settings → Voice page: pick the desktop that speaks and listens for you and the agent it talks to, once for all your devices.",
    body: r#"- **One place**: Settings → Voice on web, the IDE, iPhone and Android holds your voice settings once for all your devices, instead of a Voice group per device.
- **Voice device**: pick the desktop that speaks for you; only that one reads replies aloud.
- **Agent**: pick the agent you talk to; with no device or no agent, every switch turns off.
- **Coming next**: push to talk, the tray and the wake word get their switches here as they ship.
- **Your old choice carries over**: if Speak replies was on for a desktop, that desktop and agent are already set."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_86: ChangelogEntry = ChangelogEntry {
    id: "2026-10-25-speak-replies",
    date: "2026-10-25",
    title: "Speak replies: your assistant answers out loud in the IDE",
    summary: "Pick the agent that speaks for you and switch on Speak replies in Settings → Voice: the IDE reads its replies aloud with a voice that runs on your machine.",
    body: r#"- **Settings → Voice**: pick your voice device (the desktop that speaks) and the agent you talk to, then switch on Speak replies; the same page on web, the IDE, iPhone and Android.
- **Spoken in the IDE**: each final reply of that agent is read aloud on your voice device, with a Speaking banner and Stop above the composer; code blocks and links are skipped, issue numbers are said the way people say them.
- **Local voice**: the voice (English or German) downloads once on first use, about 2.2 to 3 GB, and runs on your desktop's GPU; nothing you hear leaves your machine."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_85: ChangelogEntry = ChangelogEntry {
    id: "2026-10-25-release-train",
    date: "2026-10-25",
    title: "Release 2026-10-25: Apps keep data, read the team and act as you, with the train's fixes",
    summary: "This release ships app records, team data sources, typed actions with owner grants, app export and import and the new phone layout to web, the IDE, Android and TestFlight, plus the fixes the release review found: a grant no longer survives a new app version from someone who is not an owner, two edits to one record keep both changes, and a chat no longer jumps to the top when its agent resumes.",
    body: r#"- **Apps**: a new version or a restore by a member who is not a team owner starts with nothing granted, until an owner grants again; previewing an older version in the editor runs it with nothing granted; two people editing the same record at once keep both changes; an import with a NUL character in it lists the problem instead of failing.
- **Chats**: an agent picking up its instructions no longer moves its chat to the top of the list.
- **Assignees**: choosing Unassigned in an older app version also clears an agent assignee.
- **Phones**: on iPhone, New board on an empty board creates the board in the team you are looking at; on Android, a large app import no longer crashes when you switch away, and app grants arrive right after the update.
- **Store builds**: Android 0.14.70 and the IDE 0.14.78 are live; iOS 0.14.68 is in TestFlight."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_84: ChangelogEntry = ChangelogEntry {
    id: "2026-10-25-app-export",
    date: "2026-10-25",
    title: "Share an app as one file: export and import",
    summary: "Export any app as a .app.json file, with its records if you like, and import one in the New app dialog by pasting it or choosing the file; anything wrong with it is listed line by line before you import.",
    body: r#"- **Export**: an app's … menu, its editor and its settings page export it as one file: its name, icon and colour, the current version and, with Include records switched on, its records.
- **Import**: New app → Import… takes the file pasted or chosen; every problem shows under the field as the place in the file and what is wrong, and Import creates the app as version 1, noted Imported, with its records as yours.
- **Grants stay home**: an imported app starts with nothing granted; a team owner decides on its settings page.
- **Everywhere**: the web downloads the file, the IDE saves it, iPhone and Android share it."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_83: ChangelogEntry = ChangelogEntry {
    id: "2026-10-24-app-actions",
    date: "2026-10-24",
    title: "Apps act as you: every Exponential operation, granted per app",
    summary: "An app calls any Exponential operation as a typed function (exponential.issues.update, exponential.comments.create, exponential.sessions.send, …); its settings page lists what it uses and you decide which run without asking; the rest ask with the operation's own words.",
    body: r#"- **Typed actions**: every MCP operation is a host function an app may call, run as you; the package lists what it uses and nothing else is reachable.
- **Grants**: Settings → Apps → the app shows a Tools group with one switch per declared action; a granted action runs without the consent card, an ungranted one asks each time, worded for the operation ("Allow this surface to set issue status APP-5?"). Only a team owner grants, and a destructive operation (delete, merge, close, end, remove, trash) can never be granted: it always asks, for everyone.
- **Working hours**: v5 posts a logged day to its issue as a comment through the typed action.
- **Platforms**: the grants sync to the IDE, iOS and Android, whose hosts honour them the same way."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_82: ChangelogEntry = ChangelogEntry {
    id: "2026-10-24-phone-shell",
    date: "2026-10-24",
    title: "Phones: one bar for every page, a board menu in the Board tab, search in your team",
    summary: "The phone bar is now Board · Chats · Apps · Team and you can slide a finger across it; the Board tab shows your board and opens a menu of the team's boards; every page has the same big title with search and inbox; each settings row opens its own page; search only looks in the active team and keeps up with your typing.",
    body: r#"- **Bottom bar**: Board · Chats · Apps · Team on iPhone, Android and the mobile web. Labels are no longer cut off, and you can slide a finger across the bar: the highlight follows it (Liquid Glass on iOS 26) and lifting picks the tab. Devices moved to a row on the Team page; Apps has its own tab.
- **Boards**: the Board tab shows the board you are on. Tap it again on the board and the bar turns into a menu of every board in the team, with New board at the end. The board pill at the top of the board is gone.
- **Headers**: Board, Chats, Apps, Team, Devices and Agent all have the same big title with round search and inbox buttons; the inbox shows a dot while something is unread.
- **Settings**: each row on the Team page opens its own page instead of one long page of every section, and iPhone, Android and the mobile web show the same rows to the same people.
- **Search**: searches the active team only, and typing no longer stutters on iPhone or Android."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_81: ChangelogEntry = ChangelogEntry {
    id: "2026-10-24-fast-chats",
    date: "2026-10-24",
    title: "Long chats open instantly, and the Chats list comes to the IDE and the web",
    summary: "Long chats open in a fraction of a second on every platform and stay light while open; the web loads only what you look at, and the IDE and wide web get the same Chats list as phones.",
    body: r#"- **Fast chats**: a chat shows its newest 100 messages and opens at once, however long it is; Load earlier above them brings the next 100 and keeps your place. An open chat no longer works in the background, so long agent chats stop stuttering and Android no longer runs out of memory on them.
- **Web**: the Chats list loads without downloading any transcript, and a chat loads only its newest messages when you open it, so the first load is much lighter.
- **Chats list**: the IDE and the web on wide screens show the same Chats list as phones: agent bubbles, filter chips and one row per chat with its last message, time and unread count.
- **Jump to bottom**: the jump button shows as soon as you scroll up in any chat, phones included, and takes you back to the newest message.
- **Working**: an agent chat ends on the same Working line as a session while the agent works."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_80: ChangelogEntry = ChangelogEntry {
    id: "2026-10-24-agents-as-themselves",
    date: "2026-10-24",
    title: "Agents act as themselves, stay in their team and take assigned issues",
    summary: "Agents now show under their own name and mark in an issue's activity and comments, work only inside their own team, can be assigned issues, and their chat says when its instructions changed.",
    body: r#"- **Agents as themselves**: what an agent does reads under its own name and mark: "Triage changed status to In Review", its comments, the issues it files and the notifications they send.
- **Assign to agents**: the assignee picker lists the team's agents after its members; assigning one nudges your chat with that agent and emits the new `issue.assigned` event for triggers.
- **Instructions changed**: after you edit an agent's instructions its chat shows a banner with Start new conversation; a new conversation restarts the agent so it runs on the new instructions.
- **Team boundary**: an agent and everything it starts work only inside the agent's team, on a key that reaches no other team."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_79: ChangelogEntry = ChangelogEntry {
    id: "2026-10-24-app-sources",
    date: "2026-10-24",
    title: "Apps read the team: boards, members, sessions, comments and files",
    summary: "An app binds exp:boards, exp:members, exp:sessions, exp:comments and exp:attachments beside a richer exp:issues, the same rows on every platform; the Working hours app shows a teammate's month and suggests your issues.",
    body: r#"- **Sources**: exp:issues now filters by board, status, priority, assignee (or me) and a search, sorts by list order or time; exp:boards, exp:members (with the viewer), exp:sessions, exp:comments and exp:attachments join it. One shared fixture pins the derivation on web, desktop, iOS and Android.
- **Options**: issue, board and member rows carry value and label, so an app binds them straight into a Select or a ChipInput.
- **Working hours**: v4 picks a teammate's month through the member picker and suggests your assigned issues when you log a day.
- **Catalog**: the apps catalog describes every source with its query and value, so the editor session binds them without guessing."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_78: ChangelogEntry = ChangelogEntry {
    id: "2026-10-24-app-records",
    date: "2026-10-24",
    title: "Apps keep their own data: records",
    summary: "An app declares record kinds in its manifest, reads them live through exp:records and writes them without a consent card; agents fill them through the apps MCP noun.",
    body: r#"- **Records**: an app's manifest declares its kinds with a small JSON schema; the server validates every write, every client syncs the rows, and the editor's preview shows them live.
- **Bindings**: exp:records?kind=... filters, sorts, sums and lays out a month; the new bind action re-follows a path from a button, so a month switcher is one action.
- **Agents**: exponential_apps gained records_list, records_upsert and records_delete, so an agent or another app can fill an app's data.
- **Quota**: Free teams keep 1,000 records, Team plans 25,000, one record at most 16 KB; the app's settings page shows the count.
- **Working hours**: the demo app that replaces a spreadsheet: a month view with a switcher, hours and notes per day, built through the editor and seeded as the Apps pictures."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_77: ChangelogEntry = ChangelogEntry {
    id: "2026-10-24-release-train",
    date: "2026-10-24",
    title: "Release 2026-10-24: Chats on every platform, with the train's fixes",
    summary: "This release ships Chats, the new phone layout, agent compact and new conversation, live previews and the Apps cleanup to web, the IDE, Android and TestFlight, plus the fixes the release review found: a chat's menu offers Delete only to who may delete it, a team switch on phones moves the board along, an agent's rename reaches its chats, and a moved agent's chats stay readable for the team.",
    body: r#"- **Chats**: the menu offers Stop and delete or Delete only to the chat's starter or a team owner; everyone else who can steer the chat gets Stop. In the IDE a preview or tool card posted before the loaded part of the chat sits at the top instead of under the newest reply, and deleting a chat closes its child tabs too.
- **Phones**: switching team on Android moves the Board tab to that team's board, so a new issue files into the team you are looking at; the inbox dot counts the active team only; the agent chat's menu shows Agent settings for owners. On iPhone, opening an issue from a push or link switches to its team, a team without boards shows an empty board instead of the previous team's, and an inbox push opens the team it belongs to.
- **Agents**: renaming an agent renames its chats (a chat you named yourself keeps its name); an agent moved to another device keeps its chats readable for the team; a stop on an idle chat no longer says the device missed it; a newer plan no longer replaces a waiting plan the coding engine itself asked.
- **Web**: a brand-new chat waits for its conversation to arrive instead of flashing Untitled.
- **Store builds**: Android 0.14.69 and the IDE 0.14.77 are live; iOS 0.14.67 is in TestFlight."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_76: ChangelogEntry = ChangelogEntry {
    id: "2026-10-24-chats",
    date: "2026-10-24",
    title: "Chats: a WhatsApp-like phone app, stop and delete from the list, agents that compact and start over, and live previews in the chat",
    summary: "Threads are now Chats everywhere. Phones get a new bottom bar, a chat list with agent bubbles, chat bubbles and a Team tab; every platform can stop or delete a chat from its row, compact an agent or start it fresh, and agents can draw live previews right into the chat.",
    body: r#"- **Chats**: threads are called chats on every platform. On phones the bottom bar is Board · Devices · Chats · Team, and the Chats tab lists your active team's chats like a messenger: agents as bubbles on top (a ring when they need you or wrote something new), filter chips, and one row per chat with its last message, time and unread count.
- **Stop and delete**: right-click a chat (long-press on phones) to Stop it or Stop and delete it; an ended chat offers Delete. The Stop button in a chat's header now ends the chat, and a stop the device never received says so.
- **Agents**: Compact conversation and New conversation in an agent chat's menu, or type `/compact` and `/new`; both leave a marker in the chat.
- **Live previews**: agents can draw an interactive preview (charts, stats, buttons) straight into the chat, and it animates in as it arrives.
- **Plans**: a waiting plan always shows in the chat with its buttons, and a newer plan replaces the waiting one.
- **Phones**: one active team at a time, switched on the Team tab, which also holds the settings; My issues and Drafts are chips on the board, and the inbox has its own page.
- **Triggers**: agents can wake every 5, 10, 15, 20 or 30 minutes; hourly and shorter schedules no longer show a time zone, and the minute and hour lists scroll."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_75: ChangelogEntry = ChangelogEntry {
    id: "2026-10-24-no-built-in-apps",
    date: "2026-10-24",
    title: "Apps lists only your team's apps",
    summary: "The built-in Devices app is gone: Apps, the sidebar's Apps group and the IDE rail list only the apps your team makes.",
    body: r#"- **Apps**: Apps lists only your team's apps; the built-in Devices app is gone. A team without apps sees the empty list. The Devices page stays where it was."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_74: ChangelogEntry = ChangelogEntry {
    id: "2026-10-23-conversations-on-every-client",
    date: "2026-10-23",
    title: "Every app reads threads through conversations, and actions are retired",
    summary: "Web, the IDE, iOS and Android now load a thread's messages through its conversation and stop syncing the retired actions, ready for the server cleanup that follows.",
    body: r#"- **Threads**: messages load through their conversation on every platform, so direct messages and channels can follow without another app update.
- **Actions**: the apps no longer sync actions (prompts replaced them); an action pinned in the sidebar disappears.
- **Devices**: moving a session to another device takes effect at once; the Moving to pin and its cancel are gone.
- **Session cards**: the old caption line under a session's name is gone."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_73: ChangelogEntry = ChangelogEntry {
    id: "2026-10-23-threads-follow-ups",
    date: "2026-10-23",
    title: "Threads: the Conversation tab on every thread, filed issues stay where they are, and pickup after an update",
    summary: "Every thread's strip shows Conversation again, which on a one-session thread is that session's own work; an issue a session files keeps its status until a session takes it up; a session whose device updated itself under it picks up its waiting message; and the pull request tools find an issue's own pull request.",
    body: r#"- **Conversation tab**: every thread's strip starts with Conversation. On a thread with one session it is that session's work with its composer; the first child session turns it into the thread's conversation as before. On web, the IDE and Android; on iPhone in the TestFlight build.
- **Filed issues**: an issue a session files is linked to its thread but keeps its status. It moves to In Progress only when a session takes it up: a start, a forward, a link or your message into the session that holds it.
- **Pickup after an update**: a thread whose desktop app or daemon updated itself mid-turn picks up its waiting message instead of sitting idle with it.
- **Pull request tools**: an issue's pull request resolves to the one opened by the session that holds the issue, so the MCP pr tools no longer refuse a choice they can make."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_72: ChangelogEntry = ChangelogEntry {
    id: "2026-10-23-release-train",
    date: "2026-10-23",
    title: "Release 2026-10-23: the Dock, Apps and the inline pickers on every platform, with the train's fixes",
    summary: "This release ships the Dock, the Apps editor, the inline pickers and the Exponential UI layout components to web, the IDE, Android and TestFlight, plus the fixes the release review found: a refused launch ends its thread, a closed Dock sheet stays closed, editor sessions stay out of issue threads, and the app host's private-range filter covers CGNAT and the IPv6 forms that embed an IPv4.",
    body: r#"- **Threads**: a thread whose launch was refused ends instead of waiting idle and nudging its device every minute; a message to an ended thread still resumes the same session.
- **The Dock**: a sheet you closed stays closed when its card comes back later (the Tasks sheet no longer reopens on the next run's first todo); on phones the transcript keeps its room above the composer. Android's Connection lost banner has its Reconnect button.
- **Apps**: an app survives its creator leaving the team (ownership passes to the owners), a trashed app's slug can be reused, and a version written from the editor session names its author. An app editor session no longer shows up as an issue thread or in the IDE's Running list.
- **App host**: the private-range URL filter also denies carrier-grade NAT (100.64/10) and the IPv4-compatible, NAT64 and 6to4 IPv6 forms, and the web renderers re-check a redirect's target before reading it.
- **Store builds**: Android 0.14.68 and the IDE 0.14.76 are live; iOS 0.14.66 is in TestFlight."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_71: ChangelogEntry = ChangelogEntry {
    id: "2026-10-23-exponential-ui-ergonomics",
    date: "2026-10-23",
    title: "Apps: live counts and icons, six layout components, and better defaults",
    summary: "App packages bind a count, an icon, a tone or a selection to their data, build screens from Page, Toolbar, SplitView, FieldGroup, FormActions and StatTile, and get centred control rows and the Section's own spacing without tuning.",
    body: r#"- **Live data props**: a Section count, a row's icon, chevron and selection, a chip's tone, an image's alt text, a slider's range and more now take a binding, so an app's counts and states follow its data.
- **Layout components**: Page, Toolbar, SplitView (list and detail, one at a time on phones), FieldGroup, FormActions and StatTile join the catalog, drawn from the existing parts on every platform.
- **Better defaults**: a horizontal Stack centres its controls unless told otherwise, a Section keeps one small gap under its band, and the built-in Devices app shows its device count in the band.
- **Agents**: the catalog prompt names the layout components, asks for one primary button per screen and for List rows instead of Markdown bullets."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_70: ChangelogEntry = ChangelogEntry {
    id: "2026-10-22-exponential-apps",
    date: "2026-10-22",
    title: "Exponential apps: build small apps with the editor session, previewed live and kept in versions",
    summary: "A team's Apps are declarative Exponential UI packages rendered inside every client; the editor shows your chat with the app's own session beside a live preview, every published package is a version you can view and restore, and agents publish apps over the new exponential_apps tool.",
    body: r#"- **Apps**: the sidebar gains an Apps section drawn like Agents and Boards, the built-in Devices app first; the plus names a new app and opens the editor, the hover gear opens its settings page under Settings → Apps. Phones keep the Apps list behind the Threads header button.
- **The editor**: your chat with the app's editor session sits beside the live preview; the first message starts the session on your device, leaving the editor ends it, the next message continues it. Phones switch between Chat, Preview and Versions.
- **Versions**: every package the session publishes is a version with a note; open Versions to preview an older one and restore it as the new head. No git involved.
- **MCP**: the sixteenth tool, exponential_apps, lists, reads, publishes and deletes apps and hands an agent the catalog of components, bindings and host functions a package may use.
- **SDK**: the Exponential UI package API is named for apps (AppPackage, createAppHost); the dev-only Devices template route is gone, the Devices package is the built-in app."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_69: ChangelogEntry = ChangelogEntry {
    id: "2026-10-22-the-dock",
    date: "2026-10-22",
    title: "The Dock: one scroll, floating cards above the composer, and plans that open as a sheet",
    summary: "A session's transcript is the one scroll again; what waits on you (a plan, a question, the task queue, queued messages, a usage wall) floats above the composer as cards that pile like toasts, and a plan opens as a bottom sheet you can scroll, with Yes and No, keep planning in its footer; the composer is one row on every platform.",
    body: r#"- **One scroll**: the transcript runs from the header to the bottom edge; nothing splits the session view into panes any more, and the plan is never cut off below the window.
- **The Dock**: a plan, a question, the suggested tasks, the task queue, the queued messages and the status banners (offline, connection lost, usage wall, compacting, rate limit) are floating cards above the composer; several pile like the toasts and fan out on hover or tap; the composer stays put. Suggested tasks open as a sheet with Start and a small menu per task.
- **One gray**: every card, the composer and the sheets wear the same opaque card tone on every platform.
- **Plans open as a sheet**: a tap (or a plan arriving while you watch) opens a bottom sheet with the plan, one page per question, and the decision in its footer: No, keep planning (with your feedback) or Yes (its menu starts with a fresh context).
- **One composer row**: the + menu is the only tool; the device glyph, the model pin and Plan sit in the card and the usage ring beside Send, on web, the IDE and the phones alike.
- **Exponential UI**: DockCard joins the catalog as a one-row card that opens its children in a bottom sheet with a pinned footer."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_68: ChangelogEntry = ChangelogEntry {
    id: "2026-10-22-threads-survive-a-quit",
    date: "2026-10-22",
    title: "A thread survives an app quit, and a message to an ended thread resumes it",
    summary: "Quitting the desktop app or losing a thread's process no longer ends the thread: it waits, and your next message resumes the same session with its context. A thread you ended on purpose resumes the same way instead of starting a copy.",
    body: r#"- **Threads survive their process**: a thread's own session goes idle when the app quits, the daemon stops or the agent crashes, exactly like an agent's conversation; only End or the agent's own close ends it. Sub-sessions still end with their process and report to their parent.
- **Resume, not copy**: a message to an ended thread resumes that same session (its branch, title, issues and context) instead of starting a one-turn copy that ended after its reply, left your answer unread and showed an identically named session waiting forever.
- **Desktop quit**: ⌘Q suspends the sessions it hosts the way the daemon already did on stop; their worktrees stay where the agent left them for the resume."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_67: ChangelogEntry = ChangelogEntry {
    id: "2026-10-22-new-session-system",
    date: "2026-10-22",
    title: "Release 2026-10-22: the new session system, agents with their own conversations, prompts instead of actions, and 15 MCP tools",
    summary: "Every thread is one session that works itself and is woken by messages; agents are definitions that run as a conversation per member with server-fired triggers and an events stream; prompts replace actions in the / menu and Settings; the MCP surface is 15 noun tools with one runbook; devices re-claim their sessions after a restart and clean up their worktrees.",
    body: r#"- **A thread is its session**: the root works the task itself, no dispatcher; children are started only when the work calls for them, every child transition lands in the parent as a waking message, and a one-minute watchdog flags a session that stalls. The plan card asks one question at a time, with a stepper.
- **Agents**: an agent is a definition (name, icon, instructions, device, model) and runs as its own conversation per member; triggers fire from the server on a schedule or on team events, and the Agents page shows the Events stream. Message an agent from the Home composer's + menu; the reply opens in your DM with it.
- **Prompts replace actions**: a prompt is a name, an icon and a body; it lives in the / menu of every composer and under Settings → Prompts. Inputs, picks and per-action triggers are gone (triggers belong to agents). Existing actions were converted.
- **15 MCP tools**: one tool per noun (issues, comments, sessions, pr, ...) taking an op, plus sessions_send and guide_write; one runbook under 6 KB written around the bus (end your turn, you are woken by messages). An op you lack errors instead of vanishing.
- **Devices**: a restarted daemon or IDE re-claims every session its dead process left behind and continues a turn that was cut mid-way; a session's worktree is removed when it ends (uncommitted work becomes a WIP commit on an exp/leftover branch); repo-less runs are confined to their scratch folder; the dispatcher checkout and the handoff package are gone.
- **Phones**: Android and iPhone follow with the same screens (threads, agents with DMs, Events, Settings → Prompts, prompts in the / menus). The iPhone build you have keeps working until the new one is live; the Android update is required."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_66: ChangelogEntry = ChangelogEntry {
    id: "2026-10-21-release-train-pre-cleanup",
    date: "2026-10-21",
    title: "Release 2026-10-21: a faster iPhone app, moves that clean up after themselves, an unlink tool for pull requests, and the review fixes",
    summary: "The iPhone app stops syncing while idle and does far less work per keystroke, a moved session's source cleans up its worktree once the target has restored it, agents can detach a mislinked issue from a pull request, and the release review hardened computer use and the move flow.",
    body: r#"- **iPhone, faster**: the idle sync drumbeat is gone, lists, the session view and the editors do far less work per keystroke and per sync tick, and the Threads title sits in the sticky bar like every other tab.
- **Moves clean up**: the source device keeps a moved session's worktree and branch until the target reports the restore, then removes them; a move keeps its index, drops the unpacked package on any outcome and consumes owed messages once.
- **Unlink from a pull request**: the exponential_pr_unlink tool detaches one mislinked issue from a pull request; the issue's status stays as is and the thread keeps the issue.
- **Android capture lanes** bundle the Exponential UI FFI, so the plan card's Preview renders in the store and gallery shots.
- **Review fixes**: the computer-use Esc abort is answered only by your next message, never by a turn the agent opens itself, the X11 backend keeps an Escape on a keymap without one and reads back wheel releases, the Windows hook start no longer waits forever, a week-old package no longer expires a move you just requested, a restore marker from a later move no longer deletes the first source's recovery copy, a moved package with staged changes still restores on a device that has not updated yet, a board that is still syncing no longer flashes the "Syncing team" banner on iPhone, and a renamed status reaches the #EXP chips at once."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_65: ChangelogEntry = ChangelogEntry {
    id: "2026-10-19-computer-use-foreground",
    date: "2026-10-19",
    title: "Computer use works in the foreground, with your input held and Esc to take it back",
    summary: "An agent now drives the screen the way you would, in front of you: while it works your keyboard and mouse are held, a banner says so, and Esc hands them back and stops the turn. The background mode and its second pointer are gone.",
    body: r#"- **Foreground only**: the agent moves the real pointer and types on the real keyboard, one display at a time, the same way on Linux (X11), macOS and Windows.
- **Your input is held**: while a session drives, your keys and clicks reach no window, and the banner at the bottom of the screen names the session.
- **Esc takes it back**: Esc releases your input at once, stops the session's turn and tells the agent; your next message lets it continue.
- **Gone**: the background mode, the second pointer on X11, the agent cursor overlay and the linked driver. A Wayland session says so in the device doctor; sign in with the X11 session to use it."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_64: ChangelogEntry = ChangelogEntry {
    id: "2026-10-19-release-train-pre-cleanup",
    date: "2026-10-19",
    title: "Release 2026-10-19: sessions with real names, the Ask dock and plan card, moving a session between devices, and the review fixes",
    summary: "Every session carries a real name on every device, a waiting question or plan takes the composer's place, a running session moves to another of your devices with its worktree, quick replies sit above the composer, and the release review fixed what those changes left open on older apps.",
    body: r#"- **Sessions name themselves**: every session, tab and row shows a real name instead of "Session 2", and a thread's dispatcher groups related issues into one session, on web, IDE, iPhone and Android.
- **Ask dock and plan card**: a waiting question or plan sits where the composer was, several stack up and open one at a time, and a plan reads as one rich card with pictures, choice questions and an inline Exponential UI surface, answered in the card on every platform.
- **Move a session**: the Device pin in a session's options moves a running session, with its worktree and conversation, to another of your online devices; the source keeps its worktree and branch as the recovery point until the move is fully restored.
- **Quick replies**: short answers the agent suggests sit above the composer and send with one tap.
- **Threads**: thread rows drop the private lock and the merged word, the Threads tab carries the live dot, the Inbox tab shows a dot while you have drafts on phones, and the session page lives inside its thread with one Stop in the header that hands queued messages back to the composer.
- **Issues and pull requests**: follow-up work moves an issue back to In Progress and its end returns it to In Review, issue-targeted PR tools resolve to the issue's own PR, and a Guide remove reaches the caller's subtree.
- **Accounts**: switching accounts keeps the model, shows itself in the session, and names accounts by email.
- **Review fixes**: a move keeps the source's worktree and branch until the target has restored it, a re-targeted or cancelled move never loses the uploaded package, a plan approved with a fresh context starts fresh even after the daemon restarted, a refused Stop puts the queued text back exactly once, the session transcript on iPhone shows the agent's own plan and question cards, a plan card's links and media open nothing, moving to a device on an older build is refused with an update note on every platform, and an agent on an older desktop build asks inline instead of posting a card it could never see answered."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_63: ChangelogEntry = ChangelogEntry {
    id: "2026-10-19-cli-cargo-target-gc",
    date: "2026-10-19",
    title: "Command line devices keep the shared build cache bounded",
    summary: "Devices running only the exponential command line daemon now trim a repository's shared Rust build cache once it grows past 20 GiB, like the desktop app already does.",
    body: r#"- **Same rule as the desktop**: once a repository's shared cargo build cache passes 20 GiB, build units untouched for three days are evicted, plus examples and incremental data.
- **Safe while you build**: a running build keeps its profile untouched, built binaries are never removed, and each repository is checked at most once an hour."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_62: ChangelogEntry = ChangelogEntry {
    id: "2026-10-18-follow-up-in-progress",
    date: "2026-10-18",
    title: "Follow-up work moves an issue back to In Progress",
    summary: "When a session starts follow-up work on an issue in review, the issue moves back to In Progress, and returns to In Review once the work ends with its pull request still open.",
    body: r#"- **Back to In Progress**: a new sub-session, or a message to a working sub-session, in a thread whose pull request is open moves its In Review issues back to In Progress.
- **Back to In Review**: once no session in the thread is still working and the pull request is still open, the issue returns to In Review on its own.
- **Your status wins**: an issue you set by hand is never moved, and a status you change while the work runs stays where you put it.
- **Same switch**: a team that turned off the pull request opened automation gets neither move."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_61: ChangelogEntry = ChangelogEntry {
    id: "2026-10-17-quick-replies",
    date: "2026-10-17",
    title: "Quick replies above the composer",
    summary: "When an agent finishes a step it can offer its likely next messages as one-tap replies above the composer; tap one to send it, the others stay.",
    body: r#"- **One tap instead of typing**: an agent offers a few likely next messages, like "Raise the version numbers" or "Open the PR", as small buttons right above the composer of the session or thread.
- **Sends at once**: a tap sends that reply as your next message, even while the agent is still working; the other replies stay, so you can queue another right away.
- **Stays until replaced**: the replies stay across turns until the agent offers new ones; the × dismisses them.
- **Everywhere**: the same row on the web, the desktop app, iOS and Android."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_60: ChangelogEntry = ChangelogEntry {
    id: "2026-10-17-custom-plan-mode",
    date: "2026-10-17",
    title: "Plans you can see and shape",
    summary: "A plan now arrives as a visual card with pictures of the proposed UI, choice questions and a live preview, and you answer it in the card on every device.",
    body: r#"- **Visual plans**: a session in plan mode presents its plan with screenshots and design drafts inline, instead of a wall of text with a bare yes or no.
- **Choices, not just approval**: the plan asks what it needs to know as single or multiple choice, as text options or picture tiles, with a free-text answer where it fits; Yes waits until every question is answered, No keeps planning with your note.
- **Live preview**: an agent can draw the proposed screen itself as an Exponential UI surface inside the card; what you set in it travels back with your answer.
- **Rich questions anywhere**: outside a plan, an agent can ask one question the same way, with pictures and several picks.
- **Same on every device**: the card, the tiles and the answers look and work the same on web, iOS, Android and the desktop app, and a session cannot open a pull request before its plan is approved."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_59: ChangelogEntry = ChangelogEntry {
    id: "2026-10-17-move-session-device",
    date: "2026-10-17",
    title: "Move a session to another device",
    summary: "A running session moves to another of your devices with its worktree and conversation, and continuing or stopping a session no longer loses its place.",
    body: r#"- **Move to device**: the Device pin in a session's options moves it to another online device that can run its agent. It moves after its current turn, taking its worktree, unpushed commits and conversation along; logins stay on each device, and the session keeps the same account when the other device has it.
- **Moving…**: while a move waits, the pin reads "Moving to Studio…" with the other device's name; pick the current device again to cancel.
- **Continue where it ended**: a message to an ended sub-session continues it next to the original, on the same branch with the same issues, instead of starting over from the thread.
- **Stop always frees a session**: a session whose device went away no longer stays busy forever; Stop sets it idle, and its account shows in the session list."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_58: ChangelogEntry = ChangelogEntry {
    id: "2026-10-17-ask-dock",
    date: "2026-10-17",
    title: "Questions and plans take the composer's place",
    summary: "A waiting question or plan replaces the composer in a session and a thread alike, several stack up and open one at a time, and every plan offers the same three answers.",
    body: r#"- **One place to answer**: a question or plan waiting on you now sits where the composer was, in a session and in a thread alike, so a thread with one waiting card looks exactly like the session that asked.
- **Several at once**: when sub-sessions wait on you together, their cards stack above each other with the plan's first line and the session that asked; tap one to open it in place, answer it, or go back to the stack with the arrow.
- **Long plans**: a plan never stretches the conversation any more; it scrolls inside its card.
- **The same plan answers everywhere**: Yes, Yes and start with a fresh context, or No, keep planning with a note, from a thread as well as a session, on every device."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_57: ChangelogEntry = ChangelogEntry {
    id: "2026-10-17-session-names-grouped-work",
    date: "2026-10-17",
    title: "Sessions name themselves, and the dispatcher groups the work",
    summary: "Every session has a real name, never \"Session 2\", and a thread's dispatcher groups related issues into one session.",
    body: r##"- **Session names**: every session, tab and row shows a real name: the one it was started with, else the name the agent gives its own conversation, else its issues ("#EXP-12 Login fails on Safari + 1"), else its action, else the first line of its request. "Session 2" is gone everywhere.
- **Grouped work**: a thread's dispatcher hands related or dependent issues to ONE session and independent ones to separate sessions, and names each one. A newly linked issue goes to the session already doing related work, or to a new one.
- **Pull requests**: a session's pull request closes exactly the issues handed to it.
- **Agents**: a team agent that was running when its device went away starts again when the device comes back."##,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_56: ChangelogEntry = ChangelogEntry {
    id: "2026-10-17-threads-live-dot",
    date: "2026-10-17",
    title: "Threads: the live dot on the phone",
    summary: "On the phone the running-session dot sits on Threads, not the chat launcher.",
    body: r#"- **Threads dot**: on Android, iOS and the mobile web the green running-session dot now sits on the Threads tab, like the sidebar on web and desktop; the chat launcher no longer wears one."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_55: ChangelogEntry = ChangelogEntry {
    id: "2026-10-17-account-switch-keeps-model",
    date: "2026-10-17",
    title: "Account switching keeps your model and shows itself",
    summary: "Switching a session to another account, by hand or by auto-rotate, keeps the model you picked, and every device shows the switch while it happens.",
    body: r#"- **Model stays put**: a model you pick with /model stays when the session moves to another account, by hand or by auto-rotate, instead of falling back to the one the session started on.
- **You see the switch**: while auto-rotate checks your other accounts, the session reads "Checking other accounts…"; while it moves, its card spins with "Switching to you@example.com…".
- **Emails only**: the note a rotated session starts with names accounts by email, never an internal name.
- **No needless pushes**: a usage limit the device rotates past no longer notifies you."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_54: ChangelogEntry = ChangelogEntry {
    id: "2026-10-17-thread-header-one-composer",
    date: "2026-10-17",
    title: "Threads: a header that never scrolls, one composer, and options mid-session",
    summary: "A thread header that never scrolls, one composer, and options that change mid-session.",
    body: r#"- **Thread header**: nothing scrolls any more. "+" and Conversation sit on the left, Guide on the right, and issues then sessions fill the middle, shortening to identifiers and folding into "+N" when space runs out. Sub-issues fold under their parent issue and deeper sessions under their top-level session; click a folded chip to open its tree.
- **Link an issue**: the "+" left of Conversation offers Link an issue and New issue on every device; linking posts "Link #IDENT" so the thread hands it to the right session.
- **The thread is its dispatcher**: no extra session chip or card for it, and a sub-session's question or plan appears in the thread's conversation, answered right there.
- **One composer**: the thread, a session and a new start share one composer, "Type / for commands", with image, file, # and emoji in the box and the model, Plan and usage under it.
- **Options mid-session**: plan mode switches at once; effort, ultracode, computer use and MCP servers apply from the next message, in the same session.
- **Issue status**: linking an issue moves it to In progress; a pull request moves only its own session's issues to In review and, once merged into the default branch, to Done; a parent issue waits for its linked sub-issues."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_53: ChangelogEntry = ChangelogEntry {
    id: "2026-10-16-release-train-trace-queue",
    date: "2026-10-16",
    title: "Release 2026-10-16: the transcript as trace lines, the Queue, and the review fixes",
    summary: "A session's work reads as one trace line per step with its output as a code block, the bottom of a session shows what is left to do and what is done, /btw works on web, Android gains Auto-rotate accounts, and the release review kept thread renames working on older apps.",
    body: r#"- **Trace transcript**: every step in a session is one line, "Ran bun test", "Thought for 5s", "Ran the Explore agent" with its duration, shimmering while it runs and red when it failed; a command's output opens under its line as a code block with a copy button, coloured like its terminal output.
- **Failed tests**: a command whose output is a failing test run reads as its result, "Failed: 1 of 12 tests", instead of the command.
- **The Queue**: the bottom of a session shows what is left to do and what is done as two folds, the running work first, with a background task's elapsed time ticking.
- **Issue activity**: event lines on an issue use the same trace line, with the time on the right.
- **Review fixes**: /btw on web asks its side question as a short fork instead of sending it to the session, and a /btw fork can no longer change issues or pull requests; the thread's model switch is off once the thread ended on iPhone; Android gains the Auto-rotate accounts toggle; parallel tool calls read as running until each one settles; a sub-session's report lands exactly once; a shared thread is not marked unread for everyone the moment this release lands; and renaming a thread keeps working from Android 0.14.61 and the desktop 0.14.68 until they update."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_52: ChangelogEntry = ChangelogEntry {
    id: "2026-10-15-session-tree-talk",
    date: "2026-10-15",
    title: "Sessions talk back: reports, suggestions, live status and /btw",
    summary: "A sub-session's end lands as a report in its parent's conversation, agents propose follow-ups you start with one tap, every running session shows what it is doing right now, unread threads stand out and /btw asks a side question without interrupting.",
    body: r#"- **Reports**: when a sub-session ends, a Report line in its parent's conversation opens it, and the parent picks the summary up as its next turn.
- **Suggestions**: an agent can propose follow-up tasks as one card. Start one as a sub-session, do it in the same session, file it as an issue or dismiss it, or start them all at once.
- **Live status**: a running session's card shows its current step under the name, and sub-sessions nest under their thread in the Threads list.
- **Unread**: a thread with agent messages you have not read yet shows its title in bold, on every device.
- **/btw**: ask a running claude session a side question; the answer comes from a short fork and the session keeps working."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_51: ChangelogEntry = ChangelogEntry {
    id: "2026-10-15-thread-dispatcher",
    date: "2026-10-15",
    title: "Threads hand work to their own sessions",
    summary: "A thread now runs a small dispatcher on the thread model (Haiku by default): it answers quick questions itself and starts a session for real work, with the same reply box and model switch as a session. Every model pick is explicit.",
    body: r#"- **Threads**: starting a thread or a new agent runs a dispatcher on the thread model (agents created before this release keep running on their own model). Quick questions get answered in the thread, real work starts its own session with your model, effort and plan mode, and the dispatcher reports back in a line or two.
- **Reply box**: the thread and agent channel use the session's reply box on every device, with the model switch, the context ring, slash commands and Stop.
- **Thread model**: device settings gain a Thread model under Model for each agent, Haiku for Claude by default.
- **Models**: every model picker lists Fable, Opus, Sonnet and Haiku (Codex its three models) with no Default entry. Saved CLI defaults become Fable for the model and Opus for subagents."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_50: ChangelogEntry = ChangelogEntry {
    id: "2026-10-15-compat-cleanup",
    date: "2026-10-15",
    title: "Older apps and older servers retired",
    summary: "This release drops the server's compatibility paths for app builds older than the ones in the stores and on the desktop fleet, and every app now expects a server from the same release.",
    body: r#"- **Update your apps**: the oldest builds this server speaks to are iPhone 0.14.59, Android 0.14.61 and the desktop app or CLI 0.14.68. Older builds should update now; the update screen follows when the floors move.
- **Self-hosting**: upgrade the server before the apps, since new apps no longer work against an older server, and set the `CLIENT_MIN_VERSION_*` floors to the builds your users run (see `selfhost/.env.example`)."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_49: ChangelogEntry = ChangelogEntry {
    id: "2026-10-15-release-train-described-options",
    date: "2026-10-15",
    title: "Release 2026-10-15: described options everywhere, the phone dawn, and the review fixes",
    summary: "An agent's question options can carry a short description on web, IDE, iPhone and Android, Home on phones puts the composer on the dawn, a walled session switches accounts again, and the release review fixed sending from Safari and thread renames on older apps.",
    body: r#"- **Described options**: when an agent asks a question, each option can carry a short description under its label, on every device. Older phones and desktops still see the plain labels.
- **Phones**: Home puts the composer on the dawn at the bottom, and the floating reply box no longer folds away when you tap Send in Safari.
- **IDE**: New agent opens the Settings page like on web, animations repaint only on their own step, a thread never carries the Inbox beside it and pictures in thread messages load.
- **Accounts**: a session that hits its usage limit can switch to another signed-in account from its wall on every device, and switching on its own now also covers threads without a repository.
- **Members**: adding a person without an invite and inviting them at the same time no longer fails on the invite side.
- **Sign-in**: a personal API key the server rejects answers with a plain sign-in error instead of a server failure.
- **Older apps**: renaming a thread by hand keeps working from Android 0.14.61 and the desktop 0.14.69 until they update."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_48: ChangelogEntry = ChangelogEntry {
    id: "2026-10-15-add-without-inviting",
    date: "2026-10-15",
    title: "Add a teammate without inviting them",
    summary: "Settings → Members puts a person on the team by name and email without sending a link: assignable and mentionable right away, no seat, no mail, invited whenever you choose.",
    body: r#"- **Add without inviting**: next to Send invite, Members gains a button that adds the person to the team without mailing them. They can be assigned issues and mentioned at once, cost no seat and get no mail. The row reads Not invited until you use Send invite on it, and signing in or accepting that invite makes the account theirs.
- **Agents**: the exponential_members_add tool does the same, and exponential_members_list says which members have no account yet."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_47: ChangelogEntry = ChangelogEntry {
    id: "2026-10-14-threads-consistency",
    date: "2026-10-14",
    title: "Threads look the same everywhere: titles, header chips, lists and the Guide's pictures",
    summary: "A thread is named after its work, its header chips stand as tall as its tabs, lists drop their dividers, the Guide's pictures open with their notes and phones get a floating reply box.",
    body: r#"- **Thread titles**: a thread is named after its work, like Implement EXP-12 or Run Triage, or the name the agent gave it. Threads are no longer renamed by hand.
- **Header chips**: the issue chips in a thread's header are the same height as Conversation and Guide, and an issue opened in a thread shows only the issue, without its own Start button or bar.
- **Questions**: a question in a thread is the same card as in the session, so you can type your own answer there too, and a queued message has an X to take it back.
- **Lists**: every list shows its rows without dividers, and the rail's Active section keeps threads that wait on a merge.
- **Pictures and phones**: a picture in the Guide opens next to its topic's notes with arrows to the next one, and on phones the reply box floats over the conversation."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_46: ChangelogEntry = ChangelogEntry {
    id: "2026-10-13-threads-v2",
    date: "2026-10-13",
    title: "Threads v2: one list of your work, team agents, and one way to start",
    summary: "Threads replace Reviews and the session history: every piece of work is one conversation with its sessions as cards, team agents look after a board from their own device, and everything starts from the same composer.",
    body: r#"- **Threads**: the rail's Threads page lists your work as Needs you, Running and Recent. Reviews and the Agent page's history are gone.
- **One conversation**: a thread shows the people, the agent's replies and one card per session. Tap a card to see the session's raw work. Reply in the same composer you start sessions with.
- **Private or shared**: a thread started on a device shared with the team is visible to the whole team, who can reply, answer and stop it. A thread on your own device stays yours.
- **Team agents**: Settings → Agents adds agents like boards: a name, an icon, a color, a device, an account, a schedule and instructions. An agent waits for messages or its schedule and starts a thread for each piece of work. Agents sit in the rail above Boards.
- **One start**: the composer, Start on an issue, an action, an agent's schedule and a follow-up all start a session the same way, in the team you have open.
- **Actions are saved prompts**: an action is a name, an icon, a prompt and the things it asks for. Schedules moved to team agents.
- **Pull requests**: a thread can open several pull requests, and each one's body is the thread's Guide. Merging no longer ends a session."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_45: ChangelogEntry = ChangelogEntry {
    id: "2026-10-12-threads",
    date: "2026-10-12",
    title: "Threads: one place for a piece of work, its issues and its sessions",
    summary: "A thread holds the issues and sessions of one piece of work. The agent links the issues it files or picks up, the thread shows every session and link as one conversation, and lists show threads instead of single sessions.",
    body: r#"- **Threads**: every session starts in a thread. A session on an issue joins that issue's thread, a resume continues its thread, and anything else opens a new one.
- **The agent links its work**: the agent files new issues into its thread, links existing ones and detaches what does not belong. Each link shows as a row in the conversation.
- **One page**: the tabs across the top read Conversation, each linked issue, a plus button and Guide. On phones they are the pager.
- **Many sessions**: resumes continue the conversation, ended sessions fold to one line with Show work, and sessions one session started side by side share one card.
- **One PR, one Guide**: a thread's PR covers every issue in it by default, and its Guide is the PR body.
- **Menus**: link an issue, file a new issue into the thread, start a session in it or unlink an issue from the plus button or an issue's menu.
- **Lists**: Running and Recent show threads with their issue and session counts, and an issue names the threads it sits in."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_44: ChangelogEntry = ChangelogEntry {
    id: "2026-10-12-home-and-feedback",
    date: "2026-10-12",
    title: "The Agent page is home, one issue search, Getting started as a checklist, one way to wait and fail, and sessions",
    summary: "Every app opens on the Agent page with a greeting over the composer, # opens a real issue search with every match, Getting started is one short checklist, sessions started elsewhere no longer take over the desktop app, and loading and errors look the same everywhere.",
    body: r#"- **Home**: web, desktop, iOS and Android open on the Agent page. It greets you by time of day and asks what you want to build; nothing else sits under the composer. The suggestion rows are gone.
- **Tabs**: on the web and the desktop app the close button shows on hover, and a plus button after the last tab opens a New session tab that becomes the session's tab once it starts.
- **Issue search**: typing # opens a search box at the cursor (a bottom sheet on phones) that takes spaces and lists every match, not six. The plus menu's Implement issue and Run action open the same list beside the menu.
- **Getting started**: one checklist of eight steps in the order you need them, with a Docs button. The next step opens with one sentence on why and a button to do it. It shows on an empty board and on the Agent page of a team without boards; done steps stay clickable. Action suggestions are gone.
- **Starting a session**: after Start the composer shows the Exponential mark and where the session is starting until it lands. A session started from another device, a teammate or an agent no longer takes over the desktop app; it appears in Running with a toast to open it.
- **Loading and errors**: skeleton rows replace loading text, buttons show a spinner while they work, and a failed action is a toast with Retry. Mistakes in a form still show under the field. The team prompt saves as you type.
- **Session threads**: workflows, task lists and subagents share one Work card, a code-mode script shows the tools it called, and a comment the agent posted shows the real comment and opens it on the issue.
- **Polish**: the session thread's fades no longer tint or blur the card border, the desktop diff page scrolls as one page, and menus, popovers and buttons follow one set of motion rules.
- **Sessions**: what used to be called a run is a session everywhere: tabs, menus, toasts and the docs."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_43: ChangelogEntry = ChangelogEntry {
    id: "2026-10-12-issue-activity-mcp",
    date: "2026-10-12",
    title: "Issue history over MCP, and renames in the activity timeline",
    summary: "Agents can read who changed an issue's status, assignee, labels, priority, estimate or title and when, including whether a person, an agent run or an automation did it; renames now show in the timeline.",
    body: r#"- **Issue history for agents**: the new `exponential_issues_activity` MCP tool lists an issue's changes oldest first, each with the field, the old and new value, who made it, whether it was a person, an agent or an automation, the run behind an agent's change and the time. `since` returns only what changed after a point in time.
- **Renames in the timeline**: changing an issue's title adds a "changed the title to" row to its activity on web, desktop, iOS and Android, folded like other quick edits."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_42: ChangelogEntry = ChangelogEntry {
    id: "2026-10-11-one-empty-state",
    date: "2026-10-11",
    title: "One empty state, no refresh buttons, and a filter for the built-in tools",
    summary: "Every empty list on web, desktop, iOS and Android says the same short thing under the same glyph, refresh buttons are gone from lists that reload on their own, and the MCP settings show the built-in Exponential tools expanded with a filter box.",
    body: r#"- **One empty state**: every list, page and picker with nothing in it draws the same block on every client: a glyph, a short title and, on a page, one sentence and a button. The copy is shared across web, desktop, iOS and Android.
- **No refresh buttons**: lists that load from the server reload when you open them, when you come back to the tab and after every change; a small spinner in the list band shows a reload. Refresh stays only where the source is outside Exponential, like GitHub or the local disk. Pull to refresh on the phone issue lists is gone too: they are live.
- **MCP servers**: the built-in Exponential tools are always listed with a filter box, the explanatory line under the band is gone, and Add server is always there for owners.
- **Link previews**: an Exponential link pasted into Linear, Slack and similar tools shows the Exponential logo and the issue identifier instead of a blank link."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_41: ChangelogEntry = ChangelogEntry {
    id: "2026-10-11-compat-cleanup",
    date: "2026-10-11",
    title: "Older apps must update: the compatibility paths for pre-October builds are gone",
    summary: "The server now requires iOS 0.14.57, Android 0.14.59 and desktop or CLI 0.14.66, and the code that kept older builds working is removed.",
    body: r#"- **Older clients must update**: the server now requires iOS 0.14.57, Android 0.14.59 and desktop or CLI 0.14.66. An older app shows the update screen until it is updated.
- **Files for runs**: attaching a non-image file to a run works on every device; the "needs the device on 0.14.66 or newer" refusal is gone from web, desktop, iOS and Android.
- **Cleanup**: the automations table and shape that mirrored action triggers for pre-October apps, the empty workflow shapes, the retired GitHub claim tables and the old `exponential_sessions_results` tool name are removed. Nothing changes for apps on the required versions."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_40: ChangelogEntry = ChangelogEntry {
    id: "2026-10-10-accounts-by-email",
    date: "2026-10-10",
    title: "Accounts by email, Import your terminal login, and no duplicate logins",
    summary: "Every agent login is named by its email, runs only use logins added in Exponential, the device check offers to import your terminal login, and signing in as an account you already have refreshes it instead of adding it twice.",
    body: r#"- **One login per email**: account rows and pickers no longer carry names like "Claude Code account 2" or "Default"; a login is its email on every client.
- **No shared terminal login**: runs, usage and rotation only use logins added in Exponential, so switching accounts in your own claude or codex CLI never changes what a run uses. A run that used the terminal login resumes on your last used account.
- **Import**: when a device still has a terminal login, its device check offers Import next to Sign in. Import moves that login into Exponential (sign in again outside of it), on web, desktop, iOS and Android.
- **No duplicates**: signing in as an account the device already has refreshes that account, leaves the others untouched and says so in a warning."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_39: ChangelogEntry = ChangelogEntry {
    id: "2026-10-10-release-train",
    date: "2026-10-10",
    title: "Fix merge conflicts from a refused merge, Reviews across teams, and a polish round",
    summary: "A merge refused for conflicts offers the Fix merge conflicts run, Reviews names the issue of a pull request whose repository is connected in another team, stacked merges heal their base first, and rows, spacing and copy line up on every client.",
    body: r#"- **Fix merge conflicts**: when a merge is refused because the branch has conflicts, the Merge control opens the composer on the built-in Fix merge conflicts action with the pull request picked, on web, desktop, iOS and Android. The run rebases, resolves and merges; a pull request inside an open stack is pushed and left for Merge stack.
- **Reviews**: a pull request linked to an issue in another team no longer lists as "not linked to an issue", and a plain merge refused on a stack member explains that Merge stack lands it with the pull requests below it.
- **Stacks**: Merge stack heals a landing whose bottom pull request still sits on a merged base, a pull request that failed to join its GitHub stack still opens with a warning instead of an error, and a report topic's pull request link is matched whatever form the agent wrote it in.
- **Files for runs**: attaching a non-image file to a run needs the device on 0.14.66 or newer; images keep working with older devices, and the composer says so.
- **Polish round**: relative times read "5h" and "3d", status changes read "from Backlog to In Progress", list bands drop their counts, settings rows share one layout, and dozens of small alignment and copy fixes land on web, desktop, iOS and Android.
- **Android**: the Work screen no longer redraws every frame while idle (battery), and pictures an agent shows inline render under the transcript call.
- **iOS**: session text no longer overlaps and the doubled Discard button is gone.
- **Code mode**: scripts can no longer import files from the host machine, a runaway script is stopped at its deadline, and bulk tool calls run on a bounded worker pool.
- **MCP**: `exponential_sessions_get` waits up to 10 minutes for a run to go idle without the request timing out, and a chat run's report survives the client ending the row.
- **Older apps**: apps from before this release keep working."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_38: ChangelogEntry = ChangelogEntry {
    id: "2026-10-10-agent-links",
    date: "2026-10-10",
    title: "Links in agent replies: sources, runs and issues",
    summary: "Agents cite sources as links in their replies and Guides, a link to a run or issue on your instance opens inside the app, and a placeholder link opens nothing.",
    body: r#"- **Sources**: agents now cite documentation and references as inline links, and a link to another run opens it.
- **In-app links**: on web, desktop, iOS and Android a link in a run's thread, transcript or Guide that points at a run or issue on your instance opens it in the app instead of a browser tab. On iOS and Android every other web link opens in the in-app browser; on web and desktop in your browser.
- **Bare URLs**: plain URLs in the desktop transcript and the iOS thread and Guide are now clickable, and the iOS Guide shows issue pills."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_37: ChangelogEntry = ChangelogEntry {
    id: "2026-10-09-guide",
    date: "2026-10-09",
    title: "The Guide, runs as a conversation, and one Merge stack button",
    summary: "Changes and Results become one Guide with a diff per section, a run reads turn by turn with your messages, a stacked pull request merges with one Merge stack control, and the composer, lists and menus get quieter.",
    body: r#"- **Guide**: an issue's Changes and Results faces are now one Guide face on every client. Each section of the agent's report ends in a Changes row (files and line counts) that opens just that section's diff with a back button, files no section names land in Other changes, and Show complete diff sits at the bottom.
- **Runs as a conversation**: the run thread reads turn by turn, with a status row per turn and your own messages in between. Teammates still see the single status row.
- **Merge stack**: a pull request in an open stack has one Merge stack control that lands the whole chain, and hovering a member offers Merge through here. Merging a stack member on its own is refused with the list of what would land.
- **GitHub stacks**: an agent's pull request opened on another open pull request's branch joins its native GitHub stack, and the base is picked for it when the agent leaves it out.
- **Composer**: the + button opens one menu (implement an issue, run an action, add a file, effort, subagents, ultracode, MCP servers) with computer use switchable per run. Suggestions are quiet text rows under the box, and the Agent page shows a faint brand mark.
- **Lists**: run rows drop their chevrons and keep the run mark aligned at every depth, Reviews rows are one line without merge buttons (stacks draw as a rail), and list headers no longer carry counts.
- **Navigation**: a second sidebar shows only for the Inbox and the Agent page's recent runs, stepping through them reuses one tab, and the desktop keeps each team's tabs when you switch teams.
- **Drafts**: a draft's Files list no longer repeats its inline images, a picture attached with the paperclip stays a file, and the × on the New issue page is gone (leaving still asks what to do).
- **MCP**: `exponential_sessions_results` is now `exponential_sessions_guide`; the old name keeps working."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_36: ChangelogEntry = ChangelogEntry {
    id: "2026-10-08-code-mode",
    date: "2026-10-08",
    title: "Code mode: scripts that drive many tools at once, and a model for the screen",
    summary: "Every coding run gets a Code mode server whose scripts call the run's other tools in bulk and in parallel, computer-use tool calls run side by side, and a device setting picks the model the screen-driving subagents use.",
    body: r#"- **Code mode**: every coding run gets a `codemode` MCP server whose `codemode_exec` tool runs JavaScript that calls the run's other MCP tools in bulk and in parallel (Exponential, team servers, computer use); only the script's output comes back.
- **Computer use**: a run's tool calls now execute side by side, so a script can drive several windows at once.
- **Computer use model**: a device setting under Computer use picks the model the run's screen-driving subagents use; Haiku 5.5 by default for speed."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_35: ChangelogEntry = ChangelogEntry {
    id: "2026-10-07-release-train",
    date: "2026-10-07",
    title: "Runs open as a thread, drafts stay in step, and team actions bring their own MCP servers",
    summary: "A run opens as a status row over its results with Show work for the transcript, a draft created or discarded on another device follows you, an action carries the team MCP servers every run connects to, Agent leads the navigation, and Codex needs no CLI install.",
    body: r#"- **Runs read as a thread**: on web, desktop, iOS and Android a run opens as a status row (Building on your machine, elapsed time, the last tool line) over its results in publish order, with the agent's open question in place. Show work expands the full transcript and remembers your choice.
- **Drafts in step**: an issue created from a draft on another tab or device lands on that issue here, and a draft discarded elsewhere closes with a note, on every client.
- **Team actions with MCP servers**: an action lists the team MCP servers every run of it connects to. A member can share their own connection with the team (Share with team / Stop sharing in Settings, MCP servers), and the server spends it only for that action's runs.
- **Navigation**: Agent is the first entry on web, matching the desktop rail.
- **Codex**: the IDE and the CLI fetch a pinned Codex build on the first sign-in (checked against its published checksum); no CLI install and no doctor step.
- **Desktop**: right-click a run row or the transcript for its menu, and every confirm and choice prompt takes its words and button roles from the shared prompts fixture. Deleting an action asks first on every client.
- **Fixes**: merging or updating an agent's pull request that has no issue no longer fails with a GitHub error, and results pictures upload reliably when the file store is rate-limiting.
- **Linux**: the desktop AppImage now needs glibc 2.39 or newer (Ubuntu 24.04 and later); the CLI binaries are unchanged.
- **Older apps**: apps from before this release keep working."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_34: ChangelogEntry = ChangelogEntry {
    id: "2026-10-06-release-train",
    date: "2026-10-06",
    title: "Computer use, one dialog style, and drafts that ask before they leave",
    summary: "Agents can drive the screen from the IDE and the CLI, every confirm prompt looks the same on web, iOS and Android, leaving a draft with content asks what to do, a run reads Working until the agent is really done, and a session that started a run can read its transcript.",
    body: r#"- **Computer use**: a per-device switch (off by default) lets Claude see and drive the screen on macOS, Linux and Windows from the IDE and the CLI daemon. The doctor block on every client says whether the machine is ready.
- **One dialog style**: every confirm and choice prompt on web, iOS and Android uses the same card with the same wording, and Return never picks a destructive answer.
- **Drafts**: the × asks before discarding a draft, and leaving a draft with content asks Create, Keep as draft or Discard, on web, desktop, iOS and Android.
- **Runs**: a run keeps reading Working while the agent continues after a background task finishes, a live tool row shows only its headline, the run mark leads every list row, and the red needs-you dot is gone (an amber badge still marks a run waiting for you).
- **MCP**: `exponential_sessions_messages` lets the session that started a run read that run's transcript, `exponential_sessions_get` can wait for the run to go idle, and `exponential_sessions_end` is registered in every run (a person-started run calls it only when asked).
- **Account**: unlink a sign-in method as long as another way in remains; the email code no longer counts as one.
- **Phones**: the board picker no longer shows the other-teams dot.
- **Desktop**: the markdown editor keeps a paragraph's leading spaces and Tab works on images, and Claude runs show an Exponential tool's preview (chips, inline pictures).
- **Older apps**: apps from before this release keep working."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_33: ChangelogEntry = ChangelogEntry {
    id: "2026-10-05-release-train",
    date: "2026-10-05",
    title: "Agent first, and run pull requests you can read in the app",
    summary: "Phones open on the Agent page, tabs swipe sideways, a chat or action run's pull request opens its changes in the app, Actions and Drafts are top-level entries again, and Exponential's MCP server draws issue, run and inbox views inside MCP clients.",
    body: r#"- **Phones open on the Agent page**: web, iOS and Android land on the Agent page with the tab bar kept, and the chat button is lit while you are there.
- **Swipe between tabs**: Inbox, My Issues and Drafts page sideways like the issue tabs, and so do Actions and Suggestions and the action page on the phone web.
- **Run pull requests**: a pull request from a chat or action run opens the run's Changes tab on web, desktop, iOS and Android instead of GitHub. Teammates get a read-only view of the diff.
- **Navigation**: the More menu is gone on web and desktop. Actions and Drafts (while you have any) are top-level entries, actions wear a rocket, and the desktop footer has a Computer menu with Terminal, Files and Source Control.
- **Desktop sidebar**: the sidebar stays expanded everywhere and the app opens on Agent. Inbox, a Reviews detail and the Agent page's recent runs open a second list inside the content card; the Reviews list there includes pull requests from agent runs, and the file tree only shows when the diff keeps enough room.
- **Issue and run pages**: the properties stay in view under the header on every tab, the transcript fades into the composer, an arrow jumps to the newest message, the task list shows its progress, a plain × discards a draft, and Merge is a circle in the phone's bottom bar.
- **MCP Apps**: in MCP clients that support MCP Apps, Exponential's server draws the issue list with issue detail, runs, the inbox and devices as interactive views. New tools: `exponential_issues_show` and `exponential_devices_account_login`. An OpenClaw plugin ships in the repository.
- **Safer by default**: signing a machine's agent account in through MCP needs a full-access key, and a run's pull request diff is readable only when its repository belongs to the run's team."#,
};

/// An earlier head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_32: ChangelogEntry = ChangelogEntry {
    id: "2026-10-04-release-train",
    date: "2026-10-04",
    title: "Fixes on top of the one-path wave",
    summary: "Reporter replies reach the team, Reply to reporter switches off after each send, the GitHub connection stays signed in, phone lists span every team, and the issue title collapses into the header as you scroll.",
    body: r#"- **Reporter replies**: a widget reporter's answer reaches every team member when nobody is assigned to or following the issue, and the Reply to reporter toggle switches off after each send on web and desktop, so a follow-up note stays inside the team. A comment an agent sent to the reporter reads "to reporter · via MCP", and comments from people who left the team read "Former member" on every client.
- **GitHub connection**: refreshing the connection no longer signs you out when two screens refresh at once, the GitHub user token stays on the server (API keys cannot read it), Continue with GitHub only works when the instance enables it, and the connect page never leaves the site after the flow.
- **Pull requests**: an issue-less run's pull request keeps its base branch across a close and reopen, old links to the separate review page open the issue's Changes face, and opening an issue with an open pull request no longer asks GitHub for the diff on every visit.
- **Phones**: Reviews, runs, actions and devices list every team you belong to, grouped by team; the Actions tab replaces the More menu; the issue title collapses into the header as you scroll and content slides under a blurred edge on iOS, Android, web and desktop.
- **Runs**: a run shows a screenshot while it works, filed under its Results; queued merges report as queued; the Run face collapses to a status row when there is nothing live.
- **Joining a team**: the invite flow offers the device setup step when you own no device yet, on web, iOS and Android.
- **Emails**: report confirmation and resolution emails show the title as plain text, and the resolution subject no longer repeats it.
- **Android**: reporter comments an earlier version failed to show appear after updating.
- **Older apps**: apps from before this release keep working. App Store iOS and current Android builds do not show reporter comments until they update; older desktops' email settings save again."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS: ChangelogEntry = ChangelogEntry {
    id: "2026-10-04-review-on-the-issue",
    date: "2026-10-04",
    title: "Review the pull request on the issue",
    summary: "A pull request's diff is the issue's Changes face now, and Results reads as a numbered guide whose text is also the pull request's description on GitHub.",
    body: r#"- **One page**: a Reviews row, the issue's pull request row and the Related work rows open the issue on its Changes face, with the file tree beside it, Merge PR in the properties and GitHub in the header. The separate review screen is gone on web, desktop, iOS and Android.
- **Close PR**: closing a pull request without merging moved into the issue's actions menu.
- **Results as a guide**: the summary leads, then one numbered section per change with the files it touched and its screenshots. A file opens the Changes face on that file. An open pull request without a report shows its GitHub description.
- **Pull request description**: the agent's report is the pull request's description, so the two never disagree; screenshots stay in Exponential.
- **Phones**: Merge PR is the white button on the bottom bar again, on every face."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_1: ChangelogEntry = ChangelogEntry {
    id: "2026-10-04-run-status",
    date: "2026-10-04",
    title: "Run status at a glance",
    summary: "Every live run says what it is doing wherever it is named: Claude's spark spins while it works, amber waits on you, green has an open pull request, blue is done.",
    body: r#"- **Working**: while the agent works, its mark is Claude's own spark, spinning in the sidebar's Running rows, the run lists, the tabs and the Run face. Other agents pulse their mark.
- **Waiting on you**: an amber badge, also when the run already has a pull request open and asks again.
- **In review**: a green badge once the agent finished and its pull request is open.
- **Done**: a blue badge once the agent finished with no open pull request, or after the merge.
- **Follow-ups**: a follow-up on a run in review shows it working again and returns to green when the turn ends; the issue's status does not change."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_2: ChangelogEntry = ChangelogEntry {
    id: "2026-10-04-one-github-flow",
    date: "2026-10-04",
    title: "One way to connect GitHub",
    summary: "Connect GitHub, install the app, pick a repository: one guided page that every Start coding checklist, board form and settings section opens, listing the repositories you can push to.",
    body: r#"- **One flow**: Connect GitHub links your GitHub account to your Exponential account (any login works), installs the Exponential app on the accounts you choose, and lists the repositories you can push to. The team claim step, the install round-trip and the stale-access banners are gone.
- **Start coding**: the checklist's Connect GitHub fix opens that page over the issue; coming back ticks the row off.
- **Settings → Repositories**: your GitHub connection with its accounts and a Configure link per account, Install on another account, and Add repository from the live list or by name.
- **Self-hosting**: the GitHub App now needs its OAuth client id and secret, and its callback URL is the login callback; INSTALL.md walks through it."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_3: ChangelogEntry = ChangelogEntry {
    id: "2026-10-03-one-path-for-feedback",
    date: "2026-10-03",
    title: "One path for feedback",
    summary: "Every widget submission is an issue, you reply to the reporter from the comment composer, and their answers come back as comments and Inbox rows. The helpdesk and its Support inbox are gone.",
    body: r#"- **Widget submissions are issues**: whatever a visitor sends through the widget lands as an issue on the widget's board, with their screenshot, page context and contact attached. The widget has no modes any more.
- **Reply to the reporter**: the comment composer on such an issue carries a Reply to reporter toggle. Switch it on and your comment is emailed to the reporter, marked "to reporter" in the thread.
- **Reporter answers**: the reporter answers from the emailed link. Their reply shows up as a comment on the issue, marked "reporter", and reaches you as an Inbox row and a notification. A reply on a done issue reopens it.
- **Helpdesk removed**: the helpdesk, its Support inbox and the Support entries in the sidebar are gone. Old conversations were moved to issues on a Support board so nothing is lost."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_4: ChangelogEntry = ChangelogEntry {
    id: "2026-10-03-new-issue-page",
    date: "2026-10-03",
    title: "New issue is a page",
    summary: "New issue opens as the issue detail in draft mode on web, desktop, iOS and Android, saving a draft as you type until you press Create.",
    body: r#"- **New issue page**: the create dialog is gone. New issue opens a blank issue page with the same title, properties, description and files you know from the detail, on every platform.
- **Drafts as you type**: what you write is kept as a draft a moment after you stop typing, when you pick a property and when you leave the page. Come back from Drafts and continue where you stopped.
- **Create**: the Create button in the header files the issue and lands you on it. Discard draft in the menu throws it away.
- **Files on drafts**: paste or attach files before the issue exists; they move onto the issue when you create it."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_5: ChangelogEntry = ChangelogEntry {
    id: "2026-10-01-one-run",
    date: "2026-10-01",
    title: "One kind of run",
    summary: "Workflows are gone: a run plans and fans out its own work. Batches, stacked pull requests and the Related work badge keep working.",
    body: r#"- **Runs**: workflows are removed. A run splits its own work with subagents and starts further runs for the follow-ups it files, each on its own pull request.
- **Batches**: starting several issues as one batch run with one combined pull request works as before.
- **Dependent work**: a pull request can still be based on another one. Merge the first one first: its merge moves the ones built on it to the default branch.
- **Reviews**: one flat list of open pull requests. The Workflows group is gone.
- **Blocked issues**: starting a blocked issue asks Cancel, Start anyway or Stacked PR. A stacked start builds the whole line bottom-up: the lowest issue without a pull request starts first, and each run starts the next one on top of its own.
- **Header**: the Related work badge stays (blockers, batch, stack). A batch run's title also lists its issues.
- **Merging**: Merge on a stacked pull request asks Merge stack or Merge this pull request, and lands the stack bottom-up.
- **MCP**: the `exponential_workflows_*` tools and `stackOnIssueId` are gone. `exponential_pr_open` takes `base`, and `exponential_pr_merge` merges `issueIds` in the given order or a stack with `mergeStack`.
- **Older apps**: older apps keep working and show an empty Workflows page."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_6: ChangelogEntry = ChangelogEntry {
    id: "2026-10-01-release-train",
    date: "2026-10-01",
    title: "Triggers, accounts and connections, tightened",
    summary: "Fixes on top of today's changes: trigger switches that no longer undo each other, older apps that keep editing their automations, and steadier account picks, retries and desktop connections.",
    body: r#"- **Triggers**: two quick changes to an action's triggers no longer undo each other on web, desktop, iOS and Android, and an action that came out of the move with more than ten triggers stays editable.
- **Older apps**: an app from before the triggers release still pauses, edits, creates and deletes automations, and its Set as default on an account answers instead of failing.
- **Accounts**: a run started without an account skips a last used login that is signed out. A desktop app or CLI that had a default account keeps it as its last used login when it updates.
- **API errors**: the automatic retry stops after three failed attempts, even when each attempt got a few words out first.
- **Desktop connection**: slow requests no longer make the app rebuild its connection, apps do not all reconnect in the same second, and after wake the new connection is in place before sync restarts.
- **Phone tabs**: on Android the keyboard closes when you change the tab, and on iOS Stop and Resume stay correct after swiping between Run and Changes, the issue page runs to the screen edge again and Properties opens on the first tap.
- **Desktop**: the list beside an open issue or run is wider and its edge can be dragged, the run dialog of an action fits its content, and the compaction bar is smaller.
- **Stacks and batches**: one quiet badge in the header opens Related work with what blocks the issue, what shares its pull request and its stack. Closed rows are dimmed on desktop too."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_7: ChangelogEntry = ChangelogEntry {
    id: "2026-10-01-action-triggers",
    date: "2026-10-01",
    title: "Actions have triggers",
    summary: "Automations are now triggers on the action they run: one Actions list, and each action is a page with its prompt, its triggers and its runs.",
    body: r#"- **Triggers**: a schedule or an event that starts an action now lives on that action. Your automations became triggers on their action and keep firing on the same device, with the same account, model and effort.
- **Action page**: opening an action shows its prompt, its triggers and its runs, as sections of one page on desktop and as tabs on a phone. The Automations page and tab are gone.
- **Actions list**: a clock marks an action with a schedule, a bolt one that an event starts. The mark is dimmed while that trigger is paused.
- **Runs**: an action lists every run of it, the triggered ones marked. It replaces Recent automated runs.
- **Tidy up**: a team that had scheduled Tidy up now owns it as an action with the same prompt, free to edit.
- **MCP**: `exponential_actions_update` takes `triggers`. The `exponential_automations_*` tools are gone.
- **Older apps**: apps from before this release keep firing their triggers and still pause, edit and delete them as automations."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_8: ChangelogEntry = ChangelogEntry {
    id: "2026-10-01-last-used-account",
    date: "2026-10-01",
    title: "Last used account",
    summary: "There is no default account any more: every start picks the account you last used on that device.",
    body: r#"- **Last used**: the composer on every client starts on the account you last started or switched a run on, per device, and a run that names no account (an automation without a pinned account, a CLI or MCP start) runs on it too.
- **Removed**: the Default account row in Device settings and Settings > Agents, and Set as default in a device's account menu.
- **Unattended runs**: automations, workflows, agent-started runs and automatic account rotation never change it."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_9: ChangelogEntry = ChangelogEntry {
    id: "2026-10-01-runs-recover",
    date: "2026-10-01",
    title: "Runs recover from API errors and dead connections",
    summary: "A run whose turn died on an API error retries by itself, a child run that switches accounts is never resumed twice, and the desktop app drops a dead server connection instead of showing cached data for minutes.",
    body: r#"- **API errors**: when a turn stops on an API outage, a timeout or a dropped connection, the run sends its own "continue" after 30 seconds, then 2 and 10 minutes, and says in the transcript that the retry was automatic. Three failures in a row stop the retries, and Stop cancels a pending one.
- **Accounts**: a new run counts the runs already live on each account, so two runs started close together go to different accounts instead of sharing one usage window. A run you start on a named account stays on it. The rate-limit banner says the limit is per account on web, desktop, iOS and Android.
- **Child runs**: a child run that its machine moves to another account tells its parent that it resumes itself and under which new id, instead of "ended without a report". Resuming a run that is already live again under another id is refused and names that run.
- **Desktop connection**: the desktop app and the CLI detect a dead server connection within seconds, rebuild their connection after repeated failures, on wake and on Retry, and log the cause of a failed request."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_10: ChangelogEntry = ChangelogEntry {
    id: "2026-09-30-phone-face-pager",
    date: "2026-09-30",
    title: "Phone tabs follow your finger",
    summary: "Issue, Run, Changes and Results on the phone drag like native tabs on web, iOS and Android, and the Changes tab shows the diff counts.",
    body: r#"- **Dragging**: the Issue, Run, Changes and Results faces of the phone's Work screen move with your finger and settle on the neighbour, as a native pager on iOS and Android and as a slide on web. Tapping a tab animates the same way. Before, the face switched only after the swipe ended.
- **Changes counts**: the Changes tab reads the diff's added and removed line counts, as the desktop app does, once the run's diff or the pull request's files are known.
- **Styleguide**: the face strip has its own entry, Work face tabs, so every client draws the same control."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_11: ChangelogEntry = ChangelogEntry {
    id: "2026-09-30-api-errors",
    date: "2026-09-30",
    title: "API errors are not rate limits",
    summary: "An API outage or dropped connection inside a run shows as a warning line in the transcript instead of a rate limit and never switches the account; the banners under the transcript sit in the reading column.",
    body: r#"- **API errors**: when the agent's API times out, drops the connection or rejects a request, the run shows the error as a warning line where it happened. It no longer reads as a rate limit, no longer marks the run blocked and no longer resumes it on another account.
- **Banners**: the rate-limit, paused, connection-lost and compaction banners under the transcript sit in the same centered column as the transcript rows on web, desktop, iPad and Android tablets."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_12: ChangelogEntry = ChangelogEntry {
    id: "2026-09-30-release-train",
    date: "2026-09-30",
    title: "Release train 2026-09-30",
    summary: "Runs report on Results and notifications open them, tall screenshots and markdown attachments preview in-app, one stacking toast everywhere, yolo mode, a Tidy up action, editable pull request descriptions, and honest session tools for agents.",
    body: r#"- **Results**: a run's close-out report sits beside its screenshots on the issue's Results, visible to every member, and a notification an agent sends can open it directly. Runs publish screenshots of their visible changes by default. A tall screenshot shows a framed tile and opens a scrolling viewer; Android pinches to zoom.
- **Markdown attachments**: .md files open in an in-app preview on the desktop app, iPhone and Android, with Download beside it.
- **Toasts**: one stacking toast on every platform, bottom-right on a pointer and top-centre on touch, tap to expand, swipe to dismiss. Alerts that only informed became toasts; destructive actions still confirm.
- **Yolo mode**: an owner switch under Settings › General. A pull request an agent opens merges at once, a follow-up tree merges root first once every run in it is done, and Reviews leaves the navigation unless a merge failed.
- **Tidy up**: a built-in action for non-destructive board cleanup (duplicates, labels, relations), the one builtin an automation can run, with matching icons for stacks, batches and workflows.
- **Pull requests**: agents update a pull request's title or description with exponential_pr_update, and Reviews shows the description with Edit while it is open. A merge that GitHub answered with a server error after landing reads as merged, and an issue moved across boards mid-run still resolves.
- **Session tools**: a failed remote start names its reason, a message reports whether the agent consumed it, queued messages reach the agent together, and a run you stop keeps its uncommitted work as a WIP commit on its branch.
- **Fixes**: a signed-out default agent login no longer blocks a signed-in named profile, MCP credentials reach only the run that picked them, an invite to a still referenced placeholder binds and merges on accept, a sub-issue composed with an attachment leaves no draft behind, the desktop app's Files and Source Control no longer flash "No repository linked" on cold start, stacked issue chips draw as outlines, and the web onboarding wizard shows who is signed in with a Sign out."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_13: ChangelogEntry = ChangelogEntry {
    id: "2026-09-30-phone-work-tabs",
    date: "2026-09-30",
    title: "Issue, Run, Changes and Results are tabs on the phone",
    summary: "The phone's Work screen shows its faces as swipeable tabs in the header, with Merge beside them while the pull request is open, on web, iOS and Android.",
    body: r#"- **Tabs**: Issue, Run, Changes and Results sit in one segmented strip inside the header, under the title, the same control as the Inbox strip, listing only the faces the issue has. Tap one or swipe the body left or right to move between them. The bottom-right switcher circle is gone.
- **Runs**: with several runs on an issue the strip reads Runs, and the run menu opens from it.
- **Merge beside the tabs**: while the pull request is open, the Merge button sits next to the tabs on every face, so a run can be merged without leaving it. It left the properties card and the Changes bar.
- **Bars**: the Issue face keeps Properties, Comment and Start coding; Changes keeps the file list; the Run face offers Start coding once a run has ended for good; Results has no bar."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_14: ChangelogEntry = ChangelogEntry {
    id: "2026-09-30-sign-in-methods",
    date: "2026-09-30",
    title: "Sign-in methods and a changeable email",
    summary: "Settings › Account lists every way into your account: change your email with a code, link or unlink Google, Apple and OIDC logins, and manage passkeys, on web, desktop, iOS and Android.",
    body: r#"- **One primary email**: it starts as the address you registered with (the Google or Apple address for those sign-ups) and Change under Settings › Account swaps it for any other after a code sent to the new address. Sign-in codes, notifications and new mentions use it from then on; earlier @mentions keep the address they were written with.
- **Sign-in methods**: Google, Apple and every OIDC provider your instance offers appear with Link or Unlink, a password row shows while one is set, and passkeys sit right below (moved back from Security, which keeps the API keys). Every method is optional, but the last way in can never be removed.
- **Natives**: the same list on the desktop app, iPhone and Android. Linking runs through the browser as sign-in does and returns to the app; passkeys are still added from the web.
- **Admins**: an email change never grants admin. INITIAL_ADMIN_EMAILS keeps promoting only the address an account registered and verified with."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_15: ChangelogEntry = ChangelogEntry {
    id: "2026-09-29-stack-merge-asks",
    date: "2026-09-29",
    title: "Merging a stacked pull request asks first",
    summary: "Merge on a pull request that belongs to a stack asks whether to merge the whole stack or this pull request only, on every client.",
    body: r#"- **Stacked pull requests**: the Merge button on the Changes face, the issue header and the run view asks first when the pull request is part of a stack with other open members. Merge stack lands every open pull request bottom-up; Merge this pull request lands it and the open ones below it, while the ones above stay open on the base branch. A pull request whose stack has nothing else open merges as before.
- **Agents**: the result of exponential_pr_merge on a stack member names the pull requests it landed below and the ones it left open, and never lists an already merged one."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_16: ChangelogEntry = ChangelogEntry {
    id: "2026-09-29-account-sign-out",
    date: "2026-09-29",
    title: "Sign agent accounts out",
    summary: "Every agent login under a device has a Sign out entry in its menu, and the machine's own login can be removed too.",
    body: r#"- **Devices**: every agent login listed under a machine offers Sign out in its "..." menu, on the web, in the desktop app and on iOS and Android. The machine's own login (the one the CLI in your terminal uses) can now be removed as well: it is signed out there and hidden until it signs in again. Removing never touches the account itself. Update the desktop app or CLI on the machine to pick this up."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_17: ChangelogEntry = ChangelogEntry {
    id: "2026-09-29-release-train",
    date: "2026-09-29",
    title: "Release train 2026-09-29",
    summary: "Start coding is always on an issue with a \"Ready to code?\" checklist that fixes what is missing, sub-issues and relations get their own bands, sign-up asks for your name, the CLI adds devices and sessions commands, and a new machine installs with one command.",
    body: r#"- **Start coding**: every issue shows Start coding on every client. When something is missing (GitHub, the board's repository, an online machine) it opens a "Ready to code?" checklist with one fix per step, including picking the repository right there; the bulk bar, the Agent composer and Getting started use the same checklist.
- **Issues**: a sub-issue names its parent above the title, sub-issues sit in their own band with a progress ring and a +, and every other relation folds into its own band. The stack, batch and runs badge shows on every face of a piece of work.
- **Sign-up**: signing up with an email code asks for your name instead of guessing it from the address.
- **Devices**: Add device shows an install command carrying a one-time token, so a new machine signs in, installs the daemon and turns on auto-update with one command. The CLI adds `exponential devices` and `exponential sessions` (list, show, log, message, kill) and starts runs on another machine with `--device`.
- **Desktop app and CLI**: fetches and pushes that stall stop and clean up after themselves instead of hanging, account rotation probes each agent once per beat and keeps an account you picked, and workflow node tooltips always say what the node is.
- **Self-hosting**: this release removes server support for iOS below 0.14.45, Android below 0.14.46 and desktop or CLI below 0.14.54. Set `CLIENT_MIN_VERSION_*` to those versions before upgrading (see `selfhost/.env.example`)."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_18: ChangelogEntry = ChangelogEntry {
    id: "2026-09-28-release-train",
    date: "2026-09-28",
    title: "Release train 2026-09-28",
    summary: "Plan-mode claude runs use their tools again, a run on an empty repository starts instead of hanging, the Recent runs panel has a back row, and phones wait for their issues instead of flashing an empty board.",
    body: r#"- **Agent runs**: a claude run in plan mode no longer falls into the auto-mode classifier that refused every tool call, because the desktop app and the CLI pin auto mode off for every run they launch (update the machine to pick it up). A run on an empty repository creates its first commit and starts instead of hanging.
- **Agent page**: the Recent runs panel on the web and in the desktop app wears the back row every side panel does, and the history button hides while the panel is up.
- **Phones**: after a fresh sign-in or a full resync a board shows a spinner until its issues arrive, instead of flashing "No issues yet" and the getting-started checklist, and the iPhone waits for its boards the same way."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_19: ChangelogEntry = ChangelogEntry {
    id: "2026-09-25-work-header-badge",
    date: "2026-09-25",
    title: "The work header badge",
    summary: "The stack, batch and runs badge in the work header stands as tall as the controls beside it, the runs badge wears the workflow glyph, and the web shows it too.",
    body: r#"- **One height**: the badge that says what a piece of work is part of (a stacked pull request, a batch, the runs around this one) now stands at the face toggle's height beside Stop, Resume and Merge, on the web and in the desktop app, instead of a small chip next to tall controls.
- **Runs badge**: on the Run face of a run that started or was started by other runs, the badge wears the workflow glyph instead of the robot, and the web shows it where only the desktop app did.
- **Styleguide**: the badge and its popover have their own entry, with the glyph per relation and the rung it stands at."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_20: ChangelogEntry = ChangelogEntry {
    id: "2026-09-25-issue-context-menu",
    date: "2026-09-25",
    title: "One issue context menu",
    summary: "The issue context menu opens from every list, the sidebar and issue chips, no longer closes on its own, takes the estimate, and every web menu is tighter.",
    body: r#"- **Everywhere**: a right-click (or a long-press on a phone) opens the issue menu on a board row, the sidebar list beside an issue, a reviews row and any issue chip, with the same items in the same order.
- **Stays open**: the menu no longer fires the item under the cursor when the right button is released a few pixels into it, which is what made it open and close in one go.
- **Estimate**: the menu sets the estimate on the team's scale, beside status, assignee, priority, labels and due date.
- **Menu density**: menus on the web at desktop widths use tighter rows, matching the desktop app's rhythm at the web's text size; phones keep their touch-sized rows. The values are shared design tokens, shown in the styleguide with the issue context menu as its own entry."#,
};

/// The head before that, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_21: ChangelogEntry = ChangelogEntry {
    id: "2026-09-24-release-train",
    date: "2026-09-24",
    title: "Release train 2026-09-24",
    summary: "The usage popover shows what fills a run's context, owners add a team prompt to every run, machines update their agent CLIs remotely, and fixes for invited members and the Linear import.",
    body: r#"- **Context window**: the usage popover shows a segmented bar of what fills a run's context, with a legend per segment (base, tools, playbook, team prompt, project instructions, task) on every client, and the account switch says why a switch is refused right where the control is.
- **Team prompt**: owners write a prompt in Settings, General that every run on every member's machine carries in its system prompt, on the web and in the desktop app.
- **Machines**: the machine settings update the Claude and Codex CLIs remotely and show each login's version, on the web and in the desktop app.
- **Sidebar**: one look for the sidebar sections on the web and the desktop app, with the What's new card floating above the footer.
- **Workflows**: the node strip reuses the work face toggle, and the MCP workflows update takes the runner machine so an agent can bind it.
- **Invited members**: an invite can no longer seat another team's invited member or move their address, an existing member who opens a colleague's invite link is not merged into it, invited members count once against the seats, signing in with Google, Apple or an identity provider claims the invite, and the invite page welcomes the person instead of calling the link used.
- **Linear import**: cancelling holds against a finishing discovery, attachments stream with a size cap and a timeout, bundle attachments are checked against private addresses, duplicates keep their target, and finished imports drop their snapshot after a week."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_22: ChangelogEntry = ChangelogEntry {
    id: "2026-09-23-linear-import",
    date: "2026-09-23",
    title: "Import from Linear",
    summary: "Bring a Linear workspace into a team from Settings, with its issues, comments, images, labels, statuses, relations and estimates, keeping identifiers and dates.",
    body: r#"- **Import from Linear**: a new owner-only Settings section connects to Linear with a personal API key, previews what it found, lets you map teams to boards, statuses onto yours, labels and members, dry-runs the plan and then imports. Issues keep their identifiers, timestamps, priorities, assignees and comments; inline images are rehosted; activity history is optional.
- **Relations come along**: sub-issues, blocking, duplicate and related links are recreated between imported issues.
- **Members before they sign in**: an email invite puts the person on the team at once, with their name, so issues and comments can be theirs before they ever sign in; when they do, everything carries over. The Linear import invites a workspace's members straight from its member mapping, and the Members settings show who has not joined yet, with a way to resend the invite.
- **Archived issues**: an option puts a source's archived issues on a separate board per team, archived once the import is through, or leaves them out.
- **Estimates**: a team setting picks an estimate scale (exponential, fibonacci, linear or t-shirt sizes), off by default. With one on, issues take an estimate from their properties row on every client, over the API and the agent tools; a Linear import brings its estimates and switches the matching scale on.
- **Settings**: a new Issues section holds the estimate scale and the pull request automation, with Labels and Statuses as its sub-pages, on the web and in the desktop app alike.
- **Resumable and repeatable**: an import that stops resumes where it left off, and running it again adds nothing twice.
- **Bundles**: the same importer accepts a provider-neutral bundle over the API, so other trackers can follow without a new code path."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_23: ChangelogEntry = ChangelogEntry {
    id: "2026-09-23-release-train",
    date: "2026-09-23",
    title: "Release train 2026-09-23",
    summary: "Automations pick an account, the issue list shows its blocking arrows as a rail, and a round of fixes for the desktop app and the CLI.",
    body: r#"- **Automations**: the editor picks an account on the bound machine, with its brand mark and email, instead of an agent alone, on every client. A machine switch re-seeds the pick from the new machine.
- **Blocking arrows**: on wide screens the issue list draws its blocks relations as a rail at the right edge, with a dot per node at rest, the arrows on hover or keyboard focus, and the mini graph on click. Phones keep the badge.
- **Tree lines**: nested issues draw a tighter corner and one unbroken vertical where a later sibling follows, on every client.
- **Desktop and CLI**: a workflow node keeps its identity across an account switch or a resume, the remote sign-in dialog takes the code from the browser again, and the Agent entry no longer duplicates the Running section's dot.
- **MCP**: merging a plain pull request based on your default branch no longer reports it as a stacked merge."#,
};

#[allow(dead_code)]
const PREVIOUS_24: ChangelogEntry = ChangelogEntry {
    id: "2026-09-22-release-train",
    date: "2026-09-22",
    title: "Release train 2026-09-22",
    summary: "One account picker with live limits, a calmer issue activity, MCP servers per machine, and workflows that are always reviewed by an agent.",
    body: r#"- **Accounts**: the composer has one account picker across Claude and Codex with each login's limits inline, a default account per machine, and every login is named by its email, signed out ones included.
- **Activity**: an issue's activity folds quick back and forth edits into their net change, and Show all brings every event back.
- **Runs**: tool calls to Exponential show the issues, runs and pull requests they touched as chips with a preview, resumed runs sit behind one run menu, the GitHub button lives on Changes, and phones show the changed line counts.
- **Search**: open work ranks above done work, the same way on every client and over MCP.
- **MCP servers per machine**: a machine can carry its own MCP servers, picked up from your Claude and Codex config in the desktop app or the CLI, and they ride every run there. Entries that carry a secret are never imported.
- **MCP tools**: agents can list an issue's attachments, upload large files through a signed link, and compact their own session.
- **Workflows**: every node is reviewed by an agent before it lands, the review gate setting is gone, and a pull request merged outside the workflow lands where its code actually is.
- **Fixes**: the link field in the create issue dialog takes focus again, uploaded files no longer show zero bytes, markdown attachments preview in the app, and the comment edit button has its icon back."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_25: ChangelogEntry = ChangelogEntry {
    id: "2026-09-19-release-train",
    date: "2026-09-19",
    title: "Release train 2026-09-19",
    summary: "Workflows ship on every client, the phone tab bar keeps its Agents and New issue buttons, and a round of fixes from the release review.",
    body: r#"- **Workflows**: plan a set of backlog issues as one dependency graph, let a device run them in parallel with a merge train and a review gate, and follow it from the web, the desktop app, iOS and Android.
- **Phones**: the tab bar shows the Agents and New issue buttons on every tab, the Agent page keeps past runs behind a history button, and the board switcher lists teams and boards only.
- **Review fixes**: a workflow dependent can no longer be merged into your default branch behind the final pull request, a node's approval is bound to the reviewer run and the commit it reviewed, the metrics beat is recorded again, a skipped node's issue no longer moves to done with the final pull request, and a stacked start refuses a blocking cycle that runs through a closed issue.
- **Cleanup**: batch runs are named from their stored issue list on every client (older runs were backfilled), the staleness sweep only deletes runs whose machine is gone, and retired external-agent run records are dropped when the desktop app or CLI loads them."#,
};


/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_26: ChangelogEntry = ChangelogEntry {
    id: "2026-09-18-release-train",
    date: "2026-09-18",
    title: "Release train 2026-09-18",
    summary: "Device icons, the run's task list, one picker everywhere, and a round of fixes from the release review.",
    body: r#"- **Device icons**: every machine can carry its own icon, and the board and action picker grows to 96 icons.
- **The run's task list**: the strip under a run leads with the agent's own task list, a background lane keeps its tab while it runs, and calls to Exponential group as "Read 3 issues" instead of a generic tool run.
- **One picker**: every picker on the web opens the same searchable menu, and the keyboard works in every one of them again, including the status, priority, effort, model and device menus and the phone's issue search.
- **Runs**: several runs on one issue fold behind a caret on the Runs tab, an empty task list clears instead of sticking, and a turn that changed no file no longer shows an empty edits card.
- **Phones**: the Work screen's menu sits on the Issue face only, Stop and Resume are plain bar buttons, the face switcher counts the run's changed files and tucks into the composer, and a batch run's issues open in a sheet.
- **Devices**: machines fold to one row each, usage windows say when they reset, and a login can be removed from a machine.
- **Under the hood**: the transcript no longer re-parses every patch per frame on the desktop, iOS and Android, a stray invisible character can no longer leak from the desktop editor into a description, and typing with an IME no longer sends a message mid-composition."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_27: ChangelogEntry = ChangelogEntry {
    id: "2026-09-18-device-icons",
    date: "2026-09-18",
    title: "Device icons and 36 more board icons",
    summary: "Give each machine its own icon, from a desktop, server or laptop to the Apple, Windows and Linux marks, and pick from 96 icons for boards and actions.",
    body: r#"- **Device icons**: the device settings carry an icon picker next to the name, on web, desktop, iOS and Android. Choose a monitor, a server, a laptop or the Apple, Windows or Linux mark; the pick shows on the devices list and in every device picker, for you and for teammates who see a shared server.
- **Defaults stay**: a machine you never touched keeps what it had, the monitor for a desktop app and the server for a CLI daemon.
- **More icons**: the board and action icon picker grows from 60 to 96 icons, with music, camera, gift, store, truck, map, mountain, flame, crown, brain, fingerprint, wallet, smartphone and more."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_28: ChangelogEntry = ChangelogEntry {
    id: "2026-09-18-component-foundation",
    date: "2026-09-18",
    title: "One picker, one search field, one date picker",
    summary: "Every picker on the web now looks and behaves the same, search fields carry a glyph and a clear button, and the styleguide sorts into Views, Components and Style.",
    body: r#"- **One picker**: assignee, board, labels, actions, branches, MCP servers, automation filters and the batch issue picker share a single searchable picker. A picked row shows a trailing check; in a multi-select every row carries the same filled or empty circle the iOS and Android sheets draw, and "Unassign" or "None" is a real row instead of a hidden placeholder value.
- **Search fields**: the Reviews file filter, the emoji search and the issue search sheet are one field with the search glyph inside it and a clear button that appears once you type, the same field iOS and Android already had.
- **Due dates**: the three date pickers on the issue page, the editor chips and the phone properties tray are one calendar popover, and a date never shifts by a day west of Greenwich anymore.
- **Autocomplete**: the @, #, : and / menus under the composers share one keyboard model: arrows wrap, Enter or Tab picks, Cmd or Ctrl plus Enter still sends.
- **Styleguide**: a top bar switches between Views (the screenshots), Components (the live controls, grouped by kind) and Style (colours, shape, type, motion and the icon registry). Each component names which app surfaces still draw it by hand.
- **Desktop**: the IDE draws the same set: one search field, one searchable picker, one fold header, one text button and one count capsule. The @, # and : menus in the editors and the composer, and the / menu over the run composer, are one menu now, and it opens above the caret when the window runs out of room below. A run that is working ripples its green dot like the web does; the loading bars, the bulk-select boxes and the checklist boxes are the same shapes as on the web."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_29: ChangelogEntry = ChangelogEntry {
    id: "2026-09-17-release-train",
    date: "2026-09-17",
    title: "Release train 2026-09-17",
    summary: "Every diff reads the same, tabs keep what you typed, issue search is one engine on every client, and the Devices page holds your logins and usage.",
    body: r#"- **One diff design**: a review, a run's changes and the transcript draw the same file card on all four clients, a run's edits collect into one card in the conversation, and the file list beside a diff is a folder tree with counts and a filter.
- **Tabs keep their state**: on the web and the desktop app a half-typed comment, an open reply box and the scroll position come back when you return to a work tab, and a chat run carries the title the agent gave it.
- **One issue search**: every picker, the # autocomplete and the search sheet rank issues the same way on web, desktop, iOS and Android, with the top row preselected and Enter or Tab to pick. The desktop search sheet finds issues only now; files stay in the Files rail.
- **Accounts and usage**: the context ring opens one usage view that names the account a run spends, the Devices page lists each machine's logins with their usage in place, and idle runs no longer pin a stale rate-limit reading.
- **Runs that are still there stay**: the crash sweep no longer deletes a run whose transcript still exists on its device, and the issue page keeps a run's diff after the merge.
- **Merge and conflicts**: a merge refused by a real conflict turns the Merge PR button into Fix conflicts wherever you clicked it, and the Merge PR pill sits on the small phone rung too.
- **Desktop app**: Back returns where you came from, the sidebar list no longer re-derives every row per frame, the terminal copies and pastes, and long chips and diff headers no longer overlap.
- **Web**: every page has a title, and the composers share one attachment thumbnail and one floating-bar chrome."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS_30: ChangelogEntry = ChangelogEntry {
    id: "2026-09-tab-shell-polish",
    date: "2026-09-17",
    title: "Tabs keep their state, chats get their names",
    summary: "Switching work tabs no longer drops a half-typed comment, a live run's tab no longer cuts its label off, and a chat run is named after what the agent called it.",
    body: r#"- **Tabs keep what you typed**: on the web and the desktop app, a comment or reply you started, the reply box you opened and how far you had scrolled come back when you return to the tab. Closing the tab forgets them.
- **Room on the right**: a live run's tab has no close button, so its label now gets the same padding on both sides instead of touching the edge, on the web and the desktop app alike.
- **Chats named by the agent**: a chat run shows the title Claude Code or codex gave the conversation instead of "Chat", in every session list, tab and review row on the web, the desktop app, iOS and Android. Renaming it in the agent renames it here."#,
};

/// The entry before that.
#[allow(dead_code)]
const PREVIOUS_31: ChangelogEntry = ChangelogEntry {
    id: "2026-09-diff-ui-one-design",
    date: "2026-09-17",
    title: "One diff design, everywhere",
    summary: "Every diff in Exponential is now built from the same file card, the files a run edits appear in the transcript itself, and the file list beside a diff is a tree.",
    body: r#"- **One file card**: a review, a run's changes and a transcript all draw the same card now, with the same header, the same counts and the same way to open the lines it hides. Cards start open, so a diff reads top to bottom without a click.
- **Edits in the transcript**: the files a run touches in a row are collected into one "N files edited" card in the conversation, with the file it is writing right now open and the rest a tap away. No jumping to another screen to see what changed.
- **A file tree**: the list beside a diff is a tree of folders now, with the counts per folder, a filter, and long folder chains folded into one line. On a review it sits in the sidebar, where every other page keeps its context.
- **Reviews polish**: a review's header says which issue it is, how big the diff is and what the pull request's state is, with reject, Merge PR and the GitHub link beside it. Batch rows in the review queue no longer lose their first column. On phones the floating bottom bar is now the same on web, iOS and Android: one geometry, no drop shadow, and Merge PR is one white pill centred between the circles, on a review and on a run's changes alike."#,
};

/// Whether the rail's "What's new" card renders, given the stored
/// `changelogSeenId`. Pure so the rule is testable without a gpui App: a
/// user who has never dismissed anything sees the card, and a user whose
/// stored id is an OLDER entry sees it again (that is the point of the id
/// being the dismissal key rather than a boolean).
pub(crate) fn whats_new_visible(seen: Option<&str>) -> bool {
    seen != Some(LATEST.id)
}

/// Persist [`LATEST`]'s id as seen — the ✕ on the card, and opening the
/// dialog. Idempotent: a no-op write still costs one settings save, so the
/// callers gate on [`whats_new_visible`] where it matters.
pub(crate) fn mark_seen(cx: &mut App) {
    let hub = coding_flow::CodingHub::global(cx);
    let mut settings = hub.read(cx).settings.clone();
    if settings.changelog_seen_id.as_deref() == Some(LATEST.id) {
        return;
    }
    settings.changelog_seen_id = Some(LATEST.id.to_string());
    if let Err(err) = coding_flow::CodingHub::save_settings(&hub, settings, cx) {
        log::warn!("[ui] persisting the changelog dismissal failed: {err}");
    }
}

/// Open the "What's new" dialog: date, title and the entry's GFM body. Also
/// marks the entry seen — reading it IS dismissing it, exactly like the web
/// card's "open the sheet" path.
pub(crate) fn open_whats_new(window: &mut Window, cx: &mut App) {
    mark_seen(cx);
    // Roughly the web sheet's `sm:max-w-lg`; the body is a handful of bullets
    // and the view scrolls when a longer entry lands.
    let spec = DialogSpec::new("What's new", size(px(520.), px(440.)))
        .resizable(size(px(360.), px(240.)));
    native_dialog::open_dialog_window(window, cx, spec, move |_window, cx| {
        DialogContent::new(cx.new(|_| WhatsNewView))
    });
}

/// The dialog body — a pure read-only render of [`LATEST`].
struct WhatsNewView;

impl Render for WhatsNewView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(LATEST.date),
            )
            .child(
                div()
                    .text_base()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(LATEST.title),
            )
            .child(
                div().pt_2().text_sm().child(
                    // Same glass code-block treatment as every other markdown
                    // surface in the app.
                    TextView::markdown("whats-new-body", SharedString::from(LATEST.body))
                        .style(crate::surface::markdown_style())
                        .selectable(true),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_shows_until_the_head_entry_itself_is_seen() {
        assert!(whats_new_visible(None), "a fresh install sees the card");
        assert!(
            whats_new_visible(Some("2026-09-merge-agent-run-prs")),
            "an OLDER dismissal re-surfaces the card — the id is the key"
        );
        assert!(!whats_new_visible(Some(LATEST.id)));
    }

    /// The mirror's shape, so a bad copy/paste fails here rather than in the
    /// web suite that reads this file.
    #[test]
    fn the_mirrored_entry_is_filled_in() {
        assert!(LATEST.id.starts_with("2026-"));
        assert_eq!(LATEST.date.len(), 10, "ISO date, display only");
        assert!(!LATEST.title.is_empty());
        assert!(!LATEST.summary.is_empty());
        assert!(LATEST.body.starts_with("- **"), "GFM bullets");
        // The authoring convention the web file documents.
        assert!(!LATEST.summary.contains('—'), "no em dashes in changelog copy");
        assert!(!LATEST.body.contains('—'), "no em dashes in changelog copy");
    }
}
