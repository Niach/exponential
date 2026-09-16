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
    id: "2026-09-16-release-train",
    date: "2026-09-16",
    title: "Release train 2026-09-16",
    summary: "The phone gets one Work screen, every diff reads the same, pull requests can be stacked and merged as a chain, and a run can publish its results.",
    body: r#"- **The phone Work screen**: an issue, its coding run, the changes and the results are faces of one screen on iOS, Android and the mobile web, switched from a bar at the bottom, with Stop or Resume in the top-right corner.
- **One diff view**: every diff reads the same on all four clients now, with one palette and one set of line numbers, unchanged lines you can open around a change, and renamed, copied and binary files named instead of shown empty.
- **Stacked pull requests**: an issue blocked by another can be started on top of it, the review queue nests the chain and offers Merge stack on the bottom of it, a badge says which stack or batch a piece of work belongs to, and every run list nests a run under the one that started it.
- **Results from a run**: a run that publishes screenshots gains a Results face beside Issue, Run and Changes, grouped by topic with one labelled tile per platform.
- **Batch runs by name**: a run covering several issues now reads `EXP-874 +2` beside the first issue's title instead of "Batch run", in every session list and in its own header.
- **Drafts**: an issue you start and do not finish is kept as a draft, attachments included, and reopens where you left it.
- **Session history stays on your machine**: finished runs keep their transcripts for as long as the desktop app's new Sessions setting says, and an issue with several runs of yours gets a picker to switch between them.
- **Under the hood**: the web app, the styleguide and the marketing demos now share one component library, so cards, menus and glyphs read the same everywhere, and a Claude login refreshes itself in the background instead of expiring mid-run."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS: ChangelogEntry = ChangelogEntry {
    id: "2026-09-batch-run-names",
    date: "2026-09-16",
    title: "Batch runs say what they are working on",
    summary: "A run covering several issues is now named after them instead of reading \"Batch run\", so two batches can be told apart at a glance.",
    body: r#"- **Named by its issues**: a batch run's row reads `EXP-874 +2` beside the first issue's title, in every session list and in the run's own header, on the web, the desktop app, iOS and Android.
- **Right from the start**: the issues a batch covers are recorded when it launches, so the name is correct while it is still coding, not only once its pull request is open. Older batch runs take their name from the issues on their pull request."#,
};

/// The entry before that.
#[allow(dead_code)]
const PREVIOUS_2: ChangelogEntry = ChangelogEntry {
    id: "2026-09-session-results",
    date: "2026-09-16",
    title: "Results, right on the run",
    summary: "A coding run can now publish screenshots of what it built, grouped by topic and labelled per platform, as a Results face beside Issue, Run and Changes on every client.",
    body: r#"- **A fourth face**: when a run has published pictures, the header toggle gains Results next to Issue, Run and Changes, on the web, the desktop app, iOS and Android.
- **Grouped by topic**: pictures sit under the topic the agent filed them in, one row of equal-height tiles with a label under each, so an iOS, Android and web shot of the same screen read side by side. Tap one to see it full size.
- **The agent publishes them**: a run asks Exponential for an upload link, uploads the screenshot with one command, and can replace or remove a picture later. The playbook every run gets now asks for a screenshot of UI work before it finishes."#,
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
