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
    id: "2026-09-diff-ui-one-design",
    date: "2026-09-17",
    title: "One diff design, everywhere",
    summary: "Every diff in Exponential is now built from the same file card, the files a run edits appear in the transcript itself, and the file list beside a diff is a tree.",
    body: r#"- **One file card**: a review, a run's changes and a transcript all draw the same card now, with the same header, the same counts and the same way to open the lines it hides. Cards start open, so a diff reads top to bottom without a click.
- **Edits in the transcript**: the files a run touches in a row are collected into one "N files edited" card in the conversation, with the file it is writing right now open and the rest a tap away. No jumping to another screen to see what changed.
- **A file tree**: the list beside a diff is a tree of folders now, with the counts per folder, a filter, and long folder chains folded into one line. On a review it sits in the sidebar, where every other page keeps its context.
- **Reviews polish**: a review's header says which issue it is, how big the diff is and what the pull request's state is, with reject, Merge PR and the GitHub link beside it. Batch rows in the review queue no longer lose their first column. On phones the floating bottom bar is now the same on web, iOS and Android: one geometry, no drop shadow, and Merge PR is one white pill centred between the circles, on a review and on a run's changes alike."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS: ChangelogEntry = ChangelogEntry {
    id: "2026-09-accounts-usage",
    date: "2026-09-17",
    title: "Accounts and usage",
    summary: "One usage view on every client that names the account a run is on, shows that account's numbers, keeps them current, and lists each device's logins in place.",
    body: r#"- **One usage overlay**: the context ring opens the same view on web, desktop, iOS and Android: the run's own account at the top with its three rate-limit windows, the context window below, and every other account on that machine as a compact row with a switch button beside it instead of across it.
- **The right account**: the overlay names the login the run actually spends, so a run started on a second account no longer wears the default account's name or its re-login badge.
- **Numbers that move**: an idle run no longer pins its last rate-limit reading, a window that has reset re-reads within a heartbeat, opening the overlay asks the machine for a fresh read, and an account nobody ran for days refreshes its own token to report instead of showing days-old numbers.
- **Devices list their logins**: the Devices page lists each machine's accounts under the machine, with the same compact usage line and one menu per login, in place of the separate Accounts section, its tabs and its chips."#,
};

/// The entry before that.
#[allow(dead_code)]
const PREVIOUS_2: ChangelogEntry = ChangelogEntry {
    id: "2026-09-fix-conflicts-everywhere",
    date: "2026-09-16",
    title: "Fix conflicts, on every Merge button",
    summary: "A merge refused by a real conflict now turns the Merge PR button into Fix conflicts wherever you clicked it, instead of only showing an error.",
    body: r#"- **The swap works everywhere**: on the web the issue header, the run header, the Changes faces and the review page all showed an error notification on a conflict and kept the plain Merge PR button. They now swap it for Fix conflicts with a Retry merge beside it, like the Reviews list always did.
- **Desktop app**: a batch or chat run's header and its Changes bar make the same swap, and a merge refused for any other reason now shows its message under the button instead of only in the log.
- **iOS**: merging a stack from the overlay on the Work screen reports a refusal instead of silently doing nothing."#,
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
