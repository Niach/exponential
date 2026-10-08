package at.exponential.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.test.junit4.createComposeRule
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.extension.ExtensionContext
import at.exponential.ui.extension.ExtensionLeaf
import at.exponential.ui.extension.ExtensionPainter
import at.exponential.ui.extension.ExtensionRegistry
import at.exponential.ui.ffi.HostRouter
import at.exponential.ui.ffi.JsonlDecoder
import at.exponential.ui.ffi.SseDecoder
import at.exponential.ui.ffi.basicCatalogId
import at.exponential.ui.ffi.builtinThemeIds
import at.exponential.ui.ffi.builtinThemeJson
import at.exponential.ui.ffi.clientCapabilitiesJson
import at.exponential.ui.ffi.combineDecisions
import at.exponential.ui.ffi.coreCatalogId
import at.exponential.ui.ffi.decideFunction
import at.exponential.ui.ffi.decideUrlJson
import at.exponential.ui.ffi.jsonDiff
import at.exponential.ui.ffi.jsonEqual
import at.exponential.ui.ffi.loadThemeJson
import at.exponential.ui.ffi.mcpActionCallJson
import at.exponential.ui.ffi.mediaRequestJson
import at.exponential.ui.ffi.messagesFromMcpResultJson
import at.exponential.ui.ffi.parseSourceJson
import at.exponential.ui.ffi.placeOverlay
import at.exponential.ui.ffi.reduceNestedJson
import at.exponential.ui.ffi.reduceSurfaceJson
import at.exponential.ui.ffi.resolveRecipeJson
import at.exponential.ui.ffi.supportedCatalogIdsFor
import at.exponential.ui.ffi.themeIssuesJson
import at.exponential.ui.ffi.validatePackageJson
import at.exponential.ui.host.HostPlugin
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.OverlayPresentation
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.SurfaceOptions
import at.exponential.ui.model.toRect
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File
import kotlin.math.abs

/**
 * VAPP-91: the Compose painter's Exponential UI conformance runner
 * (renderer `at.exponential:ui-compose`, platform `android`). Replays
 * EVERY suite of `packages/exponential-ui/conformance/manifest.json`,
 * counting cases as the manifest's `unit` says (`src/conformance.ts`):
 * the painted suites THROUGH THIS PAINTER (catalog + replay composed by
 * `ExponentialSurface` under Robolectric's native graphics, control
 * geometry from `SurfaceMeasurer` on a real Compose text measurer, layout
 * and overlays the frames the painter places), the pure ones through the
 * facade bindings. Writes the report to `$EXPONENTIAL_UI_CONFORMANCE_REPORT`,
 * default `<repo>/.conformance/exponential-ui-compose.json`; check it with
 * `bun run --filter @exponential-at/ui conformance:check <report>`.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34], qualifiers = "w390dp-h800dp-mdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class ConformanceTest {
    @get:Rule
    val compose = createComposeRule()

    @Before
    fun setUp() = Fixtures.require()

    private class Case(val name: String, val error: String?)

    private fun case(name: String, check: () -> String?): Case = try {
        Case(name, check())
    } catch (e: Throwable) {
        Case(name, "${e::class.simpleName}: ${e.message}")
    }

    /** null when equal, else the first differing path. */
    private fun same(got: String, expected: JsonValue): String? = if (jsonEqual(got, expected.json)) null else "differs at ${jsonDiff(got, expected.json).ifEmpty { "/" }}"

    private val json = Fixtures::json
    private fun list(v: JsonValue?, key: String): List<JsonValue> = v?.get(key)?.array ?: emptyList()
    private fun s(v: JsonValue?, key: String): String = v?.get(key)?.string ?: ""

    /** Never touches the network (media placeholders). */
    private object Offline : HostPlugin {
        override fun resolveUrl(src: String): String = ""
    }

    // The one composition every painted suite swaps its model into.
    private var current: SurfaceModel? by mutableStateOf(null)
    private var composed = false

    @Composable
    private fun Stage() {
        val m = current ?: return
        key(m) { Box(Modifier.fillMaxWidth()) { ExponentialSurface(m) } }
    }

    /** Paint `m` through `ExponentialSurface`; the node indices composed, in paint order. */
    private fun paint(m: SurfaceModel): List<Int> {
        val trace = LinkedHashSet<Int>()
        m.paintTrace = trace
        if (!composed) {
            compose.setContent { Stage() }
            composed = true
        }
        compose.runOnIdle { current = m }
        compose.waitForIdle()
        m.paintTrace = null
        return trace.toList()
    }

    private fun paintedModel(id: String, theme: String, mode: Mode, overlays: OverlayPresentation = OverlayPresentation.Painted): SurfaceModel =
        SurfaceModel(id, SurfaceOptions(theme = ThemeHandle.builtin(theme), mode = mode, overlays = overlays), Offline, CoroutineScope(Dispatchers.Unconfined)) {
            error("the shaper is set by ExponentialSurface")
        }

    /** Every node that must be on screen: itself and its ancestors shown, in the main tree or an open layer. */
    private fun visible(m: SurfaceModel): List<Int> {
        val open = m.layers.map { it.layer }.toSet()
        return m.nodes.filter { n ->
            if (n.layer != 0 && n.layer !in open) return@filter false
            var cur: Int? = n.index
            while (cur != null) {
                val c = m.node(cur) ?: return@filter false
                if (c.hidden) return@filter false
                cur = c.parent
            }
            true
        }.map { it.index }
    }

    private fun SurfaceModel.describe(index: Int): String {
        val n = node(index)!!
        return "${n.id} (${n.component}${n.part?.let { "/$it" } ?: ""}, parent ${n.parent?.let { node(it)?.id }}, layer ${n.layer})"
    }

    // Suites

    private fun catalogSuite(): List<Case> = list(json("catalog-components.json"), "cases").map { c ->
        case(s(c, "name")) {
            val node = c["node"]!!.json
            val reduced = JsonValue.parse(reduceNestedJson(node, coreCatalogId(), null))
            if (reduced["issues"]?.array?.isNotEmpty() == true) return@case "issues ${reduced["issues"]}"
            val m = paintedModel("catalog", "exponential", Mode.Light)
            val out = m.setNested(node)
            if (out.issuesJson != "[]") return@case "surface issues ${out.issuesJson}"
            val painted = paint(m).toSet()
            if (m.nodes.isEmpty()) return@case "no nodes"
            m.nodes.firstOrNull { it.component == "Unknown" }?.let { return@case "Unknown placeholder at ${it.id}" }
            val missing = visible(m).filter { it !in painted }.map { m.describe(it) }
            if (missing.isNotEmpty()) "not painted: ${missing.take(5)}" else null
        }
    }

    private fun macrosSuite(): List<Case> = list(json("catalog-macros.json"), "cases").map { c ->
        case(s(c, "name")) {
            val got = reduceNestedJson(c["input"]!!.json, coreCatalogId(), null)
            val expected = JsonValue.Obj(mapOf("root" to c["expected"]!!, "issues" to JsonValue.Arr(emptyList())))
            same(got, expected)
        }
    }

    private fun basicMapSuite(): List<Case> = list(json("catalog-basic-map.json"), "cases").map { c ->
        case(s(c, "name")) { same(reduceSurfaceJson(c["components"]!!.json, basicCatalogId(), null), c["expected"]!!) }
    }

    /** A painter that answers a fixed box (the extension suite checks it is the one measuring). */
    private class Probe : ExtensionPainter {
        var measured = 0

        override fun measure(leaf: ExtensionLeaf, wrap: Float?): Size {
            measured += 1
            return Size(120f, 44f)
        }

        @Composable
        override fun Paint(context: ExtensionContext) {}
    }

    private fun extensionSuite(): List<Case> {
        val f = json("catalog-extension.json")
        val ext = f["extension"]!!.json
        return list(f, "cases").mapIndexed { i, c ->
            case(s(c, "name")) {
                same(reduceSurfaceJson(c["components"]!!.json, s(c, "catalogId"), "[$ext]"), c["expected"]!!)?.let { return@case it }
                if (i != 0) return@case null
                // Its natives reach this painter's extension painter.
                val probe = Probe()
                ExtensionRegistry.shared.reset()
                try {
                    ExponentialUi.register(ext, JsonValue.parse(ext)["components"]!!.obj!!.keys.associateWith { probe })
                    val m = makeModel("ext", options = SurfaceOptions(catalogId = s(c, "catalogId")))
                    m.setComponents(c["components"]!!.json)
                    val leaves = m.nodes.filter { it.component == "Extension" }
                    when {
                        leaves.isEmpty() -> "no Extension leaf"
                        probe.measured == 0 -> "the extension painter never measured"
                        leaves.any { abs(m.frame(it.index).height - 44f) > 0.01f } -> "an Extension leaf is not at its painter's size"
                        else -> {
                            val painted = paint(m).toSet()
                            if (leaves.any { it.index !in painted }) "an Extension leaf was not painted" else null
                        }
                    }
                } finally {
                    ExtensionRegistry.shared.reset()
                }
            }
        }
    }

    private fun themeResolvedSuite(): List<Case> = json("theme-resolved.json")["themes"]!!.obj!!.map { (id, expected) ->
        case(id) { same(builtinThemeJson(id) ?: return@case "not a built-in", expected) }
    }

    private fun themeRecipesSuite(): List<Case> {
        val themes = HashMap<String, String?>()
        return list(json("theme-recipes.json"), "cases").map { c ->
            val component = s(c, "component")
            val part = s(c, "part")
            case("$component/$part ${c["props"]}") {
                for ((themeId, byMode) in c["visuals"]!!.obj!!) {
                    val t = themes.getOrPut(themeId) { builtinThemeJson(themeId) } ?: return@case "$themeId: not a built-in"
                    for ((mode, byState) in byMode.obj!!) for ((state, style) in byState.obj!!) {
                        val states = if (state == "default") emptyList() else listOf(state)
                        same(resolveRecipeJson(t, component, part, c["props"]!!.json, states, mode), style)?.let { return@case "$themeId/$mode/$state: $it" }
                    }
                }
                null
            }
        }
    }

    private fun themeExtendsSuite(): List<Case> = list(json("theme-extends.json"), "cases").map { c ->
        case(s(c, "name")) {
            val t = loadThemeJson(c["theme"]!!.json, null)
            val chain = JsonValue.parse(t)["chain"] ?: JsonValue.Null
            same(chain.json, c["expected"]!!["chain"]!!)?.let { return@case "chain $it" }
            for (p in list(c["expected"], "probes")) {
                val states = p["states"]?.array?.mapNotNull { it.string } ?: emptyList()
                same(resolveRecipeJson(t, s(p, "component"), s(p, "part"), (p["props"] ?: JsonValue.Obj(emptyMap())).json, states, s(p, "mode")), p["style"]!!)
                    ?.let { return@case "${s(p, "component")}/${s(p, "part")}: $it" }
            }
            null
        }
    }

    private fun themeInvalidSuite(): List<Case> = list(json("theme-invalid.json"), "cases").map { c ->
        case(s(c, "name")) {
            val issues = themeIssuesJson(c["theme"]!!.json, null)
            if (issues == "[]") "loaded" else same(issues, c["issues"]!!)
        }
    }

    /**
     * The border box THIS painter gives each control: a real-measure pass
     * (`SurfaceMeasurer` on a Compose text measurer) of the control with the
     * fixture's props over the catalog example; the sizing part's frame and
     * the visual the painter draws it with.
     */
    private fun controlGeometrySuite(): List<Case> {
        val examples = JsonValue.parse(File(Fixtures.dir.parentFile, "catalog/core.catalog.json").readText())["components"]!!
        val out = ArrayList<Case>()
        for ((themeId, byComponent) in json("control-geometry.json")["themes"]!!.obj!!) {
            for ((component, entry) in byComponent.obj!!) {
                val part = s(entry, "part")
                for ((name, c) in entry["cases"]!!.obj!!) {
                    out.add(
                        case("$themeId $component $name") {
                            val props = (examples[component]?.get("example")?.obj ?: emptyMap()) + (c["props"]?.obj ?: emptyMap())
                            val node = JsonValue.Obj(mapOf("id" to JsonValue.Str("c"), "component" to JsonValue.Str(component), "props" to JsonValue.Obj(props)))
                            val m = makeModel("geo", theme = themeId, mode = Mode.Light, host = RecordingHost())
                            m.setNested(node.json)
                            val box = measuredBox(m, component, part) ?: return@case "part $part not painted"
                            checkGeometry(c["geometry"]!!.obj!!, box)
                        },
                    )
                }
            }
        }
        return out
    }

    /** (width, height, paddingHorizontal, paddingVertical, borderWidth, borderRadius) of the sizing part. */
    private fun measuredBox(m: SurfaceModel, component: String, part: String): Map<String, Float>? {
        fun box(index: Int): Map<String, Float> {
            val f = m.frame(index)
            val st = m.boxStyle(index)
            return mapOf("width" to f.width, "height" to f.height, "paddingHorizontal" to st.paddingHorizontal, "paddingVertical" to st.paddingVertical, "borderWidth" to st.borderWidth, "borderRadius" to st.radius)
        }
        fun partNode(p: String): Int? = m.nodes.firstOrNull { it.owner == "c" && it.part == p }?.index
        return when {
            part == "root" -> m.indexOf("c")?.let(::box)
            // The field paints the trigger recipe (SurfaceModel.boxStyle).
            component == "Select" || component == "DatePicker" -> partNode("field")?.let(::box)
            // The Radio item recipe is the dot's.
            component == "Radio" -> partNode("dot")?.let(::box)
            // ToggleGroup: one leaf; the measurer sizes every item from the item recipe.
            component == "ToggleGroup" -> {
                val i = m.indexOf("c") ?: return null
                val root = m.boxStyle(i)
                val item = m.part("ToggleGroup", "item", m.node(i)!!.props)
                mapOf(
                    "height" to m.frame(i).height - 2 * (root.paddingVertical + root.borderWidth),
                    "paddingHorizontal" to (item.px("paddingHorizontal") ?: item.px("padding") ?: 12f),
                    "borderWidth" to item.style.borderWidth,
                    "borderRadius" to item.style.radius,
                )
            }
            // Slider: the thumb the painter draws (drawn or the M3 thumb) at the recipe's box.
            component == "Slider" -> {
                val i = partNode("track") ?: return null
                val thumb = m.part("Slider", "thumb", m.ownerProps(i))
                mapOf("width" to (thumb.width ?: 16f), "height" to (thumb.height ?: thumb.width ?: 16f), "borderWidth" to thumb.style.borderWidth, "borderRadius" to thumb.style.radius)
            }
            else -> partNode(part)?.let(::box)
        }
    }

    /** `src/geometry.ts` `checkGeometry` (tolerance 0.5): every key the theme fixes; `height` not probed under a `minHeight`. */
    private fun checkGeometry(expected: Map<String, JsonValue>, measured: Map<String, Float>, tolerance: Double = 0.5): String? {
        val issues = ArrayList<String>()
        fun probe(key: String) {
            val want = expected[key]?.number ?: return
            val got = measured[key]
            if (got == null || abs(got - want) > tolerance) issues.add("$key $want≠$got")
        }
        probe("width")
        val minHeight = expected["minHeight"]?.number
        if (minHeight == null) probe("height")
        listOf("paddingHorizontal", "paddingVertical", "borderWidth", "borderRadius").forEach(::probe)
        if (minHeight != null) {
            val h = measured["height"]
            if (h == null || h + tolerance < minHeight) issues.add("minHeight $minHeight≠$h")
        }
        return if (issues.isEmpty()) null else issues.joinToString(", ")
    }

    /** The exact taffy frames reach the painter's model (`FfiFrame.toRect`), in pre-order. */
    private fun layoutSuite(): List<Case> {
        val fx = json("layout-geometry.json")
        return fx["cases"]!!.obj!!.map { (name, c) ->
            case(name) {
                val tree = fx["surface"]!!.obj!!.toMutableMap()
                tree["style"] = JsonValue.Obj((tree["style"]?.obj ?: emptyMap()) + ("direction" to c["direction"]!!))
                val m = SurfaceModel("geometry", SurfaceOptions(theme = null, mode = Mode.Light), RecordingHost(), CoroutineScope(Dispatchers.Unconfined), noShaper)
                m.fixedMeasure = true
                val outcome = m.setNested(JsonValue.Obj(tree).json)
                if (outcome.issuesJson != "[]") return@case "issues ${outcome.issuesJson}"
                val width = c["width"]!!.number!!.toFloat()
                m.surface.setViewport(width, 0f, null)
                val out = m.surface.layoutFixed(fx["measures"]!!.json, false)
                // The model reads the same nodes (its own pass runs after the fixture's).
                m.setViewport(width, 0f)
                val expected = c["frames"]!!.array!!
                if (out.frames.size != expected.size) return@case "${out.frames.size} frames, expected ${expected.size}"
                for ((i, want) in expected.withIndex()) {
                    val r = out.frames[i].toRect()
                    val id = m.nodes[out.frames[i].index.toInt()].id
                    if (id != s(want, "id")) return@case "order at $i: $id"
                    for ((k, v) in listOf("x" to r.left, "y" to r.top, "w" to r.width, "h" to r.height)) {
                        val e = want[k]!!.number!!
                        if (abs(v - e) > 0.001) return@case "$id.$k = $v, expected $e"
                    }
                }
                null
            }
        }
    }

    private fun overlaySuite(): List<Case> {
        val fx = json("overlay-geometry.json")
        val tolerance = fx["tolerancePx"]?.number ?: 0.0
        return list(fx, "cases").map { c ->
            case(s(c, "name")) {
                val a = c["anchor"]!!
                val z = c["size"]!!
                val v = c["viewport"]!!
                val p = placeOverlay(
                    a["x"]!!.number!!, a["y"]!!.number!!, a["width"]!!.number!!, a["height"]!!.number!!,
                    z["width"]!!.number!!, z["height"]!!.number!!, v["width"]!!.number!!, v["height"]!!.number!!, s(c, "side"),
                )
                val e = c["expected"]!!
                when {
                    abs(p.x - e["x"]!!.number!!) > tolerance -> "x ${p.x}"
                    abs(p.y - e["y"]!!.number!!) > tolerance -> "y ${p.y}"
                    p.side != s(e, "side") -> "side ${p.side}"
                    e["flipped"]?.bool != null && p.flipped != e["flipped"]!!.bool -> "flipped ${p.flipped}"
                    else -> null
                }
            }
        }
    }

    /** The kitchen sink composed by `ExponentialSurface` in every built-in theme × mode, real text measure. */
    private fun replaySuite(): List<Case> {
        val sink = Fixtures.text("kitchen-sink.json")
        val expanded = json("kitchen-sink.expanded.json").obj!!.filterKeys { it != "\$comment" }
        val out = ArrayList<Case>()
        for (theme in builtinThemeIds()) for (mode in Mode.entries) {
            out.add(
                case("$theme/${mode.wire}") {
                    same(reduceNestedJson(sink, coreCatalogId(), null), JsonValue.Obj(expanded))?.let { return@case "reduced tree $it" }
                    val m = paintedModel("ks", theme, mode)
                    val outcome = m.setNested(sink)
                    if (outcome.issuesJson != "[]") return@case "issues ${outcome.issuesJson}"
                    m.apply("""{"version":"v0.9","updateDataModel":{"surfaceId":"ks","value":{"posts":[{"title":"One"},{"title":"Two"}],"ui":{"confirmOpen":false}}}}""")
                    val painted = paint(m)
                    m.nodes.firstOrNull { it.component == "Unknown" }?.let { return@case "Unknown at ${it.id}" }
                    if (painted.size < 50) return@case "only ${painted.size} nodes painted"
                    val missing = visible(m).filter { it !in painted.toSet() }
                    if (missing.isNotEmpty()) return@case "not painted: ${missing.take(5).map { m.describe(it) }}"
                    // Paint order (= the accessibility traversal order) is the pre-order.
                    if (painted.zipWithNext().any { (a, b) -> b <= a }) return@case "paint order is not the pre-order"
                    if (m.frames.none { it.width > 0f && it.height > 0f }) "every frame empty" else null
                },
            )
        }
        return out
    }

    private fun feed(make: () -> Pair<(String) -> String, () -> String>, chunks: List<JsonValue>): String {
        val (push, end) = make()
        val messages = ArrayList<JsonValue>()
        val issues = ArrayList<JsonValue>()
        for (d in chunks.map { push(it.string ?: "") } + end()) {
            val v = JsonValue.parse(d)
            messages.addAll(v["messages"]?.array ?: emptyList())
            issues.addAll(v["issues"]?.array ?: emptyList())
        }
        return JsonValue.Obj(mapOf("messages" to JsonValue.Arr(messages), "issues" to JsonValue.Arr(issues))).json
    }

    private fun hostTransportSuite(): List<Case> {
        val f = json("host-transport.json")
        val jsonl = { JsonlDecoder().let { d -> Pair<(String) -> String, () -> String>({ d.push(it) }, { d.end() }) } }
        val sse = { SseDecoder().let { d -> Pair<(String) -> String, () -> String>({ d.push(it) }, { d.end() }) } }
        return list(f, "jsonl").map { c -> case("jsonl: ${s(c, "name")}") { same(feed(jsonl, list(c, "chunks")), c["expected"]!!) } } +
            list(f, "sse").map { c -> case("sse: ${s(c, "name")}") { same(feed(sse, list(c, "chunks")), c["expected"]!!) } } +
            list(f, "mcp").map { c -> case("mcp: ${s(c, "name")}") { same(messagesFromMcpResultJson(c["result"]!!.json), c["expected"]!!) } } +
            list(f, "mcpAction").map { c -> case("mcpAction: ${s(c, "name")}") { same(mcpActionCallJson(c["message"]!!.json, c["tool"]?.string), c["expected"]!!) } }
    }

    private fun hostPolicySuite(): List<Case> {
        val f = json("host-policy.json")
        val out = ArrayList<Case>()
        for (c in list(f, "functions")) out.add(case("function: ${s(c, "name")}") { same(JsonValue.Str(decideFunction(c["policy"]?.json, s(c, "fn"), c["registered"]!!.bool!!)).json, c["expected"]!!) })
        for (c in list(f, "combine")) out.add(case("combine: ${s(c, "a")} + ${s(c, "b")}") { same(JsonValue.Str(combineDecisions(s(c, "a"), s(c, "b"))).json, c["expected"]!!) })
        for (c in list(f, "urls")) out.add(case("url: ${s(c, "name")}") { same(decideUrlJson(c["policy"]?.json, s(c, "url")), c["expected"]!!) })
        for (c in list(f, "media")) out.add(case("media: ${s(c, "name")}") { same(mediaRequestJson(s(c, "url"), c["options"]!!.json) ?: "null", c["expected"] ?: JsonValue.Null) })
        for (c in list(f, "sources")) out.add(case("source: ${s(c, "uri")}") { same(parseSourceJson(s(c, "uri")) ?: "null", c["expected"] ?: JsonValue.Null) })
        for (c in list(f, "negotiation")) {
            val ids = c["extensionIds"]!!.array!!.map { it.string!! }
            out.add(
                case("negotiation: ${ids.size} extension ids") {
                    same(JsonValue.Arr(supportedCatalogIdsFor(ids).map(JsonValue::Str)).json, c["expected"]!!["supportedCatalogIds"]!!)
                        ?: same(clientCapabilitiesJson(ids), c["expected"]!!["clientCapabilities"]!!)
                },
            )
        }
        return out
    }

    private fun hostRouterSuite(): List<Case> {
        val f = json("host-router.json")
        val packages = f["packages"]!!
        val out = ArrayList<Case>()
        for (v in list(f, "validation")) {
            val id = s(v, "package")
            out.add(case("validation: $id") { same(validatePackageJson(packages[id]!!.json, null), v["expected"]!!) })
        }
        for (flow in list(f, "flows")) {
            out.add(
                case("flow: ${s(flow, "name")}") {
                    val router = HostRouter(flow["extensionIds"]?.array?.map { it.string!! } ?: emptyList())
                    try {
                        for (id in flow["packages"]?.array?.map { it.string!! } ?: emptyList()) {
                            same(router.installPackage(packages[id]!!.json), flow["installIssues"]!![id]!!)?.let { return@case "install $id: $it" }
                        }
                        for ((i, step) in list(flow, "steps").withIndex()) {
                            same(router.route(step["message"]!!.json), step["expected"]!!)?.let { return@case "step $i: $it" }
                        }
                        null
                    } finally {
                        router.close()
                    }
                },
            )
        }
        return out
    }

    private fun suite(id: String): List<Case> = when (id) {
        "catalog" -> catalogSuite()
        "macros" -> macrosSuite()
        "basic-map" -> basicMapSuite()
        "extension" -> extensionSuite()
        "theme-resolved" -> themeResolvedSuite()
        "theme-recipes" -> themeRecipesSuite()
        "theme-extends" -> themeExtendsSuite()
        "theme-invalid" -> themeInvalidSuite()
        "control-geometry" -> controlGeometrySuite()
        "layout" -> layoutSuite()
        "overlay" -> overlaySuite()
        "replay" -> replaySuite()
        "host-transport" -> hostTransportSuite()
        "host-policy" -> hostPolicySuite()
        "host-router" -> hostRouterSuite()
        else -> error("the runner does not know the suite $id: add it")
    }

    private fun reportFile(): File =
        File(System.getenv("EXPONENTIAL_UI_CONFORMANCE_REPORT") ?: System.getProperty("exponential.ui.conformanceReport") ?: "../../../.conformance/exponential-ui-compose.json")

    @Test
    fun thePainterPassesEveryConformanceSuiteAndWritesItsReport() {
        val manifest = JsonValue.parse(File(Fixtures.dir.parentFile, "conformance/manifest.json").readText())
        val suites = LinkedHashMap<String, JsonValue>()
        val problems = ArrayList<String>()
        for (sm in manifest["suites"]!!.array!!) {
            val id = s(sm, "id")
            val cases = suite(id)
            val failed = cases.filter { it.error != null }.map { "${it.name}: ${it.error}" }
            val want = sm["cases"]!!.number!!.toInt()
            if (cases.size != want) problems.add("$id: ran ${cases.size} of $want cases")
            if (failed.isNotEmpty()) problems.add("$id: ${failed.size} failed\n  ${failed.joinToString("\n  ")}")
            suites[id] = JsonValue.Obj(
                mapOf(
                    "cases" to JsonValue.Num(cases.size.toDouble()),
                    "passed" to JsonValue.Num((cases.size - failed.size).toDouble()),
                    "failed" to JsonValue.Arr(failed.map(JsonValue::Str)),
                ),
            )
        }
        val report = JsonValue.Obj(
            mapOf(
                "renderer" to JsonValue.Str("at.exponential:ui-compose"),
                "platform" to JsonValue.Str("android"),
                "version" to JsonValue.Str(System.getProperty("exponential.ui.version") ?: "0.1.0"),
                "conformanceVersion" to manifest["version"]!!,
                "suites" to JsonValue.Obj(suites),
            ),
        )
        val file = reportFile().absoluteFile
        file.parentFile.mkdirs()
        file.writeText(prettyJson(report).replace("\\/", "/") + "\n")
        println("conformance report: ${file.canonicalPath}")
        assertEquals("not conformant:\n${problems.joinToString("\n")}", emptyList<String>(), problems)
    }
}
