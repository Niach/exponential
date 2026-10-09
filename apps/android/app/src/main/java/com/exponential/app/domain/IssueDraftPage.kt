package com.exponential.app.domain

/**
 * EXP-1170: the New issue PAGE, one view ×4 (web `issue-draft-page.ts`, iOS
 * `IssueDraftPage`, desktop `domain::issue_draft`). Copy + the autosave
 * debounce are locked against `issue-draft.json` (`IssueDraftPageTest`), as
 * are EXP-1231's concurrency copy + grace.
 */
object IssueDraftPage {
    const val HEADER = "New issue"
    const val TITLE_PLACEHOLDER = "Issue title"
    const val DESCRIPTION_PLACEHOLDER = "Add description..."
    const val CREATE = "Create"
    const val UNTITLED = "Untitled draft"

    // EXP-1212: the held-navigation prompt (`copy.leave`).
    const val LEAVE_TITLE = "Save this issue as a draft?"
    const val LEAVE_CREATE = "Create issue"
    const val LEAVE_KEEP = "Save draft"
    const val LEAVE_DISCARD = "Discard"

    // EXP-1231: the toast when another client discarded this draft (`copy.discardedElsewhere`).
    const val DISCARDED_ELSEWHERE = "Draft discarded elsewhere"

    /** Quiet time after the last title/description edit before the autosave. */
    const val AUTOSAVE_DEBOUNCE_MS = 800L

    /**
     * A title, description or attachment is content; chips alone never are.
     * A reopened draft whose file list is not known yet ([attachmentsKnown]
     * false) counts as content: it may be a file-only draft.
     */
    fun hasContent(
        title: String,
        description: String,
        attachmentCount: Int,
        attachmentsKnown: Boolean = true,
    ): Boolean =
        title.isNotBlank() || description.isNotBlank() || attachmentCount > 0 || !attachmentsKnown

    /** Create (the header's and the leave prompt's): a title, a board, idle. */
    fun createEnabled(title: String, hasBoard: Boolean, creating: Boolean, uploadsInFlight: Int): Boolean =
        title.isNotBlank() && hasBoard && !creating && uploadsInFlight == 0

    /** What leaving the page asks before it happens. */
    enum class Prompt { None, Leave }

    /**
     * EXP-1212: a draft WITH content never goes silently: back, a nav entry or
     * opening another screen asks the leave question (EXP-1247: the page has
     * no close button, Back + Create only, so discarding is the leave
     * prompt's answer). Nothing is asked while a Create is in flight (the
     * page goes where the create lands).
     */
    fun prompt(hasContent: Boolean, creating: Boolean = false): Prompt = when {
        creating || !hasContent -> Prompt.None
        else -> Prompt.Leave
    }

    /** The leave prompt's answers (the dialog lays them out: Discard leading, Save draft · Create issue trailing). */
    enum class LeaveChoice { Discard, Keep, Create }

    /**
     * The leave prompt's choices in row order (Discard … Save draft · Create
     * issue). [canKeep] = the page writes a draft row; a mode that never does
     * (sub-issue or share compose) has nothing to keep, so it offers Discard
     * and Create only.
     */
    fun leaveChoices(canKeep: Boolean): List<LeaveChoice> =
        if (canKeep) listOf(LeaveChoice.Discard, LeaveChoice.Keep, LeaveChoice.Create)
        else listOf(LeaveChoice.Discard, LeaveChoice.Create)

    /**
     * The leave prompt's default answer (primary-ranked: initial focus +
     * Enter): Create issue, unless it is disabled (no title), then Save draft
     * where the mode offers it. Never Discard.
     */
    fun leaveDefault(choices: List<LeaveChoice>, createEnabled: Boolean): LeaveChoice? = when {
        createEnabled && LeaveChoice.Create in choices -> LeaveChoice.Create
        LeaveChoice.Keep in choices -> LeaveChoice.Keep
        else -> null
    }

    /**
     * EXP-1231: how long a SEEN draft row may be missing (with no issue
     * carrying its id) before the page concludes it was discarded elsewhere
     * (`concurrency.discardedGraceMs`). A row back within it resumes editing.
     */
    const val DISCARDED_GRACE_MS = 3000L

    /** EXP-1231: what the synced store says about this page's draft. */
    sealed interface Fate {
        /** Still editable (or never synced yet). */
        data object Open : Fate

        /** Created elsewhere: replace the page with [issueId]'s detail, no prompt, no toast. */
        data class Created(val issueId: String) : Fate

        /** The seen row is gone with no issue: discarded elsewhere once the grace passes. */
        data object Gone : Fate
    }

    /**
     * EXP-1231 (×4: web `issue-draft-page.ts`, iOS, desktop). [createdIssueId]
     * = an issue whose `draft_id` is this draft (proof it was created,
     * wherever); [seen] = the row was observed in the local store during this
     * page's life; [present] = it is there now. Created wins; a row never seen
     * is never Gone (a new page whose first write has not landed stays open).
     */
    fun fate(seen: Boolean, present: Boolean, createdIssueId: String?): Fate = when {
        createdIssueId != null -> Fate.Created(createdIssueId)
        seen && !present -> Fate.Gone
        else -> Fate.Open
    }
}
