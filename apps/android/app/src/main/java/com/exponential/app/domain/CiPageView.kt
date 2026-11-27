package com.exponential.app.domain

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonObjectBuilder
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject

/**
 * #EXP-2172 (fixtures/ci-page.json, pinned by fixtures/ci-page-view.json):
 * THE CI page as ONE genUI tree, a node-for-node port of web
 * `apps/web/src/lib/ci-page-view.ts` (`CiPageViewTest` replays every case).
 * Every rule is [CiPage]'s; this file only lays the page out. The screen
 * adds its own chrome (title, back) around the tree.
 */
object CiPageView {

    /** Events the tree fires. Context keys: `run`, `pr`, `device`; the
     *  pickers and tabs carry the control's own `value`. */
    object Events {
        const val TAB = "ci-tab"
        const val STATUS = "ci-filter-status"
        const val REPOSITORY = "ci-filter-repo"
        const val SOURCE = "ci-filter-source"
        const val OPEN_RUN = "ci-open-run"
        const val OPEN_PR = "ci-open-pr"
        const val OPEN_RUNNER = "ci-open-runner"
        const val RUNNERS = "ci-open-runners"
    }

    data class State(val tab: String = "runs", val filter: CiPage.RunFilter = CiPage.RunFilter(), val phone: Boolean = true)

    private val ids = CiPage.TestIds

    private fun trainTone(glyph: PrChecks.Glyph): String = when (glyph) {
        PrChecks.Glyph.Running -> CiPage.STATUS_TONES.getValue(CiRun.RunStatus.Running).second
        PrChecks.Glyph.Queued, PrChecks.Glyph.Skipped -> "neutral"
        PrChecks.Glyph.Failed -> "danger"
        PrChecks.Glyph.Passed -> "success"
    }

    private fun JsonObjectBuilder.call(key: String, name: String, args: JsonObjectBuilder.() -> Unit) {
        putJsonObject(key) {
            put("@call", name)
            putJsonObject("args", args)
        }
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

    private fun stack(id: String, direction: String, gap: String, children: List<JsonElement>) =
        node(id, "Stack", { put("direction", direction); put("gap", gap); put("align", "stretch") }, children)

    private fun option(label: String, value: String) = buildJsonObject { put("label", label); put("value", value) }

    private fun trainSection(group: CiPage.TrainGroup, id: String) = node(
        id,
        "Section",
        { put("title", group.title); put("caption", group.caption); put("icon", "pr-batch") },
        group.rows.map { row ->
            val rowId = "${ids.TRAIN_ROW}.${row.id}"
            node(rowId, "Row", {
                put("identifier", row.identifier)
                put("title", row.title)
                row.subtitle?.let { put("subtitle", it) }
                put("pressable", true)
            }) {
                putJsonObject("slots") {
                    put("leading", node("$rowId.glyph", "Icon", { put("name", row.glyph.wire); put("tone", trainTone(row.glyph)); put("size", "sm") }))
                    put("trailing", node("$rowId.badge", "Badge", { put("text", row.badge); put("variant", "secondary") }))
                }
                on("press", Events.OPEN_PR) { put("pr", row.pullRequestId) }
            }
        },
    )

    /** The filter bar (#EXP-2169 "one look"): ONE Toolbar of three
     *  `Select{button}` pickers (status, repository, source); wide adds the
     *  runner Chip at its end. */
    private fun filters(input: CiPage.Input, state: State): JsonObject {
        fun picker(id: String, label: String, value: String, options: List<JsonObject>, event: String) = node(id, "Select", {
            put("name", id)
            put("label", label)
            put("triggerVariant", "button")
            put("value", value)
            put("options", JsonArray(options))
        }) { on("change", event) }
        val labels = CiPage.Copy.FILTER_LABELS
        val status = picker(
            ids.STATUS_FILTER, labels.getValue("status"), state.filter.status,
            CiPage.Copy.FILTERS.map { (value, label) -> option(label, value) }, Events.STATUS,
        )
        val repository = picker(
            ids.REPO_FILTER, labels.getValue("repository"), state.filter.repositoryId ?: "all",
            CiPage.repoOptions(input).map { option(it.label, it.value) }, Events.REPOSITORY,
        )
        val source = picker(
            ids.SOURCE_FILTER, labels.getValue("source"), state.filter.source ?: "all",
            listOf(
                option(CiPage.Copy.SOURCES_ALL, "all"),
                option(CiPage.Copy.SOURCES.getValue("exponential"), "exponential"),
                option(CiPage.Copy.SOURCES.getValue("github"), "github"),
            ),
            Events.SOURCE,
        )
        val chip = if (state.phone) null else CiPage.runnerChip(input)
        val runners = chip?.let {
            node(ids.RUNNER_CHIP, "Chip", {
                put("label", it.label)
                put("dot", if (it.online) "\$color.success" else "\$color.mutedForeground")
                put("pressable", true)
            }) { on("press", Events.RUNNERS) }
        }
        return node(ids.FILTERS, "Toolbar", children = listOf(status, repository, source)) {
            if (runners != null) putJsonObject("slots") { put("trailing", runners) }
        }
    }

    /** One run's row. Wide: title, subtitle, trailing duration · age, the
     *  source mark and the author; phones: meta = duration | queued. */
    private fun runRow(row: CiPage.RunListRow, phone: Boolean): JsonObject {
        val id = "${ids.RUN_ROW}.${row.id}"
        val leading = node("$id.glyph", "Icon", { put("name", row.glyph.wire); put("tone", row.tone); put("size", "sm") })
        if (phone) {
            return node(id, "Row", {
                put("title", row.title)
                put("subtitle", row.phoneSubtitle)
                put("meta", row.phoneMeta)
                put("pressable", true)
            }) {
                putJsonObject("slots") { put("leading", leading) }
                on("press", Events.OPEN_RUN) { put("run", row.id) }
            }
        }
        val meta: JsonObjectBuilder.() -> Unit = {
            call("text", "fill") {
                val duration = row.duration
                put("template", if (duration != null) "{duration} · {age}" else "{queued} · {age}")
                putJsonObject("params") {
                    if (duration != null) put("duration", duration) else put("queued", CiPage.Copy.WAITING_FOR_RUNNER)
                    call("age", "formatRelativeTime") { put("value", row.at) }
                }
            }
            put("variant", "muted")
        }
        val trailing = mutableListOf<JsonElement>(
            node("$id.meta", "Text", meta),
            node(
                "$id.source", "Tooltip", { put("content", row.sourceLabel) },
                listOf(node("$id.source.icon", "Icon", { put("name", row.sourceIcon); put("size", "sm"); put("tone", "neutral"); put("label", row.sourceLabel) })),
            ),
        )
        row.author?.let { author ->
            trailing += node("$id.author", "Avatar", {
                put("name", author.name)
                author.image?.let { put("src", it) }
                put("seed", author.id)
                put("size", "sm")
            })
        }
        return node(id, "Row", { put("title", row.title); put("subtitle", row.subtitle); put("pressable", true) }) {
            putJsonObject("slots") {
                put("leading", leading)
                put("trailing", node("$id.trailing", "Stack", { put("direction", "horizontal"); put("gap", "sm"); put("align", "center") }, trailing))
            }
            on("press", Events.OPEN_RUN) { put("run", row.id) }
        }
    }

    /** The runs as day Sections (or the EmptyState): the Runs tab's body,
     *  and the repo page's Runs tab ([ciRepoRunsTree]). */
    private fun runSections(input: CiPage.Input, filter: CiPage.RunFilter, phone: Boolean, filtered: Boolean): JsonObject {
        val groups = CiPage.runGroups(input, filter)
        if (groups.isEmpty()) {
            return node(ids.RUNS, "EmptyState", {
                put("icon", "nav-ci")
                put("title", if (filtered) CiPage.Copy.EMPTY_FILTERED else CiPage.Copy.EMPTY_RUNS)
                put("size", "list")
            })
        }
        return stack(
            ids.RUNS, "vertical", "md",
            groups.map { group ->
                node(
                    "${ids.RUNS}.${group.id}", "Section",
                    { put("title", group.title); group.count?.let { put("count", it) } },
                    group.rows.map { runRow(it, phone) },
                )
            },
        )
    }

    private fun runsPanel(input: CiPage.Input, state: State): JsonObject {
        val filtered = state.filter.status != "all" || state.filter.repositoryId != null || state.filter.source != null
        return stack("${ids.PAGE}.runs", "vertical", "md", listOf(filters(input, state), runSections(input, state.filter, state.phone, filtered)))
    }

    /** THE REPO PAGE'S RUNS TAB (#EXP-2169 "one look"): the CI page's runs of
     *  ONE repository, the same day Sections and rows, no filter bar. Events:
     *  [Events.OPEN_RUN] `{run}`. */
    fun ciRepoRunsTree(input: CiPage.Input, repositoryId: String, phone: Boolean): JsonObject =
        runSections(input, CiPage.RunFilter("all", repositoryId, null), phone, false)

    private fun trainPanel(input: CiPage.Input): JsonObject {
        val groups = CiPage.trainGroups(input, true)
        return stack(
            "${ids.PAGE}.train", "vertical", "md",
            if (groups.isEmpty()) {
                listOf(node(ids.TRAIN, "EmptyState", { put("icon", "pr-batch"); put("title", CiPage.Copy.EMPTY_TRAIN); put("size", "list") }))
            } else {
                groups.map { trainSection(it, "${ids.TRAIN}.${it.repositoryId}") }
            },
        )
    }

    private fun runnersPanel(input: CiPage.Input): JsonObject {
        val rows = CiPage.runnerRows(input)
        if (rows.isEmpty()) {
            return node(ids.RUNNERS, "EmptyState", { put("icon", "server"); put("title", CiPage.Copy.EMPTY_RUNNERS); put("size", "list") })
        }
        return node(ids.RUNNERS, "Section", {}, rows.map { row ->
            val rowId = "${ids.RUNNER_ROW}.${row.id}"
            node(rowId, "Row", {
                put("icon", "server")
                put("title", row.title)
                put("subtitle", row.subtitle)
                val busy = row.busy
                val since = row.offlineSince
                if (busy != null) {
                    put("meta", busy)
                } else if (since != null) {
                    putJsonObject("meta") {
                        put("@call", "fill")
                        putJsonObject("args") {
                            put("template", CiPage.Copy.RUNNERS_OFFLINE_SINCE)
                            putJsonObject("params") {
                                putJsonObject("time") {
                                    put("@call", "formatRelativeTime")
                                    putJsonObject("args") { put("value", since) }
                                }
                            }
                        }
                    }
                }
                put("pressable", true)
            }) {
                row.queue?.let { queue ->
                    putJsonObject("slots") { put("trailing", node("$rowId.queue", "Badge", { put("text", queue); put("variant", "outline") })) }
                }
                on("press", Events.OPEN_RUNNER) { put("device", row.id) }
            }
        })
    }

    /** The CI page's body (ci-page.json LAYOUT): the CURRENT tab's panel.
     *  The tab switcher is the platform's own segmented control (#EXP-2169
     *  "one look": the board's and the repo page's switcher, counts from
     *  [CiPage.tabCounts]), drawn by the host above this tree. */
    fun ciPageTree(input: CiPage.Input, state: State): JsonObject = when (state.tab) {
        "runs" -> runsPanel(input, state)
        "train" -> trainPanel(input)
        else -> runnersPanel(input)
    }
}
