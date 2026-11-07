package at.exponential.ui

import androidx.compose.ui.geometry.Size
import androidx.compose.runtime.Composable
import at.exponential.ui.extension.ExtensionContext
import at.exponential.ui.extension.ExtensionLeaf
import at.exponential.ui.extension.ExtensionPainter
import at.exponential.ui.host.ExponentialHost
import at.exponential.ui.host.FilePickRequest
import at.exponential.ui.host.FunctionCallInfo
import at.exponential.ui.host.FunctionDecision
import at.exponential.ui.host.FunctionOutcome
import at.exponential.ui.host.FunctionPolicy
import at.exponential.ui.host.HostExtension
import at.exponential.ui.host.HostOptions
import at.exponential.ui.host.HostPolicy
import at.exponential.ui.host.MediaOptions
import at.exponential.ui.host.MediaRule
import at.exponential.ui.host.MemoryTransport
import at.exponential.ui.host.SourceResolver
import at.exponential.ui.host.TransportStatus
import at.exponential.ui.host.UrlPolicy
import at.exponential.ui.host.Utf8Chunks
import at.exponential.ui.host.sourceWithParams
import at.exponential.ui.json.JsonValue
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.fire
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * VAPP-91: the Kotlin host runtime (`ExponentialHost`) over the core's
 * router, decoders and policy through the facade: router ops on surface
 * models, sources + cancel, the function gate (consent, package
 * narrowing), the URL policy, media requests, the in-memory transport, the
 * painter's events → client messages, catalog negotiation. Plain JVM; the
 * host runs on an unconfined scope so every hop is synchronous.
 */
class HostTest {
    private val core = "https://ui.exponential.at/catalogs/core/v1"

    private fun msg(json: String) = JsonValue.parse(json)

    private fun host(options: HostOptions = HostOptions()) = ExponentialHost(options, CoroutineScope(Dispatchers.Unconfined))

    private fun create(id: String = "s1", catalog: String = core) = msg("""{"version":"v0.9","createSurface":{"surfaceId":"$id","catalogId":"$catalog"}}""")

    private fun components(id: String, vararg c: String) = msg("""{"version":"v0.9","updateComponents":{"surfaceId":"$id","components":[${c.joinToString(",")}]}}""")

    /** Round 9 §6.5: a function the host lacks goes to the surface's agent and the press gets the answer (fixtures a2ui10-functions.json `sequences`). */
    @Test
    fun aFunctionTheHostLacksGoesToTheAgent() {
        val transport = MemoryTransport()
        val h = host(HostOptions(transport = transport, agent = true))
        h.connect()
        h.receive(msg("""{"version":"1.0","createSurface":{"surfaceId":"s1","catalogId":"$core","sendDataModel":true}}"""))
        h.surface("s1")!!.setData("/name", JsonValue.Str("Ada"))
        val call = FunctionCallInfo("s1", "b", "lookup", mapOf("q" to JsonValue.Str("hi")))
        var outcome: FunctionOutcome? = null
        h.scope.launch { outcome = h.callFunction(call) }
        // `sendDataModel`: the metadata travels beside the message.
        val wrapped = transport.sent.last()
        assertEquals(JsonValue.Str("Ada"), wrapped["metadata"]?.get("a2uiRendererDataModel")?.get("surfaces")?.get("s1")?.get("name"))
        val sent = wrapped["message"]!!["callAgentFunction"]!!
        assertEquals("s1:1", sent["functionCallId"]?.string)
        assertEquals("lookup", sent["callFunction"]?.get("@call")?.string)
        assertEquals(JsonValue.Str("hi"), sent["callFunction"]?.get("args")?.get("q"))
        assertNull(outcome)
        h.receive(msg("""{"version":"1.0","agentFunctionResponse":{"functionCallId":"s1:1","value":42}}"""))
        assertEquals(FunctionDecision.Allow, outcome?.decision)
        assertEquals(JsonValue.Num(42.0), outcome?.result)
        assertNull(outcome?.error)
        // A press never shares a call.
        outcome = null
        h.scope.launch { outcome = h.callFunction(call) }
        assertEquals("s1:2", transport.sent.last()["message"]?.get("callAgentFunction")?.get("functionCallId")?.string)
        h.receive(msg("""{"version":"1.0","agentFunctionResponse":{"functionCallId":"s1:2","error":{"code":"UNKNOWN_FUNCTION","message":"no"}}}"""))
        assertEquals("no", outcome?.error)
        // The failure names the call, no surface: nothing to sync beside it.
        assertEquals("EXPRESSION_ERROR", transport.sent.last()["error"]?.get("code")?.string)
    }

    /** Round 9 §6.5: a reactive position's call leaves after the layout, once, and its answer binds the surface again. */
    @Test
    fun aReactiveCallGoesToTheAgentAndItsAnswerRebinds() {
        val transport = MemoryTransport()
        val h = host(HostOptions(transport = transport, agent = true))
        h.connect()
        h.receive(msg("""{"version":"1.0","createSurface":{"surfaceId":"s1","catalogId":"$core"}}"""))
        h.receive(msg("""{"version":"1.0","updateComponents":{"surfaceId":"s1","components":[{"id":"root","component":"Text","text":{"@call":"lookup","args":{"q":"hi"}}}]}}"""))
        val m = laidOut(h.surface("s1")!!)
        assertEquals(transport.sent.toString(), "s1:1", transport.sent.single()["callAgentFunction"]?.get("functionCallId")?.string)
        h.receive(msg("""{"version":"1.0","agentFunctionResponse":{"functionCallId":"s1:1","value":"Ada"}}"""))
        assertEquals(JsonValue.Str("Ada"), m.nodes.first().props["text"])
        assertEquals(1, transport.sent.size)
    }

    private fun laidOut(m: SurfaceModel): SurfaceModel {
        m.fixedMeasure = true
        m.setViewport(390f, 800f)
        return m
    }

    private fun errorCode(m: JsonValue): String? = m["error"]?.get("code")?.string

    @Test
    fun routerOpsDriveSurfaceModels() {
        val t = MemoryTransport()
        val ops = ArrayList<String>()
        val h = host(HostOptions(transport = t, onOp = { ops.add(it["op"]?.string ?: "") }))
        // Queued before connect, delivered on start.
        t.feed(create())
        assertEquals(TransportStatus.Closed, h.status)
        h.connect()
        assertEquals(TransportStatus.Open, h.status)
        assertTrue(h.hasTransport)
        val m = h.surface("s1")
        assertNotNull(m)
        t.feed(components("s1", """{"id":"root","component":"Stack","children":["a","b"]}""", """{"id":"a","component":"Text","text":"A"}""", """{"id":"b","component":"Text","text":{"path":"/b"}}"""))
        // updateComponents MERGES by id (A2UI): only `a` changes.
        t.feed(components("s1", """{"id":"a","component":"Text","text":"A2"}"""))
        laidOut(m!!)
        assertEquals(m.issues.toString() + " " + m.surface.dataJson(), listOf("root", "a", "b"), m.nodes.map { it.id })
        assertEquals("A2", m.node(m.indexOf("a")!!)!!.props["text"]?.string)
        val epoch = m.surface.writeEpoch()
        t.feed(msg("""{"version":"v0.9","updateDataModel":{"surfaceId":"s1","path":"/b","value":"B"}}"""))
        assertEquals("B", m.data["b"]?.string)
        // #VAPP-288: host data is not a programmatic write.
        assertEquals(epoch, m.surface.writeEpoch())
        // No `value` = remove the path; path "/" = the whole model.
        t.feed(msg("""{"version":"v0.9","updateDataModel":{"surfaceId":"s1","path":"/b"}}"""))
        assertNull(m.data["b"])
        t.feed(msg("""{"version":"v0.9","updateDataModel":{"surfaceId":"s1","path":"/","value":{"x":1}}}"""))
        assertEquals("""{"x":1}""", m.data.json)
        // Round 9 §1.4: a live id is not created twice.
        t.feed(create())
        assertTrue(h.surface("s1") === m)
        assertEquals("INTEGRITY_ERROR", errorCode(t.sent.last()))
        t.feed(msg("""{"version":"v0.9","deleteSurface":{"surfaceId":"s1"}}"""))
        assertNull(h.surface("s1"))
        assertEquals(emptyList<String>(), h.surfaceIds())
        assertEquals(listOf("create", "components", "components", "data", "data", "data", "send", "delete"), ops)
        // An error op goes out on the transport.
        t.feed(components("ghost", """{"id":"root","component":"Text","text":"x"}"""))
        assertEquals("SURFACE_NOT_FOUND", errorCode(t.sent.last()))
        h.close()
        assertEquals(TransportStatus.Closed, h.status)
    }

    @Test
    fun memoryTransportRoundTripsJsonl() {
        val t = MemoryTransport()
        val sent = ArrayList<JsonValue>()
        val h = host(HostOptions(transport = t, onSend = { sent.add(it) }))
        h.connect()
        t.feedJsonl(
            """
            {"version":"v0.9","createSurface":{"surfaceId":"g","catalogId":"$core"}}
            not json
            {"version":"v0.9","updateComponents":{"surfaceId":"g","components":[{"id":"root","component":"Text","text":{"path":"/t"}}]}}

            {"version":"v0.9","updateDataModel":{"surfaceId":"g","path":"/t","value":"hello"}}
            """.trimIndent(),
        )
        val m = laidOut(h.surface("g")!!)
        assertEquals("hello", m.data["t"]?.string)
        assertEquals(1, m.nodes.size)
        // Client messages land in `sent` (+ onSend).
        h.send(msg("""{"version":"v0.9","error":{"code":"VALIDATION_FAILED","surfaceId":"g","message":"x"}}"""))
        assertEquals(1, t.sent.size)
        assertEquals(t.sent, sent)
    }

    @Test
    fun utf8ChunksKeepSplitSequences() {
        val bytes = "21 °C ✓".toByteArray()
        val d = Utf8Chunks()
        val out = StringBuilder()
        for (b in bytes) out.append(d.decode(byteArrayOf(b), 1))
        assertEquals("21 °C ✓", out.toString())
    }

    /** VAPP-240: a surface's components read back raw (a `{path}` binding), and a file request names its surface. */
    @Test
    fun aSurfaceReadsItsComponentsRaw() {
        val t = MemoryTransport()
        val h = host(HostOptions(transport = t))
        h.connect()
        t.feed(create())
        t.feed(msg("""{"version":"v0.9","updateComponents":{"surfaceId":"s1","components":[{"id":"root","component":"FileUpload","files":{"path":"/draft/files"}}]}}"""))
        val m = h.surface("s1")!!
        assertEquals("/draft/files", m.components.first()["files"]?.get("@path")?.string)
        assertEquals("s1", FilePickRequest(m, "root", null, false).surfaceId)
    }

    @Test
    fun sourcesBindAndCancel() {
        val t = MemoryTransport()
        var emit: ((JsonValue?) -> Unit)? = null
        var cancelled = 0
        var seen: String? = null
        val h = host(
            HostOptions(
                transport = t,
                sources = mapOf(
                    "EXP" to SourceResolver { source, e ->
                        seen = "${source.scheme}:${source.name} ${source.params}"
                        emit = e
                        e(JsonValue.Arr(listOf(JsonValue.Str("d1"))))
                        ({ cancelled += 1 })
                    },
                ),
            ),
        )
        h.connect()
        t.feed(create())
        h.receive(msg("""{"version":"v0.9","bindDataModel":{"surfaceId":"s1","path":"/devices","source":"exp:devices?team=t1"}}"""), trusted = true)
        assertEquals("exp:devices {team=t1}", seen)
        val m = h.surface("s1")!!
        assertEquals("""["d1"]""", m.data["devices"]?.json)
        emit!!(JsonValue.Arr(listOf(JsonValue.Str("d1"), JsonValue.Str("d2"))))
        assertEquals("""["d1","d2"]""", m.data["devices"]?.json)
        // No resolver for a scheme → VALIDATION_FAILED back to the server.
        h.receive(msg("""{"version":"v0.9","bindDataModel":{"surfaceId":"s1","path":"/x","source":"acme:things"}}"""), trusted = true)
        val err = t.sent.last()["error"]!!
        assertEquals("VALIDATION_FAILED", err["code"]?.string)
        assertEquals("no resolver for the source scheme acme", err["message"]?.string)
        assertEquals("/x", err["path"]?.string)
        // Deleting the surface cancels its subscriptions.
        t.feed(msg("""{"version":"v0.9","deleteSurface":{"surfaceId":"s1"}}"""))
        assertEquals(1, cancelled)
    }

    @Test
    fun bindActionRefollowsAPathAndCancelsTheEarlierSubscription() = runBlocking {
        val t = MemoryTransport()
        val subscribed = ArrayList<String>()
        val cancelled = ArrayList<String>()
        val h = host(
            HostOptions(
                transport = t,
                sources = SourceResolver { source, e ->
                    val uri = source.uri
                    subscribed.add(uri)
                    e(JsonValue.Str(uri))
                    ({ cancelled.add(uri) })
                }.let { resolver -> mapOf("exp" to resolver, "records" to resolver) },
            ),
        )
        h.connect()
        t.feed(create())
        h.receive(msg("""{"version":"v0.9","bindDataModel":{"surfaceId":"s1","path":"/records","source":"records:entry"}}"""), trusted = true)
        h.receive(msg("""{"version":"v0.9","bindDataModel":{"surfaceId":"s1","path":"/other","source":"exp:devices"}}"""), trusted = true)
        val args = mapOf(
            "path" to JsonValue.Str("/records"),
            "source" to JsonValue.Str("records:entry"),
            "params" to JsonValue.Obj(mapOf("month" to JsonValue.Str("2026-10"))),
        )
        val outcome = h.callFunction(FunctionCallInfo("s1", "c", "bind", args))
        assertEquals(FunctionDecision.Allow, outcome.decision)
        assertEquals(listOf("records:entry", "exp:devices", "records:entry?month=2026-10"), subscribed)
        // Only the re-bound path's earlier subscription was cancelled.
        assertEquals(listOf("records:entry"), cancelled)
        assertEquals("records:entry?month=2026-10", h.surface("s1")!!.data["records"]?.string)
        // No source, or no such surface = Deny.
        assertEquals(FunctionDecision.Deny, h.callFunction(FunctionCallInfo("s1", "c", "bind", mapOf("path" to JsonValue.Str("/x")))).decision)
        assertEquals(FunctionDecision.Deny, h.callFunction(FunctionCallInfo("nope", "c", "bind", args)).decision)
        // Deleting the surface cancels the remaining two.
        t.feed(msg("""{"version":"v0.9","deleteSurface":{"surfaceId":"s1"}}"""))
        assertEquals(3, cancelled.size)
    }

    @Test
    fun sourceWithParamsMatchesTheFixture() {
        // packages/exponential-ui/fixtures/host-policy.json `sourceParams`.
        assertEquals(
            "exp:x?q=a%20b%26c%3Dd%2F%C3%BC",
            sourceWithParams("exp:x", JsonValue.Obj(mapOf("q" to JsonValue.Str("a b&c=d/ü"), "skip" to JsonValue.Null, "gone" to JsonValue.Null))),
        )
        assertEquals(
            "records:entry?limit=50&mine=true",
            sourceWithParams("records:entry", JsonValue.Obj(mapOf("mine" to JsonValue.Bool(true), "limit" to JsonValue.Num(50.0)))),
        )
        assertEquals("exp:x", sourceWithParams("exp:x", null))
    }

    @Test
    fun functionGateConsentAndPackages() = runBlocking {
        val t = MemoryTransport()
        val ran = ArrayList<String>()
        var consent = true
        val asked = ArrayList<String>()
        val pkg = """{"formatVersion":5,"id":"acme.devices","name":"Devices","version":"1.0.0","catalogId":"$core","templates":{"list":{"components":[{"id":"root","component":"Text","text":"hi"}]}},"functions":["harness_toast"]}"""
        val h = host(
            HostOptions(
                transport = t,
                functions = mapOf(
                    "harness_toast" to { args, _ -> ran.add("toast ${args["text"]?.string}"); "ok" },
                    "harness_wipe" to { _, _ -> ran.add("wipe") },
                    "harness_ask" to { _, _ -> ran.add("ask") },
                    "harness_boom" to { _, _ -> error("boom") },
                ),
                packages = listOf(pkg),
                policy = HostPolicy(
                    functions = FunctionPolicy(deny = listOf("harness_wipe"), ask = listOf("harness_ask")),
                    onFunctionCall = { asked.add(it.name); consent },
                ),
            ),
        )
        h.connect()
        t.feed(create("free"))
        fun call(name: String, surface: String = "free", args: Map<String, JsonValue> = emptyMap()) = FunctionCallInfo(surface, "c", name, args)

        val ok = h.callFunction(call("harness_toast", args = mapOf("text" to JsonValue.Str("hi"))))
        assertEquals(FunctionDecision.Allow, ok.decision)
        assertEquals("ok", ok.result)
        assertEquals(FunctionDecision.NotFound, h.callFunction(call("harness_nope")).decision)
        assertEquals("UNKNOWN_FUNCTION", errorCode(t.sent.last()))
        assertEquals("no function harness_nope", t.sent.last()["error"]?.get("message")?.string)
        assertEquals(FunctionDecision.Deny, h.callFunction(call("harness_wipe")).decision)
        assertEquals("INVALID_FUNCTION_CALL", errorCode(t.sent.last()))
        assertEquals("harness_wipe was not allowed", t.sent.last()["error"]?.get("message")?.string)
        assertEquals(FunctionDecision.Allow, h.callFunction(call("harness_ask")).decision)
        consent = false
        assertEquals(FunctionDecision.Deny, h.callFunction(call("harness_ask")).decision)
        assertEquals(listOf("harness_ask", "harness_ask"), asked)
        assertEquals("boom", h.callFunction(call("harness_boom")).error)
        // A built-in value function is allowed and has no effect as an action.
        assertEquals(FunctionDecision.Allow, h.decide("free", "formatDate"))

        // A package surface: its `functions` list narrows the host's decision.
        h.receive(msg("""{"version":"v0.9","applyTemplate":{"surfaceId":"pkg","templateId":"list","packageId":"acme.devices"}}"""), trusted = true)
        assertNotNull(h.surface("pkg"))
        assertEquals("acme.devices", h.packageIdOf("pkg"))
        assertEquals(FunctionDecision.Allow, h.decide("pkg", "harness_toast"))
        assertEquals(FunctionDecision.Deny, h.decide("pkg", "harness_boom"))
        assertEquals(FunctionDecision.Allow, h.decide("free", "harness_boom"))
        assertEquals(listOf("toast hi", "ask"), ran)
        // A broken package does not install.
        assertTrue(h.installPackage("""{"id":"broken","name":"","version":"1","catalogId":"x","templates":{}}""").isNotEmpty())
    }

    /** Round 9 §1.4: a re-open mints, applies, and only then deletes the old surface. */
    @Test
    fun aReopenedSurfacePaintsUnderAMintedId() {
        val t = MemoryTransport()
        val pkg = """{"formatVersion":6,"id":"acme.screens","name":"Screens","version":"1.0.0","catalogId":"$core","templates":{"one":{"components":[{"id":"root","component":"Text","text":"one"}]},"two":{"components":[{"id":"root","component":"Text","text":"two"}]}}}"""
        val h = host(HostOptions(transport = t, packages = listOf(pkg)))
        h.connect()
        fun apply(id: String, template: String) = h.receive(msg("""{"version":"1.0","applyTemplate":{"surfaceId":"$id","templateId":"$template","packageId":"acme.screens"}}"""), trusted = true)
        fun delete(id: String) = h.receive(msg("""{"version":"1.0","deleteSurface":{"surfaceId":"$id"}}"""))

        val first = h.mintSurfaceId("app")
        apply(first, "one")
        assertNotNull(h.surface(first))
        val second = h.mintSurfaceId("app")
        assertTrue(first != second)
        apply(second, "two")
        assertNotNull("the old surface paints until the new one exists", h.surface(first))
        delete(first)
        assertNull(h.surface(first))
        assertNotNull(h.surface(second))
        val third = h.mintSurfaceId("app")
        apply(third, "one")
        delete(second)
        assertNotNull(h.surface(third))
        assertEquals(emptyList<String>(), t.sent.mapNotNull(::errorCode))

        // The fixed id the hosts used to re-apply is refused for good.
        apply(first, "two")
        assertEquals(listOf("INTEGRITY_ERROR"), t.sent.mapNotNull(::errorCode))
        assertNull(h.surface(first))
    }

    @Test
    fun urlPolicyAndMediaRequests() {
        val opened = ArrayList<String>()
        val h = host(
            HostOptions(
                policy = HostPolicy(
                    urls = UrlPolicy(hosts = listOf("*.example.com")),
                    openUrl = { opened.add(it) },
                    media = MediaOptions(baseUrl = "https://app.example.com/", rules = listOf(MediaRule("https://app.example.com/api/attachments/", mapOf("authorization" to "Bearer t")))),
                ),
            ),
        )
        assertTrue(h.openUrl("https://docs.example.com/a"))
        assertFalse(h.openUrl("https://evil.test/"))
        assertFalse(h.openUrl("javascript:alert(1)"))
        // Relative urls resolve against the media base.
        assertTrue(h.openUrl("/help"))
        assertEquals(listOf("https://docs.example.com/a", "https://app.example.com/help"), opened)
        val r = h.mediaRequest("/api/attachments/42")!!
        assertEquals("https://app.example.com/api/attachments/42", r.url)
        assertEquals(mapOf("authorization" to "Bearer t"), r.headers)
        assertEquals(emptyMap<String, String>(), h.mediaRequest("https://cdn.example.com/x.png")!!.headers)
        // openUrl through callFunction takes the same gate.
        runBlocking {
            assertEquals(FunctionDecision.Deny, h.callFunction(FunctionCallInfo("s", "c", "openUrl", mapOf("url" to JsonValue.Str("https://evil.test")))).decision)
        }
        // Without a policy the default schemes apply and relative urls are denied.
        val plain = host(HostOptions(policy = HostPolicy(openUrl = { opened.add(it) })))
        assertTrue(plain.openUrl("mailto:a@b.c"))
        assertFalse(plain.openUrl("/relative"))
    }

    @Test
    fun painterEventsBecomeClientMessages() {
        val t = MemoryTransport()
        val calls = ArrayList<String>()
        val opened = ArrayList<String>()
        val h = host(
            HostOptions(
                transport = t,
                functions = mapOf("harness_toast" to { args, call -> calls.add("${call.componentId} ${args["text"]?.string}") }),
                policy = HostPolicy(openUrl = { opened.add(it) }),
            ),
        )
        h.connect()
        t.feed(create())
        t.feed(
            components(
                "s1",
                """{"id":"root","component":"Stack","children":["go","fn","docs"]}""",
                """{"id":"go","component":"Button","label":"Go","on":{"press":{"event":{"name":"refresh","context":{"note":{"path":"/note"}}}}}}""",
                """{"id":"fn","component":"Button","label":"Toast","on":{"press":{"functionCall":{"call":"harness_toast","args":{"text":"hi"}}}}}""",
                """{"id":"docs","component":"Button","label":"Docs","on":{"press":{"functionCall":{"call":"openUrl","args":{"url":"https://ui.exponential.at"}}}}}""",
            ),
        )
        t.feed(msg("""{"version":"v0.9","updateDataModel":{"surfaceId":"s1","path":"/note","value":"n1"}}"""))
        val m = laidOut(h.surface("s1")!!)
        m.fire(m.indexOf("go")!!, "press")
        val action = t.sent.last()["action"]!!
        assertEquals("refresh", action["name"]?.string)
        assertEquals("s1", action["surfaceId"]?.string)
        assertEquals("go", action["sourceComponentId"]?.string)
        assertEquals("""{"note":"n1"}""", action["context"]?.json)
        assertNotNull(action["timestamp"]?.string)
        m.fire(m.indexOf("fn")!!, "press")
        assertEquals(listOf("fn hi"), calls)
        m.fire(m.indexOf("docs")!!, "press")
        assertEquals(listOf("https://ui.exponential.at/"), opened)
        assertEquals(1, t.sent.size)
    }

    private object NoPaint : ExtensionPainter {
        override fun measure(leaf: ExtensionLeaf, wrap: Float?): Size = Size(100f, 48f)

        @Composable
        override fun Paint(context: ExtensionContext) {}
    }

    @Test
    fun catalogNegotiationAndUnsupportedCatalog() {
        val ext = """{"id":"https://acme.example/catalog/v1","name":"Acme","extends":"$core","components":{"Spark":{"kind":"native","group":"data","children":"none","description":"x","props":{"values":{"type":"array","items":{"type":"number"},"description":"v"}},"example":{"values":[1,2]}}}}"""
        val t = MemoryTransport()
        val h = host(HostOptions(transport = t, extensions = listOf(HostExtension(ext, mapOf("Spark" to NoPaint)))))
        h.connect()
        assertEquals("https://acme.example/catalog/v1", h.supportedCatalogIds.last())
        assertEquals(h.supportedCatalogIds, h.rendererCapabilities()["1.0"]?.get("supportedCatalogIds")?.array?.map { it.string })
        // A surface on the extension catalog paints its native.
        t.feed(create("x", "https://acme.example/catalog/v1"))
        t.feed(components("x", """{"id":"root","component":"Spark","values":[1,2,3]}"""))
        val m = laidOut(h.surface("x")!!)
        assertEquals("Extension", m.nodes.single().component)
        assertNull(h.unsupportedCatalog)
        t.feed(create("y", "https://other.example/catalog/v1"))
        assertEquals("https://other.example/catalog/v1", h.unsupportedCatalog)
        assertEquals("UNSUPPORTED_CATALOG", errorCode(t.sent.last()))
        assertNull(h.surface("y"))
    }
}
