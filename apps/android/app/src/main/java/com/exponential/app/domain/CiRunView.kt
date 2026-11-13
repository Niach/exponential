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
        const val CANCEL = "ci-cancel"
    }

    /** Events the runner section fires. */
    object RunnerEvents {
        const val ENABLED = "runner-enabled"
        const val LABELS = "runner-labels"
        const val SLOTS = "runner-slots"
        const val TIMEOUT = "runner-timeout"
    }

    /** The pointer the ChipInput's `values` is bound to (the host writes the
     *  normalized list back, so a refused entry leaves the field). */
    const val RUNNER_LABELS_PATH = "/labels"

    /** Which button's call is in flight. */
    enum class Busy { Rerun, Cancel }

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

    /** One job's detail: its steps as a Trace, then its log. */
    private fun jobDetail(jobs: List<CiRun.Job>, jobId: String, log: String?, nowMs: Long, phone: Boolean): JsonObject {
        val ids = CiRun.TestIds
        val job = jobs.first { it.id == jobId }
        val steps = CiRun.steps(job.steps, nowMs)
        val view = CiRun.logView(log ?: "")
        val running = job.status != "completed"
        val children = mutableListOf<JsonElement>()
        if (steps.isNotEmpty()) {
            children += node(
                "${ids.STEPS}.${job.id}",
                "Trace",
                children = steps.mapIndexed { i, step ->
                    node("${ids.STEPS}.${job.id}.$i", "TraceItem", {
                        put("title", step.title)
                        put("status", step.status.wire)
                        if (!step.description.isNullOrEmpty()) put("description", step.description)
                    })
                },
            )
        }
        if (view.code.isEmpty() && job.status != "in_progress") {
            children += node("${ids.LOG}.${job.id}", "Text", { put("text", CiRun.Copy.EMPTY_LOG); put("variant", "muted") })
        } else {
            children += node("${ids.LOG}.${job.id}", "CodeBlock", {
                put("code", view.code)
                put("title", CiRun.Copy.LOG)
                put("language", "ansi")
                put("lineNumbers", true)
                put("wrap", false)
                put("copyable", true)
                put("maxLines", if (phone) CiRun.LOG_MAX_LINES_PHONE else CiRun.LOG_MAX_LINES_WIDE)
                put("variant", if (CiRun.isFailing(job)) "destructive" else "default")
                putJsonArray("highlight") { view.highlight.forEach { add(JsonPrimitive(it)) } }
                put("follow", running)
            })
        }
        return stack("ci-run-detail.${job.id}", "vertical", "md", "stretch", children)
    }

    /** The run page (ci-run.json LAYOUT). [log] = the SELECTED job's log
     *  text as read so far (null = not read yet). */
    fun ciRunTree(
        run: CiRun.Run,
        jobs: List<CiRun.Job>,
        selected: String?,
        log: String?,
        nowMs: Long,
        phone: Boolean,
        busy: Busy? = null,
    ): JsonObject {
        val ids = CiRun.TestIds
        val tiles = CiRun.tiles(run, jobs, nowMs)
        val actions = CiRun.actions(run.state, run.error)
        val children = mutableListOf<JsonElement>(
            stack(
                "ci-run-title", "vertical", "xs", "stretch",
                listOf(
                    node("ci-run-heading", "Heading", { put("text", CiRun.Copy.title(run.number)); put("level", "h2") }),
                    node("ci-run-name", "Text", { put("text", run.name); put("variant", "muted") }),
                ),
            ),
            node(
                ids.TILES,
                "Grid",
                { put("columns", if (phone) 2 else 4); put("gap", "md") },
                tiles.mapIndexed { i, tile ->
                    node("${ids.TILES}.$i", "StatTile", { put("label", tile.label); put("value", tile.value); put("tone", tile.tone.wire) })
                },
            ),
        )
        if (actions.rerunFailed || actions.cancel) {
            val buttons = mutableListOf<JsonElement>()
            if (actions.rerunFailed) {
                buttons += node(ids.RERUN, "Button", {
                    put("label", CiRun.Copy.RERUN_FAILED)
                    put("loading", busy == Busy.Rerun)
                    put("disabled", busy != null)
                }) { on("press", Events.RERUN) }
            }
            if (actions.cancel) {
                // ci-run.json: variant destructive-outline (the catalog's
                // `destructive`; it has no outline twin).
                buttons += node(ids.CANCEL, "Button", {
                    put("label", CiRun.Copy.CANCEL)
                    put("variant", "destructive")
                    put("loading", busy == Busy.Cancel)
                    put("disabled", busy != null)
                }) { on("press", Events.CANCEL) }
            }
            children += stack("ci-run-actions", "horizontal", "sm", "center", buttons)
        }
        val error = run.error
        if (error != null) {
            children += node("ci-run-error", "Alert", { put("type", "error"); put("title", CiRun.Copy.PIPELINE_ERROR); put("message", error) })
        } else if (jobs.isNotEmpty()) {
            val rows = CiRun.jobRows(jobs, nowMs)
            if (phone) {
                children += stack(
                    "ci-run-jobs", "vertical", "sm", "stretch",
                    rows.map { row ->
                        node(
                            "${ids.JOB_ROW}.${row.id}",
                            "Collapsible",
                            { put("title", row.title); put("open", row.id == selected) },
                            listOf(jobDetail(jobs, row.id, if (row.id == selected) log else null, nowMs, phone)),
                        ) { on("change", Events.SELECT) { put("job", row.id) } }
                    },
                )
            } else {
                val list = node(
                    "ci-run-jobs-list", "Box",
                    children = listOf(
                        node(
                            "ci-run-jobs-section", "Section", { put("title", CiRun.Copy.JOBS) },
                            rows.map { row ->
                                node(
                                    "${ids.JOB_ROW}.${row.id}",
                                    "Row",
                                    {
                                        put("title", row.title)
                                        row.subtitle?.let { put("subtitle", it) }
                                        row.duration?.let { put("meta", it) }
                                        put("selected", row.id == selected)
                                        put("pressable", true)
                                    },
                                ) {
                                    putJsonObject("slots") { put("leading", glyphNode("${ids.JOB_ROW}.${row.id}.glyph", row)) }
                                    on("press", Events.SELECT) { put("job", row.id) }
                                }
                            },
                        ),
                    ),
                ) { putJsonObject("style") { put("flexDirection", "column") } }
                val grid = mutableListOf<JsonElement>(list)
                if (selected != null) {
                    grid += node("ci-run-detail", "Box", children = listOf(jobDetail(jobs, selected, log, nowMs, phone))) {
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
        platform: String?,
        settings: RunnerSettings.Settings,
        owner: Boolean,
        refused: Boolean,
        recent: List<CiRun.JobRow>,
    ): JsonObject {
        val copy = RunnerSettings.Copy
        val ids = RunnerSettings.TestIds
        val disabled = !owner
        val notice = RunnerSettings.notice(client, caps)
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
                    put("disabled", disabled)
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
        }
        return null to false
    }
}
