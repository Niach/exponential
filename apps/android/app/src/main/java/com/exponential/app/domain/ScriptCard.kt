package com.exponential.app.domain

// EXP-1279 (EXP-1259 D13): a code-mode call (`mcp__codemode__exec`, contract
// toolKind `script`) renders as ONE card, "Ran a script · 59 calls · 3.0 s",
// with the calls it made grouped by tool and the script itself behind a
// "Script" disclosure. Pure model, hand-ported from the TS reference
// `packages/domain-contract/src/script-card.ts` and locked to
// `fixtures/script-card.json` by ScriptCardTest. The Compose half is
// `ui/session/ScriptCard.kt`.

object ScriptCardSpec {
    const val SEPARATOR = " · "
    const val LOG_CAP = 200
    const val TOOL_NAME_SUFFIX = "codemode__exec"
    /** Codex titles the same call `mcp.codemode.exec` (wire doc §1b). */
    const val TOOL_NAME_SUFFIX_CODEX = "codemode.exec"
    const val TOOL_KIND = "script"

    // fixture `copy`
    const val RUNNING = "Running a script"
    const val DONE = "Ran a script"
    const val SCRIPT = "Script"
    const val CALL = "call"
    const val CALLS = "calls"
    const val FAILED = "failed"
    const val MORE_CALL = "more call"
    const val MORE_CALLS = "more calls"

    // fixture `testIds`
    const val TEST_ID_CARD = "script-card"
    const val TEST_ID_GROUP = "script-card-group"
    const val TEST_ID_SOURCE = "script-card-source"
}

/** One entry of codemode's call log (the settling `tool_update.script`). */
data class ScriptCall(
    /** `<server>.<tool>` as the script called it. */
    val tool: String,
    val subject: String? = null,
    val ok: Boolean = true,
    val ms: Long? = null,
)

/** The settled call's code-mode log as the wire carries it (STEER-WIRE §1b). */
data class ScriptLog(
    val count: Int? = null,
    val elapsedMs: Long? = null,
    val calls: List<ScriptCall> = emptyList(),
    val source: String? = null,
)

data class ScriptCardInput(
    val running: Boolean,
    /** Total calls the script made (the log is capped). */
    val count: Int? = null,
    val calls: List<ScriptCall> = emptyList(),
    val elapsedMs: Long? = null,
)

data class ScriptCardGroup(
    val tool: String,
    /** An Exponential tool (draw the brand mark), else the generic tool glyph. */
    val exponential: Boolean,
    val label: String,
    val subject: String?,
    /** `+54`, or null. */
    val more: String?,
    /** `×55`, or null; replaced by [failed] when calls failed. */
    val count: String?,
    /** `1 failed` (rose), or null. */
    val failed: String?,
)

data class ScriptCard(
    val title: String,
    val meta: String?,
    val groups: List<ScriptCardGroup>,
    /** `12 more calls`: the log's cap cut these off. */
    val overflow: String?,
)

object ScriptCards {

    /** A code-mode `exec` call: contract toolKind `script`, or (a publisher
     *  that predates the kind) a tool whose name ends in `codemode__exec`
     *  (claude) or `codemode.exec` (codex). */
    fun isScriptCall(name: String?, toolKind: String?): Boolean {
        if (toolKind == ScriptCardSpec.TOOL_KIND) return true
        val trimmed = name?.trim().orEmpty()
        return trimmed.endsWith(ScriptCardSpec.TOOL_NAME_SUFFIX) ||
            trimmed.endsWith(ScriptCardSpec.TOOL_NAME_SUFFIX_CODEX)
    }

    /** `3.0 s` under a minute (integer tenths, half up: 1250 ms = `1.3 s`,
     *  script-card.json `durations`), the Work card's coarse format above. */
    fun duration(ms: Long): String {
        if (ms >= 60_000) return WorkCards.durationShort(ms)
        val tenths = (maxOf(0L, ms) + 50) / 100
        return "${tenths / 10}.${tenths % 10} s"
    }

    private fun plural(count: Int, one: String, many: String) = "$count ${if (count == 1) one else many}"

    fun of(input: ScriptCardInput): ScriptCard {
        if (input.running) return ScriptCard(ScriptCardSpec.RUNNING, null, emptyList(), null)
        val calls = input.calls
        val count = maxOf(input.count ?: calls.size, calls.size)
        val meta = mutableListOf<String>()
        if (count > 0) meta += plural(count, ScriptCardSpec.CALL, ScriptCardSpec.CALLS)
        input.elapsedMs?.let { meta += duration(it) }

        val byTool = LinkedHashMap<String, MutableList<ScriptCall>>()
        for (call in calls) byTool.getOrPut(call.tool.trim()) { mutableListOf() } += call
        val groups = byTool.map { (tool, members) ->
            val name = tool.substring(tool.lastIndexOf('.') + 1)
            val display = ExpToolDisplay.forName(name, settled = true)
            val subject = members.map { it.subject?.trim().orEmpty() }.firstOrNull { it.isNotEmpty() }
            val failed = members.count { !it.ok }
            ScriptCardGroup(
                tool = tool,
                exponential = display != null,
                label = display?.caption ?: tool,
                subject = subject,
                more = if (subject != null && members.size > 1) "+${members.size - 1}" else null,
                count = if (failed == 0 && members.size > 1) "×${members.size}" else null,
                failed = if (failed > 0) "$failed ${ScriptCardSpec.FAILED}" else null,
            )
        }
        val cut = count - calls.size
        return ScriptCard(
            title = ScriptCardSpec.DONE,
            meta = meta.takeIf { it.isNotEmpty() }?.joinToString(ScriptCardSpec.SEPARATOR),
            groups = groups,
            overflow = if (cut > 0) {
                "$cut ${if (cut == 1) ScriptCardSpec.MORE_CALL else ScriptCardSpec.MORE_CALLS}"
            } else {
                null
            },
        )
    }
}
