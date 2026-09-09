package com.exponential.app.domain

/**
 * One tool call as the group caption sees it. `kind` is a contract `toolKind`
 * value; `detail` is the path (or command / query) the row carries.
 */
data class ToolCallSummary(
    val kind: String,
    val detail: String? = null,
    val failed: Boolean = false,
)

/**
 * EXP-785: the ONE caption a collapsed tool group renders, derived from the
 * group's tool rows. Hand-mirrored ×4 (web `@exp/domain-contract`
 * `toolGroupSummary`, iOS `ToolGroupSummary.swift`, desktop
 * `steer::tool_group_summary`) and byte-locked by
 * `packages/domain-contract/fixtures/tool-group-summary.json`, which every
 * client's test runs.
 *
 * Rules:
 * - `kind` is a contract `toolKind` value (`read`, `edit`, `delete`, `move`,
 *   `search`, `execute`, `think`, `fetch`, `switch_mode`, `other`); an unknown
 *   kind counts as `other`, so an older client never chokes on a newer agent.
 * - `edit`/`delete`/`move` are "edited N files", `read` is "read N files",
 *   each DEDUPED by `detail` (the path). A null/empty detail is its own
 *   distinct file every time.
 * - Fixed segment order: ran N commands · edited N files · read N files ·
 *   searched N times · fetched N pages · N other tools · N failed. Zero
 *   segments are omitted; `failed` counts calls with `failed` of ANY kind and
 *   is always last.
 * - Only the first character of the whole caption is capitalised.
 * - Every call `think`/`switch_mode`/`other` → `Used N tools` (plus
 *   ` · N failed`); no calls at all → `No tool calls`.
 */
object ToolGroupSummary {
    /** The segment separator: space, MIDDLE DOT (U+00B7), space. */
    const val SEPARATOR: String = " · "

    private fun count(n: Int, singular: String, plural: String = "${singular}s"): String =
        "$n ${if (n == 1) singular else plural}"

    fun summarize(calls: List<ToolCallSummary>): String {
        if (calls.isEmpty()) return "No tool calls"
        var commands = 0
        var searches = 0
        var fetches = 0
        var other = 0
        var failed = 0
        val edited = HashSet<String>()
        var editedBlank = 0
        val read = HashSet<String>()
        var readBlank = 0
        for (call in calls) {
            if (call.failed) failed += 1
            val detail = call.detail?.takeIf { it.isNotEmpty() }
            when (call.kind) {
                "execute" -> commands += 1
                "edit", "delete", "move" ->
                    if (detail != null) edited.add(detail) else editedBlank += 1
                "read" ->
                    if (detail != null) read.add(detail) else readBlank += 1
                "search" -> searches += 1
                "fetch" -> fetches += 1
                else -> other += 1
            }
        }
        val editedCount = edited.size + editedBlank
        val readCount = read.size + readBlank
        val segments = ArrayList<String>()
        if (commands > 0) segments.add("ran ${count(commands, "command")}")
        if (editedCount > 0) segments.add("edited ${count(editedCount, "file")}")
        if (readCount > 0) segments.add("read ${count(readCount, "file")}")
        if (searches > 0) segments.add("searched ${count(searches, "time")}")
        if (fetches > 0) segments.add("fetched ${count(fetches, "page")}")
        if (segments.isEmpty()) {
            segments.add("used ${count(other, "tool")}")
        } else if (other > 0) {
            segments.add(count(other, "other tool"))
        }
        if (failed > 0) segments.add("$failed failed")
        val text = segments.joinToString(SEPARATOR)
        return text.replaceFirstChar { it.uppercaseChar() }
    }
}
