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
    id: "2026-09-queued-messages-and-one-session-header",
    date: "2026-09-13",
    title: "Messages that wait their turn, Stop that reaches the machine, and one session header",
    summary: "A message sent while the agent is working waits in a Queued bar until the turn ends, Stop from the web and the phone now interrupts the run, the desktop session header and usage sheet match the web, the sidebar issue list gets bulk edits, and a run that switched accounts no longer stays marked rate limited.",
    body: r#"- **Queued messages**: send while the agent is still working or compacting and the message waits in a Queued bar above the composer, on web, desktop, iOS and Android. Its × removes it from the queue, an empty draft takes the text back, and Stop drops the queue with the turn. The message shows in the transcript the moment the agent takes it.
- **Stop reaches the machine**: Stop pressed on the web or the phone now interrupts the running turn on the machine. Every client had been sending it; the relay was dropping it.
- **One session header**: the desktop session header lost its Back button, centers the run's identity like the web, carries a quieter Plan chip and the web's usage and open-issue icons, and the issue band is the real issue header with Open issue in the Watch slot.
- **One usage sheet**: the usage overlay has the same structure on web and desktop: the active account once, Context, the windows, the other accounts, and one footer note.
- **A sidebar header that stays put**: the team switcher, Search and New issue sit fixed above the sliding pane, pinned rows unpin on hover, and the sidebar issue list gets batch selection with a bulk bar, the row context menu and inline status.
- **Switching accounts clears the wall**: a run ended by an account switch no longer keeps Rate limited on its row for good, and a resumed run no longer re-arms a rate limit from its replayed transcript."#,
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
