package at.exponential.ui

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import at.exponential.ui.compose.MotionList
import at.exponential.ui.compose.MotionPainter
import at.exponential.ui.compose.MotionSource
import at.exponential.ui.compose.motionMayAnimate
import at.exponential.ui.ffi.MotionPlayer
import at.exponential.ui.json.JsonValue
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

/**
 * EXP-1604: Motion over the FFI. The shared clock replays
 * `fixtures/motion-playback.json`; the painter paints every case of
 * `fixtures/motion-frames.json` at 160×160 (Robolectric's native Skia) and
 * matches the raster's `conformance/motion/parity/<id>@<frame>.png`: ≥ 99 %
 * of pixels within 2/255 per channel or inside the reference's 3×3 range
 * ([compare]); the strict share is printed per case.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class MotionTest {
    @Test
    fun playbackFixtureReplays() {
        Fixtures.require()
        val f = Fixtures.json("motion-playback.json")
        val lottie = f["lottie"]!!.json
        for (c in f["cases"]!!.array!!) {
            val id = c["id"]!!.string!!
            val player = MotionPlayer(lottie, "")
            player.setPlayback(c["props"]!!.json)
            assertEquals("$id poster", c["poster"]!!.number!!, player.posterFrame(), 1e-6)
            assertEquals("$id progress", c["progressHalf"]!!.number!!, player.progressFrame(0.5), 1e-6)
            for (t in c["ticks"]!!.array!!) {
                val ms = t["elapsedMs"]!!.number!!
                val got = player.tick(ms)
                assertEquals("$id @$ms", t["frame"]!!.number!!, got.frame, 1e-6)
                assertEquals("$id done @$ms", t["done"]!!.bool!!, got.done)
            }
        }
    }

    @Test
    fun packedFramesDecodeLikeTheirLists() {
        Fixtures.require()
        for (c in Fixtures.json("motion-frames.json")["cases"]!!.array!!) {
            val player = MotionPlayer(c["lottie"]!!.json, "")
            for (f in c["frames"]!!.array!!) {
                val packed = f["packed"]!!.array!!.map { it.number!!.toFloat() }
                val list = MotionList.decode(packed)
                assertNotNull(c["id"]!!.string, list)
                assertEquals(c["id"]!!.string, f["list"]!!["ops"]!!.array!!.size, list!!.ops.size)
                assertEquals(c["id"]!!.string, packed, player.sample(f["frame"]!!.number!!))
            }
        }
    }

    @Test
    fun pixelParityWithTheRaster() {
        Fixtures.require()
        val parity = File(Fixtures.dir.parentFile, "conformance/motion/parity")
        val out = System.getenv("EXPONENTIAL_UI_MOTION_OUT")?.let(::File)?.also { it.mkdirs() }
        val report = ArrayList<String>()
        val failed = ArrayList<String>()
        for (c in Fixtures.json("motion-frames.json")["cases"]!!.array!!) {
            val id = c["id"]!!.string!!
            val player = MotionPlayer(c["lottie"]!!.json, "")
            val images = player.images().map { MotionPainter.decodeDataUrl(it) }
            for (f in c["frames"]!!.array!!) {
                val frame = f["frame"]!!.number!!
                val name = "$id@${frame.toInt()}"
                val list = MotionList.decode(player.sample(frame))!!
                val got = Bitmap.createBitmap(160, 160, Bitmap.Config.ARGB_8888)
                MotionPainter.paint(Canvas(got), list, 160f, 160f, "contain", images)
                val want = BitmapFactory.decodeFile(File(parity, "$name.png").path)
                assertNotNull(name, want)
                val (strict, worst, edgeAware) = compare(got, want)
                report += "$name: ${"%.2f".format(strict * 100)} % within 2/255 (worst $worst), ${"%.2f".format(edgeAware * 100)} % edge-aware"
                if (edgeAware < 0.99) failed += name
                out?.let { File(it, "$name.png").outputStream().use { s -> got.compress(Bitmap.CompressFormat.PNG, 100, s) } }
            }
        }
        println("Motion parity (Compose vs raster):\n" + report.joinToString("\n"))
        assertTrue("Motion parity under 99 %: $failed\n" + report.joinToString("\n"), failed.isEmpty())
    }

    @Test
    fun themeSlotsAndModeReachTheFile() {
        val lottie = """{"slots":{"${'$'}color.primary":{"p":{"a":0,"k":[0.5,0.5,0.5,1]}},"ld:#ffffff:#000000":{}}}"""
        val v = JsonValue.parse(MotionSource.slots(lottie, mapOf("slots" to JsonValue.Obj(mapOf("x" to JsonValue.Num(1.0)))), { if (it == "primary") "#ff0000" else null }, dark = true))
        assertEquals(listOf(1.0, 0.0, 0.0, 1.0), v["\$color.primary"]!!["p"]!!["k"]!!.array!!.map { it.number })
        assertEquals("dark", v["\$mode"]!!.string)
        assertEquals(1.0, v["x"]!!.number!!, 0.0)
        assertNull(v["ld:#ffffff:#000000"])
    }

    @Test
    fun inlineLayersCompileThroughTheCore() {
        val props = JsonValue.parse("""{"size":[48,48],"duration":1000,"layers":[{"type":"shape","shapes":[{"type":"ellipse","size":[36,36]},{"type":"fill","color":"${'$'}color.primary"}]}]}""").obj!!
        val lottie = MotionSource.inlineLottie(props)!!
        assertNotNull(JsonValue.parse(lottie)["slots"]!!["\$color.primary"])
        assertEquals(listOf(48f, 48f), MotionPlayer(lottie, "").size())
    }

    @Test
    fun atMostEightAnimatePerSurface() {
        val m = makeModel()
        val kids = (0 until 10).joinToString(",") { """{"id":"m$it","component":"Motion","props":{"size":[10,10],"duration":100,"layers":[]}}""" }
        m.setNested("""{"id":"col","component":"Stack","children":[$kids]}""")
        val motions = m.nodes.filter { it.component == "Motion" }
        assertEquals(10, motions.size)
        assertEquals(List(8) { true } + listOf(false, false), motions.map { m.motionMayAnimate(it.index) })
    }

    /** The strict share (every channel within 2/255), the worst delta and the EDGE-AWARE share (also passing inside the reference's 3×3 range ± 2), premultiplied. */
    private fun compare(a: Bitmap, b: Bitmap): Triple<Double, Int, Double> {
        fun px(bm: Bitmap): IntArray = IntArray(160 * 160).also { bm.getPixels(it, 0, 160, 0, 0, 160, 160) }
        fun ch(p: Int, c: Int): Int {
            val alpha = p ushr 24
            if (c == 3) return alpha
            val v = (p shr (16 - 8 * c)) and 0xff
            return (v * alpha + 127) / 255
        }
        val pa = px(a)
        val pb = px(b)
        var ok = 0
        var near = 0
        var worst = 0
        for (y in 0 until 160) for (x in 0 until 160) {
            val i = y * 160 + x
            var d = 0
            for (c in 0 until 4) d = max(d, abs(ch(pa[i], c) - ch(pb[i], c)))
            worst = max(worst, d)
            if (d <= 2) { ok++; near++; continue }
            var inside = true
            for (c in 0 until 4) {
                var lo = 255
                var hi = 0
                for (dy in -1..1) for (dx in -1..1) {
                    val v = ch(pb[min(159, max(0, y + dy)) * 160 + min(159, max(0, x + dx))], c)
                    lo = min(lo, v)
                    hi = max(hi, v)
                }
                val v = ch(pa[i], c)
                if (v < lo - 2 || v > hi + 2) { inside = false; break }
            }
            if (inside) near++
        }
        val n = 160.0 * 160.0
        return Triple(ok / n, worst, near / n)
    }
}
