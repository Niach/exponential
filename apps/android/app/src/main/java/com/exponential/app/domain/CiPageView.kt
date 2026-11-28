package com.exponential.app.domain

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonObjectBuilder
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject

/**
 * #EXP-2172 (fixtures/ci-page.json, pinned by fixtures/ci-page-view.json):
 * THE CI page as ONE genUI tree, a node-for-node port of web
 * `apps/web/src/lib/ci-page-view.ts` (`CiPageViewTest` replays every case).
 * #EXP-2214: four tabs, Merge queue · Pull requests · Releases · Runners.
 * Every rule is [CiPage]'s; this file only lays the page out. The screen
 * adds its own chrome (title, back) and the tab switcher around the tree.
 */
object CiPageView {

    /** Events the tree fires. Context keys: `run`, `job`, `pr`, `device`,
     *  `url`; the pickers, tabs and menus carry the control's own `value`. */
    object Events {
        const val TAB = "ci-tab"
        const val STATUS = "ci-filter-status"
        const val REPOSITORY = "ci-filter-repo"
        const val SOURCE = "ci-filter-source"
        const val OPEN_RUN = "ci-open-run"
        const val OPEN_JOB = "ci-open-job"
        const val OPEN_URL = "ci-open-url"
        const val OPEN_PR = "ci-open-pr"
        const val OPEN_RUNNER = "ci-open-runner"
        const val RUNNERS = "ci-open-runners"
        /** The Up next ⋯ menu: `value` = merge-now | hold, context `pr`. */
        const val QUEUE_MENU = "ci-queue-menu"
        const val RESUME = "ci-resume"
    }

    data class State(val tab: String = "train", val filter: CiPage.RunFilter = CiPage.RunFilter(), val phone: Boolean = true)

    private val ids = CiPage.TestIds

    private val DOT = mapOf(
        "neutral" to "\$color.mutedForeground",
        "info" to "\$color.info",
        "success" to "\$color.success",
        "danger" to "\$color.destructive",
        "warning" to "\$color.warning",
    )

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

    private fun event(name: String, context: (JsonObjectBuilder.() -> Unit)? = null) = buildJsonObject {
        putJsonObject("event") {
            put("name", name)
            if (context != null) putJsonObject("context", context)
        }
    }

    private fun JsonObjectBuilder.on(trigger: String, name: String, context: (JsonObjectBuilder.() -> Unit)? = null) {
        val action = event(name, context)
        putJsonObject("on") { put(trigger, action) }
    }

    private fun vstack(id: String, children: List<JsonElement>, gap: String = "md") =
        node(id, "Stack", { put("direction", "vertical"); put("gap", gap); put("align", "stretch") }, children)

    private fun hstack(id: String, children: List<JsonElement>) =
        node(id, "Stack", { put("direction", "horizontal"); put("gap", "sm"); put("align", "center") }, children)

    private fun relative(value: String) = buildJsonObject {
        put("@call", "formatRelativeTime")
        putJsonObject("args") { put("value", value) }
    }

    private fun option(label: String, value: String) = buildJsonObject { put("label", label); put("value", value) }

    private fun icon(id: String, name: String, tone: String) = node(id, "Icon", { put("name", name); put("tone", tone); put("size", "sm") })

    private fun avatar(id: String, user: CiPage.User) = node(id, "Avatar", {
        put("name", user.name)
        user.image?.let { put("src", it) }
        put("seed", user.id)
        put("size", "sm")
    })

    /** The filter bar (#EXP-2169 "one look"): ONE Toolbar of `Select{button}`
     *  pickers, status and repository (#EXP-2214: the source picker is
     *  gone); wide adds the runner Chip at its end. */
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
        val chip = if (state.phone) null else CiPage.runnerChip(input)
        val runners = chip?.let {
            node(ids.RUNNER_CHIP, "Chip", {
                put("label", it.label)
                put("dot", if (it.online) "\$color.success" else "\$color.mutedForeground")
                put("pressable", true)
            }) { on("press", Events.RUNNERS) }
        }
        return node(ids.FILTERS, "Toolbar", children = listOf(status, repository)) {
            if (runners != null) putJsonObject("slots") { put("trailing", runners) }
        }
    }

    private fun JsonObjectBuilder.openRun(run: String) = on("press", Events.OPEN_RUN) { put("run", run) }
    private fun JsonObjectBuilder.openPr(pr: String) = on("press", Events.OPEN_PR) { put("pr", pr) }

    private fun fill(template: String, params: JsonObjectBuilder.() -> Unit) = buildJsonObject {
        put("@call", "fill")
        putJsonObject("args") {
            put("template", template)
            putJsonObject("params", params)
        }
    }

    /** One run's row. Wide: title, subtitle, trailing duration · age, the
     *  source mark and the author; phones: meta = duration | queued. */
    private fun runRow(row: CiPage.RunListRow, phone: Boolean): JsonObject {
        val id = "${ids.RUN_ROW}.${row.id}"
        val leading = icon("$id.glyph", row.glyph.wire, row.tone)
        if (phone) {
            return node(id, "Row", {
                put("title", row.title)
                put("subtitle", row.phoneSubtitle)
                put("meta", row.phoneMeta)
                put("pressable", true)
            }) {
                putJsonObject("slots") { put("leading", leading) }
                openRun(row.id)
            }
        }
        val duration = row.duration
        val meta = if (duration != null) {
            fill("{duration} · {age}") { put("duration", duration); put("age", relative(row.at)) }
        } else {
            fill("{queued} · {age}") { put("queued", CiPage.Copy.WAITING_FOR_RUNNER); put("age", relative(row.at)) }
        }
        val trailing = mutableListOf<JsonElement>(
            node("$id.meta", "Text", { put("text", meta); put("variant", "muted") }),
            node(
                "$id.source", "Tooltip", { put("content", row.sourceLabel) },
                listOf(node("$id.source.icon", "Icon", { put("name", row.sourceIcon); put("size", "sm"); put("tone", "neutral"); put("label", row.sourceLabel) })),
            ),
        )
        row.author?.let { trailing += avatar("$id.author", it) }
        return node(id, "Row", { put("title", row.title); put("subtitle", row.subtitle); put("pressable", true) }) {
            putJsonObject("slots") {
                put("leading", leading)
                put("trailing", hstack("$id.trailing", trailing))
            }
            openRun(row.id)
        }
    }

    /** Runs as day Sections (In progress first), or the EmptyState. */
    private fun runSections(id: String, input: CiPage.Input, filter: CiPage.RunFilter, kinds: List<String>, phone: Boolean, empty: String): JsonObject {
        val groups = CiPage.runGroups(input, filter, kinds)
        if (groups.isEmpty()) return node(id, "EmptyState", { put("icon", "nav-ci"); put("title", empty); put("size", "list") })
        return vstack(
            id,
            groups.map { group ->
                node("$id.${group.id}", "Section", { put("title", group.title); group.count?.let { put("count", it) } }, group.rows.map { runRow(it, phone) })
            },
        )
    }

    private fun filtered(filter: CiPage.RunFilter) = filter.status != "all" || filter.repositoryId != null

    // ── Pull requests ─────────────────────────────────────────────────

    /** One PR (or branch): its rolled-up status, the deciding run's line
     *  and, wide, the merge queue pill; a press opens that run (#EXP-2214:
     *  the runs do not fold under it, genUI's foldable Row drops its slots). */
    private fun subjectRow(row: CiPage.SubjectRow, phone: Boolean): JsonObject {
        val id = "${ids.PR_ROW}.${row.id}"
        val trailing = if (phone) {
            null
        } else {
            hstack(
                "$id.trailing",
                listOfNotNull(
                    node("$id.meta", "Text", { put("text", row.meta); put("variant", "muted") }),
                    row.pill?.let { pill -> node("$id.pill", "Chip", { put("label", pill.text); put("dot", DOT.getValue(pill.tone)) }) },
                ),
            )
        }
        return node(id, "Row", {
            row.identifier?.let { put("identifier", it) }
            put("title", row.title)
            put("subtitle", if (phone) row.phoneSubtitle else row.subtitle)
            if (phone) put("meta", row.meta)
            put("pressable", true)
        }) {
            putJsonObject("slots") {
                put("leading", icon("$id.glyph", row.glyph.wire, row.tone))
                if (trailing != null) put("trailing", trailing)
            }
            openRun(row.runId)
        }
    }

    private fun subjectSections(id: String, groups: List<CiPage.SubjectGroup>, phone: Boolean, empty: String): JsonObject {
        if (groups.isEmpty()) return node(id, "EmptyState", { put("icon", "git-pull-request"); put("title", empty); put("size", "list") })
        return vstack(
            id,
            groups.map { group ->
                node("$id.${group.id}", "Section", {
                    put("title", group.title)
                    put("count", group.count)
                    if (!group.open) { put("collapsible", true); put("open", false) }
                }, group.rows.map { subjectRow(it, phone) })
            },
        )
    }

    // ── Merge queue ───────────────────────────────────────────────────

    private fun nowCard(now: CiPage.QueueNow, id: String): JsonObject {
        val copy = CiPage.Copy.Queue
        val progress = now.progress
        val children = mutableListOf<JsonElement>(
            if (progress == null) {
                node("$id.elapsed", "Text", { put("text", now.elapsed); put("variant", "muted") })
            } else {
                node("$id.progress", "Progress", {
                    put("value", progress)
                    put("max", 100)
                    put("label", now.elapsed)
                    put("valueText", now.eta)
                    put("tone", "info")
                })
            },
            node("$id.members", "Section", { put("title", copy.BATCH); put("count", now.members.size) }, now.members.map { m ->
                node("$id.member.${m.pullRequestId}", "Row", { put("identifier", m.identifier); put("title", m.title); put("pressable", true) }) {
                    m.author?.let { author -> putJsonObject("slots") { put("trailing", avatar("$id.member.${m.pullRequestId}.author", author)) } }
                    openPr(m.pullRequestId)
                }
            }),
        )
        now.runId?.let { runId ->
            children += hstack("$id.actions", listOf(
                node("$id.open", "Button", {
                    put("label", copy.OPEN_RUN)
                    put("icon", "external-link")
                    put("variant", "outline")
                    put("size", "sm")
                }) { openRun(runId) },
            ))
        }
        return node(id, "Card", { put("title", copy.TESTING); put("description", now.subtitle) }, listOf(vstack("$id.body", children)))
    }

    private fun queueRow(row: CiPage.QueueRow, id: String, phone: Boolean): JsonObject {
        val copy = CiPage.Copy.Queue
        val trailing = mutableListOf<JsonObject>()
        val at = row.at
        if (at != null && !phone) trailing += node("$id.at", "Text", { put("text", relative(at)); put("variant", "muted") })
        row.badge?.let { badge -> trailing += node("$id.badge", "Chip", { put("label", badge.text); put("dot", DOT.getValue(badge.tone)) }) }
        if (row.action == "open-log") {
            val jobId = row.jobId
            val press = if (jobId != null) {
                event(Events.OPEN_JOB) { put("run", row.runId); put("job", jobId) }
            } else {
                event(Events.OPEN_URL) { put("url", row.url) }
            }
            trailing += node("$id.log", "Button", {
                put("label", copy.OPEN_LOG)
                put("icon", "file-text")
                put("variant", "outline")
                put("size", "sm")
            }) { putJsonObject("on") { put("press", press) } }
        }
        if (row.action == "resume") {
            trailing += node("$id.resume", "Button", { put("label", copy.RESUME); put("variant", "outline"); put("size", "sm") }) {
                on("press", Events.RESUME) { put("pr", row.pullRequestId) }
            }
        }
        if (row.menu) {
            trailing += node(
                "$id.menu", "Menu",
                {
                    put("label", copy.MORE)
                    put("items", JsonArray(listOf(option(copy.MENU.getValue("mergeNow"), "merge-now"), option(copy.MENU.getValue("hold"), "hold"))))
                },
                listOf(node("$id.menu.button", "Button", { put("icon", "ellipsis"); put("variant", "ghost"); put("size", "icon-xs"); put("label", copy.MORE) })),
            ) { on("select", Events.QUEUE_MENU) { put("pr", row.pullRequestId) } }
        }
        return node(id, "Row", {
            put("identifier", row.identifier)
            put("title", row.title)
            row.subtitle?.takeIf { it.isNotEmpty() }?.let { put("subtitle", it) }
            if (at != null && phone && row.action == null) put("meta", relative(at))
            put("pressable", true)
        }) {
            putJsonObject("slots") {
                put("leading", icon("$id.glyph", row.glyph, row.tone))
                if (trailing.isNotEmpty()) put("trailing", if (trailing.size == 1) trailing[0] else hstack("$id.trailing", trailing))
            }
            openPr(row.pullRequestId)
        }
    }

    private fun queueSection(id: String, title: String, rows: List<CiPage.QueueRow>, phone: Boolean, fold: Boolean = false): JsonObject? {
        if (rows.isEmpty()) return null
        return node(id, "Section", {
            put("title", title)
            put("count", rows.size)
            if (fold) { put("collapsible", true); put("open", rows.size <= 5) }
        }, rows.map { queueRow(it, "${ids.QUEUE_ROW}.${it.id}", phone) })
    }

    private fun queueRepo(repo: CiPage.QueueRepo, phone: Boolean, header: Boolean): JsonObject {
        val copy = CiPage.Copy.Queue
        val id = "${ids.QUEUE_REPO}.${repo.repositoryId}"
        val children = mutableListOf<JsonElement>()
        if (header) {
            children += hstack("$id.head", listOf(
                node("$id.title", "Heading", { put("text", repo.title); put("level", "h3") }),
                node("$id.caption", "Text", { put("text", repo.caption); put("variant", "muted") }),
            ))
        }
        if (!phone) {
            fun stat(key: String, value: String) = node("$id.stat.$key", "StatTile", { put("label", copy.STATS.getValue(key)); put("value", value) })
            children += node("${ids.QUEUE_STATS}.${repo.repositoryId}", "Grid", {
                putJsonObject("columns") { put("base", 2); put("md", 4) }
                put("gap", "sm")
            }, listOf(
                stat("inQueue", repo.stats.inQueue.toString()),
                stat("landedToday", repo.stats.landedToday.toString()),
                stat("sentBackToday", repo.stats.sentBackToday.toString()),
                stat("typicalRun", repo.stats.typicalRun ?: "—"),
            ))
        }
        for (now in repo.testing) children += nowCard(now, "${ids.QUEUE_NOW}.${now.trainId}")
        val upNext = queueSection("$id.next", copy.UP_NEXT, repo.upNext, phone)
        val waiting = queueSection("$id.waiting", copy.NOT_READY, repo.waiting, phone, fold = true)
        val sentBack = queueSection("$id.sent", copy.SENT_BACK, repo.sentBack, phone)
        val landed = queueSection("$id.landed", copy.LANDED, repo.landed, phone)
        if (phone) {
            children += listOfNotNull(upNext, sentBack, waiting, landed)
        } else {
            val left = listOfNotNull(upNext, waiting)
            val right = listOfNotNull(sentBack, landed)
            if (left.isNotEmpty() && right.isNotEmpty()) {
                children += node("$id.columns", "Grid", {
                    putJsonObject("columns") { put("base", 1); put("lg", 2) }
                    put("gap", "lg")
                }, listOf(vstack("$id.left", left, "lg"), vstack("$id.right", right, "lg")))
            } else {
                children += left + right
            }
        }
        return vstack(id, children, "lg")
    }

    private fun queuePanel(input: CiPage.Input, phone: Boolean, repositoryId: String?): JsonObject {
        val repos = CiPage.queue(input).filter { repositoryId == null || it.repositoryId == repositoryId }
        if (repos.isEmpty()) {
            return node(ids.QUEUE, "EmptyState", { put("icon", "pr-batch"); put("title", CiPage.Copy.EMPTY_TRAIN); put("size", "list") })
        }
        return vstack(ids.QUEUE, repos.map { queueRepo(it, phone, repositoryId == null) }, "xl")
    }

    // ── Runners ───────────────────────────────────────────────────────

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
                    put("meta", fill(CiPage.Copy.RUNNERS_OFFLINE_SINCE) { put("time", relative(since)) })
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

    // ── The page ──────────────────────────────────────────────────────

    /** The CI page's body (ci-page.json LAYOUT): the CURRENT tab's panel.
     *  The tab switcher is the platform's own segmented control (labels
     *  copy.tabs, phones copy.phoneTabs; counts [CiPage.tabCounts]), drawn
     *  by the host above this tree. */
    fun ciPageTree(input: CiPage.Input, state: State): JsonObject = when (state.tab) {
        "train" -> queuePanel(input, state.phone, null)
        "prs" -> vstack("${ids.PAGE}.prs", listOf(
            filters(input, state),
            subjectSections(ids.PRS, CiPage.prGroups(input, state.filter), state.phone, if (filtered(state.filter)) CiPage.Copy.EMPTY_FILTERED else CiPage.Copy.EMPTY_PRS),
        ))
        "releases" -> vstack("${ids.PAGE}.releases", listOf(
            filters(input, state),
            runSections(ids.RELEASES, input, state.filter, listOf("release"), state.phone, if (filtered(state.filter)) CiPage.Copy.EMPTY_FILTERED else CiPage.Copy.EMPTY_RELEASES),
        ))
        else -> runnersPanel(input)
    }

    /** The repo page's Runs tab's own switch: the CI page's first three tabs. */
    val REPO_TABS = listOf("train", "prs", "releases")

    /** THE REPO PAGE'S RUNS TAB (#EXP-2214): a `Segmented` of Merge queue ·
     *  Pull requests · Releases (copy.tabs, phones copy.phoneTabs; `ci-tab`
     *  carries the value), then that panel for ONE repository: no repository
     *  header, no filter bar. */
    fun ciRepoRunsTree(input: CiPage.Input, repositoryId: String, tab: String, phone: Boolean): JsonObject {
        val labels = if (phone) CiPage.Copy.PHONE_TABS else CiPage.Copy.TABS
        val filter = CiPage.RunFilter("all", repositoryId, null)
        val panel = when (tab) {
            "train" -> queuePanel(input, phone, repositoryId)
            "prs" -> subjectSections(ids.PRS, CiPage.prGroups(input, filter), phone, CiPage.Copy.EMPTY_PRS)
            else -> runSections(ids.RELEASES, input, filter, listOf("release"), phone, CiPage.Copy.EMPTY_RELEASES)
        }
        return vstack(ids.RUNS, listOf(
            node(ids.TABS, "Segmented", {
                put("value", tab)
                put("size", "sm")
                put("items", JsonArray(REPO_TABS.map { option(labels.getValue(it), it) }))
            }) { on("change", Events.TAB) },
            panel,
        ))
    }
}
