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
    id: "2026-09-one-composer",
    date: "2026-09-11",
    title: "One composer for chat, coding and actions",
    summary: "The Agent page composer starts everything: pick issues or an action, type instructions, attach images, choose the machine and agent. The Start coding dialog is gone on every platform.",
    body: r#"- **One place to start anything**: the Agent page composer is the launcher on web, desktop, iOS and Android. Pick issues with # or an action with the play button, or type to chat. The three-tab Start coding dialog and the Create action form are gone.
- **Every play button lands here**: Start coding on an issue, the bulk bar, an action's Run, a machine's play button and Fix merge conflicts all open the composer with the right things preselected.
- **Instructions with a start**: whatever you type beside the picked issues or action reaches the run as additional instructions. For a chat it is the prompt, for Create action it is the request.
- **Images on a start**: attach up to four images to the composer. The machine downloads them before the first turn, the same way steering images work.
- **Actions keep only picks**: text fields on actions are retired. An action declares repository, board, pull request or icon inputs; the free text comes from the composer. Existing text fields were removed from your actions.
- **Options where you need them**: machine, agent, model and plan mode sit under the composer; effort, ultracode, MCP servers and the account are one click away.
- **Answer inside the card**: a pending plan or question is answered in its own card. Free-text and reject rows open an inline field, the composer steps aside until the card is done, and an answered ask step reopens until you submit.
- **Agent messages**: an agent can notify your team over MCP with the new notifications_send tool. It lands as a push and an Inbox row for every member; a notification preference blocks other members' agents while your own always get through."#,
};

/// The previous head entry, kept so the mirror's history reads in place.
#[allow(dead_code)]
const PREVIOUS: ChangelogEntry = ChangelogEntry {
    id: "2026-09-rate-limited-means-rate-limited",
    date: "2026-09-10",
    title: "Rate limited means rate limited, and a queued update that arrives",
    summary: "A usage warning no longer marks a run blocked, the wall names its window, and a queued CLI update ends idle sessions instead of waiting forever, with Update now on the Devices page.",
    body: r#"- **Warnings are not walls**: Claude's "approaching your limit" notice no longer marks the run "Rate limited" or tells the run that started it to wait. A run is blocked only when the agent itself refused a call.
- **The wall names its window**: a blocked run's window and reset time now describe the same thing (the 5-hour session window, the weekly one or a model's), on the badge, in the session tools and in the message a parent run receives ("rate limited (weekly window) until ...").
- **Queued updates arrive**: a CLI daemon with an update queued no longer waits for an attended chat that never closes. Sessions idle for 2 hours are ended and the daemon restarts on the new version.
- **What holds an update**: the Devices page lists the live sessions a queued update is waiting on, who started them and when, and the Update button's tooltip says how the machine will get there.
- **Update now**: a machine whose daemon supports it offers Update now next to a queued update. It ends every live session on that machine (repo-backed runs can be resumed from their session page) and restarts on the new version.
- **Switch account on another machine**: clicking Switch account in a remote machine's device settings crashed the desktop app on the machine you were sitting at. The switch now queues on the remote machine as intended."#,
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
