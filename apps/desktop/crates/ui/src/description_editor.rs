//! The §4.5 editor-seam filler (masterplan-v3 §4.5 / P3 integration step):
//! adapts the from-scratch GFM [`MarkdownEditor`] onto
//! [`crate::issue_detail`]'s [`DescriptionEditor`] trait and installs the
//! factory at bootstrap, so the issue-detail description edits through the
//! real block editor instead of the read-only fallback.
//!
//! Also exports [`build_editor`] — the one configured-editor constructor the
//! New issue page and the sub-issue composer share (transport + completion source + `#IDENT`/`@`
//! resolver all wired identically, §4.6).

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, AppContext as _, Entity, IntoElement as _, Window};
use sync::Store;

use crate::issue_detail::{
    install_description_editor, DescriptionEditor, DescriptionEditorParams,
};
use crate::markdown::{store_completion_source, MarkdownEditor, RefResolver};
use crate::navigation::{navigate, Screen};
use crate::queries;

/// Install the [`DescriptionEditorFactory`] (call once from `ui::init`,
/// before any window opens).
///
/// EXP-261: the description surface is the vendored WYSIWYG editor
/// ([`crate::wysiwyg::WysiwygDescription`]). The classic block editor stays
/// compiled (the comment composer still uses [`MarkdownEditor`]); to revert
/// this surface, swap `WysiwygSeamEditor::build` back to `SeamEditor::build`
/// below — nothing else changes.
pub(crate) fn install(cx: &mut App) {
    install_description_editor(
        cx,
        Rc::new(|params, window, cx| {
            Rc::new(WysiwygSeamEditor::build(params, window, cx)) as Rc<dyn DescriptionEditor>
        }),
    );
}

/// Build a configured [`crate::wysiwyg::WysiwygDescription`] entity — the
/// WYSIWYG analog of [`build_editor`], shared by the detail seam, the New
/// issue page and the sub-issue composer so transport, autocomplete and pill
/// wiring never diverge.
pub(crate) fn build_wysiwyg_editor(
    team_id: Option<String>,
    upload_issue: Option<String>,
    placeholder: &str,
    initial_markdown: &str,
    on_save: Option<crate::wysiwyg::OnSave>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<crate::wysiwyg::WysiwygDescription> {
    let placeholder = placeholder.to_string();
    let initial = initial_markdown.to_string();
    cx.new(|cx| {
        crate::wysiwyg::WysiwygDescription::new(
            team_id,
            upload_issue,
            &placeholder,
            &initial,
            on_save,
            window,
            cx,
        )
    })
}

/// The WYSIWYG adapter behind the detail-view seam. Unlike [`SeamEditor`]
/// there is no markdown mirror cell: the vendored editor communicates via
/// `cx.emit`/subscriptions delivered AFTER its own update completes, so
/// reading the entity from the save hook can never double-borrow.
struct WysiwygSeamEditor {
    editor: Entity<crate::wysiwyg::WysiwygDescription>,
}

impl WysiwygSeamEditor {
    fn build(params: &DescriptionEditorParams, window: &mut Window, cx: &mut App) -> Self {
        let team_id = queries::issue_team_id(cx, &params.issue_id);
        let editor = build_wysiwyg_editor(
            team_id,
            Some(params.issue_id.clone()),
            &params.placeholder,
            &params.initial_markdown,
            // Save-on-blur (web `handleDescriptionBlur`); the detail view
            // dedupes unchanged saves against `last_saved_description`.
            Some(params.on_save.clone()),
            window,
            cx,
        );
        // EXP-335: the rail's attach button routes non-inline-image picks
        // to the detail view's Files-section upload flow.
        // EXP-1247: the rail's "Attach file" uploads every pick as a FILE;
        // a drop's non-image files keep the legacy routing.
        let on_attach = params.on_attach_files.clone();
        editor.update(cx, |editor, _| {
            let on_drop = on_attach.clone();
            editor.set_attach_handler(Rc::new(move |paths, window, cx| {
                on_drop(paths, false, window, cx)
            }));
            editor.set_attach_as_file_handler(Rc::new(move |paths, window, cx| {
                on_attach(paths, true, window, cx)
            }));
        });
        Self { editor }
    }
}

impl DescriptionEditor for WysiwygSeamEditor {
    fn set_markdown(&self, markdown: &str, window: &mut Window, cx: &mut App) {
        self.editor.update(cx, |editor, cx| {
            editor.set_markdown(markdown, window, cx);
        });
    }

    fn markdown(&self, cx: &App) -> String {
        self.editor.read(cx).markdown(cx)
    }

    fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.editor.read(cx).is_focused(window, cx)
    }

    fn element(&self, _window: &mut Window, _cx: &mut App) -> gpui::AnyElement {
        self.editor.clone().into_any_element()
    }

    fn focus(&self, window: &mut Window, cx: &mut App) {
        self.editor
            .update(cx, |editor, cx| editor.focus(window, cx));
    }

    // EXP-261: the vendored editor tracks real edits via its revision
    // counter, so the flush path can tell "user typed" apart from "the
    // serializer normalized the loaded bytes".
    fn set_scroll_handle(&self, handle: gpui::ScrollHandle, cx: &mut App) {
        self.editor
            .update(cx, |editor, cx| editor.set_scroll_handle(handle, cx));
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.editor.read(cx).is_dirty(cx)
    }

    fn mark_clean(&self, cx: &mut App) {
        self.editor.update(cx, |editor, cx| editor.mark_clean(cx));
    }
}

/// Build a fully configured [`MarkdownEditor`] entity — the single
/// constructor the detail seam and the issue composers use, so
/// image transport, autocomplete and pill resolution never diverge (§4.5
/// "one upload path").
///
/// `upload_issue`: `Some(id)` = detail mode (immediate upload on paste);
/// `None` = create-dialog mode (stage as `draft://`, resolve at submit).
#[allow(dead_code)] // EXP-261: kept as the block-editor revert path.
pub(crate) fn build_editor(
    team_id: Option<String>,
    upload_issue: Option<String>,
    placeholder: &str,
    initial_markdown: &str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<MarkdownEditor> {
    let placeholder = placeholder.to_string();
    let initial = initial_markdown.to_string();
    cx.new(|cx| {
        let mut editor = MarkdownEditor::new(window, cx);
        editor.set_placeholder(placeholder);
        if !initial.is_empty() {
            editor.set_markdown(&initial, window, cx);
        }
        editor.set_upload_issue(upload_issue);
        if let Some(transport) = queries::attachment_transport(cx) {
            editor.set_transport(transport, cx);
        }
        if let Some(team_id) = team_id {
            editor.set_completion_source(store_completion_source(team_id.clone()));
            // `@email`/`#IDENT` pills in the blurred preview resolve against
            // the issue's team, and a resolved issue pill navigates to
            // its detail — same wiring as comment bodies (EXP-161).
            editor.set_resolver(RefResolver::from_store(team_id.clone()));
            editor.set_on_open_issue(move |identifier, window, cx| {
                open_issue_by_identifier(&team_id, identifier, window, cx);
            });
        }
        editor
    })
}

/// Resolve a `#IDENT` pill click against the synced issues of a team and
/// open it ([`open_issue_pill`], §4.5 "clicking navigates to that issue's
/// detail").
pub(crate) fn open_issue_by_identifier(
    team_id: &str,
    identifier: &str,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(issue_id) = issue_id_by_identifier(team_id, identifier, cx) {
        open_issue_pill(issue_id, window, cx);
    }
}

/// EXP-1347 (session-thread.json): where a tapped issue pill lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IssuePillTarget {
    /// The issue is one of the open thread's issue tabs: select that tab
    /// (never leaving the thread).
    ThreadTab,
    /// A top (IDE) tab already holds the issue: focus it.
    ExistingTab,
    /// Open a new top tab.
    NewTab,
}

/// EXP-1347: the pill decision (pure). `thread_issue_ids` = the issue tabs of
/// the thread the pill was tapped inside (empty outside a thread);
/// `existing_tab` = a top tab already holds the issue.
pub(crate) fn issue_pill_target(thread_issue_ids: &[String], issue_id: &str, existing_tab: bool) -> IssuePillTarget {
    if thread_issue_ids.iter().any(|id| id == issue_id) {
        IssuePillTarget::ThreadTab
    } else if existing_tab {
        IssuePillTarget::ExistingTab
    } else {
        IssuePillTarget::NewTab
    }
}

/// EXP-1347: open an issue pill's issue. Inside a thread (the window shows
/// `Screen::Thread`, or a session of a thread in place: its Conversation, an
/// embedded session view, a card, the issue child) an issue of THAT thread
/// selects its thread tab; else an existing top tab holding it is focused;
/// else a new top tab opens. Deferred: the click may land inside an update
/// of a view this touches.
pub(crate) fn open_issue_pill(issue_id: String, window: &mut Window, cx: &mut App) {
    window.defer(cx, move |window, cx| {
        let screen = Screen::IssueDetail { issue_id: issue_id.clone() };
        let Some(panel) = crate::screens::screens_for_window(window, cx) else {
            navigate(window, cx, screen);
            return;
        };
        let nav = crate::navigation::nav_for_window(window, cx);
        let root = match crate::navigation::resolved_screen(&nav, cx) {
            Some(Screen::Thread { session_id }) => Some(session_id),
            Some(Screen::Session { session_id }) => crate::screens::in_place_root(&session_id, cx),
            _ => None,
        };
        let thread = root.and_then(|root| panel.read(cx).thread_view(&root));
        let thread_ids = thread
            .as_ref()
            .map(|thread| thread.read(cx).issue_tab_ids())
            .unwrap_or_default();
        let existing = panel.read(cx).has_tab(&screen);
        match (issue_pill_target(&thread_ids, &issue_id, existing), thread) {
            (IssuePillTarget::ThreadTab, Some(thread)) => thread.update(cx, |thread, cx| {
                thread.open_issue_tab(issue_id, window, cx);
            }),
            (IssuePillTarget::ExistingTab, _) => panel.update(cx, |panel, cx| panel.open_screen(screen, window, cx)),
            _ => navigate(window, cx, screen),
        }
    });
}

/// EXP-760: the lookup half of [`open_issue_by_identifier`] — the row id a
/// `#IDENT` token names inside `team_id`, or `None` when nothing with that
/// identifier has synced. The hover preview needs the id without navigating.
pub(crate) fn issue_id_by_identifier(
    team_id: &str,
    identifier: &str,
    cx: &App,
) -> Option<String> {
    Store::global(cx)
        .collections()
        .issue_refs_in_team(team_id, cx)
        .into_iter()
        .find(|issue| issue.identifier.eq_ignore_ascii_case(identifier))
        .map(|issue| issue.id.clone())
}

/// The CLASSIC block-editor adapter: owns the editor entity + a markdown
/// mirror cell. EXP-261 replaced it with [`WysiwygSeamEditor`] as the
/// installed seam; it stays compiled as the documented revert path (swap the
/// constructor in [`install`]).
///
/// The cell (not `editor.read(..)`) backs [`DescriptionEditor::markdown`] and
/// the blur-save hook because those fire from *inside* the editor's own
/// update cycle (`on_change`/`on_blur` callbacks) — reading the leased entity
/// there would double-borrow. `on_change` keeps the cell current instead.
#[allow(dead_code)]
struct SeamEditor {
    editor: Entity<MarkdownEditor>,
    current: Rc<RefCell<String>>,
}

impl SeamEditor {
    #[allow(dead_code)] // EXP-261: kept as the block-editor revert path.
    fn build(params: &DescriptionEditorParams, window: &mut Window, cx: &mut App) -> Self {
        // Scope autocomplete + pills to the issue's team (§4.6).
        let team_id = queries::issue_team_id(cx, &params.issue_id);

        let editor = build_editor(
            team_id,
            Some(params.issue_id.clone()),
            &params.placeholder,
            &params.initial_markdown,
            window,
            cx,
        );

        let current = Rc::new(RefCell::new(params.initial_markdown.clone()));
        let on_save = params.on_save.clone();
        editor.update(cx, |editor, _| {
            // Detail mode: read the description rendered (pills, clickable
            // links) until the user clicks in to edit — web parity (EXP-161).
            // The New issue page keeps the always-editable surface.
            editor.set_preview_when_blurred(true);
            // …and edit mode stays chrome-less (EXP-256): no border card, no
            // horizontal padding shift when clicking into the description.
            editor.set_chrome(false);
            let cell = current.clone();
            editor.set_on_change(move |markdown, _, _| {
                *cell.borrow_mut() = markdown.to_string();
            });
            let cell = current.clone();
            let on_blur_save = on_save.clone();
            editor.set_on_blur(move |window, cx| {
                // Save-on-blur (web `handleDescriptionBlur`); the detail view
                // dedupes unchanged saves against `last_saved_description`.
                on_blur_save(cell.borrow().clone(), window, cx);
            });
            let cell = current.clone();
            editor.set_on_commit(move |markdown, window, cx| {
                // Structural edits (image insert/remove) persist immediately —
                // there is no blur to ride on (masterplan §8.2). Keep
                // the mirror current, then save through the same path.
                *cell.borrow_mut() = markdown.to_string();
                on_save(markdown.to_string(), window, cx);
            });
        });

        Self { editor, current }
    }
}

impl DescriptionEditor for SeamEditor {
    fn set_markdown(&self, markdown: &str, window: &mut Window, cx: &mut App) {
        *self.current.borrow_mut() = markdown.to_string();
        self.editor.update(cx, |editor, cx| {
            editor.set_markdown(markdown, window, cx);
        });
    }

    fn markdown(&self, _cx: &App) -> String {
        self.current.borrow().clone()
    }

    fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.editor.read(cx).is_focused(window, cx)
    }

    fn element(&self, _window: &mut Window, _cx: &mut App) -> gpui::AnyElement {
        self.editor.clone().into_any_element()
    }

    fn focus(&self, window: &mut Window, cx: &mut App) {
        self.editor
            .update(cx, |editor, cx| editor.focus(window, cx));
    }
}

#[cfg(test)]
mod pill_tests {
    use super::*;

    /// EXP-1347: a thread's own issue selects its tab; otherwise an open
    /// top tab is focused; otherwise a new tab opens.
    #[test]
    fn issue_pill_prefers_the_thread_tab_then_an_open_tab() {
        let thread = vec!["i1".to_string(), "i2".to_string()];
        assert_eq!(issue_pill_target(&thread, "i2", true), IssuePillTarget::ThreadTab);
        assert_eq!(issue_pill_target(&thread, "i2", false), IssuePillTarget::ThreadTab);
        assert_eq!(issue_pill_target(&thread, "i3", true), IssuePillTarget::ExistingTab);
        assert_eq!(issue_pill_target(&thread, "i3", false), IssuePillTarget::NewTab);
        assert_eq!(issue_pill_target(&[], "i1", true), IssuePillTarget::ExistingTab);
        assert_eq!(issue_pill_target(&[], "i1", false), IssuePillTarget::NewTab);
    }
}
