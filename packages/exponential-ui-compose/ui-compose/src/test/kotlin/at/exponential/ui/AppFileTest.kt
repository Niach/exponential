package at.exponential.ui

import android.graphics.Bitmap
import at.exponential.ui.compose.LeafImages
import at.exponential.ui.compose.PlaybackSource
import at.exponential.ui.host.ClosureHost
import at.exponential.ui.host.ExponentialHost
import at.exponential.ui.host.HostOptions
import at.exponential.ui.host.HostPolicy
import at.exponential.ui.host.MediaLimits
import at.exponential.ui.host.MediaRequest
import at.exponential.ui.host.MemoryTransport
import at.exponential.ui.json.JsonValue
import at.exponential.ui.host.NoHost
import at.exponential.ui.host.appFileId
import at.exponential.ui.host.policedMediaRequest
import at.exponential.ui.model.SurfaceModel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import java.io.ByteArrayOutputStream

/**
 * VAPP-271 (catalog/host.json media.appFile): an `app-file:<fileId>` src
 * loads through the host's `appFile` hook (the Engine's runners: their
 * BlobStore) under the image limits, cached like any picture; a host
 * without one loads nothing, so the missing-image fallback paints.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class AppFileTest {
    private fun png(w: Int, h: Int): ByteArray =
        ByteArrayOutputStream().also { Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888).compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()

    /** fixtures/host-policy.json `appFiles`: appFileId(src) → the id | null. */
    @Test
    fun appFilesFixture() {
        Fixtures.require()
        val cases = Fixtures.json("host-policy.json")["appFiles"]?.array.orEmpty()
        assertTrue(cases.isNotEmpty())
        for (c in cases) assertEquals(c["name"]?.string, c["expected"]?.string, appFileId(c["src"]!!.string!!))
    }

    @Test
    fun aHostWithoutAppFilesLoadsNothing() = runBlocking {
        val m = SurfaceModel("none", host = NoHost, scope = CoroutineScope(Dispatchers.Unconfined), shaper = noShaper)
        assertNull("no hook: the fallback", m.mediaRequest("app-file:blob-1"))
        assertNull("the media policy alone never loads the form", policedMediaRequest(NoHost, "app-file:blob-1"))
        assertNull(LeafImages.load(MediaRequest("app-file:blob-none")))
    }

    @Test
    fun theHookAnswersTheBytesOnceAndThePictureIsCached() = runBlocking {
        LeafImages.clear()
        val bytes = png(3, 2)
        val asked = ArrayList<String>()
        val host = ClosureHost(appFile = { id -> synchronized(asked) { asked.add(id) }; if (id == "blob-1") bytes else null })
        val m = SurfaceModel("files", host = host, scope = CoroutineScope(Dispatchers.Unconfined), shaper = noShaper)
        val request = m.mediaRequest("app-file:blob-1")
        assertEquals(MediaRequest("app-file:blob-1"), request)
        assertNull("not an id: the media policy, which denies the scheme", m.mediaRequest("app-file:a/b"))

        val image = LeafImages.load(request!!, appFile = host.appFile)
        assertEquals(3, image?.width)
        assertNotNull(LeafImages.cached("app-file:blob-1"))
        assertNotNull(LeafImages.load(request, appFile = host.appFile))
        assertEquals(listOf("blob-1"), asked)
        // A missing file fails like a 404.
        assertNull(LeafImages.load(MediaRequest("app-file:gone"), appFile = host.appFile))
        // A player opens no app file.
        assertNull(PlaybackSource.of(request))
        // ExponentialHost forwards its policy's hook, else its plugin's.
        fun surfaceOf(options: HostOptions): SurfaceModel {
            val t = MemoryTransport()
            val h = ExponentialHost(options.copy(transport = t), CoroutineScope(Dispatchers.Unconfined))
            h.connect()
            t.feed(JsonValue.parse("""{"version":"v0.9","createSurface":{"surfaceId":"s1","catalogId":"https://ui.exponential.at/catalogs/core/v1"}}"""))
            return h.surface("s1")!!
        }
        assertNotNull(surfaceOf(HostOptions(policy = HostPolicy(appFile = { bytes }))).mediaRequest("app-file:blob-1"))
        assertNotNull(surfaceOf(HostOptions(plugin = host)).mediaRequest("app-file:blob-1"))
        assertNull(surfaceOf(HostOptions()).mediaRequest("app-file:blob-1"))
    }

    @Test
    fun anAppFileFitsTheImageLimits() = runBlocking {
        val bytes = png(4, 4)
        fun reason(block: suspend () -> Unit): LeafImages.Failure {
            try {
                runBlocking { block() }
            } catch (e: LeafImages.LoadFailure) {
                return e.reason
            }
            fail("expected a LoadFailure")
            error("unreachable")
        }
        assertEquals(LeafImages.Failure.TooLarge, reason { LeafImages.appFileBytes("big", { bytes }, MediaLimits(8, 30_000, 1_000_000)) })
        assertEquals(LeafImages.Failure.Http, reason { LeafImages.appFileBytes("gone", { null }) })
        assertEquals(LeafImages.Failure.Timeout, reason { LeafImages.appFileBytes("slow", { kotlinx.coroutines.delay(1_000); bytes }, MediaLimits(1 shl 20, 10, 1_000_000)) })
        assertEquals(LeafImages.Failure.TooManyPixels, reason { LeafImages.decode(LeafImages.appFileBytes("wide", { bytes }), MediaLimits(1 shl 20, 30_000, 4)) })
    }
}
