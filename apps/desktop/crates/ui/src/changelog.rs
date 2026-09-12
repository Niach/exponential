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
    id: "2026-09-turn-signal-accounts-and-switching",
    date: "2026-09-12",
    title: "Working… that clears, accounts that tell the truth, and switching mid-run",
    summary: "The Working indicator follows the agent's real turn, Exponential tool calls render as cards, account health comes from a live probe, a Claude run can switch accounts when it hits a limit, and the UI wave lands on all four clients.",
    body: r#"- **Working… ends when the agent does**: the session footer, the Stop button and every session list now follow a real end-of-turn signal from the engine instead of the connection state, on web, desktop, iOS and Android. A rate-limited or compacting run no longer says Working, and a replayed transcript starts idle.
- **Exponential tool cards**: when the agent calls Exponential, the transcript shows the Exponential mark with a plain caption (Creating issue, Opened pull request) and, once done, a preview of the result: the issue pill, the PR link or a result count.
- **Subagents by name**: subagent chips carry the task the agent gave them, their rows summarize the tools they used, and a read-only Plan chip shows while the run is still in plan mode.
- **Sentences no longer split**: a subagent's tool call no longer cuts the main narration mid-word.
- **Accounts you can trust**: an account's health comes from a real usage probe, so a dead login reads Needs re-login instead of Signed in. Accounts is the decision page (per-agent tabs, live usage, health); Devices is where you sign in, re-login or pick which signed-in account a machine uses. Codex accounts stay signed in on their own, and switching a codex account never signs it out elsewhere.
- **Switch account mid-run**: a Claude run that hits its limit can continue on another account from the usage sheet or the rate-limit notice. It carries on as a new run linked to the old one; the first message after a switch is a little slower.
- **The machine you clicked**: a machine's play button opens the composer with that machine selected, and the composer says why when a machine cannot take the run.
- **Sessions that remember where you came from**: Back from a session returns to the issue, board or inbox it was opened from, resume and remote start jump straight into the run, sub-sessions nest under their parent, and an issue band inside the session opens the issue.
- **Mobile parity**: Stop in the session header, a Watch pill instead of the Coding now card, suggestion chips on a new chat, pin buttons on iOS, pin on Android action rows, a Pinned section on the phone web sidebar, and flat rows under filled group headers on every list.
- **Desktop polish**: an icon-only send button, a device settings dialog that fits, Usage as a round button next to Switch account, action icons in the automation picker, and dragging the window no longer selects text in a session.
- **Leaner agent context**: issue lists returned to agents default to open work with short descriptions, and batch runs are told to share exploration instead of repeating it.
- **pi retired**: the pi agent is gone. Any ACP binary still runs through the external agent option, with two caveats: pi does not speak ACP natively, so it is not a drop-in there, and an external agent runs without the MCP bridge, plan mode, usage and account reporting, the slash-command catalog and remote start."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS: ChangelogEntry = ChangelogEntry {
    id: "2026-09-pins-shared-machines-and-video",
    date: "2026-09-11",
    title: "Pins, machines shared with several teams, and video that plays inline",
    summary: "Pin issues, sessions and actions to the sidebar, share a build server with every team that needs it, watch a clip right inside a description or comment, and connect a second GitHub organisation.",
    body: r#"- **Pinned**: pin an issue, a session or an action from its menu and it sits in a Pinned group at the top of the sidebar on web, desktop, iOS and Android. Pins are yours alone and follow you across devices.
- **A machine shared with several teams**: a build server's device settings now show one switch per team instead of a single picker, so one runner can serve every team you belong to. Withdrawing a share still ends that team's runs on it and pauses its automations.
- **Video and audio inline**: drop a clip into a description or a comment and it plays in place, with a poster, a duration chip and a lightbox, on every client. Uploads are normalised on your device (H.264 MP4, 720p on mobile); older apps show the same clip as a plain link.
- **Connect another GitHub account**: team settings list every connected GitHub account with a Configure link and offer Connect another account separately from Refresh access, so a second organisation can be installed even when you already control one. An organisation waiting on the App's approval is named on the claim page with an approve link.
- **Accounts on mobile**: the iOS and Android Devices pages carry the Accounts section (one row per agent account, machines as chips, a check on the active login) that web and desktop got last release.
- **Live-session dot on Agent**: the green or amber running-session dot moved from Devices to the Agent entry on every client; on the phone it rides the chat launcher, which is now reachable from every tab. The mobile composer always names the machine it will start on.
- **A quieter session list**: past runs read "<machine> · <time>" instead of repeating the agent and the ending reason, and ended runs no longer take a rail row.
- **Composer polish**: an icon-only submit, subject chips with their own close button, tables in the chat feed that scroll sideways without stealing the wheel, dark-scheme time pickers, and boards on mobile merge chat and new issue into one capsule. Remote agent login now targets a specific account profile, and you can add a machine to an account from the web.
- **Rate-limit wall that expires**: a run that keeps working after its reset time no longer wears a stale "Rate limited" banner; the wall clears on the next allowed call or once the reset passes.
- **Faster Actions and Automations in the IDE**: hovering those rows no longer re-derives every session on each frame; the Automations render went from 744 µs to 180 µs on a large team.
- **Resume after a merge**: resuming a chat run whose worktree was reclaimed after its PR landed re-creates the worktree from the recorded branch instead of refusing."#,
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
