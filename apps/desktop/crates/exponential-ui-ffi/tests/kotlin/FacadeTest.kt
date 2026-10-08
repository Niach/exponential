// VAPP-86: the Kotlin binding suite (JNA on the JVM; the Compose painter
// links the same bindings on Android). Replays the shared fixtures through
// the facade and drives a surface with a Kotlin `Measurer`.
//   bash apps/desktop/crates/exponential-ui-ffi/run-binding-tests.sh kotlin
package tests

import at.exponential.ui.ffi.*
import java.io.File
import kotlin.system.exitProcess

var failures = 0
var checks = 0

fun check(ok: Boolean, message: () -> String) {
    checks += 1
    if (!ok) {
        failures += 1
        println("FAIL: ${message()}")
    }
}

// A tiny JSON reader/writer (no library on the test classpath): parses into
// Map/List/String/Double/Boolean/null and writes them back.
object Json {
    fun parse(text: String): Any? = Parser(text).value()
    fun write(v: Any?): String = StringBuilder().also { emit(v, it) }.toString()

    private fun emit(v: Any?, sb: StringBuilder) {
        when (v) {
            null -> sb.append("null")
            is String -> { sb.append('"'); for (c in v) when (c) { '"' -> sb.append("\\\""); '\\' -> sb.append("\\\\"); '\n' -> sb.append("\\n"); '\r' -> sb.append("\\r"); '\t' -> sb.append("\\t"); else -> if (c < ' ') sb.append(String.format("\\u%04x", c.code)) else sb.append(c) }; sb.append('"') }
            is Boolean -> sb.append(v)
            is Double -> if (v == Math.floor(v) && !v.isInfinite() && Math.abs(v) < 1e15) sb.append(v.toLong()) else sb.append(v)
            is Number -> sb.append(v)
            is Map<*, *> -> { sb.append('{'); var first = true; for ((k, x) in v) { if (!first) sb.append(','); first = false; emit(k as String, sb); sb.append(':'); emit(x, sb) }; sb.append('}') }
            is List<*> -> { sb.append('['); var first = true; for (x in v) { if (!first) sb.append(','); first = false; emit(x, sb) }; sb.append(']') }
            else -> emit(v.toString(), sb)
        }
    }

    private class Parser(val s: String) {
        var i = 0
        fun ws() { while (i < s.length && s[i].isWhitespace()) i++ }
        fun value(): Any? {
            ws()
            return when (s[i]) {
                '{' -> { i++; val m = LinkedHashMap<String, Any?>(); ws(); if (s[i] == '}') { i++; return m }; while (true) { ws(); val k = str(); ws(); i++; m[k] = value(); ws(); if (s[i] == ',') { i++; continue }; i++; return m } }
                '[' -> { i++; val l = ArrayList<Any?>(); ws(); if (s[i] == ']') { i++; return l }; while (true) { l.add(value()); ws(); if (s[i] == ',') { i++; continue }; i++; return l } }
                '"' -> str()
                't' -> { i += 4; true }
                'f' -> { i += 5; false }
                'n' -> { i += 4; null }
                else -> { val start = i; while (i < s.length && (s[i].isDigit() || s[i] in "+-.eE")) i++; s.substring(start, i).toDouble() }
            }
        }
        fun str(): String {
            i++
            val sb = StringBuilder()
            while (s[i] != '"') {
                if (s[i] == '\\') { i++; when (s[i]) { 'n' -> sb.append('\n'); 't' -> sb.append('\t'); 'r' -> sb.append('\r'); 'b' -> sb.append('\b'); 'f' -> sb.append('\u000c'); 'u' -> { sb.append(s.substring(i + 1, i + 5).toInt(16).toChar()); i += 4 }; else -> sb.append(s[i]) }; i++ } else { sb.append(s[i]); i++ }
            }
            i++
            return sb.toString()
        }
    }
}

@Suppress("UNCHECKED_CAST")
fun main(args: Array<String>) {
    val fixtures = if (args.isNotEmpty()) args[0] else "../../../../packages/exponential-ui/fixtures"
    fun load(name: String): Map<String, Any?> = Json.parse(File("$fixtures/$name").readText()) as Map<String, Any?>
    fun equal(got: String, expected: Any?, label: String) { val e = Json.write(expected); check(jsonEqual(got, e)) { "$label: ${jsonDiff(got, e)}" } }
    fun obj(v: Any?) = v as Map<String, Any?>
    fun list(v: Any?) = v as List<Any?>

    val macros = load("catalog-macros.json")
    for (c in list(macros["cases"]).map(::obj)) equal(reduceNestedJson(Json.write(c["input"]), coreCatalogId(), null), mapOf("root" to c["expected"], "issues" to emptyList<Any>()), "macros ${c["name"]}")
    val basic = load("catalog-basic-map.json")
    for (c in list(basic["cases"]).map(::obj)) equal(reduceSurfaceJson(Json.write(c["components"]), basicCatalogId(), null), c["expected"], "basic ${c["name"]}")
    val ext = load("catalog-extension.json")
    check(extensionErrors(Json.write(ext["extension"])).isEmpty()) { "extension validates" }
    for (c in list(ext["cases"]).map(::obj)) equal(reduceSurfaceJson(Json.write(c["components"]), c["catalogId"] as String, Json.write(listOf(ext["extension"]))), c["expected"], "extension ${c["name"]}")
    val components = load("catalog-components.json")
    for (c in list(components["cases"]).map(::obj)) {
        val got = obj(Json.parse(reduceNestedJson(Json.write(c["node"]), coreCatalogId(), null)))
        check(list(got["issues"]).isEmpty()) { "components ${c["name"]}: ${got["issues"]}" }
    }
    println("catalog fixtures: $checks checks")

    val resolved = load("theme-resolved.json")
    val themes = HashMap<String, String>()
    for ((id, expected) in obj(resolved["themes"])) { val got = builtinThemeJson(id)!!; themes[id] = got; equal(got, expected, "theme-resolved $id") }
    val recipes = load("theme-recipes.json")
    for (c in list(recipes["cases"]).map(::obj)) for ((themeId, byMode) in obj(c["visuals"])) for ((mode, byState) in obj(byMode)) for ((state, style) in obj(byState)) {
        val states = if (state == "default") emptyList() else listOf(state)
        equal(resolveRecipeJson(themes[themeId]!!, c["component"] as String, c["part"] as String, Json.write(c["props"]), states, mode), style, "theme-recipes $themeId/$mode/$state ${c["component"]}/${c["part"]}")
    }
    val extends = load("theme-extends.json")
    for (c in list(extends["cases"]).map(::obj)) {
        val theme = loadThemeJson(Json.write(c["theme"]), null)
        val parsed = obj(Json.parse(theme))
        val expected = obj(c["expected"])
        check(list(parsed["chain"]) == list(expected["chain"])) { "theme-extends ${c["name"]}: chain" }
        for (p in list(expected["probes"]).map(::obj)) equal(resolveRecipeJson(theme, p["component"] as String, p["part"] as String, Json.write(p["props"]), (p["states"] as List<String>?) ?: emptyList(), p["mode"] as String), p["style"], "theme-extends ${c["name"]} ${p["component"]}/${p["part"]}")
    }
    val invalid = load("theme-invalid.json")
    for (c in list(invalid["cases"]).map(::obj)) equal(themeIssuesJson(Json.write(c["theme"]), null), c["issues"], "theme-invalid ${c["name"]}")
    val geometry = load("control-geometry.json")
    for ((themeId, byComponent) in obj(geometry["themes"])) for ((component, entry) in obj(byComponent)) for ((name, cs) in obj(obj(entry)["cases"])) equal(controlGeometryJson(themes[themeId]!!, component, Json.write(obj(cs)["props"])), obj(cs)["geometry"], "control-geometry $themeId $component $name")
    println("theme fixtures: $checks checks so far")

    val overlay = load("overlay-geometry.json")
    for (c in list(overlay["cases"]).map(::obj)) {
        val a = obj(c["anchor"]); val s = obj(c["size"]); val v = obj(c["viewport"]); val e = obj(c["expected"])
        val p = placeOverlay(a["x"] as Double, a["y"] as Double, a["width"] as Double, a["height"] as Double, s["width"] as Double, s["height"] as Double, v["width"] as Double, v["height"] as Double, c["side"] as String)
        check(p.x == e["x"] && p.y == e["y"] && p.side == e["side"] && p.flipped == e["flipped"]) { "overlay ${c["name"]}: $p" }
    }
    val layout = load("layout-geometry.json")
    for ((name, csAny) in obj(layout["cases"])) {
        val cs = obj(csAny)
        val tree = LinkedHashMap(obj(layout["surface"]))
        val style = LinkedHashMap(obj(tree["style"])); style["direction"] = cs["direction"]; tree["style"] = style
        val surface = Surface("geometry", coreCatalogId(), "", "light")
        check(surface.setNested(Json.write(tree)).issuesJson == "[]") { "layout $name: issues" }
        surface.setViewport((cs["width"] as Double).toFloat(), 0f, null)
        val out = surface.layoutFixed(Json.write(layout["measures"]), false)
        val nodes = surface.nodes()
        val expected = list(cs["frames"]).map(::obj)
        check(out.frames.size == expected.size) { "layout $name: ${out.frames.size} frames" }
        for ((i, want) in expected.withIndex()) if (i < out.frames.size) {
            val f = out.frames[i]
            check(nodes[f.index.toInt()].id == want["id"]) { "layout $name: order at $i" }
            for ((k, v) in listOf("x" to f.x, "y" to f.y, "w" to f.w, "h" to f.h)) check(Math.abs(v - (want[k] as Double).toFloat()) <= 0.001f) { "layout $name: ${want["id"]}.$k = $v vs ${want[k]}" }
        }
        check(out.upcalls == 1u) { "layout $name: ${out.upcalls} upcalls" }
    }
    println("layout fixtures: $checks checks so far")

    class CountingMeasurer : Measurer {
        var intrinsics = 0
        var heights = 0
        override fun measureId(): ULong = 5u
        override fun measureIntrinsics(leaves: List<FfiLeaf>): List<FfiIntrinsics> {
            intrinsics += 1
            return leaves.map { l ->
                val chars = l.text.length.toFloat()
                val w = l.control.width ?: (8 * chars + 2 * l.control.paddingHorizontal)
                val h = l.control.height ?: (l.textStyle.lineHeight + 2 * l.control.paddingVertical)
                val minW = if (l.component == "Text") (l.text.split(" ").maxOfOrNull { it.length * 8f } ?: 0f) else w
                FfiIntrinsics(minW, w, h)
            }
        }
        override fun measureHeights(leaves: List<FfiLeaf>, requests: List<FfiHeightRequest>): List<Float> = requests.map { r ->
            heights += 1
            val l = leaves.first { it.index == r.index }
            val perLine = Math.max(1f, Math.floor((r.width / 8f).toDouble()).toFloat())
            Math.max(1f, Math.ceil((l.text.length / perLine).toDouble()).toFloat()) * l.textStyle.lineHeight
        }
    }
    val bench = Surface("bench", coreCatalogId(), null, "light")
    check(bench.setNested(benchTreeJson(200u)).issuesJson == "[]") { "bench: issues" }
    bench.setViewport(390f, 0f, null)
    val measurer = CountingMeasurer()
    val first = bench.layout(measurer)
    check(bench.nodeCount() >= 200u) { "bench: ${bench.nodeCount()} nodes" }
    check(first.upcalls <= 3u) { "bench: ${first.upcalls} upcalls" }
    check(first.frames.size.toUInt() == bench.nodeCount()) { "bench: frames" }
    check(bench.layout(measurer).upcalls == 0u) { "bench: warm pass" }
    var best = Double.MAX_VALUE
    for (i in 0 until 30) {
        bench.setViewport(if (i % 2 == 0) 390f else 900f, 0f, null)
        val t0 = System.nanoTime()
        bench.layout(measurer)
        bench.nodes().size
        val dt = (System.nanoTime() - t0) / 1_000_000.0
        if (i >= 2) best = Math.min(best, dt)
    }
    var bestLayout = Double.MAX_VALUE
    for (i in 0 until 30) {
        bench.setViewport(if (i % 2 == 0) 390f else 900f, 0f, null)
        val t0 = System.nanoTime()
        bench.layout(measurer)
        bestLayout = Math.min(bestLayout, (System.nanoTime() - t0) / 1_000_000.0)
    }
    println(String.format("bench 200 nodes through Kotlin/JNA: %d layout nodes, best warm layout() %.3f ms, layout() + nodes() %.3f ms", bench.nodeCount().toInt(), bestLayout, best))
    check(bestLayout < 2.0) { "bench: warm pass $bestLayout ms" }

    val s = Surface("s", coreCatalogId(), null, "dark")
    s.apply(Json.write(mapOf("version" to "v0.9", "updateComponents" to mapOf("surfaceId" to "s", "components" to listOf(
        mapOf("id" to "root", "component" to "Box", "style" to mapOf("display" to "flex", "flexDirection" to "column"), "children" to listOf("dlg", "t")),
        mapOf("id" to "t", "component" to "Text", "text" to mapOf("path" to "/title")),
        mapOf("id" to "dlg", "component" to "Dialog", "title" to "Hi", "open" to mapOf("path" to "/open"), "slots" to mapOf("trigger" to "b"), "children" to listOf("body")),
        mapOf("id" to "b", "component" to "Button", "label" to "Open", "on" to mapOf("press" to mapOf("event" to mapOf("name" to "opened")))),
        mapOf("id" to "body", "component" to "Text", "text" to "Body"),
    )))))
    s.setData("/title", "\"Hello\"")
    s.setViewport(400f, 700f, null)
    check(s.layoutFixed(null, true).layers.isEmpty()) { "dialog starts closed" }
    val b = s.indexOf("b")!!
    val events = s.event(b, "press", null).map { it.kind }
    check(events.contains("action") && events.contains("dataChanged") && events.contains("relayout")) { "press events $events" }
    val opened = s.layoutFixed(null, true)
    check(opened.layers.size == 1 && opened.layers[0].position == "centered") { "dialog layer" }
    check(s.nodes().any { it.id == "dlg.title" && it.ownerComponent == "Dialog" }) { "title part" }

    // --- host API (VAPP-91) ---------------------------------------------------
    fun decodeFeed(push: (String) -> String, end: () -> String, chunks: List<Any?>): Map<String, Any?> {
        val messages = ArrayList<Any?>()
        val issues = ArrayList<Any?>()
        for (out in chunks.map { push(it as String) } + listOf(end())) {
            val v = obj(Json.parse(out))
            messages.addAll(list(v["messages"]))
            issues.addAll(list(v["issues"]))
        }
        return mapOf("messages" to messages, "issues" to issues)
    }
    val transport = load("host-transport.json")
    for (c in list(transport["jsonl"]).map(::obj)) { val d = JsonlDecoder(); equal(Json.write(decodeFeed({ d.push(it) }, { d.end() }, list(c["chunks"]))), c["expected"], "jsonl ${c["name"]}") }
    for (c in list(transport["sse"]).map(::obj)) { val d = SseDecoder(); equal(Json.write(decodeFeed({ d.push(it) }, { d.end() }, list(c["chunks"]))), c["expected"], "sse ${c["name"]}") }
    for (c in list(transport["mcp"]).map(::obj)) equal(messagesFromMcpResultJson(Json.write(c["result"])), c["expected"], "mcp ${c["name"]}")
    for (c in list(transport["mcpAction"]).map(::obj)) equal(mcpActionCallJson(Json.write(c["message"]), c["tool"] as String?), c["expected"], "mcpAction ${c["name"]}")
    val policy = load("host-policy.json")
    for (c in list(policy["functions"]).map(::obj)) {
        val got = decideFunction(if (c.containsKey("policy")) Json.write(c["policy"]) else null, c["fn"] as String, c["registered"] as Boolean)
        check(got == c["expected"]) { "function ${c["name"]}: $got" }
    }
    for (c in list(policy["combine"]).map(::obj)) check(combineDecisions(c["a"] as String, c["b"] as String) == c["expected"]) { "combine $c" }
    for (c in list(policy["urls"]).map(::obj)) equal(decideUrlJson(if (c.containsKey("policy")) Json.write(c["policy"]) else null, c["url"] as String), c["expected"], "url ${c["name"]}")
    for (c in list(policy["media"]).map(::obj)) equal(mediaRequestJson(c["url"] as String, Json.write(c["options"])) ?: "null", c["expected"], "media ${c["name"]}")
    for (c in list(policy["sources"]).map(::obj)) equal(parseSourceJson(c["uri"] as String) ?: "null", c["expected"], "source ${c["uri"]}")
    for (c in list(policy["negotiation"]).map(::obj)) {
        val ids = list(c["extensionIds"]).map { it as String }
        val expected = obj(c["expected"])
        check(supportedCatalogIdsFor(ids) == list(expected["supportedCatalogIds"])) { "negotiation $ids" }
        equal(clientCapabilitiesJson(ids), expected["clientCapabilities"], "capabilities $ids")
    }
    val routerFixture = load("host-router.json")
    val hostPackages = obj(routerFixture["packages"])
    for (v in list(routerFixture["validation"]).map(::obj)) equal(validatePackageJson(Json.write(hostPackages[v["package"] as String]), null), v["expected"], "validation ${v["package"]}")
    for (flow in list(routerFixture["flows"]).map(::obj)) {
        val name = flow["name"] as String
        val router = HostRouter((flow["extensionIds"] as List<String>?) ?: emptyList())
        for (id in (flow["packages"] as List<String>?) ?: emptyList()) equal(router.installPackage(Json.write(hostPackages[id])), obj(flow["installIssues"])[id], "$name: install $id")
        for ((i, step) in list(flow["steps"]).map(::obj).withIndex()) equal(router.route(Json.write(step["message"])), step["expected"], "$name: step $i")
    }
    val hostRouter = HostRouter(emptyList())
    hostRouter.installPackage(Json.write(hostPackages["acme.devices"]))
    hostRouter.route(Json.write(mapOf("version" to "v0.9", "applyTemplate" to mapOf("surfaceId" to "d", "templateId" to "list"))))
    check(hostRouter.surfaceIds() == listOf("d") && hostRouter.packageIdOf("d") == "acme.devices") { "router state" }
    equal(errorMessageJson("FUNCTION_DENIED", "s", "m", null), mapOf("version" to "v0.9", "error" to mapOf("code" to "FUNCTION_DENIED", "surfaceId" to "s", "message" to "m")), "errorMessage")
    equal(actionMessageJson("s", "b", "go", "{}", null, "t"), mapOf("version" to "v0.9", "action" to mapOf("name" to "go", "surfaceId" to "s", "sourceComponentId" to "b", "timestamp" to "t", "context" to emptyMap<String, Any>())), "actionMessage")
    check(templateMessagesJson(Json.write(hostPackages["acme.devices"]), "nope", "x", null) == null) { "missing template" }
    val fnSurface = Surface("f", coreCatalogId(), null, "light")
    fnSurface.setNested(Json.write(mapOf("id" to "root", "component" to "Box", "children" to listOf(mapOf("id" to "b", "component" to "Button", "props" to mapOf("label" to "Go"), "on" to mapOf("press" to mapOf("functionCall" to mapOf("call" to "harness.toast", "args" to mapOf("message" to mapOf("path" to "/m"))))))))))
    fnSurface.setData("/m", "\"hi\"")
    fnSurface.setViewport(400f, 0f, null)
    fnSurface.layoutFixed(null, false)
    val fnEvents = fnSurface.event(fnSurface.indexOf("b")!!, "press", null)
    val fnCall = fnEvents.firstOrNull { it.kind == "functionCall" }
    check(fnCall != null) { "functionCall event ${fnEvents.map { it.kind }}" }
    if (fnCall != null) equal(fnCall.json, mapOf("kind" to "functionCall", "componentId" to "b", "name" to "harness.toast", "args" to mapOf("message" to "hi")), "functionCall json")
    println("host fixtures: $checks checks so far")

    println("kotlin binding suite: $checks checks, $failures failures")
    exitProcess(if (failures == 0) 0 else 1)
}
