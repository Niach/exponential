// #VAPP-263: the Kotlin binding suite (JNA on the JVM; Android links the same
// bindings). Runs an app through the facade with Kotlin providers (the
// lifecycle, writes, the consent round trip, the default SQLite store) and
// replays packages/eengine/fixtures/providers.json `validateCases`.
//   bash apps/desktop/crates/eengine-ffi/run-binding-tests.sh kotlin
package tests

import at.exponential.engine.ffi.*
import java.io.File
import java.nio.file.Files
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

class Keychain : Vault {
    val values = HashMap<String, String>()
    override fun `read`(`key`: String): String? = values[key]
    override fun `write`(`key`: String, `value`: String) { values[key] = value }
    override fun `delete`(`key`: String) { values.remove(key) }
}

class Nav : WindowHost {
    val opened = ArrayList<String>()
    override fun `present`(`app`: String, `screen`: String, `paramsJson`: String, `how`: Presentation) { opened.add(screen) }
}

class Photos : Capability {
    override fun `name`() = "photos"
    override fun `toolsJson`() = "[]"
    // A native result per capabilities.json's nativeOutputSchema.
    override fun `call`(`tool`: String, `argumentsJson`: String) = """{"structuredContent":{"photos":[]}}"""
}

class Pinned : EngineClock {
    override fun `now`() = "2026-11-06T10:00:00.000Z"
}

val PACKAGE = """
{"formatVersion":6,"id":"acme.album","name":"Album","version":"1.0.0","catalogId":"https://ui.exponential.at/catalogs/core/v1",
 "functions":["records.*","app_navigate","photos_pick"],"sources":["records:*"],
 "records":{"note":{"title":"Note","schema":{"type":"object","properties":{"text":{"type":"string"}}},"policy":{"update":"author","delete":"owners"}}},
 "capabilities":[{"id":"photos.read","why":"Pick the photos for an album."}],
 "screens":[{"template":"albums","title":"Albums"},{"template":"page","title":"Page","nav":false}],
 "templates":{"albums":{"components":[{"id":"root","component":"Text","text":"Albums"}]},"page":{"components":[{"id":"root","component":"Text","text":"Page"}]}}}
"""

fun host(dataDir: String?) = EngineHost("exported", "android", "phone", "en", null, NetworkPolicy.OFF, dataDir)

fun count(app: AppInstance): Int {
    val value = app.recordsValue("records:note") ?: return -1
    return ((Json.parse(value) as Map<*, *>)["count"] as Double).toInt()
}

fun main(args: Array<String>) {
    // An app runs over the default stores.
    val keychain = Keychain()
    val nav = Nav()
    val engine = Engine(host(null), EngineIdentity.Local("d-1"), EngineProviders(keychain, nav, capabilities = listOf(Photos()), clock = Pinned()))
    val app = engine.load(PACKAGE)
    check(app.appId() == "acme.album") { "appId" }
    check(!app.queues()) { "no sync provider: writes go to the store" }
    val effects = Json.parse(app.event("""{"type":"launch"}""")) as List<*>
    check(effects.any { (it as Map<*, *>)["effect"] == "flush" }) { "launch flushes: $effects" }
    check(nav.opened.lastOrNull() == "albums") { "launch opens the first screen: ${nav.opened}" }
    val created = app.call("records_upsert", """{"kind":"note","data":{"text":"hi"}}""", listOf("n1"))
    check(created == CallResult.Value("""{"id":"n1"}""")) { "upsert answers the id: $created" }
    check(count(app) == 1) { "the record is there" }
    app.call("app_navigate", """{"screen":"page"}""", listOf())
    check(nav.opened.lastOrNull() == "page") { "navigate opens the screen" }
    check(try { app.event("{"); false } catch (e: EngineException.Invalid) { true }) { "bad JSON throws" }

    // The consent round trip.
    val ask = app.call("photos_pick", "{}", listOf()) as CallResult.Ask
    check(ask.sheet.scope == "photos.read") { "the sheet's scope" }
    val title = ((Json.parse(ask.sheet.componentsJson) as List<*>)[0] as Map<*, *>)["title"]
    check(title == "Allow Album to access your photos?") { "the sheet's title: $title" }
    val picked = app.answerConsent("photos_pick", "{}", ask.verdictJson, ConsentAnswer.ALLOW)
    check(picked is CallResult.Value && picked.json.contains(""""photos":[]""")) { "the capability ran: $picked" }
    check(app.grantsJson() == """{"photos.read":"granted"}""") { "the grant persists: ${app.grantsJson()}" }
    check(keychain.values["eengine/grants/acme.album"]?.contains("granted") == true) { "in the Vault" }
    app.setGrant("photos.read", null)
    check(app.call("photos_pick", "{}", listOf()) is CallResult.Ask) { "revoked: asks again" }
    val refused = app.call("camera_capture", "{}", listOf())
    check(refused is CallResult.Refused && refused.message == "notDeclared") { "undeclared: $refused" }

    // A bad package says why.
    check(try { engine.load("""{"formatVersion":4}"""); false } catch (e: EngineException.Load) { e.kind == "invalid" }) { "format 4 loaded" }

    // The default SQLite store keeps records across Engines (feature `sqlite`).
    if ("sqlite" in engineFeatures()) {
        val dir = Files.createTempDirectory("eengine-kotlin").toString()
        for (round in 0..1) {
            val a = Engine(host(dir), EngineIdentity.Local("d-1"), EngineProviders(Keychain(), Nav())).load(PACKAGE)
            a.event("""{"type":"launch"}""")
            if (round == 0) a.call("records_upsert", """{"kind":"note","data":{}}""", listOf("n1"))
            check(count(a) == 1) { "round $round: the record survives" }
        }
        File(dir).deleteRecursively()
    }

    // providers.json validateCases.
    if (args.isNotEmpty()) {
        val fixture = Json.parse(File(args[0]).readText()) as Map<*, *>
        for (c in fixture["validateCases"] as List<*>) {
            c as Map<*, *>
            val got = Json.parse(validateHostConfig(Json.write(c["config"])))
            check(got == c["expected"]) { "${c["name"]}: $got" }
        }
    }

    println("${checks - failures}/$checks checks passed (eengine-ffi ${engineVersion()}, features ${engineFeatures()})")
    exitProcess(if (failures == 0) 0 else 1)
}
