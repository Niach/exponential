package com.exponential.app.domain

/**
 * EXP-846: how a transcript renders a tool call that is one of OUR OWN MCP
 * tools. An agent working on an issue spends half its calls talking back to
 * Exponential, and those rows read as `mcp__exponential__exponential_issues_create`
 * — the raw wire name — beside the file edits. They get the product's own
 * vocabulary instead: the Exponential mark, a caption in the tense the call is
 * in ("Creating issue" → "Created issue"), the SUBJECT the call named, and,
 * once it settles, a small preview of what came back ([ToolResultPreview]).
 *
 * Every string comes from the contract's `expToolDisplay` tables
 * (`DomainContract.expTool*`, generated ×4) — nothing here is written down
 * twice, so adding an MCP tool is a contract edit and a regenerate.
 */
data class ExpToolRow(
    /** The contract row this call matched (`issues_create`). */
    val row: String,
    /** The caption to render: the progressive form while the call is in
     *  flight, the done form once it settled. */
    val caption: String,
    /** Which INPUT field names the call's subject — `` when the call has no
     *  meaningful one (a plain list). */
    val subjectKey: String,
    /** What the answer is: a contract `expToolDisplay.resultKinds` value,
     *  deciding which preview (if any) the settled row draws. */
    val result: String,
)

object ExpToolDisplay {
    // The result kinds this build renders. An unknown kind (a newer contract)
    // falls back to caption-only, which is the [RESULT_NONE] behaviour.
    const val RESULT_NONE = "none"
    const val RESULT_ISSUE = "issue"
    const val RESULT_PR = "pr"
    const val RESULT_COMMENT = "comment"
    const val RESULT_SESSION = "session"
    const val RESULT_BOARD = "board"
    const val RESULT_ACTION = "action"
    const val RESULT_AUTOMATION = "automation"
    const val RESULT_LIST = "list"

    /**
     * The Exponential MCP tool a call NAMES, or null for anything else.
     * Byte-identical with the engine's `exp_tool_row` (mapper.rs) and its
     * siblings ×4: a wire name ENDING in `exponential_<row>` for a contract
     * `expToolNames` row, whatever namespace an adapter put in front of it
     * (`mcp__exponential__exponential_issues_create` on claude, the bare
     * `exponential_pr_open` elsewhere). The contract PREFIX is required —
     * that is what keeps another MCP server's `issues_create` out.
     */
    fun row(name: String?): String? {
        val trimmed = name?.trim().orEmpty()
        if (trimmed.isEmpty()) return null
        return DomainContract.expToolNames.firstOrNull { row ->
            trimmed.length > row.length &&
                trimmed.endsWith(row) &&
                trimmed.dropLast(row.length).endsWith(DomainContract.expToolPrefix)
        }
    }

    /**
     * The row's whole presentation, or null when the call is not one of ours.
     * [settled] is the call's own state — a status landed, so the caption flips
     * from the progressive form to the done one. A FAILED call keeps the
     * generic failed styling; the caption still reads in the past tense,
     * because the call is over either way.
     */
    fun forName(name: String?, settled: Boolean): ExpToolRow? {
        val row = row(name) ?: return null
        val at = DomainContract.expToolNames.indexOf(row)
        val captions = if (settled) DomainContract.expToolDone else DomainContract.expToolProgressive
        return ExpToolRow(
            row = row,
            caption = captions.getOrNull(at) ?: row,
            subjectKey = DomainContract.expToolSubjectKeys.getOrNull(at).orEmpty(),
            result = DomainContract.expToolResults.getOrNull(at) ?: RESULT_NONE,
        )
    }
}
