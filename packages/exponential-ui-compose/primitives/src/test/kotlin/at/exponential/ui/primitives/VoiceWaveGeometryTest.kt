package at.exponential.ui.primitives

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File

/**
 * VAPP-160: the pure-Kotlin VoiceWave rings ([VoiceWaveGeometry]) against
 * the SDK's contract: its constants are catalog/style.json `voiceWave`'s and
 * every `cases` entry of fixtures/voice-wave.json replays through it.
 */
class VoiceWaveGeometryTest {
    /** packages/exponential-ui (the test task runs in the module dir; skipped without the checkout). */
    private val sdk: File = File("../../exponential-ui")

    private fun json(path: String): Any? {
        val file = File(sdk, path)
        assumeTrue("$file not in this checkout", file.isFile)
        return MiniJson(file.readText()).parse()
    }

    @Suppress("UNCHECKED_CAST")
    private fun Any?.obj(): Map<String, Any?> = this as Map<String, Any?>

    @Suppress("UNCHECKED_CAST")
    private fun Any?.arr(): List<Any?> = this as List<Any?>

    private fun Any?.nums(): DoubleArray = arr().map { (it as Number).toDouble() }.toDoubleArray()

    @Test
    fun constantsAreTheCatalogs() {
        val v = json("catalog/style.json").obj()["voiceWave"].obj()
        assertEquals((v["steps"] as Number).toInt(), VoiceWaveStyle.STEPS)
        assertArrayEquals(v["radius"].nums(), VoiceWaveStyle.RADIUS, 0.0)
        val harmonics = v["harmonics"].arr()
        assertEquals(harmonics.size, VoiceWaveStyle.HARMONICS.size)
        harmonics.forEachIndexed { k, h -> assertArrayEquals(h.nums().map { it.toInt() }.toIntArray(), VoiceWaveStyle.HARMONICS[k]) }
        assertArrayEquals(v["phase0"].nums(), VoiceWaveStyle.PHASE0, 0.0)
        assertArrayEquals(v["ringAmp"].nums(), VoiceWaveStyle.RING_AMP, 0.0)
        assertEquals((v["ampBase"] as Number).toDouble(), VoiceWaveStyle.AMP_BASE, 0.0)
        assertEquals((v["ampLevel"] as Number).toDouble(), VoiceWaveStyle.AMP_LEVEL, 0.0)
        assertEquals((v["thinkingAmp"] as Number).toDouble(), VoiceWaveStyle.THINKING_AMP, 0.0)
        assertEquals((v["phaseSpeed"] as Number).toDouble(), VoiceWaveStyle.PHASE_SPEED, 0.0)
        assertArrayEquals(v["fillAlpha"].nums(), VoiceWaveStyle.FILL_ALPHA, 0.0)
        assertArrayEquals(v["strokeAlpha"].nums(), VoiceWaveStyle.STROKE_ALPHA, 0.0)
        assertArrayEquals(v["strokeWidth"].nums(), VoiceWaveStyle.STROKE_WIDTH, 0.0)
        assertEquals((v["childScale"] as Number).toDouble(), VoiceWaveStyle.CHILD_SCALE, 0.0)
        assertEquals((v["childScaleLarge"] as Number).toDouble(), VoiceWaveStyle.CHILD_SCALE_LARGE, 0.0)
        assertEquals((v["maxBands"] as Number).toInt(), VoiceWaveStyle.MAX_BANDS)
    }

    @Test
    fun replaysTheFixtureCases() {
        val cases = json("fixtures/voice-wave.json").obj()["cases"].arr()
        assertTrue(cases.isNotEmpty())
        val cache = HashMap<Double, VoiceWaveGeometry>()
        for (raw in cases) {
            val case = raw.obj()
            val name = case["name"] as String
            val input = case["input"].obj()
            val expected = case["expected"].obj()
            val size = (input["size"] as Number).toDouble()
            // One geometry per edge, reused across cases: what a painter does across frames.
            val g = cache.getOrPut(size) { VoiceWaveGeometry(size) }
            g.update(
                input["activity"] as String,
                (input["level"] as Number?)?.toDouble() ?: 0.0,
                (input["bands"] as List<*>?)?.map { (it as Number).toDouble() } ?: emptyList(),
                (input["elapsed"] as Number?)?.toDouble() ?: 0.0,
                input["reducedMotion"] as Boolean? ?: false,
            )
            assertEquals("$name phase", (expected["phase"] as Number).toDouble(), g.phase, 0.0)
            val rings = expected["rings"].arr()
            assertEquals("$name rings", rings.size, g.rings.size)
            rings.forEachIndexed { k, r ->
                val ring = r.obj()
                val got = g.rings[k]
                assertEquals("$name ring $k part", ring["part"], got.part)
                assertEquals("$name ring $k fillAlpha", (ring["fillAlpha"] as Number).toFloat(), got.fillAlpha, 0f)
                assertEquals("$name ring $k strokeAlpha", (ring["strokeAlpha"] as Number).toFloat(), got.strokeAlpha, 0f)
                assertEquals("$name ring $k strokeWidth", (ring["strokeWidth"] as Number).toFloat(), got.strokeWidth, 0f)
                val points = ring["points"].arr()
                assertEquals("$name ring $k points", points.size * 2, got.points.size)
                points.forEachIndexed { i, p ->
                    val xy = p.nums()
                    assertEquals("$name ring $k point $i x", xy[0], got.points[2 * i].toDouble(), 0.011)
                    assertEquals("$name ring $k point $i y", xy[1], got.points[2 * i + 1].toDouble(), 0.011)
                }
            }
        }
    }

    @Test
    fun roundsHalvesUpLikeJs() {
        assertEquals(0.01, VoiceWaveGeometry.round2(0.005), 1e-12)
        assertEquals(-0.0, VoiceWaveGeometry.round2(-0.005), 1e-12)
        assertEquals(-0.01, VoiceWaveGeometry.round2(-0.006), 1e-12)
        assertEquals(0.0, VoiceWaveGeometry.clamp01(Double.NaN), 0.0)
        assertEquals(0.0, VoiceWaveGeometry.clamp01(Double.POSITIVE_INFINITY), 0.0)
        assertEquals(1.0, VoiceWaveGeometry.clamp01(1.4), 0.0)
    }
}

/** A minimal JSON reader for the tests (objects, arrays, strings, numbers, booleans, null): the module has no JSON library and android.jar's org.json is stubbed. */
private class MiniJson(private val s: String) {
    private var i = 0

    fun parse(): Any? = value().also { ws(); check(i == s.length) { "trailing input at $i" } }

    private fun ws() {
        while (i < s.length && s[i].isWhitespace()) i++
    }

    private fun value(): Any? {
        ws()
        return when (val c = s[i]) {
            '{' -> objectValue()
            '[' -> arrayValue()
            '"' -> string()
            't' -> { i += 4; true }
            'f' -> { i += 5; false }
            'n' -> { i += 4; null }
            else -> {
                check(c == '-' || c.isDigit()) { "unexpected '$c' at $i" }
                val start = i
                while (i < s.length && (s[i].isDigit() || s[i] in "+-.eE")) i++
                s.substring(start, i).toDouble()
            }
        }
    }

    private fun objectValue(): Map<String, Any?> {
        i++
        val m = LinkedHashMap<String, Any?>()
        ws()
        if (s[i] == '}') { i++; return m }
        do {
            ws()
            val k = string()
            ws()
            check(s[i++] == ':')
            m[k] = value()
            ws()
        } while (s[i++] == ',')
        return m
    }

    private fun arrayValue(): List<Any?> {
        i++
        val a = ArrayList<Any?>()
        ws()
        if (s[i] == ']') { i++; return a }
        do {
            a.add(value())
            ws()
        } while (s[i++] == ',')
        return a
    }

    private fun string(): String {
        check(s[i++] == '"')
        val b = StringBuilder()
        while (true) {
            val c = s[i++]
            when (c) {
                '"' -> return b.toString()
                '\\' -> {
                    when (val e = s[i++]) {
                        'n' -> b.append('\n')
                        't' -> b.append('\t')
                        'r' -> b.append('\r')
                        'b' -> b.append('\b')
                        'f' -> b.append('\u000c')
                        'u' -> { b.append(s.substring(i, i + 4).toInt(16).toChar()); i += 4 }
                        else -> b.append(e)
                    }
                }
                else -> b.append(c)
            }
        }
    }
}
