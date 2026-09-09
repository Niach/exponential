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
    id: "2026-09-mcp-servers-and-agent-accounts",
    date: "2026-09-09",
    title: "Your own MCP servers in every run, and more than one agent account per machine",
    summary: "Add a team's MCP servers once, sign in from the browser on any of your machines, pick them per run; agents get accounts and a usage page.",
    body: r#"- **MCP servers**: Settings has a new MCP servers page. Add a remote server or a local command with the header or variable names it needs; the values never leave your machines. Pick servers per run in the Start coding dialog and on the chat page.
- **Sign in from anywhere**: an OAuth server shows a readiness chip per machine. Click Sign in on the machine you want, consent in the browser you are already in, and that machine finishes the sign-in itself. The exponential mcp command signs in locally or takes a pasted redirect URL.
- **Sign in, one click**: a start that fails because an agent is signed out now offers Sign in right there, on the toast and on the machine list, instead of sending you to a terminal.
- **Agent accounts**: a machine can hold more than one Claude or Codex login, and a run picks the account it uses.
- **Usage page**: every machine and account you own, grouped by agent with its rate-limit windows, with a Refresh that respects the provider's limits."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS: ChangelogEntry = ChangelogEntry {
    id: "2026-09-session-page-restructure",
    date: "2026-09-09",
    title: "A session page you can read, and a rail you can find it in",
    summary: "Tool groups say what happened, edits show their diff inline, plan approval reads top to bottom, and every live run sits in the desktop rail.",
    body: r#"- **Tool groups that say what happened**: a collapsed run of tool calls reads "Ran 4 commands · edited 2 files · 1 failed" instead of "12 tool calls", with failures last, on web, desktop, iOS and Android.
- **Edits inline**: an edit's diff renders under its row in the web transcript, capped so a big rewrite stays scrollable; failed calls tint the row.
- **Plan approval, top to bottom**: the primary choice is "Yes", the fresh-context choice comes second and "No, keep planning" sits last with a line saying your next message goes back to planning. Options are numbered buttons; keys 1 to 9 and Enter pick them, and typing in the composer answers the card. The extra text field inside the card is gone.
- **One composer**: the mention-capable field from issues is the session field too, the send glyph is the same everywhere, and it turns into Stop while the agent is working. The plan-mode pill left the composer; the Plan switch stays on the chat page, model and effort are picked at start.
- **Subagent tabs on desktop**: a subagent gets its own tab above the transcript, like the other clients, and the collapsed row opens it.
- **Rate limits in the transcript**: a claude session that hits its limit shows one banner with the reset time instead of a run of identical messages.
- **The desktop rail**: sessions and terminals fill the window on their own, every live run is a rail row under Sessions with an amber dot when it needs you, Agent opens chat, and the machine's Files and Source Control sit under a This device heading. Watch slides the run in over the issue instead of leaving the page.
- **Transcripts stay loadable**: after a device has replayed a finished run, scrolling to the top still fetches earlier pages from that device instead of stalling.
- **Session ids survive /clear**: a claude session that clears its context keeps its page, its tab and its resume record."#,
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
