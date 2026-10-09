package at.exponential.ui

import android.graphics.Bitmap
import androidx.compose.runtime.Composable
import androidx.compose.ui.geometry.Size
import at.exponential.ui.compose.LeafImages
import at.exponential.ui.extension.ExtensionContext
import at.exponential.ui.extension.ExtensionLeaf
import at.exponential.ui.extension.ExtensionPainter
import at.exponential.ui.host.ExponentialHost
import at.exponential.ui.host.HostExtension
import at.exponential.ui.host.HostOptions
import at.exponential.ui.host.MediaLimits
import at.exponential.ui.host.MemoryTransport
import at.exponential.ui.host.RENDER_FAILED
import at.exponential.ui.json.JsonValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.ByteArrayOutputStream
import java.io.DataOutputStream
import java.util.zip.CRC32

/**
 * VAPP-103 (safe hosts) on Robolectric's native graphics: the image
 * loader reads width × height from the header and refuses a picture over
 * `media.limits.maxPixels` BEFORE decoding; an extension painter that
 * throws measures 0×0, paints an empty box and reaches the agent as ONE
 * RENDER_FAILED error.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class SafePaintTest {
    private fun chunk(out: ByteArrayOutputStream, type: String, data: ByteArray) {
        val body = type.toByteArray() + data
        val crc = CRC32().apply { update(body) }.value
        DataOutputStream(out).apply {
            writeInt(data.size)
            write(body)
            writeInt(crc.toInt())
        }
    }

    /** A PNG whose IHDR claims [w]×[h] (+ a token IDAT and IEND: far too little pixel data for that size). */
    private fun pngHeader(w: Int, h: Int): ByteArray {
        val out = ByteArrayOutputStream()
        out.write(byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A))
        val ihdr = ByteArrayOutputStream()
        DataOutputStream(ihdr).apply {
            writeInt(w)
            writeInt(h)
            writeByte(8) // bit depth
            writeByte(6) // RGBA
            writeByte(0)
            writeByte(0)
            writeByte(0)
        }
        chunk(out, "IHDR", ihdr.toByteArray())
        val z = java.util.zip.Deflater().run {
            setInput(ByteArray(64))
            finish()
            val buf = ByteArray(256)
            buf.copyOf(deflate(buf))
        }
        chunk(out, "IDAT", z)
        chunk(out, "IEND", ByteArray(0))
        return out.toByteArray()
    }

    @Test
    fun aHugeHeaderIsRefusedBeforeDecoding() {
        val bytes = pngHeader(40_000, 30_000)
        assertEquals(40_000 to 30_000, LeafImages.bounds(bytes))
        try {
            LeafImages.decode(bytes, MediaLimits.contract)
            fail("decoded a 1.2 gigapixel picture")
        } catch (e: LeafImages.LoadFailure) {
            assertEquals(LeafImages.Failure.TooManyPixels, e.reason)
        }
        // A real small picture decodes within the limits.
        val png = ByteArrayOutputStream().also { Bitmap.createBitmap(4, 3, Bitmap.Config.ARGB_8888).compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
        val img = LeafImages.decode(png, MediaLimits.contract)
        assertEquals(4, img.width)
        assertEquals(3, img.height)
    }

    private object Throws : ExtensionPainter {
        override fun measure(leaf: ExtensionLeaf, wrap: Float?): Size = error("no values")

        @Composable
        override fun Paint(context: ExtensionContext) {}
    }

    @Test
    fun aThrowingExtensionPainterIsOneRenderFailed() {
        val core = "https://ui.exponential.at/catalogs/core/v1"
        val ext = """{"id":"https://acme.example/catalog/v1","name":"Acme","extends":"$core","components":{"Spark":{"kind":"native","group":"data","children":"none","description":"x","props":{"values":{"type":"array","items":{"type":"number"},"description":"v"}},"example":{"values":[1,2]}}}}"""
        val t = MemoryTransport()
        val h = ExponentialHost(HostOptions(transport = t, extensions = listOf(HostExtension(ext, mapOf("Spark" to Throws)))), CoroutineScope(Dispatchers.Unconfined))
        h.connect()
        t.feed(JsonValue.parse("""{"version":"v0.9","createSurface":{"surfaceId":"x","catalogId":"https://acme.example/catalog/v1"}}"""))
        t.feed(JsonValue.parse("""{"version":"v0.9","updateComponents":{"surfaceId":"x","components":[{"id":"root","component":"Spark","values":[1,2,3]}]}}"""))
        val m = h.surface("x")!!
        var shaper: at.exponential.ui.measure.TextShaper? = null
        m.shaper = { shaper ?: robolectricShaper().also { shaper = it } }
        m.setViewport(390f, 600f)
        m.setViewport(300f, 600f)
        val leaf = m.nodes.single { it.component == "Extension" }
        assertEquals(0f, m.frame(leaf.index).height, 0f)
        assertTrue(m.paintFailures.containsKey("root"))
        val errors = t.sent.filter { it["error"] != null }
        assertEquals(1, errors.size)
        val err = errors.single()["error"]!!
        assertEquals(RENDER_FAILED, err["code"]?.string)
        assertEquals("/components/root", err["path"]?.string)
        assertTrue(err["message"]?.string ?: "", (err["message"]?.string ?: "").contains("no values"))
    }
}
