package at.exponential.ui.primitives

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/** #EXP-2204: fixtures/ci-log.json and fixtures/job-graph.json replay through [CiLog] and [JobGraph]. */
class CiLogTest {
    private val sdk: File = File("../../exponential-ui")

    @Suppress("UNCHECKED_CAST")
    private fun Any?.obj(): Map<String, Any?> = this as Map<String, Any?>

    @Suppress("UNCHECKED_CAST")
    private fun Any?.arr(): List<Any?> = this as List<Any?>

    private fun Any?.int(): Int = (this as Number).toInt()

    private fun fixture(name: String): Map<String, Any?> {
        val file = File(sdk, "fixtures/$name")
        assumeTrue("$file not in this checkout", file.isFile)
        return MiniJson(file.readText()).parse().obj()
    }

    /** Every number → Double, so ours compare with the parsed fixture's. */
    private fun norm(v: Any?): Any? = when (v) {
        is Number -> v.toDouble()
        is Map<*, *> -> v.entries.associate { it.key to norm(it.value) }
        is List<*> -> v.map(::norm)
        else -> v
    }

    @Test
    fun statusTable() {
        val t = fixture("ci-log.json")["statusTable"].obj()
        assertEquals(CiLog.SPIN_MS, (t["spinMs"] as Number).toDouble(), 0.0)
        for (r in t["rows"].arr()) {
            val row = r.obj()
            val want = CiLog.StatusEntry(row["status"] as String, row["icon"] as String, row["spin"] as Boolean, row["color"] as String, row["text"] as String, row["label"] as String)
            assertEquals("$row", want, CiLog.entry(row["input"]))
        }
    }

    @Test
    fun anchors() {
        val a = fixture("ci-log.json")["anchors"].obj()
        val parse = a["parse"].arr()
        assertTrue(parse.size > 20)
        for (c in parse.map { it.obj() }) {
            val want = (c["range"] as? Map<*, *>)?.let { CiLog.LineRange(it["start"].int(), it["end"].int()) }
            assertEquals("$c", want, CiLog.parseAnchor(c["text"]))
        }
        for (c in a["format"].arr().map { it.obj() }) {
            assertEquals("$c", c["anchor"], CiLog.formatAnchor(c["start"].int(), (c["end"] as? Number)?.toInt()))
        }
        for (c in a["extend"].arr().map { it.obj() }) {
            assertEquals("$c", c["anchor"], CiLog.extendAnchor(c["current"], c["line"].int(), c["shift"] as Boolean))
        }
    }

    private fun rowJson(r: CiLog.Row): Map<String, Any?> = when (r) {
        is CiLog.Row.LineRow -> mapOf("kind" to "line", "n" to r.n)
        is CiLog.Row.SectionRow -> mapOf("kind" to "section", "section" to r.section)
    }

    @Test
    fun logs() {
        val logs = fixture("ci-log.json")["logs"].arr().map { it.obj() }
        assertTrue(logs.size > 5)
        for (c in logs) {
            val name = c["name"] as String
            val model = CiLog.model(c["lines"], c["sections"])
            val want = c["model"].obj()
            assertEquals(name, norm(want["lines"]), norm(model.lines.map { mapOf("n" to it.n, "section" to it.section, "time" to it.time, "kind" to it.kind, "text" to it.text) }))
            assertEquals(name, norm(want["sections"]), norm(model.sections.map { mapOf("index" to it.index, "title" to it.title, "start" to it.start, "end" to it.end, "count" to it.count, "status" to it.status, "duration" to it.duration) }))
            assertEquals(name, c["openDefault"].arr().map { it.int() }, CiLog.openDefault(model))
            for (r in c["rows"].arr().map { it.obj() }) {
                assertEquals("$name rows ${r["open"]}", norm(r["rows"]), norm(CiLog.rows(model, CiLog.openSet(r["open"], model)).map(::rowJson)))
            }
            for (s in c["search"].arr().map { it.obj() }) {
                assertEquals("$name search ${s["query"]}", norm(s["matches"]), norm(CiLog.search(model, s["query"]).map { mapOf("line" to it.line, "start" to it.start, "end" to it.end) }))
            }
            for (r in c["reveal"].arr().map { it.obj() }) {
                assertEquals("$name reveal", r["result"].arr().map { it.int() }, CiLog.reveal(model, r["open"], r["line"].int()))
            }
        }
    }

    @Test
    fun blame() {
        for (c in fixture("ci-log.json")["blame"].arr().map { it.obj() }) {
            val got = CiLog.blameBlocks(c["blame"], c["lineCount"].int()).map { mapOf("block" to it.block, "first" to it.first, "border" to it.border) }
            assertEquals(c["name"] as String, norm(c["lines"]), norm(got))
        }
    }

    private fun layoutJson(l: JobGraph.Layout): Any? {
        fun p(pt: JobGraph.Point) = listOf(pt.x, pt.y)
        return norm(
            mapOf(
                "mode" to l.mode, "width" to l.width, "height" to l.height,
                "stages" to l.stages.map { mapOf("title" to it.title, "x" to it.x) },
                "nodes" to l.nodes.map { n ->
                    mapOf(
                        "id" to n.id, "name" to n.name, "status" to n.status, "duration" to n.duration,
                        "count" to n.count?.let { mapOf("passed" to it.passed, "total" to it.total) },
                        "members" to n.members, "needs" to n.needs, "stage" to n.stage, "column" to n.column, "row" to n.row,
                        "x" to n.x, "y" to n.y, "width" to n.width, "height" to n.height,
                    )
                },
                "edges" to l.edges.map { mapOf("from" to it.from, "to" to it.to, "start" to p(it.start), "c1" to p(it.c1), "c2" to p(it.c2), "end" to p(it.end)) },
            ),
        )
    }

    @Test
    fun jobGraph() {
        val f = fixture("job-graph.json")
        val g = f["geometry"].obj()
        val geo = JobGraph.GEOMETRY
        assertEquals(
            listOf("nodeWidth", "nodeHeight", "columnGap", "rowGap", "stageHeight", "stageGap", "edgeWidth", "rowHeight").map { (g[it] as Number).toDouble() },
            listOf(geo.nodeWidth, geo.nodeHeight, geo.columnGap, geo.rowGap, geo.stageHeight, geo.stageGap, geo.edgeWidth, geo.rowHeight),
        )
        val cases = f["cases"].arr().map { it.obj() }
        assertTrue(cases.size > 5)
        for (c in cases) {
            val name = c["name"] as String
            val graph = JobGraph.layout(c["jobs"], compact = false)
            assertEquals("$name graph", norm(c["graph"]), layoutJson(graph))
            assertEquals("$name list", norm(c["list"]), layoutJson(JobGraph.layout(c["jobs"], compact = true)))
            for (u in c["upstream"].arr().map { it.obj() }) {
                assertEquals("$name upstream", u["ids"], JobGraph.upstream(graph, u["id"] as String))
            }
        }
    }
}
