//! The §4.5 editor-seam filler (masterplan-v3 §4.5 / P3 integration step):
//! adapts the from-scratch GFM [`MarkdownEditor`] onto
//! [`crate::issue_detail`]'s [`DescriptionEditor`] trait and installs the
//! factory at bootstrap, so the issue-detail description edits through the
//! real block editor instead of the read-only fallback.
//!
//! Also exports [`build_editor`] — the one configured-editor constructor the
//! create-issue dialog shares (transport + completion source + `#IDENT`/`@`
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
pub(crate) fn install(cx: &mut App) {
    install_description_editor(
        cx,
        Rc::new(|params, window, cx| {
            Rc::new(SeamEditor::build(params, window, cx)) as Rc<dyn DescriptionEditor>
        }),
    );
}

/// Build a fully configured [`MarkdownEditor`] entity — the single
/// constructor both the detail seam and the create-issue dialog use, so
/// image transport, autocomplete and pill resolution never diverge (§4.5
/// "one upload path").
///
/// `upload_issue`: `Some(id)` = detail mode (immediate upload on paste);
/// `None` = create-dialog mode (stage as `draft://`, resolve at submit).
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
/// navigate to its detail (§4.5 "clicking navigates to that issue's detail").
pub(crate) fn open_issue_by_identifier(
    team_id: &str,
    identifier: &str,
    window: &mut Window,
    cx: &mut App,
) {
    let target = Store::global(cx)
        .collections()
        .issues_in_team(team_id, cx)
        .into_iter()
        .find(|issue| issue.identifier.eq_ignore_ascii_case(identifier))
        .map(|issue| issue.id);
    if let Some(issue_id) = target {
        navigate(window, cx, Screen::IssueDetail { issue_id });
    }
}

/// The adapter: owns the editor entity + a markdown mirror cell.
///
/// The cell (not `editor.read(..)`) backs [`DescriptionEditor::markdown`] and
/// the blur-save hook because those fire from *inside* the editor's own
/// update cycle (`on_change`/`on_blur` callbacks) — reading the leased entity
/// there would double-borrow. `on_change` keeps the cell current instead.
struct SeamEditor {
    editor: Entity<MarkdownEditor>,
    current: Rc<RefCell<String>>,
}

impl SeamEditor {
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
            // The create dialog keeps the always-editable surface.
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
