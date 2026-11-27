package com.exponential.app.domain

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonObjectBuilder
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject

/**
 * #EXP-1617 / #EXP-1804 (fixtures/ci-run.json, fixtures/runner-settings.json):
 * the CI run page and a device's CI runner section as genUI trees (the core
 * catalog's StatTile, Section, Row, Collapsible, Trace, CodeBlock, Alert,
 * SettingsRow, Switch, ChipInput, NumberField, Select), painted by
 * `ExponentialSurface` so Android draws exactly what the catalogue draws ×4.
 * A line-for-line port of web `apps/web/src/lib/ci-run-view.ts`: the same
 * node ids, props and child order (`CiRunViewTest` replays web's trees).
 */
object CiRunView {

    /** Events the page's tree fires (action names). */
    object Events {
        const val SELECT = "ci-select-job"
        const val RERUN = "ci-rerun"
        const val RERUN_ALL = "ci-rerun-all"
        const val CANCEL = "ci-cancel"
        const val FIX = "ci-fix"
        const val OPEN_EXTERNAL = "ci-open-external"
        const val MENU = "ci-run-menu"
    }

    /** Events the runner section fires. */
    object RunnerEvents {
        const val ENABLED = "runner-enabled"
        const val LABELS = "runner-labels"
        const val SLOTS = "runner-slots"
        const val TIMEOUT = "runner-timeout"
        const val MEMORY = "runner-memory"
        const val PAUSE = "runner-pause"
    }

    /** The pointer the ChipInput's `values` is bound to (the host writes the
     *  normalized list back, so a refused entry leaves the field). */
    const val RUNNER_LABELS_PATH = "/labels"

    /** Which button's call is in flight. */
    enum class Busy { Rerun, RerunAll, Cancel }

    private fun iconTone(tone: PrChecks.Tone): String = when (tone) {
        PrChecks.Tone.Muted -> "neutral"
        PrChecks.Tone.Green -> "success"
        PrChecks.Tone.Red -> "danger"
    }

    private fun node(
        id: String,
        component: String,
        props: (JsonObjectBuilder.() -> Unit)? = null,
        children: List<JsonElement>? = null,
        extra: JsonObjectBuilder.() -> Unit = {},
    ): JsonObject = buildJsonObject {
        put("id", id)
        put("component", component)
        if (props != null) putJsonObject("props", props)
        extra()
        if (children != null) put("children", JsonArray(children))
    }

    private fun JsonObjectBuilder.on(event: String, name: String, context: (JsonObjectBuilder.() -> Unit)? = null) {
        putJsonObject("on") {
            putJsonObject(event) {
                putJsonObject("event") {
                    put("name", name)
                    if (context != null) putJsonObject("context", context)
                }
            }
        }
    }

    private fun stack(id: String, direction: String, gap: String, align: String, children: List<JsonElement>) =
        node(id, "Stack", { put("direction", direction); put("gap", gap); put("align", align) }, children)

    /** A job row's leading glyph, tinted by its tone. */
    private fun glyphNode(id: String, row: CiRun.JobRow) =
        node(id, "Icon", { put("name", row.glyph.wire); put("tone", iconTone(row.tone)); put("size", "sm") })

    private fun JsonObjectBuilder.call(key: String, name: String, args: JsonObjectBuilder.() -> Unit) {
        putJsonObject(key) {
            put("@call", name)
            putJsonObject("args", args)
        }
    }

    /** One job's Card: its summary line, the steps with the open step
     *  holding the log (or the log alone when the job reported no steps). */
    private fun jobCard(jobs: List<CiRun.Job>, jobId: String, log: String?, nowMs: Long, phone: Boolean): JsonObject {
        val ids = CiRun.TestIds
        val job = jobs.first { it.id == jobId }
        val steps = CiRun.steps(job.steps, nowMs)
        val view = CiRun.logView(log ?: "")
        val running = job.status != "completed"
        val logNode = if (view.code.isEmpty() && job.status != "in_progress") {
            node("${ids.LOG}.${job.id}", "Text", { put("text", CiRun.Copy.EMPTY_LOG); put("variant", "muted") })
        } else {
            node("${ids.LOG}.${job.id}", "CodeBlock", {
                put("code", view.code)
                put("title", CiRun.Copy.LOG)
                put("language", "ansi")
                put("lineNumbers", true)
                put("wrap", false)
                put("copyable", true)
                put("maxLines", if (phone) CiRun.LOG_MAX_LINES_PHONE else CiRun.LOG_MAX_LINES_WIDE)
                putJsonArray("highlight") { view.highlight.forEach { add(JsonPrimitive(it)) } }
                put("follow", running)
            })
        }
        val error = steps.indexOfFirst { it.status == CiRun.StepStatus.Error }
        val runningAt = steps.indexOfFirst { it.status == CiRun.StepStatus.Running }
        val openAt = if (error >= 0) error else if (runningAt >= 0) runningAt else steps.size - 1
        val children: List<JsonElement> = if (steps.isEmpty()) {
            listOf(logNode)
        } else {
            listOf(
                node(
                    "${ids.STEPS}.${job.id}", "Trace", { put("size", "sm") },
                    steps.mapIndexed { i, step ->
                        node(
                            "${ids.STEPS}.${job.id}.$i", "TraceItem",
                            {
                                put("title", step.title)
                                put("status", step.status.wire)
                                if (!step.description.isNullOrEmpty()) put("description", step.description)
                                put("collapsible", true)
                                put("open", i == openAt)
                            },
                            if (i == openAt) listOf(logNode) else null,
                        )
                    },
                ),
            )
        }
        return node(
            "${ids.JOB_CARD}.${job.id}", "Card",
            { put("title", job.check); put("description", CiRun.jobSummary(job, nowMs)) },
            children,
        )
    }

    /** The phones' page header over the tree: `name #n` over the repository. */
    fun ciRunHeader(run: CiRun.Run, repository: String?): Pair<String, String?> = "${run.name} #${run.number}" to repository

    /** The run page (ci-run.json LAYOUT). [log] = the SELECTED job's log
     *  as read so far (null = not read yet). Phones: [jobOpen] = drilled
     *  into the selected job (the tree is its Card alone). */
    fun ciRunTree(
        run: CiRun.Run,
        jobs: List<CiRun.Job>,
        selected: String?,
        log: String?,
        nowMs: Long,
        phone: Boolean,
        busy: Busy? = null,
        repository: String? = null,
        pullRequest: CiRun.PullRequest? = null,
        author: String? = null,
        jobOpen: Boolean = false,
    ): JsonObject {
        val ids = CiRun.TestIds
        val pick = selected ?: CiRun.defaultJob(jobs, null)
        if (phone && jobOpen && pick != null) {
            return stack(ids.PAGE, "vertical", "lg", "stretch", listOf(jobCard(jobs, pick, log, nowMs, phone)))
        }
        val head = mutableListOf<JsonElement>()
        head += node("ci-run-heading", "Heading", { put("text", run.title ?: run.name); put("level", if (phone) "h3" else "h2") })
        if (!phone) {
            head += node("ci-run-name", "Text", {
                call("text", "fill") {
                    put("template", "{workflow} · {repository}{age}")
                    putJsonObject("params") {
                        put("workflow", "${run.name} #${run.number}")
                        put("repository", repository ?: "")
                        call("age", "fill") {
                            put("template", " · {t}")
                            putJsonObject("params") {
                                call("t", "formatRelativeTime") { put("value", run.startedAt ?: run.createdAt) }
                            }
                        }
                    }
                }
                put("variant", "muted")
            })
        }
        val chips = node(
            ids.CHIPS, "Stack", { put("direction", "horizontal"); put("gap", "xs"); put("align", "center"); put("wrap", true) },
            CiRun.chips(run, jobs, pullRequest, author, nowMs).map { chip ->
                node("${ids.CHIPS}.${chip.id}", "Chip", {
                    put("label", chip.label)
                    chip.icon?.let { put("icon", it) }
                    chip.dot?.let { put("dot", it) }
                })
            },
        )
        val actions = CiRun.actions(run)
        fun button(id: String, label: String, event: String, which: Busy?, extra: JsonObjectBuilder.() -> Unit = {}) =
            node(id, "Button", {
                put("label", label)
                put("size", "sm")
                extra()
                if (which != null) put("loading", busy == which)
                put("disabled", busy != null)
            }) { on("press", event) }
        val buttons = mutableListOf<JsonElement>()
        when (actions.primary) {
            "cancel" -> buttons += button(ids.CANCEL, CiRun.Copy.CANCEL, Events.CANCEL, Busy.Cancel) { put("icon", "circle-stop") }
            "rerunFailed" -> buttons += button(ids.RERUN, CiRun.Copy.RERUN_FAILED, Events.RERUN, Busy.Rerun) { put("icon", "refresh-cw") }
            "rerunAll" -> buttons += button(ids.RERUN_ALL, CiRun.Copy.RERUN_ALL, Events.RERUN_ALL, Busy.RerunAll) { put("icon", "refresh-cw") }
        }
        if (actions.fix) buttons += button(ids.FIX, CiRun.Copy.FIX, Events.FIX, null) { put("variant", "outline"); put("icon", "bot") }
        if (actions.menu.isNotEmpty()) {
            buttons += node(
                ids.MORE, "Menu",
                {
                    put("label", CiRun.Copy.MORE)
                    put("align", "end")
                    putJsonArray("items") {
                        for (item in actions.menu) {
                            add(buildJsonObject {
                                put("label", if (item == "rerunAll") CiRun.Copy.RERUN_ALL else CiRun.Copy.OPEN_EXTERNAL)
                                put("value", item)
                            })
                        }
                    }
                },
                listOf(
                    node("${ids.MORE}.button", "Button", {
                        put("icon", "ellipsis")
                        put("variant", "ghost")
                        put("size", "icon")
                        put("label", CiRun.Copy.MORE)
                        put("disabled", busy != null)
                    }),
                ),
            ) { on("select", Events.MENU) }
        }
        val actionsNode = stack("ci-run-actions", "horizontal", "sm", "center", buttons)
        val children = mutableListOf<JsonElement>(
            stack("ci-run-title", "vertical", "xs", "stretch", head),
            if (phone) {
                stack("ci-run-bar", "vertical", "sm", "stretch", listOf(chips) + (if (buttons.isNotEmpty()) listOf(actionsNode) else emptyList()))
            } else {
                node("ci-run-bar", "Toolbar", children = listOf(chips)) {
                    if (buttons.isNotEmpty()) putJsonObject("slots") { put("trailing", actionsNode) }
                }
            },
        )
        val error = run.error
        if (error != null) {
            children += node("ci-run-error", "Alert", { put("type", "error"); put("title", CiRun.Copy.PIPELINE_ERROR); put("message", error) })
            return stack(ids.PAGE, "vertical", "lg", "stretch", children)
        }
        val failedJob = jobs.firstOrNull { it.id == pick && CiRun.jobStatus(it.status, it.conclusion) == CiRun.RunStatus.Failed }
        CiRun.failureSummary(failedJob?.check, if (failedJob != null) log else null)?.let { failure ->
            children += node(ids.FAILURE, "Alert", { put("type", "error"); put("title", failure.title); put("message", failure.message) })
        }
        if (jobs.isEmpty()) return stack(ids.PAGE, "vertical", "lg", "stretch", children)
        val rows = CiRun.jobRows(jobs, nowMs)
        val section = node(
            "ci-run-jobs-section", "Section",
            { put("title", CiRun.Copy.JOBS); put("caption", CiRun.jobsCaption(jobs.map { it.status to it.conclusion })) },
            rows.map { row ->
                node(
                    "${ids.JOB_ROW}.${row.id}", "Row",
                    {
                        put("title", row.title)
                        if (row.subtitle != null && !phone) put("subtitle", row.subtitle)
                        if (row.duration != null) put("meta", row.duration) else if (row.subtitle != null && phone) put("meta", row.subtitle)
                        put("selected", row.id == pick)
                        put("pressable", true)
                        if (phone) put("chevron", "right")
                    },
                ) {
                    putJsonObject("slots") {
                        put("leading", node("${ids.JOB_ROW}.${row.id}.glyph", "Icon", { put("name", row.glyph.wire); put("tone", row.statusTone.wire); put("size", "sm") }))
                    }
                    on("press", Events.SELECT) { put("job", row.id) }
                }
            },
        )
        if (phone) {
            children += section
        } else {
            val grid = mutableListOf<JsonElement>(
                node("ci-run-jobs-list", "Box", children = listOf(section)) { putJsonObject("style") { put("flexDirection", "column") } },
            )
            if (pick != null) {
                grid += node("ci-run-detail", "Box", children = listOf(jobCard(jobs, pick, log, nowMs, phone))) {
                    putJsonObject("style") { put("minWidth", 0); put("flexDirection", "column") }
                }
            }
            children += node("ci-run-jobs", "Box", children = grid) {
                putJsonObject("style") {
                    put("display", "grid")
                    put("gridTemplateColumns", "300px minmax(0, 1fr)")
                    put("gap", "\$spacing.lg")
                    put("alignItems", "start")
                }
            }
        }
        return stack(ids.PAGE, "vertical", "lg", "stretch", children)
    }

    // ── the CI runner section (runner-settings.json) ─────────────────────

    /** A ChipInput's entries as stored: each through `normalizeLabel`,
     *  duplicates dropped, at most `limits.labels`; `refused` = an entry the
     *  rule turned down (shown inline). */
    fun runnerLabelsFrom(entries: List<String>): Pair<List<String>, Boolean> {
        val labels = mutableListOf<String>()
        var refused = false
        for (entry in entries) {
            val label = RunnerSettings.normalizeLabel(entry)
            if (label == null) refused = true
            else if (label !in labels && labels.size < RunnerSettings.MAX_LABELS) labels += label
        }
        return labels to refused
    }

    /** The section (runner-settings.json LAYOUT). [client] = runnerNotice's client. */
    fun runnerSectionTree(
        client: String,
        caps: List<String>,
        kind: String?,
        platform: String?,
        settings: RunnerSettings.Settings,
        owner: Boolean,
        refused: Boolean,
        recent: List<CiRun.JobRow>,
    ): JsonObject {
        val copy = RunnerSettings.Copy
        val ids = RunnerSettings.TestIds
        val disabled = !owner
        val notice = RunnerSettings.notice(client, caps, kind, platform)
        val switchDisabled = disabled || (RunnerSettings.unsupported(kind, platform) && !settings.enabled)
        val os = RunnerSettings.osLabel(platform)
        val children = mutableListOf<JsonElement>()
        if (notice != null) {
            val n = copy.NOTICE.getValue(notice)
            children += node("runner-notice", "Alert", { put("type", n.type); put("title", n.title); put("message", n.message) })
        }
        children += node(
            "runner-enabled-row",
            "SettingsRow",
            { put("label", copy.ENABLED_LABEL); put("description", copy.ENABLED_DESCRIPTION) },
            listOf(
                node(ids.SWITCH, "Switch", {
                    put("label", "")
                    put("name", ids.SWITCH)
                    put("checked", settings.enabled)
                    put("disabled", switchDisabled)
                }) { on("change", RunnerEvents.ENABLED) },
            ),
        )
        if (settings.enabled) {
            children += node(ids.LABELS, "ChipInput", {
                put("label", copy.LABELS_LABEL)
                put("name", ids.LABELS)
                putJsonObject("values") { put("@path", RUNNER_LABELS_PATH) }
                put("placeholder", copy.LABELS_PLACEHOLDER)
                put("max", RunnerSettings.MAX_LABELS)
                putJsonArray("suggestions") {
                    if (os != null) add(buildJsonObject { put("label", os); put("value", os) })
                }
                put("disabled", disabled)
            }) { on("change", RunnerEvents.LABELS) }
            if (refused) {
                children += node("runner-labels-invalid", "Text", {
                    put("text", copy.LABELS_INVALID)
                    put("variant", "caption")
                    put("tone", "danger")
                })
            }
            children += node(ids.SLOTS, "NumberField", {
                put("label", copy.SLOTS)
                put("name", ids.SLOTS)
                put("value", settings.slots)
                put("min", RunnerSettings.MIN_SLOTS)
                put("max", RunnerSettings.MAX_SLOTS)
                put("step", 1)
                put("disabled", disabled)
            }) { on("change", RunnerEvents.SLOTS) }
            children += node(ids.TIMEOUT, "Select", {
                put("label", copy.TIMEOUT)
                put("name", ids.TIMEOUT)
                put("options", buildJsonArray {
                    for ((value, label) in RunnerSettings.TIMEOUT_OPTIONS) add(buildJsonObject { put("value", value); put("label", label) })
                })
                put("value", settings.timeoutMin.toString())
                put("disabled", disabled)
            }) { on("change", RunnerEvents.TIMEOUT) }
            children += node(ids.MEMORY, "Select", {
                put("label", copy.MEMORY)
                put("name", ids.MEMORY)
                put("options", buildJsonArray {
                    for ((value, label) in RunnerSettings.MEMORY_OPTIONS) add(buildJsonObject { put("value", value); put("label", label) })
                })
                put("value", settings.memoryMaxMb.toString())
                put("disabled", disabled)
            }) { on("change", RunnerEvents.MEMORY) }
            children += node(
                "runner-pause-row",
                "SettingsRow",
                { put("label", copy.PAUSE) },
                listOf(
                    node(ids.PAUSE, "Switch", {
                        put("label", "")
                        put("name", ids.PAUSE)
                        put("checked", settings.pauseWhenBusy)
                        put("disabled", disabled)
                    }) { on("change", RunnerEvents.PAUSE) },
                ),
            )
        }
        children += node("runner-recent-heading", "Heading", { put("text", copy.RECENT); put("level", "h4") })
        children += if (recent.isNotEmpty()) {
            stack(
                ids.RECENT, "vertical", "none", "stretch",
                recent.map { row ->
                    node("${ids.RECENT}.${row.id}", "Row", {
                        put("title", row.title)
                        row.subtitle?.let { put("subtitle", it) }
                        row.duration?.let { put("meta", it) }
                    }) { putJsonObject("slots") { put("leading", glyphNode("${ids.RECENT}.${row.id}.glyph", row)) } }
                },
            )
        } else {
            node(ids.RECENT, "Text", { put("text", copy.NO_JOBS); put("variant", "muted") })
        }
        return node(ids.SECTION, "Section", { put("title", copy.SECTION) }, children)
    }

    /** The settings a change event writes (null = nothing changed / refused)
     *  and whether a label entry was refused. */
    fun runnerSettingsAfter(
        current: RunnerSettings.Settings,
        event: String,
        payload: JsonObject?,
    ): Pair<RunnerSettings.Settings?, Boolean> {
        val p = payload ?: JsonObject(emptyMap())
        fun prim(key: String) = p[key] as? JsonPrimitive
        when (event) {
            RunnerEvents.ENABLED -> {
                val checked = prim("checked")?.takeIf { !it.isString }?.content?.toBooleanStrictOrNull()
                if (checked != null) return (if (checked == current.enabled) null else current.copy(enabled = checked)) to false
            }
            RunnerEvents.LABELS -> {
                val values = p["values"] as? JsonArray
                if (values != null) {
                    val (labels, refused) = runnerLabelsFrom(values.map { (it as? JsonPrimitive)?.content ?: it.toString() })
                    return (if (labels == current.labels) null else current.copy(labels = labels)) to refused
                }
            }
            RunnerEvents.SLOTS -> {
                val value = prim("value")?.takeIf { !it.isString }?.content?.toDoubleOrNull()
                if (value != null && value.isFinite()) {
                    val slots = kotlin.math.floor(value + 0.5).toInt().coerceIn(RunnerSettings.MIN_SLOTS, RunnerSettings.MAX_SLOTS)
                    return (if (slots == current.slots) null else current.copy(slots = slots)) to false
                }
            }
            RunnerEvents.TIMEOUT -> {
                val value = prim("value")?.takeIf { it.isString }?.content
                if (value != null) {
                    val known = RunnerSettings.TIMEOUT_OPTIONS.any { it.first == value }
                    val minutes = value.toIntOrNull()
                    return (if (!known || minutes == null || minutes == current.timeoutMin) null else current.copy(timeoutMin = minutes)) to false
                }
            }
            RunnerEvents.MEMORY -> {
                val value = prim("value")?.takeIf { it.isString }?.content
                if (value != null) {
                    val known = RunnerSettings.MEMORY_OPTIONS.any { it.first == value }
                    val mb = value.toIntOrNull()
                    return (if (!known || mb == null || mb == current.memoryMaxMb) null else current.copy(memoryMaxMb = mb)) to false
                }
            }
            RunnerEvents.PAUSE -> {
                val checked = prim("checked")?.takeIf { !it.isString }?.content?.toBooleanStrictOrNull()
                if (checked != null) return (if (checked == current.pauseWhenBusy) null else current.copy(pauseWhenBusy = checked)) to false
            }
        }
        return null to false
    }
}
