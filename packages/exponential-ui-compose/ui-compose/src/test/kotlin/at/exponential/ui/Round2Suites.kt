package at.exponential.ui

import at.exponential.ui.ffi.Surface
import at.exponential.ui.ffi.Theme
import at.exponential.ui.ffi.animationFrameJson
import at.exponential.ui.ffi.bindRowSlotJson
import at.exponential.ui.ffi.bindTreeJson
import at.exponential.ui.ffi.coreCatalogId
import at.exponential.ui.ffi.displayStringJson
import at.exponential.ui.ffi.formatCallJson
import at.exponential.ui.ffi.jsonDiff
import at.exponential.ui.ffi.jsonEqual
import at.exponential.ui.ffi.listJson
import at.exponential.ui.ffi.reduceNestedJson
import at.exponential.ui.ffi.reduceSurfaceJson
import at.exponential.ui.ffi.resizableJson
import at.exponential.ui.ffi.resolveConditionsJson
import at.exponential.ui.ffi.runActionJson
import at.exponential.ui.ffi.tokenizeCodeJson
import at.exponential.ui.format.IcuFormatter
import at.exponential.ui.json.JsonValue
import java.io.File
import kotlin.math.abs
import kotlin.math.floor

/**
 * The manifest-v2 suites (round 1's bind / style / code fixtures and the
 * round-2 contract fixtures) replayed case by case THROUGH this painter's
 * inputs: the facade bindings for the core's arithmetic, the painter's ICU
 * [IcuFormatter] for `format`, a live `Surface`'s visuals for the
 * animation timings and the leaves' direction. Each case = (name, null) or
 * (name, the first difference). The Rust core's `tests/support/round2.rs`
 * is the twin; the counts follow `manifest.json` `unit`.
 */
object Round2Suites {
    private fun json(name: String) = Fixtures.json(name)
    private fun list(v: JsonValue?, key: String): List<JsonValue> = v?.get(key)?.array ?: emptyList()
    private fun s(v: JsonValue?, key: String): String = v?.get(key)?.string ?: ""

    private fun case(name: String, check: () -> String?): Pair<String, String?> = name to try {
        check()
    } catch (e: Throwable) {
        "${e::class.simpleName}: ${e.message}"
    }

    private fun same(got: String, expected: JsonValue): String? = if (jsonEqual(got, expected.json)) null else "differs at ${jsonDiff(got, expected.json).ifEmpty { "/" }}: got $got"

    private fun near(got: List<Double>, want: List<Double>, eps: Double): String? =
        if (got.size == want.size && got.zip(want).all { (a, b) -> abs(a - b) <= eps }) null else "got $got / expected $want"

    private fun nums(v: String): List<Double> = JsonValue.parse(v).array?.map { it.number ?: Double.NaN } ?: emptyList()

    /** A node of an expanded tree by id (children, then slot values). */
    private fun find(node: JsonValue?, id: String): JsonValue? {
        if (node == null) return null
        if (node["id"]?.string == id) return node
        for (c in list(node, "children")) find(c, id)?.let { return it }
        for (c in node["slots"]?.obj?.values ?: emptyList()) find(c, id)?.let { return it }
        return null
    }

    // Round 1 fixtures (counted since manifest v2)

    fun bind(): List<Pair<String, String?>> {
        val f = json("bind-time.json")
        return (list(f, "cases") + list(f, "extra")).map { c ->
            case(s(c, "name")) {
                val reduced = JsonValue.parse(reduceNestedJson(c["input"]!!.orderedJson, coreCatalogId(), null))
                same((reduced["issues"] ?: JsonValue.Arr(emptyList())).json, c["issues"] ?: JsonValue.Arr(emptyList()))?.let { return@case "issues $it" }
                same(reduced["root"]!!.json, c["expanded"]!!)?.let { return@case "expanded $it" }
                val expanded = c["expanded"]!!
                for (d in list(c, "datasets")) {
                    val data = d["data"] ?: JsonValue.Obj(emptyMap())
                    val bound = bindTreeJson(expanded.orderedJson, data.orderedJson, "", null)
                    same(bound ?: "null", d["bound"] ?: JsonValue.Null)?.let { return@case "bound ${data.json}: $it" }
                    for (p in list(d, "presses")) {
                        val id = s(p, "id")
                        val node = find(expanded, id) ?: return@case "no $id"
                        val action = node["on"]?.get("press") ?: return@case "no on.press on $id"
                        same(runActionJson(action.json, data.json, "", null), p["outcome"]!!)?.let { return@case "press $id: $it" }
                    }
                    for (t in list(d, "rowSlots")) {
                        val id = s(t, "id")
                        val boundNode = find(bound?.let(JsonValue::parse), id) ?: return@case "no bound $id"
                        val rowsProp = find(expanded, id)?.get("props")?.get("rows") ?: JsonValue.Null
                        val rows = boundNode["props"]?.get("rows") ?: JsonValue.Arr(emptyList())
                        for (r in list(t, "rows")) {
                            val index = (r["index"]?.number ?: 0.0).toInt()
                            for ((slotName, expected) in r["slots"]?.obj ?: emptyMap()) {
                                val slot = boundNode["slots"]?.get(slotName) ?: return@case "no slot $slotName"
                                val got = bindRowSlotJson(slot.json, rowsProp.json, rows.json, index.toUInt(), data.json, null)
                                same(got ?: "null", expected)?.let { return@case "row $index $slotName: $it" }
                            }
                        }
                    }
                }
                null
            }
        }
    }

    fun styleConditions(): List<Pair<String, String?>> {
        val f = json("style-conditions.json")
        val defaults = f["breakpoints"]
        return list(f, "cases").map { c ->
            case(s(c, "name")) {
                for ((ctx, expected) in list(c, "contexts").zip(list(c, "expected"))) {
                    val context = ctx.obj!!.toMutableMap()
                    if (context["breakpoints"] == null && defaults != null) context["breakpoints"] = defaults
                    same(resolveConditionsJson(c["style"]!!.orderedJson, JsonValue.Obj(context).orderedJson), expected)?.let { return@case "${s(ctx, "label")}: $it" }
                }
                null
            }
        }
    }

    fun codeTokens(): List<Pair<String, String?>> = list(json("code-tokens.json"), "cases").map { c ->
        case(s(c, "name")) { same(tokenizeCodeJson(s(c, "code"), s(c, "language")), c["expected"]!!) }
    }

    // Round 2

    /** Every call through the PAINTER's formatter (ICU, en-US / UTC: the fixture's), every display value. */
    fun format(): List<Pair<String, String?>> {
        val f = json("format.json")
        val formatter = IcuFormatter.create(s(f, "locale").ifEmpty { "en-US" }, s(f, "timeZone").ifEmpty { "UTC" })
        return list(f, "calls").map { c ->
            case("call ${s(c, "name")}") { same(formatCallJson(c["call"]!!.json, formatter, null) ?: "null", c["expected"]!!) }
        } + list(f, "zoned").map { c ->
            // The English fallback at the host's offset, then this painter's
            // ICU formatter in the IANA zone (U+202F / U+00A0 read as spaces).
            case("zoned ${s(c, "name")}") {
                val offset = c["offsetMinutes"]?.number?.toInt() ?: 0
                same(formatCallJson(c["call"]!!.json, null, null, offset) ?: "null", c["expected"]!!)?.let { return@case "fallback $it" }
                val icu = IcuFormatter.create("en-US", s(c, "timeZone"))
                val got = (formatCallJson(c["call"]!!.json, icu, null, null) ?: "null").replace('\u202F', ' ').replace('\u00A0', ' ')
                same(got, c["expected"]!!)?.let { "icu $it" }
            }
        } + list(f, "display").map { d ->
            case("display ${s(d, "name")}") { same(JsonValue.Str(displayStringJson(d["value"]!!.json)).json, d["expected"]!!) }
        }
    }

    /** `templateInstances` (src/list.ts): the items at the template's path, keyed by the core's `keys`. */
    private fun instances(data: JsonValue, template: JsonValue, scope: String, instance: String): List<JsonValue> {
        val p = s(template, "path")
        val path = if (p.startsWith("/")) p else if (p.isEmpty()) scope else "$scope/$p"
        val items = readPointer(data, path)?.array ?: return emptyList()
        val keyArg = template["key"]?.string
        val req = JsonValue.Obj(buildMap { put("op", JsonValue.Str("keys")); put("items", JsonValue.Arr(items)); keyArg?.let { put("key", JsonValue.Str(it)) } })
        val keys = JsonValue.parse(listJson(req.json)).array!!.map { it.displayText }
        return items.indices.map { i ->
            JsonValue.Obj(mapOf("key" to JsonValue.Str(keys[i]), "path" to JsonValue.Str("$path/$i"), "index" to JsonValue.Num(i.toDouble()), "instance" to JsonValue.Str("$instance.${instanceSegment(keys[i])}")))
        }
    }

    /** A key as one instance-suffix segment (src/list.ts `instanceSegment`): `~` → `~0`, `.` → `~1`. */
    private fun instanceSegment(key: String): String = key.replace("~", "~0").replace(".", "~1")

    private fun readPointer(v: JsonValue, pointer: String): JsonValue? {
        if (pointer.isEmpty()) return v
        var cur: JsonValue? = v
        for (raw in pointer.removePrefix("/").split("/")) {
            val seg = raw.replace("~1", "/").replace("~0", "~")
            cur = when (val c = cur) {
                is JsonValue.Obj -> c.v[seg]
                is JsonValue.Arr -> seg.toIntOrNull()?.let { c.v.getOrNull(it) }
                else -> null
            } ?: return null
        }
        return cur
    }

    fun templateItems(): List<Pair<String, String?>> {
        val f = json("template-items.json")
        val out = ArrayList<Pair<String, String?>>()
        for (c in list(f, "keys")) {
            out.add(case("keys ${s(c, "name")}") {
                val req = c.obj!!.filterKeys { it == "items" || it == "key" } + ("op" to JsonValue.Str("keys"))
                same(listJson(JsonValue.Obj(req).json), c["expected"]!!)
            })
        }
        for (c in list(f, "rowKeys")) {
            out.add(case("rowKeys ${s(c, "name")}") {
                val req = c.obj!!.filterKeys { it == "rows" || it == "rowKey" } + ("op" to JsonValue.Str("rowKeys"))
                same(listJson(JsonValue.Obj(req).json), c["expected"]!!)
            })
        }
        for (c in list(f, "instances")) {
            out.add(case("instances ${s(c, "name")}") {
                val data = c["data"]!!
                val outer = instances(data, c["template"]!!, s(c, "scope"), s(c, "instance"))
                val inner = c["inner"] ?: return@case same(JsonValue.Arr(outer).json, c["expected"]!!)
                val nested = outer.flatMap { o ->
                    instances(data, inner, s(o, "path"), s(o, "instance")).map { i ->
                        val inst = s(i, "instance")
                        JsonValue.Obj(
                            mapOf(
                                "outer" to o["key"]!!, "key" to i["key"]!!, "path" to i["path"]!!, "index" to i["index"]!!, "instance" to JsonValue.Str(inst),
                                "ids" to JsonValue.Obj(mapOf("issue" to JsonValue.Str("issue$inst"), "title" to JsonValue.Str("issue.title$inst"))),
                            ),
                        )
                    }
                }
                same(JsonValue.Arr(nested).json, c["expected"]!!)
            })
        }
        for (c in list(f, "reduce")) {
            out.add(case("reduce ${s(c, "name")}") {
                val catalog = s(c, "catalogId").ifEmpty { coreCatalogId() }
                val got = if (c["nested"] != null) reduceNestedJson(c["nested"]!!.orderedJson, catalog, null) else reduceSurfaceJson(c["components"]!!.orderedJson, catalog, null)
                same(got, c["expected"]!!)?.let { return@case it }
                // A lifted template never also renders in place.
                val parsed = JsonValue.parse(got)
                for (id in parsed["templates"]?.obj?.keys ?: emptySet()) if (find(parsed["root"], id) != null) return@case "$id in place"
                null
            })
        }
        return out
    }

    /**
     * `text-direction.json`: every node's direction and physical alignment
     * (src/direction.ts: its own `direction`, else its parent's; `textAlign`,
     * else a Text's `align`, else `start`, against the node's direction),
     * and the LEAVES' as a surface hands them to this painter (the visual's
     * `direction` + `textAlign`).
     */
    fun textDirection(): List<Pair<String, String?>> = list(json("text-direction.json"), "cases").map { c ->
        case(s(c, "name")) {
            val surfaceDir = s(c, "surface").ifEmpty { "ltr" }
            val root = JsonValue.parse(reduceNestedJson(c["tree"]!!.orderedJson, coreCatalogId(), null))["root"]!!
            val got = LinkedHashMap<String, JsonValue>()
            fun physical(align: String, dir: String) = when (align) {
                "start" -> if (dir == "rtl") "right" else "left"
                "end" -> if (dir == "rtl") "left" else "right"
                else -> align
            }
            fun walk(n: JsonValue, parentDir: String) {
                val dir = n["style"]?.get("direction")?.string ?: parentDir
                val align = n["style"]?.get("textAlign")?.string ?: (if (n["component"]?.string == "Text") n["props"]?.get("align")?.string else null) ?: "start"
                got[s(n, "id")] = JsonValue.Obj(mapOf("direction" to JsonValue.Str(dir), "textAlign" to JsonValue.Str(physical(align, dir))))
                for (k in list(n, "children")) walk(k, dir)
                for (k in n["slots"]?.obj?.values ?: emptyList()) walk(k, dir)
            }
            walk(root, surfaceDir)
            same(JsonValue.Obj(got).json, c["expected"]!!)?.let { return@case it }
            // What the painter receives: the leaves' resolved direction + align.
            val tree = c["tree"]!!.obj!!.toMutableMap()
            if (tree["style"]?.get("direction") == null) tree["style"] = JsonValue.Obj((tree["style"]?.obj ?: emptyMap()) + ("direction" to JsonValue.Str(surfaceDir)))
            val surface = Surface.withTheme("dir", coreCatalogId(), Theme.builtin("neutral"), "light")
            surface.setNested(JsonValue.Obj(tree).orderedJson)
            surface.setViewport(390f, 800f, null)
            surface.layoutFixed(null, true)
            val visuals = surface.visuals()
            for (n in surface.nodes()) {
                if (!n.isLeaf) continue
                val want = c["expected"]!![n.id] ?: continue
                val v = visuals.getOrNull(n.index.toInt()) ?: return@case "no visual for ${n.id}"
                if (v.direction != s(want, "direction")) return@case "${n.id}: visual direction ${v.direction} ≠ ${s(want, "direction")}"
                if (v.textAlign != s(want, "textAlign")) return@case "${n.id}: visual textAlign ${v.textAlign} ≠ ${s(want, "textAlign")}"
            }
            null
        }
    }

    fun resizable(): List<Pair<String, String?>> {
        val f = json("resizable.json")
        val out = ArrayList<Pair<String, String?>>()
        fun sum100(v: List<Double>) = if (abs(v.sum() - 100.0) <= 1e-5) null else "sum $v"
        for ((op, key) in listOf("normalize" to "normalize", "resize" to "resize", "key" to "keys", "extents" to "extents")) {
            for (c in list(f, key)) {
                out.add(case("$key ${s(c, "name")}") {
                    val got = nums(resizableJson(JsonValue.Obj(c.obj!! + ("op" to JsonValue.Str(op))).json))
                    near(got, c["expected"]!!.array!!.map { it.number ?: Double.NaN }, 1e-6) ?: if (op == "extents") null else sum100(got)
                })
            }
        }
        for (c in list(f, "drag")) {
            out.add(case("drag ${s(c, "name")}") {
                val got = JsonValue.parse(resizableJson(JsonValue.Obj(c.obj!! + ("op" to JsonValue.Str("drag"))).json)).number ?: Double.NaN
                near(listOf(got), listOf(c["expected"]?.number ?: Double.NaN), 1e-6)
            })
        }
        return out
    }

    private fun extents(v: JsonValue?): JsonValue = when (v) {
        is JsonValue.Arr -> v
        else -> JsonValue.Arr(List((v?.get("count")?.number ?: 0.0).toInt()) { v?.get("extent") ?: JsonValue.Num(0.0) })
    }

    fun virtualList(): List<Pair<String, String?>> {
        val f = json("virtual-list.json")
        val out = ArrayList<Pair<String, String?>>()
        for (c in list(f, "windows")) {
            out.add(case("window ${s(c, "name")}") {
                val req = c.obj!!.filterKeys { it in setOf("gap", "scroll", "viewport", "overscan") } + ("extents" to extents(c["extents"])) + ("op" to JsonValue.Str("window"))
                same(listJson(JsonValue.Obj(req).json), c["expected"]!!)
            })
        }
        for (c in list(f, "scrollTo")) {
            out.add(case("scrollTo ${s(c, "name")}") {
                val req = c.obj!!.filterKeys { it in setOf("gap", "index", "viewport", "scroll", "align", "inset") } + ("extents" to extents(c["extents"])) + ("op" to JsonValue.Str("scrollTo"))
                same(listJson(JsonValue.Obj(req).json), c["expected"]!!)
            })
        }
        for (c in list(f, "sections")) {
            out.add(case("sections ${s(c, "name")}") {
                same(listJson(JsonValue.Obj(c.obj!!.filterKeys { it == "items" || it == "sectionBy" } + ("op" to JsonValue.Str("sections"))).json), c["expected"]!!)
            })
        }
        for (c in list(f, "sectionedScrollTo")) {
            out.add(case("sectionedScrollTo ${s(c, "name")}") {
                val sections = JsonValue.parse(listJson(JsonValue.Obj(mapOf("op" to JsonValue.Str("sections"), "items" to c["items"]!!, "sectionBy" to c["sectionBy"]!!)).json))
                val rows = sections["rows"]!!.array!!
                val header = c["headerExtent"]?.number ?: 0.0
                val item = c["itemExtent"]?.number ?: 0.0
                val ext = JsonValue.Arr(rows.map { r -> JsonValue.Num(if (r["header"] != null) header else item) })
                val req = c.obj!!.filterKeys { it in setOf("gap", "index", "viewport", "scroll", "align", "stickyHeaders") } + mapOf("op" to JsonValue.Str("scrollToItem"), "rows" to JsonValue.Arr(rows), "rowExtents" to ext)
                same(listJson(JsonValue.Obj(req).json), c["expected"]!!)
            })
        }
        for (c in list(f, "sticky")) {
            out.add(case("sticky ${s(c, "name")}") {
                val got = c["scrolls"]!!.array!!.map { sc ->
                    JsonValue.parse(listJson(JsonValue.Obj(mapOf("op" to JsonValue.Str("sticky"), "rowExtents" to c["rowExtents"]!!, "headerRows" to c["headerRows"]!!, "scroll" to sc)).json))
                }
                same(JsonValue.Arr(got).json, c["expected"]!!)
            })
        }
        return out
    }

    /** A node's animation timing as the surface resolves it for this painter (the visual's `animation.timing`). */
    private fun timing(theme: String, name: String, duration: String?): JsonValue? {
        val style = buildMap {
            put("animation", JsonValue.Str(name))
            duration?.let { put("animationDuration", JsonValue.Str(it)) }
        }
        val surface = Surface.withTheme("anim", coreCatalogId(), Theme.builtin(theme), "light")
        surface.setNested(JsonValue.Obj(mapOf("id" to JsonValue.Str("a"), "component" to JsonValue.Str("Box"), "style" to JsonValue.Obj(style))).json)
        surface.setViewport(100f, 100f, null)
        surface.layoutFixed(null, true)
        val v = surface.visuals().firstOrNull()?.animationJson ?: return null
        return JsonValue.parse(v)["timing"]
    }

    private fun frameNear(got: JsonValue, want: JsonValue): String? {
        for ((k, v) in want.obj ?: emptyMap()) {
            val g = got[k]
            val ok = when {
                v.number != null && g?.number != null -> abs(v.number!! - g.number!!) <= 1e-3
                else -> (v is JsonValue.Null) && (g == null || g is JsonValue.Null)
            }
            if (!ok) return "$k: got $g / expected $v"
        }
        return null
    }

    /** `style.json` `animations` → `@keyframes xui-<name>` (src/animation.ts `keyframesCss`). */
    private fun keyframesCss(name: String): String {
        val style = JsonValue.parse(File(Fixtures.dir.parentFile, "catalog/style.json").readText())
        val def = style["animations"]?.get(name) ?: return ""
        fun css(n: Double): String {
            val r = floor(n * 10000 + 0.5) / 10000
            return displayStringJson(JsonValue.Num(r).json)
        }
        val steps = list(def, "keyframes").joinToString("") { k ->
            val decl = ArrayList<String>()
            // Opacity moves `--xui-a-opacity` (the node's own opacity multiplies it).
            k["opacity"]?.number?.let { decl.add("--xui-a-opacity:${css(it)}") }
            if (k["translateX"] != null || k["translateY"] != null) decl.add("translate:${css(k["translateX"]?.number ?: 0.0)}px ${css(k["translateY"]?.number ?: 0.0)}px")
            k["rotate"]?.number?.let { decl.add("rotate:${css(it)}deg") }
            k["scale"]?.number?.let { decl.add("scale:${css(it)}") }
            k["band"]?.number?.let { decl.add("--xui-band:${css(it)}") }
            "${css((k["offset"]?.number ?: 0.0) * 100)}%{${decl.joinToString(";")}}"
        }
        return "@keyframes xui-$name{$steps}"
    }

    fun animations(): List<Pair<String, String?>> {
        val f = json("animations.json")
        val out = ArrayList<Pair<String, String?>>()
        for ((id, entries) in f["themes"]!!.obj!!) {
            for ((name, e) in entries.obj!!) {
                out.add(case("$id/$name") {
                    val t = timing(id, name, null) ?: return@case "no animation on the visual"
                    same(t.json, e["timing"]!!)?.let { return@case "timing $it" }
                    for (fr in list(e, "frames")) {
                        val at = fr["t"]?.number ?: 0.0
                        val got = JsonValue.parse(animationFrameJson(name, t.json, at, false) ?: return@case "no frame @$at")
                        frameNear(got, fr["frame"]!!)?.let { return@case "@$at $it" }
                    }
                    same(animationFrameJson(name, t.json, 5.0, true) ?: "null", e["reduced"]!!)?.let { return@case "reduced $it" }
                    same(JsonValue.Str(keyframesCss(name)).json, f["css"]!![name]!!)?.let { "css $it" }
                })
            }
        }
        // The painted opacity = the node's own × the frame's: NodeView's two
        // layers (the animation's `graphicsLayer` over `paintedBox`'s alpha).
        for (c in list(f, "opacity")) {
            out.add(case("opacity ${s(c, "name")}") {
                val name = s(c, "animation")
                val t = timing(s(c, "theme"), name, null) ?: return@case "no animation on the visual"
                val frame = at.exponential.ui.compose.AnimFrame.parse(animationFrameJson(name, t.json, c["t"]?.number ?: 0.0, false))
                val got = (c["own"]?.number ?: 1.0) * frame.opacity.coerceIn(0f, 1f)
                val want = c["expected"]?.number ?: Double.NaN
                if (abs(got - want) <= 1e-3) null else "$got vs $want"
            })
        }
        val o = f["override"]!!
        out.add(case("override") {
            val t = timing(s(o, "theme"), s(o, "name"), s(o, "durationToken")) ?: return@case "no animation on the visual"
            same(t.json, o["timing"]!!)?.let { return@case it }
            frameNear(JsonValue.parse(animationFrameJson(s(o, "name"), t.json, 60.0, false) ?: return@case "no frame"), o["frame"]!!)
        })
        return out
    }
}
