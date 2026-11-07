package at.exponential.runner

import at.exponential.engine.ffi.RunnerFormFactor
import at.exponential.engine.ffi.runnerPackage
import at.exponential.engine.ffi.runnerToast
import at.exponential.ui.json.JsonValue
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The runner's glue against the real Engine (eengine-ffi's host build through JNA) and a fake painter. */
class RunnerControllerTest {
    private fun obj(vararg pairs: Pair<String, Any?>) = JsonValue.of(mapOf(*pairs)) as JsonValue.Obj

    @Test
    fun failuresPaintTheContractsSurface() {
        val failures = Fixtures.runner()["failureCases"]!!.array!!.associate { it["kind"]!!.string!! to it["expected"]!! }
        for (kind in listOf("missing", "config")) {
            val h = Harness()
            assertFalse(h.controller.start(BundleLoad.Failed(kind)) { error("no engine for $kind") })
            assertEquals(failures[kind], h.painter.surfaces[RunnerSurfaces.APP]!!.first)
            assertEquals(kind, h.controller.failure)
        }
        val invalid = Harness()
        assertFalse(invalid.start("""{"formatVersion":4}"""))
        assertEquals(failures["invalid"], invalid.painter.surfaces[RunnerSurfaces.APP]!!.first)
        val forged = Harness()
        val file = """{"package":$ALBUM,"author":"ed25519:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=","signature":"AAAA"}"""
        assertFalse(forged.start(file))
        assertEquals(forged.controller.failure, "signature")
        assertEquals(failures["signature"], forged.painter.surfaces[RunnerSurfaces.APP]!!.first)
    }

    @Test
    fun aLaunchInstallsTheShellAndOpensTheFirstScreen() = runBlocking {
        val h = Harness()
        assertTrue(h.start(ALBUM))
        h.controller.setFormFactor(RunnerFormFactor.COMPACT)
        assertEquals(JsonValue.parse(runnerPackage(ALBUM, RunnerFormFactor.COMPACT)), h.painter.installed)
        h.controller.lifecycle("""{"type":"launch"}""")
        assertEquals(listOf("install", "open albums {}"), h.painter.ops)
        // Crossing `md`: the shell is derived again and the open screen re-applied.
        h.controller.setFormFactor(RunnerFormFactor.WIDE)
        h.controller.setFormFactor(RunnerFormFactor.WIDE)
        assertEquals(listOf("install", "open albums {}", "install", "open albums {}"), h.painter.ops)
        assertEquals(JsonValue.parse(runnerPackage(ALBUM, RunnerFormFactor.WIDE)), h.painter.installed)
        // runner.navigate = app.navigate through the Engine.
        h.controller.onEvent("runner.navigate", obj("screen" to "pages"))
        withTimeout(5000) { while (h.painter.ops.last() != "open pages {}") yield() }
        // genUI's app.navigate {screen, params}: the params reach the screen.
        h.controller.onEvent("runner.navigate", obj("screen" to "pages", "params" to mapOf("album" to "a1")))
        withTimeout(5000) { while (h.painter.ops.last() != "open pages {album=a1}") yield() }
        // The device uuid is created once, under runner.json's key.
        val device = h.vault.values[RunnerController.DEVICE_KEY]!!
        assertEquals(device, RunnerController.deviceId(h.vault) { "other" })
    }

    @Test
    fun aPackageWithoutScreensOpensTheDerivedOne() = runBlocking {
        val h = Harness()
        assertTrue(h.start(BARE))
        h.controller.setFormFactor(RunnerFormFactor.COMPACT)
        h.controller.lifecycle("""{"type":"launch"}""")
        assertEquals("open main {}", h.painter.ops.last())
    }

    @Test
    fun recordsWriteThroughTheInstanceAndBoundSourcesFollow() = runBlocking {
        val h = Harness()
        h.start(ALBUM)
        h.controller.lifecycle("""{"type":"launch"}""")
        val seen = ArrayList<JsonValue?>()
        h.controller.bindSource("records:note") { seen.add(it) }
        withTimeout(5000) { while (seen.isEmpty()) yield() }
        assertEquals(0.0, seen.last()!!["count"]!!.number)
        val created = h.controller.call("records_upsert", obj("kind" to "note", "data" to mapOf("text" to "hi")))
        assertEquals("id-1", created!!["id"]!!.string)
        assertEquals(1.0, seen.last()!!["count"]!!.number)
        val batch = h.controller.call("records_batch", obj("writes" to listOf(mapOf("op" to "upsert", "kind" to "note", "data" to mapOf("text" to "a")))))
        assertTrue("batch: $batch", batch != null)
        assertEquals(2.0, seen.last()!!["count"]!!.number)
    }

    @Test
    fun anAskPaintsTheConsentSheetAndItsAnswerRunsTheCall() = runBlocking {
        val h = Harness()
        h.start(ALBUM)
        h.controller.lifecycle("""{"type":"launch"}""")
        val pick = async { h.controller.call("photos_pick", obj()) }
        withTimeout(5000) { while (h.painter.surfaces[RunnerSurfaces.CONSENT] == null) yield() }
        val (sheet, data) = h.painter.surfaces[RunnerSurfaces.CONSENT]!!
        assertEquals("Drawer", sheet.array!![0]["component"]!!.string)
        assertEquals("allow", data!!["answer"]!!.string)
        assertEquals(0, h.photos.calls)
        assertTrue(h.controller.onEvent("consent.answer", obj("answer" to "allow")))
        val value = withTimeout(5000) { pick.await() }
        assertEquals(1, h.photos.calls)
        // VAPP-271: app UI reads the structuredContent.
        assertTrue("$value", value!!.json.contains("\"photos\"") && !value.json.contains("structuredContent"))
        assertNull("the sheet closes", h.painter.surfaces[RunnerSurfaces.CONSENT])
        assertEquals("""{"photos.read":"granted"}""", h.controller.app!!.grantsJson())
        // Granted: the next pick runs without the sheet.
        h.controller.call("photos_pick", obj())
        assertEquals(2, h.photos.calls)
    }

    @Test
    fun aDeniedAskAndARefusedCallToast() = runBlocking {
        val h = Harness()
        h.start(ALBUM)
        h.controller.lifecycle("""{"type":"launch"}""")
        val pick = async { h.controller.call("photos_pick", obj()) }
        withTimeout(5000) { while (h.painter.surfaces[RunnerSurfaces.CONSENT] == null) yield() }
        h.controller.onEvent("consent.answer", obj("answer" to "deny"))
        assertNull(withTimeout(5000) { pick.await() })
        assertEquals(0, h.photos.calls)
        assertEquals("""{"photos.read":"denied"}""", h.controller.app!!.grantsJson())
        assertTrue(h.painter.surfaces.containsKey(RunnerSurfaces.TOAST))
        assertNull(h.controller.call("camera_capture", obj()))
        assertEquals(JsonValue.parse(runnerToast("notDeclared")), h.painter.surfaces[RunnerSurfaces.TOAST]!!.first)
        val toast = Fixtures.runner()["toastCases"]!!.array!![0]
        assertEquals(toast["expected"], JsonValue.parse(runnerToast(toast["message"]!!.string!!)))
    }

    @Test
    fun permissionsPaintAndToggleTheGrant() = runBlocking {
        val h = Harness()
        h.start(ALBUM)
        h.controller.lifecycle("""{"type":"launch"}""")
        h.controller.onEvent("runner.permissions", obj())
        withTimeout(5000) { while (h.painter.surfaces[RunnerSurfaces.PERMISSIONS] == null) yield() }
        val switch = h.painter.surfaces[RunnerSurfaces.PERMISSIONS]!!.first.array!!.first { it["component"]?.string == "Switch" }
        assertEquals(false, switch["checked"]!!.bool)
        h.painter.surfaces.remove(RunnerSurfaces.PERMISSIONS)
        h.controller.onEvent("consent.toggle", obj("scope" to "photos.read"))
        withTimeout(5000) { while (h.painter.surfaces[RunnerSurfaces.PERMISSIONS] == null) yield() }
        assertEquals("""{"photos.read":"granted"}""", h.controller.app!!.grantsJson())
        val on = h.painter.surfaces[RunnerSurfaces.PERMISSIONS]!!.first.array!!.first { it["component"]?.string == "Switch" }
        assertEquals(true, on["checked"]!!.bool)
        h.controller.onEvent("consent.toggle", obj("scope" to "photos.read"))
        withTimeout(5000) { while (h.controller.app!!.grantsJson() != "{}") yield() }
        assertFalse(h.controller.onEvent("app.custom", obj()))
    }

    @Test
    fun lifecycleEventsMapToTheEngine() = runBlocking {
        val h = Harness()
        h.start(ALBUM)
        h.controller.setFormFactor(RunnerFormFactor.COMPACT)
        // A cold deep link the package does not expose: refused (toast), then the plain-launch screen.
        h.controller.lifecycle(RunnerLinks.event("at.acme.album://pages", "at.acme.album")!!)
        assertEquals(JsonValue.parse(runnerToast(RunnerController.LINK_REFUSED)), h.painter.surfaces[RunnerSurfaces.TOAST]!!.first)
        assertEquals(listOf("install", "open albums {}", "paint toast"), h.painter.ops)
        // A notification tap of the app itself opens any declared screen.
        h.controller.lifecycle("""{"type":"notificationTap","app":"acme.album","screen":"page","params":{"id":"p1"}}""")
        assertEquals("open page {id=p1}", h.painter.ops.last())
        h.controller.lifecycle("""{"type":"background"}""")
        assertTrue("persisted: ${h.controller.app!!.stateJson()}", h.controller.app!!.stateJson().contains("background"))
        h.controller.lifecycle("""{"type":"foreground"}""")
        h.controller.lifecycle("""{"type":"terminate"}""")
        assertTrue(h.controller.app!!.stateJson().contains("stopped"))
    }

    // VAPP-271: an `app-file:<fileId>` image reads the app's files: the store the Engine was handed.
    @Test
    fun appFilesReadTheEnginesStore() = runBlocking {
        val idle = Harness()
        assertNull("not running: nothing", idle.controller.appFile("3f2c9a1e-8b7d-4c1e-9f00-1a2b3c4d5e6f"))
        val h = Harness()
        assertTrue(h.start(ALBUM))
        val id = h.blobs.put("acme.album", "a.png", "image/png", byteArrayOf(1, 2, 3))
        assertTrue(h.controller.appFile(id)!!.contentEquals(byteArrayOf(1, 2, 3)))
        assertNull(h.controller.appFile(java.util.UUID.randomUUID().toString()))
        assertNull(h.controller.appFile("../a"))
    }

    @Test
    fun runnerBlobsKeepTheEnginesLayout() {
        val root = java.nio.file.Files.createTempDirectory("runner-blobs").toFile()
        try {
            val blobs = RunnerBlobs(java.io.File(root, "blobs"))
            val id = blobs.put("acme.album", "a.png", "image/png", byteArrayOf(4, 5))
            // FsBlobStore's layout: <dataDir>/blobs/<app>/<id> + <id>.json.
            val dir = java.io.File(root, "blobs/acme.album")
            assertTrue(java.io.File(dir, id).readBytes().contentEquals(byteArrayOf(4, 5)))
            assertEquals("image/png", JsonValue.parse(java.io.File(dir, "$id.json").readText())["mime"]?.string)
            assertTrue(blobs.get("acme.album", id)!!.contentEquals(byteArrayOf(4, 5)))
            assertNull("not an id", blobs.get("acme.album", "../x"))
            blobs.delete("acme.album", id)
            assertNull(blobs.get("acme.album", id))
            assertFalse(java.io.File(dir, "$id.json").exists())
        } finally {
            root.deleteRecursively()
        }
    }
}
