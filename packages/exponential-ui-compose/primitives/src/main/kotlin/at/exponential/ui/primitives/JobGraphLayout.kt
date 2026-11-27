package at.exponential.ui.primitives

/**
 * #EXP-2204 (packages/exponential-ui/docs/ci-components-contract.md §5):
 * the JobGraph layout, the SDK's src/job-graph.ts (eui-core `job_graph.rs`,
 * Swift `JobGraphLayout.swift`); `CiLogTest` replays fixtures/job-graph.json
 * through it. Stages as columns (else the needs depth), a matrix as one node,
 * the need edges as cubics; `compact` = one full-width row per node (the
 * surface below `md`).
 */
object JobGraph {
    /** style.json `jobGraph` (px); [rowHeight] = a compact list row. */
    data class Geometry(
        val nodeWidth: Double = 200.0,
        val nodeHeight: Double = 40.0,
        val columnGap: Double = 48.0,
        val rowGap: Double = 12.0,
        val stageHeight: Double = 24.0,
        val stageGap: Double = 8.0,
        val edgeWidth: Double = 1.5,
        val rowHeight: Double = 40.0,
    )

    val GEOMETRY = Geometry()

    data class Count(val passed: Int, val total: Int)

    data class Node(
        /** The job id, or the matrix key of a collapsed matrix. */
        val id: String,
        val name: String,
        val status: String,
        val duration: String?,
        /** A matrix: passed/total; else null. */
        val count: Count?,
        val members: List<String>,
        /** Known upstream node ids, first-seen order, never itself. */
        val needs: List<String>,
        val stage: String,
        val column: Int = 0,
        val row: Int = 0,
        val x: Double = 0.0,
        val y: Double = 0.0,
        val width: Double = 0.0,
        val height: Double = 0.0,
    ) {
        /** The meta text: `3/5` for a matrix, else the duration. */
        val meta: String? get() = count?.let { "${it.passed}/${it.total}" } ?: duration
    }

    data class Point(val x: Double, val y: Double)

    data class Edge(val from: String, val to: String, val start: Point, val c1: Point, val c2: Point, val end: Point)

    data class Stage(val title: String, val x: Double)

    data class Layout(
        /** `graph` | `list`. */
        val mode: String,
        val width: Double,
        val height: Double,
        val stages: List<Stage>,
        val nodes: List<Node>,
        val edges: List<Edge>,
    )

    /** The aggregate status of a matrix: the first present, in this order. */
    private val MATRIX_ORDER = listOf("failed", "running", "queued", "cancelled", "passed", "skipped")

    private fun nonEmpty(v: Any?): String? = (v as? String)?.takeIf { it.isNotEmpty() }

    private class Draft(
        val id: String,
        val name: String,
        val duration: String?,
        val stage: String,
        val matrix: Boolean,
        val members: MutableList<String>,
        val rawNeeds: MutableList<String>,
        val statuses: MutableList<String>,
    )

    /** The jobs → stage columns (or needs depth), the node boxes and the need edges; [compact] = the list below `md`. */
    fun layout(jobs: Any?, compact: Boolean, g: Geometry = GEOMETRY): Layout {
        val drafts = ArrayList<Draft>()
        val byKey = HashMap<String, Int>()
        val nodeOf = HashMap<String, String>()
        val seen = HashSet<String>()
        for (raw in jobs as? List<*> ?: emptyList<Any?>()) {
            val job = raw as? Map<*, *> ?: continue
            val id = nonEmpty(job["id"]) ?: continue
            if (!seen.add(id)) continue
            val status = CiLog.status(job["status"])
            val needs = (job["needs"] as? List<*> ?: emptyList<Any?>()).filterIsInstance<String>()
            val stage = job["stage"] as? String ?: ""
            val matrix = nonEmpty(job["matrix"])
            if (matrix != null) {
                val existing = byKey[matrix]
                if (existing == null && nodeOf[matrix] == matrix) continue // a plain job owns that id
                nodeOf[id] = matrix
                if (existing != null) {
                    drafts[existing].members.add(id)
                    drafts[existing].statuses.add(status)
                    drafts[existing].rawNeeds.addAll(needs)
                    continue
                }
                byKey[matrix] = drafts.size
                drafts.add(Draft(matrix, matrix, null, stage, true, mutableListOf(id), needs.toMutableList(), mutableListOf(status)))
                continue
            }
            if (byKey.containsKey(id)) continue // a matrix owns that id
            nodeOf[id] = id
            val name = nonEmpty(job["name"]) ?: id
            drafts.add(Draft(id, name, job["duration"] as? String, stage, false, mutableListOf(id), needs.toMutableList(), mutableListOf(status)))
        }
        val nodeIds = drafts.map { it.id }.toHashSet()
        val nodes0 = drafts.map { d ->
            var status = d.statuses.first()
            var count: Count? = null
            if (d.matrix) {
                status = MATRIX_ORDER.firstOrNull { it in d.statuses } ?: "queued"
                count = Count(d.statuses.count { it == "passed" }, d.statuses.size)
            }
            val needs = ArrayList<String>()
            for (need in d.rawNeeds) {
                val target = nodeOf[need] ?: (if (byKey.containsKey(need)) need else null) ?: continue
                if (target !in nodeIds || target == d.id || target in needs) continue
                needs.add(target)
            }
            Node(d.id, d.name, status, d.duration, count, d.members.toList(), needs, d.stage)
        }
        val index = HashMap<String, Int>()
        nodes0.forEachIndexed { i, n -> index[n.id] = i }

        // Columns: the stages in first-appearance order, else the needs depth.
        val staged = nodes0.any { it.stage.isNotEmpty() }
        val titles = ArrayList<String>()
        val columns = IntArray(nodes0.size)
        if (staged) {
            for (i in nodes0.indices) {
                val c = titles.indexOf(nodes0[i].stage)
                if (c >= 0) {
                    columns[i] = c
                } else {
                    titles.add(nodes0[i].stage)
                    columns[i] = titles.size - 1
                }
            }
        } else {
            val depth = arrayOfNulls<Int>(nodes0.size)
            val onPath = BooleanArray(nodes0.size)
            fun visit(i: Int): Int {
                depth[i]?.let { return it }
                onPath[i] = true
                var d = 0
                for (need in nodes0[i].needs) {
                    val j = index.getValue(need)
                    if (onPath[j]) continue
                    d = maxOf(d, visit(j) + 1)
                }
                onPath[i] = false
                depth[i] = d
                return d
            }
            for (i in nodes0.indices) columns[i] = visit(i)
        }

        // Column order: column 0 first, then input order (sortedBy is stable).
        val ordered = nodes0.mapIndexed { i, n -> n.copy(column = columns[i]) }.sortedBy { it.column }

        if (compact) {
            val rows = ordered.mapIndexed { i, n -> n.copy(row = i, x = 0.0, y = i * g.rowHeight, width = 0.0, height = g.rowHeight) }
            return Layout("list", 0.0, rows.size * g.rowHeight, emptyList(), rows, emptyList())
        }

        val top = if (staged) g.stageHeight + g.stageGap else 0.0
        val pitchX = g.nodeWidth + g.columnGap
        val rowOf = HashMap<Int, Int>()
        val placed = ordered.map { n ->
            val r = rowOf[n.column] ?: 0
            rowOf[n.column] = r + 1
            n.copy(row = r, x = n.column * pitchX, y = top + r * (g.nodeHeight + g.rowGap), width = g.nodeWidth, height = g.nodeHeight)
        }
        val columnCount = (placed.maxOfOrNull { it.column } ?: -1) + 1
        val maxRows = rowOf.values.maxOrNull() ?: 0
        val width = if (columnCount > 0) columnCount * pitchX - g.columnGap else 0.0
        val height = if (maxRows > 0) top + maxRows * (g.nodeHeight + g.rowGap) - g.rowGap else 0.0
        val stages = if (staged) titles.mapIndexed { i, t -> Stage(t, i * pitchX) } else emptyList()
        val at = HashMap<String, Node>()
        placed.forEach { at[it.id] = it }
        val edges = ArrayList<Edge>()
        for (n in placed) {
            for (need in n.needs) {
                val up = at.getValue(need)
                val x1 = up.x + up.width
                val y1 = up.y + up.height / 2
                val x2 = n.x
                val y2 = n.y + n.height / 2
                val mx = (x1 + x2) / 2
                edges.add(Edge(up.id, n.id, Point(x1, y1), Point(mx, y1), Point(mx, y2), Point(x2, y2)))
            }
        }
        return Layout("graph", width, height, stages, placed, edges)
    }

    /** The node [id] and every ancestor through `needs`, in the layout's node order; an unknown id → []. */
    fun upstream(layout: Layout, id: String): List<String> {
        val byId = layout.nodes.associateBy { it.id }
        if (id !in byId) return emptyList()
        val set = hashSetOf(id)
        val stack = ArrayDeque(listOf(id))
        while (stack.isNotEmpty()) {
            val top = stack.removeLast()
            for (need in byId.getValue(top).needs) if (set.add(need)) stack.addLast(need)
        }
        return layout.nodes.map { it.id }.filter { it in set }
    }
}
