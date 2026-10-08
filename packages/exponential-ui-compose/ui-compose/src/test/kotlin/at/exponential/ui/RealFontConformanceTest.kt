package at.exponential.ui

import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.json.JsonValue
import at.exponential.ui.measure.FontResolver
import at.exponential.ui.measure.TextShaper
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.SurfaceSettings
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File
import kotlin.math.abs
import kotlin.math.floor
import kotlin.math.max

/**
 * The ANDROID half of the real-font conformance harness
 * (`packages/exponential-ui/conformance`, round 2 §8): every case of
 * `fixtures/conformance-cases.json` laid out by THIS painter (the core with
 * `SurfaceMeasurer` on a Compose `TextMeasurer`, Robolectric's native text
 * stack) with ONLY the conformance font files (`conformance/fonts.json`),
 * dumped in the shared frame-dump format (`conformance/dump.ts`, written to
 * `build/conformance/compose.json`) and compared with the committed WEB
 * baseline under the matrix's tolerance (the rules of
 * `conformance/compare.ts`, ported). The gate is a RATCHET over
 * `src/test/conformance-known.json` (the divergence counts per case today):
 * a case that gets worse fails, and so does one that improves until its
 * budget is lowered (a fix and its new budget land together). Every origin is printed. Rewrite the budget after a fix (or a
 * baseline rewrite): `EXP_UI_WRITE_FIXTURES=1 ./gradlew
 * :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'`.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-mdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class RealFontConformanceTest {
    @Before
    fun setUp() = Fixtures.require()

    private val repo: File get() = Fixtures.dir.parentFile.parentFile.parentFile
    private fun read(rel: String): JsonValue = JsonValue.parse(File(repo, rel).readText())

    class Node(
        val id: String, val component: String, val part: String?, val parent: String?,
        val x: Float, val y: Float, val w: Float, val h: Float,
        val text: String? = null, val lines: Int? = null, val lh: Float? = null,
    ) {
        fun json(): JsonValue = JsonValue.Obj(
            buildMap {
                put("id", JsonValue.Str(id))
                put("component", JsonValue.Str(component))
                part?.let { put("part", JsonValue.Str(it)) }
                parent?.let { put("parent", JsonValue.Str(it)) }
                put("x", JsonValue.Num(r2(x).toDouble())); put("y", JsonValue.Num(r2(y).toDouble()))
                put("w", JsonValue.Num(r2(w).toDouble())); put("h", JsonValue.Num(r2(h).toDouble()))
                text?.let { put("text", JsonValue.Str(it)) }
                lines?.let { put("lines", JsonValue.Num(it.toDouble())) }
                lh?.let { put("lh", JsonValue.Num(r2(it).toDouble())) }
            },
        )
    }

    class CaseDump(val width: Float, val height: Float, val nodes: List<Node>)

    companion object {
        fun r2(v: Float): Float = (floor(v * 100.0 + 0.5) / 100.0).toFloat()

        /** `conformance/dump.ts` `dropCollapsed`. */
        fun dropCollapsed(nodes: List<Node>): List<Node> {
            val parentOf = nodes.associate { it.id to it.parent }
            val visible = HashSet<String>()
            for (n in nodes) {
                if (n.w <= 0f && n.h <= 0f) continue
                var id: String? = n.id
                while (id != null && visible.add(id)) id = parentOf[id]
            }
            return nodes.filter { it.id in visible }
        }
    }

    /** The conformance font set: a Compose family per theme family NAME (substitutes resolved) + the default. */
    private class FontSet(val families: Map<String, FontFamily>, val default: FontFamily, val files: List<String>) : FontResolver {
        override fun family(name: String?): FontFamily? = if (name == null) default else families[name]
    }

    private fun fontSet(manifest: JsonValue): FontSet {
        val fonts = read(manifest["fonts"]!!.string!!)
        val table = fonts["families"]!!.obj!!
        val loaded = HashMap<String, FontFamily>()
        val files = ArrayList<String>()
        for ((name, spec) in table) {
            val faces = spec["faces"]?.array ?: continue
            loaded[name] = FontFamily(
                faces.map { f ->
                    val file = File(repo, f["file"]!!.string!!)
                    files.add(file.name)
                    Font(file, FontWeight(f["weight"]!!.number!!.toInt()), if (f["style"]?.string == "italic") FontStyle.Italic else FontStyle.Normal)
                },
            )
        }
        for ((name, spec) in table) spec["substitute"]?.string?.let { sub -> loaded[sub]?.let { loaded[name] = it } }
        val default = loaded[fonts["default"]?.string ?: "Inter"] ?: FontFamily.Default
        return FontSet(loaded, default, files)
    }

    private class FontHost(val fonts: FontSet) : HostPlugin {
        override fun fontFamily(name: String): FontFamily? = fonts.families[name]
        override fun resolveUrl(src: String): String = ""
    }

    /** The case's tree (root `direction` = the case's) and its data model (`conformance/dump.ts caseInput`). */
    private fun input(manifest: JsonValue, fixture: String, direction: String): Pair<JsonValue, Map<String, JsonValue>> {
        val spec = manifest["fixtures"]!![fixture]!!
        val (tree, data) = spec["geometry"]?.string?.let { g ->
            val geo = read(g)
            geo["surface"]!! to (geo["data"]?.obj ?: emptyMap())
        } ?: (read(spec["tree"]!!.string!!) to ((spec["data"]?.string?.let(::read)?.obj ?: emptyMap()) - "\$comment"))
        val root = tree.obj!!.toMutableMap()
        root["style"] = JsonValue.Obj((root["style"]?.obj ?: emptyMap()) + ("direction" to JsonValue.Str(direction)))
        return JsonValue.Obj(root) to data
    }

    private fun leafText(props: Map<String, JsonValue>): String? {
        for (k in listOf("text", "label", "title", "value")) {
            val v = props[k] ?: continue
            val s = when (v) {
                is JsonValue.Str -> v.v
                is JsonValue.Num, is JsonValue.Bool -> at.exponential.ui.ffi.displayStringJson(v.json)
                else -> continue
            }
            val t = s.trim().split(Regex("\\s+")).joinToString(" ")
            if (t.isNotEmpty()) return t.take(80)
        }
        return null
    }

    /** Lay one case out and dump the main tree (pre-order, relative to the root, `conformance/dump.ts`). */
    private fun dump(manifest: JsonValue, fonts: FontSet, shaper: TextShaper, key: String, textComponents: Set<String>): CaseDump {
        val (fixture, theme, mode, width, direction) = key.split("/")
        val (tree, data) = input(manifest, fixture, direction)
        val options = SurfaceOptions(theme = ThemeHandle.builtin(theme), mode = if (mode == "light") Mode.Light else Mode.Dark, settings = SurfaceSettings(locale = manifest["locale"]?.string ?: "en-US", timeZone = "UTC"))
        val m = SurfaceModel("conformance", options, FontHost(fonts), CoroutineScope(Dispatchers.Unconfined)) { shaper }
        m.setNested(tree.orderedJson)
        for ((k, v) in data) m.setData("/$k", v)
        // Unbounded: the web surface grows with its content. Height 0 =
        // content-sized (orientation unknown = landscape, round 1 §style
        // conditions), as gpui's dump; the max only bounds windowing.
        m.setViewport(width.toFloat(), 0f, 1_000_000f)
        m.pass()
        val raw = m.surface.nodes()
        val ids = raw.filter { !it.removed }.associate { it.index.toInt() to it.id }
        val root = raw.firstOrNull { !it.removed && it.parent == null && it.layer == 0u } ?: return CaseDump(0f, 0f, emptyList())
        val rf = m.frame(root.index.toInt())
        val order = ArrayList<Int>()
        val stack = ArrayDeque<Int>().apply { addLast(root.index.toInt()) }
        while (stack.isNotEmpty()) {
            val i = stack.removeLast()
            order.add(i)
            for (c in raw[i].children.reversed()) stack.addLast(c.toInt())
        }
        val out = ArrayList<Node>()
        for (i in order) {
            val n = raw.getOrNull(i) ?: continue
            if (n.removed || n.hidden || n.layer != 0u) continue
            val f = m.frame(i)
            val props = JsonValue.parse(n.propsJson).obj ?: emptyMap()
            var text: String? = null
            var lines: Int? = null
            var lh: Float? = null
            if (n.isLeaf) {
                val t = leafText(props)
                val ts = m.surface.textStyle(n.index)
                if (t != null && ts != null) {
                    val v = m.surface.visual(n.index)
                    val padX = v?.padding?.let { it[1] + it[3] } ?: 0f
                    val borderX = v?.borderWidths?.let { it[1] + it[3] } ?: ((v?.borderWidth ?: 0f) * 2f)
                    if (ts.lineHeight > 0f && n.component in textComponents) {
                        // The lines the painter lays the text out in at its frame (the web
                        // counts its text's line boxes; a stretched box is not more lines).
                        val raw = (props["text"] ?: props["label"])?.let { if (it is JsonValue.Str) it.v else null } ?: t
                        val style = at.exponential.ui.theme.ResolvedTextStyle(ts.fontSize, ts.fontWeight.toInt(), ts.lineHeight, ts.fontFamily)
                        val clamp = n.lines?.toInt()
                        val h = if (raw.isEmpty()) 0f else shaper.wrappedHeight(raw, style, if (clamp == 1) null else max(1f, f.width - padX - borderX), clamp)
                        lines = max(1, Math.round(h / ts.lineHeight))
                        lh = ts.lineHeight
                    }
                    text = t
                }
            }
            out.add(Node(n.id, n.component, n.part, n.parent?.toInt()?.let { ids[it] }, f.left - rf.left, f.top - rf.top, f.width, f.height, text, lines, lh))
        }
        return CaseDump(r2(rf.width), r2(rf.height), dropCollapsed(out))
    }

    /** `conformance/baseline.ts` `decodeBaseline`. */
    private fun baseline(): Map<String, CaseDump> {
        val b = read("packages/exponential-ui/fixtures/conformance-baseline.json")
        check(b["format"]?.string == "xui-conformance-baseline/1") { "baseline format" }
        val out = LinkedHashMap<String, CaseDump>()
        for ((key, c) in b["cases"]!!.obj!!) {
            val table = b["fixtures"]!![key.substringBefore('/')]!!["nodes"]!!.array!!
            val nodes = c["frames"]!!.array!!.map { row ->
                val r = row.array!!
                val n = table[r[0].number!!.toInt()].array!!
                fun f(i: Int) = (r[i].number ?: 0.0).toFloat()
                Node(n[0].string ?: "", n[1].string ?: "", n[2].string, n[3].string, f(1), f(2), f(3), f(4), n[4].string, r.getOrNull(5)?.number?.toInt(), r.getOrNull(6)?.number?.toFloat())
            }
            out[key] = CaseDump((c["width"]?.number ?: 0.0).toFloat(), (c["height"]?.number ?: 0.0).toFloat(), nodes)
        }
        return out
    }

    class Diff(val id: String, val component: String, val kinds: List<String>, var origin: Boolean, val line: String)

    class Report(var matched: Int = 0, var size: Int = 0, var position: Int = 0, var wrap: Int = 0) {
        val tolerated = ArrayList<String>()
        val onlyRef = ArrayList<String>()
        val onlyCand = ArrayList<String>()
        val diffs = ArrayList<Diff>()
    }

    /** `conformance/compare.ts` `compareCase`, same rules. */
    private fun compare(ref: CaseDump, cand: CaseDump, px: Float, textLines: Float): Report {
        val refBy = LinkedHashMap<String, Node>()
        for (n in ref.nodes) refBy.putIfAbsent(n.id, n)
        val candBy = HashMap<String, Node>()
        for (n in cand.nodes) candBy.putIfAbsent(n.id, n)
        val r = Report()
        for ((id, a) in refBy) {
            val b = candBy[id]
            if (b == null) {
                r.onlyRef.add(id)
                continue
            }
            r.matched++
            val dx = r2(b.x - a.x)
            val dy = r2(b.y - a.y)
            val dw = r2(b.w - a.w)
            val dh = r2(b.h - a.h)
            val lines = a.lines != null || b.lines != null
            val lh = max(a.lh ?: 0f, b.lh ?: 0f)
            val hTol = if (lines) max(px, textLines * lh) else px
            val kinds = ArrayList<String>()
            if (abs(dw) > px || abs(dh) > hTol) kinds.add("size") else if (abs(dx) > px || abs(dy) > px) kinds.add("position")
            if (a.lines != null && b.lines != null && a.lines != b.lines) {
                if (abs(a.lines - b.lines) > textLines) kinds.add("wrap") else r.tolerated.add("$id: ${a.lines} → ${b.lines} lines")
            }
            if (kinds.isEmpty()) continue
            fun box(n: Node) = "${n.x},${n.y} ${n.w}×${n.h}${n.lines?.let { " ${it}ln@${n.lh}" } ?: ""}"
            val text = (a.text ?: b.text)?.let { "  \"${it.take(40)}\"" } ?: ""
            r.diffs.add(Diff(id, a.component, kinds, false, "${kinds.joinToString("+").padEnd(13)} $id (${a.component}) web ${box(a)} → compose ${box(b)}  Δ $dx,$dy $dw×$dh$text"))
        }
        for (n in cand.nodes) if (!refBy.containsKey(n.id) && n.id !in r.onlyCand) r.onlyCand.add(n.id)
        val diverged = r.diffs.map { it.id }.toSet()
        val sizeBelow = HashSet<String>()
        for (d in r.diffs) {
            if ("size" !in d.kinds && "wrap" !in d.kinds) continue
            var p = refBy[d.id]?.parent
            while (p != null) {
                sizeBelow.add(p)
                p = refBy[p]?.parent
            }
        }
        for (d in r.diffs) {
            val parent = refBy[d.id]?.parent
            d.origin = if ("size" in d.kinds || "wrap" in d.kinds) d.id !in sizeBelow else parent == null || parent !in diverged
            if ("size" in d.kinds) r.size++
            if ("position" in d.kinds) r.position++
            if ("wrap" in d.kinds) r.wrap++
        }
        return r
    }

    @Test
    fun composeMatchesTheWebBaselineWithinTheKnownBudget() {
        val manifest = read("packages/exponential-ui/fixtures/conformance-cases.json")
        val fonts = fontSet(manifest)
        // A phone's density (xxhdpi): widths ceil to 1/3 dp, as on a device.
        val shaper = robolectricShaper(fonts, density = (System.getenv("EXP_UI_CONFORMANCE_DENSITY") ?: "3").toFloat())
        val textComponents = manifest["textComponents"]?.array?.mapNotNull { it.string }?.toSet() ?: emptySet()
        val px = (manifest["tolerance"]?.get("px")?.number ?: 1.0).toFloat()
        val textLines = (manifest["tolerance"]?.get("textLines")?.number ?: 1.0).toFloat()
        val strs = { k: String -> manifest[k]?.array?.mapNotNull { v -> v.string ?: v.number?.toInt()?.toString() } ?: emptyList() }
        val keys = ArrayList<String>()
        for (fx in manifest["fixtures"]!!.obj!!.keys) for (t in strs("themes")) for (md in strs("modes")) for (w in strs("widths")) for (d in strs("directions")) keys.add("$fx/$t/$md/$w/$d")
        val only = System.getenv("EXP_UI_CONFORMANCE_ONLY")
        val selected = keys.filter { only == null || it.contains(only) }
        val baseline = baseline()
        val knownFile = File("src/test/conformance-known.json")
        val known = if (knownFile.exists()) JsonValue.parse(knownFile.readText()) else JsonValue.Null
        val write = System.getenv("EXP_UI_WRITE_FIXTURES") != null
        val verbose = System.getenv("EXP_UI_CONFORMANCE_VERBOSE") != null
        val dumps = LinkedHashMap<String, JsonValue>()
        val worse = ArrayList<String>()
        val better = ArrayList<String>()
        val budget = LinkedHashMap<String, JsonValue>()
        val groups = LinkedHashMap<String, Pair<Int, String>>()
        val log = StringBuilder()
        for (key in selected) {
            val cand = dump(manifest, fonts, shaper, key, textComponents)
            dumps[key] = JsonValue.Obj(mapOf("width" to JsonValue.Num(cand.width.toDouble()), "height" to JsonValue.Num(cand.height.toDouble()), "nodes" to JsonValue.Arr(cand.nodes.map { it.json() })))
            val ref = baseline[key] ?: error("$key: not in the baseline")
            val r = compare(ref, cand, px, textLines)
            val origins = r.diffs.filter { it.origin }
            log.append("\n## $key  matched ${r.matched} · size ${r.size} · position ${r.position} · wrap ${r.wrap} · origins ${origins.size} · only web ${r.onlyRef.size} · only compose ${r.onlyCand.size} · height web ${ref.height} / compose ${cand.height}\n")
            for (d in origins) {
                log.append("  FIX  ${d.line}\n")
                val g = "${d.id} (${d.component}) ${d.kinds.joinToString("+")}"
                groups[g] = (groups[g]?.first ?: 0) + 1 to (groups[g]?.second ?: key)
            }
            val cascade = r.diffs.filter { !it.origin }
            if (verbose) cascade.forEach { log.append("       ${it.line}\n") } else if (cascade.isNotEmpty()) log.append("  + ${cascade.size} cascaded: ${cascade.take(8).joinToString(", ") { it.id }}${if (cascade.size > 8) " …" else ""}\n")
            if (r.tolerated.isNotEmpty()) log.append("  rewrapped within tolerance: ${r.tolerated.joinToString("; ")}\n")
            if (r.onlyRef.isNotEmpty()) log.append("  only in web (${r.onlyRef.size}): ${r.onlyRef.take(12).joinToString(", ")}\n")
            if (r.onlyCand.isNotEmpty()) log.append("  only in compose (${r.onlyCand.size}): ${r.onlyCand.take(12).joinToString(", ")}\n")
            val now = mapOf("size" to r.size, "position" to r.position, "wrap" to r.wrap, "onlyRef" to r.onlyRef.size, "onlyCand" to r.onlyCand.size)
            val was = known["cases"]?.get(key)
            for ((k, n) in now) {
                val b = was?.get(k)?.number?.toInt()
                when {
                    b == null -> worse.add("$key: $k $n (no budget)")
                    n > b -> worse.add("$key: $k $n > budget $b")
                    n < b -> better.add("$key: $k $n < budget $b")
                }
            }
            budget[key] = JsonValue.Obj(now.mapValues { JsonValue.Num(it.value.toDouble()) })
        }
        log.append("\n# divergence origins across cases (fix these; the rest cascades from them)\n")
        for ((what, v) in groups.entries.sortedWith(compareByDescending<Map.Entry<String, Pair<Int, String>>> { it.value.first }.thenBy { it.key })) log.append("  ${v.first.toString().padStart(3)}× $what   e.g. ${v.second}\n")
        val out = File("build/conformance").apply { mkdirs() }
        File(out, "compose.json").writeText(
            JsonValue.Obj(mapOf("format" to JsonValue.Str("xui-frame-dump/1"), "renderer" to JsonValue.Str("compose-robolectric"), "fonts" to JsonValue.Str(fonts.files.joinToString(",")), "cases" to JsonValue.Obj(dumps))).orderedJson + "\n",
        )
        File(out, "report.txt").writeText(log.toString())
        println(log)
        if (write && only == null) {
            val doc = LinkedHashMap<String, JsonValue>()
            doc["\$comment"] = JsonValue.Str(
                "The Android conformance RATCHET (ui-compose/src/test/kotlin/at/exponential/ui/RealFontConformanceTest.kt): per case, the divergence counts of the Compose painter (Robolectric's native text stack, the conformance fonts) against the web baseline (packages/exponential-ui/fixtures/conformance-baseline.json) today. A case may only go DOWN; rewrite after a fix or a baseline rewrite with EXP_UI_WRITE_FIXTURES=1 ./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'. `onlyRef`/`onlyCand` = nodes only the web / only Compose placed (coverage, not compared). The divergence DECISIONS live in packages/exponential-ui/fixtures/conformance-known.json `causes` and `rules`.",
            )
            doc["renderer"] = JsonValue.Str("compose-robolectric")
            doc["cases"] = JsonValue.Obj(budget)
            knownFile.writeText(prettyJson(JsonValue.Obj(doc)).replace("\\/", "/") + "\n")
            println("wrote ${knownFile.path}")
            return
        }
        // The coverage gap (nodes the web places that Compose never emits) is
        // budgeted per case like the divergences; the total is printed.
        val gap = budget.values.sumOf { it["onlyRef"]?.number ?: 0.0 }.toInt()
        println("coverage gap: $gap web-placed nodes Compose does not emit (onlyRef, budgeted)")
        assertTrue("the Compose painter diverges MORE from the web baseline than ${knownFile.path} allows:\n  ${worse.joinToString("\n  ")}\n(build/conformance/report.txt lists every divergence)", worse.isEmpty())
        // A fixed divergence must lower the budget in the same change, else a
        // later regression back up to the old count would pass unnoticed.
        assertTrue("IMPROVED: lower the budget (EXP_UI_WRITE_FIXTURES=1 ./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'):\n  ${better.joinToString("\n  ")}", better.isEmpty())
    }
}
