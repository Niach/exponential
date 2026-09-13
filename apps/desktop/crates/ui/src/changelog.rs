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
    id: "2026-09-ui-refinement",
    date: "2026-09-12",
    title: "A quieter interface: ghost buttons, pickers that show what they pick, and settings that flatten out",
    summary: "Secondary buttons lose their circles, agent and device pickers carry icons, the Agent page holds the running and past runs, board settings get a page each, API keys became Security, and issue filters are gone.",
    body: r#"- **Circles mean the main action**: only the one action a surface is for keeps a round button: start a run, send, New issue, Search, add. Everything secondary is a plain glyph that fills on hover, including the ... menus, close, folder and file toggles, chevrons, remove and refresh. Back is that same quiet button in the same place on every screen, and a back row in a list or in settings is the whole row.
- **Pickers that show what they pick**: the agent picker is one control everywhere, showing the Claude Code or Codex mark on the button and both the mark and the name in its menu. Device pickers lead with a laptop or a server icon, and every menu whose value carries an icon now shows that icon on its options too.
- **The Agent page is the composer**: the runs that are going sit in a band above the prompt, past runs are folded behind a count you can open, and with nothing running the prompt sits in the middle of the page instead of at the top.
- **Device settings hold settings**: the dialog is the device name, your default device, team sharing and the per-agent defaults, and it opens at the size of its content. Signing in and choosing which account the device uses moved onto the account chips, where the account is.
- **Add, switch and remove an account**: Add account sits in the Accounts header next to Add device, and an account chip offers exactly what applies to it: Sign in when it is signed out, Set as default, or Remove account. Removing clears that login from that one device; the account itself is untouched.
- **Security**: the API keys settings page is now Security and holds your API keys and your passkeys, which moved off the Account page. Old links land there on their own.
- **A settings page per board**: team settings list your boards one by one, the way the desktop app already did. Each board is its own page, with New board under the list and Archived boards after it.
- **Changes you can size and scope**: the diff pane drags wider and remembers its width, and opening a file or an edit from the transcript scopes it to that turn, with one click back to the whole branch.
- **Status headers in the sidebar**: the board and My issues lists in the sidebar group under the same status bands as the full list: the status glyph, its name, a count, and a click to fold.
- **Automated runs together**: opening a finished automated run shows the other automated runs beside it, and Back returns to Automations.
- **Issue filters are gone**: the filter bar left the board and the inbox on web, desktop, iOS and Android. Search covers what it was mostly used for, and grouping, sorting and the status bands stay.
- **External agents are gone**: the desktop app and the CLI no longer run an arbitrary ACP binary. Claude Code and Codex are the agents; a recorded run that used an external one cannot be resumed, so start a new run instead."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS: ChangelogEntry = ChangelogEntry {
    id: "2026-09-email-signup-and-mobile-tidy",
    date: "2026-09-12",
    title: "Sign up with your email, and the mobile apps drop what they cannot use",
    summary: "Continue with email now creates your account on Exponential Cloud, pin buttons are gone from iOS, Android and the phone-width web app, Android's Accounts and Devices pages match the other clients, and a retired agent never shows up again.",
    body: r#"- **Sign up with email**: on Exponential Cloud, Continue with email creates an account for a new address the moment you enter the code. Until today a new address silently got no mail, because only Google and Apple could create accounts.
- **No pin buttons where there is no sidebar**: iOS, Android and the phone-width web app no longer offer Pin on issues, sessions and actions. Pins made on the desktop or the wide web app still show in the board switcher's Pinned group.
- **Accounts and Devices on Android**: the Accounts page keeps only quiet machine chips (an online dot, a check on the active login). Signing in, re-login and "use this account here" moved to the machine row under Devices, as on web, desktop and iOS. On iOS a machine chip now offers the right repair: re-login for an expired login, use this account here for one the machine is not using.
- **Retired agents stay gone**: a machine still on an older build that reports the removed pi agent no longer produces a row, a tab or a picker entry anywhere. The server strips it from every heartbeat and the stored device data was cleaned up."#,
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
