package com.exponential.app.domain

// EXP-1324: the Trace patterns, ONE model ×4. The transcript's tool rows and
// groups, the reasoning line, the subagent line and the failed-tests line are
// all Trace lines (shadcn's Trace; the Compose half is `ui/components/Trace.kt`).
// Hand-ported from the TS reference `packages/domain-contract/src/trace.ts` and
// locked to `fixtures/trace.json` by TraceTest (whole objects).

/** A line's status: `running` shimmers the title, `error` paints the line
 *  destructive, `pending` dims it to [TraceSpec.PENDING_OPACITY]. */
enum class TraceStatus(val wire: String) {
    Running("running"),
    Error("error"),
    Pending("pending"),
    ;

    companion object {
        fun fromWire(value: String?): TraceStatus? = entries.firstOrNull { it.wire == value }
    }
}

/** One painted Trace line: a concept icon, the title, the optional muted
 *  description (tabular figures) and the status that tints/shimmers it. */
data class TraceLineModel(
    /** A concept name (packages/icons/icons.json). */
    val icon: String,
    val title: String,
    val description: String?,
    val status: TraceStatus?,
)

/** One size of the Trace geometry, in dp (fixture `geometry.default`/`sm`). */
data class TraceGeometry(
    val fontSize: Int,
    val lineHeight: Int,
    val icon: Int,
    val gap: Int,
    val lineGap: Int,
    val chevronGap: Int,
    val contentTop: Int,
    val contentBottom: Int,
    val railInset: Int,
    val railWidth: Int,
    val contentInset: Int,
    val contentTrailing: Int,
)

object TraceSpec {
    val DEFAULT = TraceGeometry(
        fontSize = 14, lineHeight = 20, icon = 16, gap = 8, lineGap = 8, chevronGap = 4,
        contentTop = 8, contentBottom = 4, railInset = 7, railWidth = 1, contentInset = 24,
        contentTrailing = 4,
    )
    val SM = TraceGeometry(
        fontSize = 12, lineHeight = 18, icon = 14, gap = 6, lineGap = 6, chevronGap = 4,
        contentTop = 6, contentBottom = 4, railInset = 6, railWidth = 1, contentInset = 20,
        contentTrailing = 4,
    )
    const val DESCRIPTION_ALPHA = 0.7f
    const val PENDING_OPACITY = 0.6f

    /** fixture `geometry.codeBlock` (dp; radius = the `lg` token). */
    object CodeBlock {
        const val BORDER = 1
        const val FILL_ALPHA = 0.3f
        const val PADDING = 10
        const val CONTENT_TOP_WITH_HEADER = 6
        const val TITLE_SIZE = 12
        const val FONT_SIZE = 12
        const val LINE_HEIGHT = 19.5f
        const val COPY_BUTTON = 24
        const val COPY_ICON = 12
        const val HEADER_ACTIONS_OUTSET = 4
        const val MAX_LINES = 16
        const val DESTRUCTIVE_BORDER_ALPHA = 0.4f
    }

    // fixture `icons`
    const val ICON_TOOL = "trace-tool"
    const val ICON_REASONING = "trace-reasoning"
    const val ICON_SUBAGENT = "trace-subagent"
    const val ICON_FAILED = "trace-failed"
    const val ICON_CHEVRON = "trace-chevron"

    // fixture `copy`
    const val OUTPUT = "Output"
    const val COPY = "Copy"
    const val COPIED = "Copied"

    // fixture `tool`
    const val TOOL_UNNAMED = "Tool"

    data class Verbs(val running: String, val settled: String, val blank: String, val icon: String)

    val VERBS: Map<String, Verbs> = mapOf(
        "execute" to Verbs("Running", "Ran", "a command", "trace-terminal"),
        "read" to Verbs("Reading", "Read", "a file", "trace-read"),
        "edit" to Verbs("Editing", "Edited", "a file", "trace-edit"),
        "delete" to Verbs("Deleting", "Deleted", "a file", "trace-edit"),
        "move" to Verbs("Moving", "Moved", "a file", "trace-edit"),
        "search" to Verbs("Searching", "Searched", "the code", "trace-search"),
        "fetch" to Verbs("Fetching", "Fetched", "a page", "trace-fetch"),
        "think" to Verbs("Thinking", "Thought", "it through", "trace-reasoning"),
        "wait" to Verbs("Waiting on", "Waited on", "a task", "trace-background"),
    )

    // fixture `reasoning`
    const val REASONING_RUNNING = "Thinking"
    const val REASONING_SETTLED = "Thought for {duration}"
    const val REASONING_BRIEF = "Thought for a moment"

    // fixture `subagent`
    const val SUBAGENT_NAMED_RUNNING = "Running the {label} agent"
    const val SUBAGENT_NAMED_SETTLED = "Ran the {label} agent"
    const val SUBAGENT_NAMED_FAILED = "The {label} agent failed"
    const val SUBAGENT_UNNAMED_RUNNING = "Running a subagent"
    const val SUBAGENT_UNNAMED_SETTLED = "Ran a subagent"
    const val SUBAGENT_UNNAMED_FAILED = "A subagent failed"

    // fixture `tests`
    const val TESTS_TITLE = "Failed: {failed} of {total} {tests}"
    const val TESTS_TITLE_IN = "Failed: {failed} of {total} {tests} in {suite}"
    const val TESTS_TEST = "test"
    const val TESTS_TESTS = "tests"
}

/** A command's output read as a test run's summary. */
data class TestResults(
    val passed: Int,
    val failed: Int,
    val total: Int,
    /** The ONE suite every failure names; null when none or several. */
    val suite: String?,
)

object Trace {

    private val WHITESPACE = Regex("\\s+")
    private val PLACEHOLDER = Regex("\\{(\\w+)\\}")

    internal fun clean(value: String?): String = (value ?: "").replace(WHITESPACE, " ").trim()

    private fun fill(template: String, values: Map<String, String>): String =
        PLACEHOLDER.replace(template) { values[it.groupValues[1]] ?: "" }

    // ── Tool line ────────────────────────────────────────────────────────────

    /** A tool call as its Trace line. Verb kinds read `{verb} {detail}`
     *  ("Ran bun test"); a blank detail takes the kind's `blank` object;
     *  `other`/unknown kinds keep the tool's own name as the title and the
     *  detail as the description. */
    fun tool(toolKind: String?, name: String, detail: String?, settled: Boolean, failed: Boolean): TraceLineModel {
        val status = when {
            failed -> TraceStatus.Error
            settled -> null
            else -> TraceStatus.Running
        }
        val verbs = TraceSpec.VERBS[toolKind]
        val cleaned = clean(detail)
        if (verbs == null) {
            return TraceLineModel(
                icon = TraceSpec.ICON_TOOL,
                title = clean(name).ifEmpty { TraceSpec.TOOL_UNNAMED },
                description = cleaned.ifEmpty { null },
                status = status,
            )
        }
        val verb = if (settled || failed) verbs.settled else verbs.running
        return TraceLineModel(
            icon = verbs.icon,
            title = "$verb ${cleaned.ifEmpty { verbs.blank }}",
            description = null,
            status = status,
        )
    }

    // ── Reasoning line ───────────────────────────────────────────────────────

    fun reasoning(running: Boolean, durationMs: Long?): TraceLineModel {
        if (running) {
            return TraceLineModel(TraceSpec.ICON_REASONING, TraceSpec.REASONING_RUNNING, null, TraceStatus.Running)
        }
        val title = if (durationMs == null || durationMs < 1000) {
            TraceSpec.REASONING_BRIEF
        } else {
            fill(TraceSpec.REASONING_SETTLED, mapOf("duration" to WorkCards.durationLong(durationMs)))
        }
        return TraceLineModel(TraceSpec.ICON_REASONING, title, null, null)
    }

    // ── Subagent line ────────────────────────────────────────────────────────

    /** The badge is `description` here; clients paint it as the outline
     *  Badge, not as muted text. */
    fun subagent(label: String, running: Boolean, failed: Boolean, durationMs: Long?): TraceLineModel {
        val cleaned = clean(label)
        val template = when {
            cleaned.isNotEmpty() && failed -> TraceSpec.SUBAGENT_NAMED_FAILED
            cleaned.isNotEmpty() && running -> TraceSpec.SUBAGENT_NAMED_RUNNING
            cleaned.isNotEmpty() -> TraceSpec.SUBAGENT_NAMED_SETTLED
            failed -> TraceSpec.SUBAGENT_UNNAMED_FAILED
            running -> TraceSpec.SUBAGENT_UNNAMED_RUNNING
            else -> TraceSpec.SUBAGENT_UNNAMED_SETTLED
        }
        return TraceLineModel(
            icon = TraceSpec.ICON_SUBAGENT,
            title = fill(template, mapOf("label" to cleaned)),
            description = if (durationMs == null || durationMs < 0) null else WorkCards.durationLong(durationMs),
            status = when {
                failed -> TraceStatus.Error
                running -> TraceStatus.Running
                else -> null
            },
        )
    }

    // ── Failed tests line ────────────────────────────────────────────────────

    private val ANSI_CSI = Regex("\u001b\\[[0-?]*[ -/]*[@-~]")
    private val NEWLINES = Regex("\\r\\n?")
    private val ML = setOf(RegexOption.MULTILINE)

    private val JEST = Regex(
        "^Tests:\\s+(?:(\\d+) failed, )?(?:\\d+ skipped, )?(?:\\d+ todo, )?(?:(\\d+) passed, )?(\\d+) total",
        ML,
    )
    private val VITEST = Regex("^\\s*Tests\\s+(?:(\\d+) failed)?(?:\\s*\\|\\s*)?(?:(\\d+) passed)?[^(\\n]*\\((\\d+)\\)", ML)
    private val CARGO = Regex("^test result: \\w+\\. (\\d+) passed; (\\d+) failed;", ML)
    private val XCODE = Regex("^\\s*Executed (\\d+) tests?, with (\\d+) failures?", ML)
    private val GRADLE = Regex("^(\\d+) tests? completed, (\\d+) failed", ML)
    private val PYTEST = Regex("^=+ (?:(\\d+) failed)?(?:, )?(?:(\\d+) passed)?[^=\\n]* in [\\d.]+s(?: \\([^)]*\\))? =+$", ML)
    private val BUN_PASS = Regex("^\\s*(\\d+) pass$", ML)
    private val BUN_FAIL = Regex("^\\s*(\\d+) fail$", ML)

    private val SUITE_BUN = Regex("^\\(fail\\) (.+?) > ")
    private val SUITE_JEST = Regex("^\\s*FAIL\\s+(\\S+?)(?:\\s+>\\s+(.+?)\\s+>)?(?:\\s|$)")
    private val SUITE_GRADLE = Regex("^(\\S+) > .* FAILED$")
    private val SUITE_PYTEST = Regex("^FAILED (\\S+?)::")
    private val SUITE_XCODE = Regex("^Test Case '-\\[(?:\\w+\\.)?(\\w+) \\w+\\]' failed")
    private val SUITE_CARGO = Regex("^---- (?:(\\S+)::)?\\S+ stdout ----$")

    private fun MatchResult.int(group: Int): Int = groups[group]?.value?.toIntOrNull() ?: 0
    private fun MatchResult.has(group: Int): Boolean = groups[group] != null

    private data class Counts(val passed: Int, val failed: Int, val total: Int)

    /** Parses a command's output as a test run's summary (first runner whose
     *  summary matches wins). Null = not test output. */
    fun parseTestResults(output: String?): TestResults? {
        if (output.isNullOrEmpty()) return null
        val text = output.replace(ANSI_CSI, "").replace(NEWLINES, "\n")
        var counts: Counts? = null

        JEST.find(text)?.let { counts = Counts(passed = it.int(2), failed = it.int(1), total = it.int(3)) }
        if (counts == null) {
            VITEST.find(text)?.takeIf { it.has(1) || it.has(2) }?.let {
                counts = Counts(passed = it.int(2), failed = it.int(1), total = it.int(3))
            }
        }
        if (counts == null) {
            val hits = CARGO.findAll(text).toList()
            if (hits.isNotEmpty()) {
                val passed = hits.sumOf { it.int(1) }
                val failed = hits.sumOf { it.int(2) }
                counts = Counts(passed, failed, passed + failed)
            }
        }
        if (counts == null) {
            XCODE.findAll(text).lastOrNull()?.let {
                val total = it.int(1)
                val failed = it.int(2)
                counts = Counts(total - failed, failed, total)
            }
        }
        if (counts == null) {
            GRADLE.find(text)?.let { counts = Counts(it.int(1) - it.int(2), it.int(2), it.int(1)) }
        }
        if (counts == null) {
            PYTEST.find(text)?.takeIf { it.has(1) || it.has(2) }?.let {
                counts = Counts(it.int(2), it.int(1), it.int(1) + it.int(2))
            }
        }
        if (counts == null) {
            val pass = BUN_PASS.find(text)
            val fail = BUN_FAIL.find(text)
            if (pass != null || fail != null) {
                val passed = pass?.int(1) ?: 0
                val failed = fail?.int(1) ?: 0
                counts = Counts(passed, failed, passed + failed)
            }
        }
        val found = counts ?: return null
        return TestResults(found.passed, found.failed, found.total, failedSuite(text))
    }

    /** `src/bench/StartupBenchmark.test.ts` → `StartupBenchmark`. */
    private fun fileStem(path: String): String = path.split('/').last().split('.').first()

    private fun failedSuite(text: String): String? {
        val suites = linkedSetOf<String>()
        fun add(value: String?) {
            val name = clean(value)
            if (name.isNotEmpty()) suites.add(name)
        }
        for (line in text.split('\n')) {
            // First rule that matches the line wins (the reference's else-if).
            val bun = SUITE_BUN.find(line)
            if (bun != null) { add(bun.groupValues[1]); continue }
            val jest = SUITE_JEST.find(line)
            if (jest != null) { add(jest.groups[2]?.value ?: fileStem(jest.groupValues[1])); continue }
            val gradle = SUITE_GRADLE.find(line)
            if (gradle != null) { add(gradle.groupValues[1].split('.').last()); continue }
            val pytest = SUITE_PYTEST.find(line)
            if (pytest != null) { add(fileStem(pytest.groupValues[1])); continue }
            val xcode = SUITE_XCODE.find(line)
            if (xcode != null) { add(xcode.groupValues[1]); continue }
            SUITE_CARGO.find(line)?.let { add(it.groups[1]?.value) }
        }
        return suites.singleOrNull()
    }

    /** The failed-tests line for a SETTLED command whose output parsed with
     *  at least one failure; null otherwise. */
    fun failedTests(results: TestResults?): TraceLineModel? {
        if (results == null || results.failed <= 0) return null
        val tests = if (results.total == 1) TraceSpec.TESTS_TEST else TraceSpec.TESTS_TESTS
        val title = fill(
            if (results.suite != null) TraceSpec.TESTS_TITLE_IN else TraceSpec.TESTS_TITLE,
            mapOf(
                "failed" to results.failed.toString(),
                "total" to results.total.toString(),
                "tests" to tests,
                "suite" to (results.suite ?: ""),
            ),
        )
        return TraceLineModel(TraceSpec.ICON_FAILED, title, null, TraceStatus.Error)
    }
}
