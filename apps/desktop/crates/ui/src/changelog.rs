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
    id: "2026-09-one-ui-everywhere",
    date: "2026-09-10",
    title: "Sessions beside their list, one session header, and lists the Linear way",
    summary: "Sessions open next to the list you came from with a back you can mouse, the header is the same on every device, hover is visible again, Usage lives under Devices, and lists are flat rows under group bands.",
    body: r#"- **Sessions beside their list**: a coding session opens as a screen of its own next to the list you came from (the Inbox stays when you click a running session from it, a board stays when you watch a run from one of its issues), and the Agent page lists every running and past run beside the chat prompt, on the desktop app and on the web. The web's bottom dock band is gone; running sessions sit in the sidebar instead.
- **Back and forward with the mouse**: the desktop app's history has a forward stack, and the mouse's back and forward buttons walk it everywhere, next to Alt+Left and Alt+Right.
- **One session header**: back, the run's identity, Usage, Resume and ONE Stop, identical whether you sit on the machine that runs the agent or watch it from another. The "Back to issue" row and the small kill icon are gone; the issue's property tray shows Start coding, or Watch for your own live run.
- **Honest sidebar rows**: a session row leads with its state dot, names the machine it runs on, nests the runs it started under it, and spins only while the agent is actually working.
- **Readable plan options**: the highlighted option on a plan card is white on blue again; the composer's attach button is the image glyph every other composer wears; "Latest changes" is "Changes".
- **Hover you can see**: the desktop app's context menus and list rows had an invisible hover; the whole glass ladder got one notch lighter on every client.
- **Usage under Devices**: the Usage page folded into the Devices page as its Accounts section. Each account lists the machines that hold it; a check marks the machine where it is the active login, and clicking a chip signs in or switches the account there.
- **Lists the Linear way**: group headers are filled bands and rows are flat, gapless lines under them, on Devices, Actions, Automations, Reviews, Inbox and Support; the Inbox and Support tabs are the segmented capsule; Filter is a small icon button; settings sections are grouped cards whose rows save themselves.
- **Quieter rate-limit banner**: a warning while the agent keeps working is no longer announced as "rate limited"; the banner shows for a real wall, with a countdown to the reset.
- **Usage that moves with Claude too**: a running Claude session now feeds its session and weekly windows into this machine's usage numbers on every turn, the way a Codex session already did, so the bar moves while you work instead of waiting for the next poll. The account's other windows keep their last polled numbers rather than disappearing."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS: ChangelogEntry = ChangelogEntry {
    id: "2026-09-rate-limited-runs",
    date: "2026-09-09",
    title: "A run that hits its rate limit says so",
    summary: "A rate-limited run is marked everywhere instead of going quiet, mobile gets mentions and inline diffs, and the IDE gets a Usage page and MCP servers.",
    body: r#"- **Rate limited, not stuck**: when an agent runs out of usage mid-run, the session is marked "Rate limited" with the time it resets, on web, desktop, iOS and Android. The run stays live and steerable; it simply cannot make a call until then. Until now it just went quiet and looked healthy.
- **Agents tell their orchestrator**: a run started by another run reports its wall to the run that started it, once, instead of leaving it waiting on a session that reads fine.
- **A start that would go nowhere is refused**: starting on a machine whose agent is already out of usage now says so and names when it resets, so you can pick another machine or agent.
- **Mentions in the mobile composer**: the steer field on iOS and Android takes @ for teammates, # for issues and : for emoji, the same three the comment box already had.
- **Edits inline on mobile**: an edit's diff renders under its row in the iOS and Android transcripts, folded away until you tap it and capped so a big rewrite stays scrollable.
- **Usage in the IDE**: the desktop app has the Usage page too, reached from Devices, with every machine and account grouped by agent and a Refresh that respects the provider's limits.
- **MCP servers in the IDE**: a Settings page listing the team's servers with this machine's readiness, so you can sign in, paste a redirect URL or set a value without leaving the app, and pick servers per run from the Start coding dialog and chat.
- **Bars for a second account**: a machine holding more than one Claude or Codex login now reports usage for each of them, not just the default one."#,
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
