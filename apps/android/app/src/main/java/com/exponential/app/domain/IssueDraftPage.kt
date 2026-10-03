package com.exponential.app.domain

/**
 * EXP-1170: the New issue PAGE, one view ×4 (web `issue-draft-page.ts`, iOS
 * `IssueDraftPage`, desktop `domain::issue_draft`). Copy + the autosave
 * debounce are locked against `issue-draft.json` (`IssueDraftPageTest`).
 */
object IssueDraftPage {
    const val HEADER = "New issue"
    const val TITLE_PLACEHOLDER = "Issue title"
    const val DESCRIPTION_PLACEHOLDER = "Add description..."
    const val CREATE = "Create"
    const val DISCARD = "Discard draft"
    const val UNTITLED = "Untitled draft"

    /** Quiet time after the last title/description edit before the autosave. */
    const val AUTOSAVE_DEBOUNCE_MS = 800L
}
