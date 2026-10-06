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

    // EXP-1212: the close button's confirm (`copy.discardConfirm`).
    const val DISCARD_CONFIRM_TITLE = "Discard this draft and its files?"
    const val DISCARD_CONFIRM = "Discard"

    // EXP-1212: the held-navigation prompt (`copy.leave`).
    const val LEAVE_TITLE = "Save this issue as a draft?"
    const val LEAVE_CREATE = "Create issue"
    const val LEAVE_KEEP = "Save draft"
    const val LEAVE_DISCARD = "Discard"

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

    /** How the page is asked to go. */
    enum class Exit {
        /** The close button, Discard draft. */
        Discard,

        /** Anything else: back, a nav entry, opening another screen. */
        Leave,
    }

    /** What an [Exit] asks before it happens. */
    enum class Prompt { None, DiscardConfirm, Leave }

    /**
     * EXP-1212: a draft WITH content never goes silently. Nothing is asked
     * while a Create is in flight (the page goes where the create lands).
     */
    fun prompt(hasContent: Boolean, exit: Exit, creating: Boolean = false): Prompt = when {
        creating || !hasContent -> Prompt.None
        exit == Exit.Discard -> Prompt.DiscardConfirm
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
}
