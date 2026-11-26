package at.exponential.ui

import at.exponential.ui.json.JsonValue
import at.exponential.ui.primitives.parseHexColor
import at.exponential.ui.theme.PaintStyle
import at.exponential.ui.theme.pictureRect
import at.exponential.ui.theme.radialRadius
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * genUI round 4: `fixtures/box-backgrounds.json` on Compose: the radial
 * circle ([radialRadius], [at.exponential.ui.compose.gradientBrush]'s
 * radius), the picture placement ([pictureRect], the painter's) and the
 * FFI JSON → [PaintStyle] parse (`visual`). The TS reference, the Rust core
 * and Swift's `BoxBackgroundsTests` replay the same file.
 */
class BoxBackgroundsTest {
    private fun f(v: JsonValue?): Float = (v?.number ?: error("number")).toFloat()

    @Test
    fun radial() {
        Fixtures.require()
        val cases = Fixtures.json("box-backgrounds.json")["radial"]?.array.orEmpty()
        assertFalse(cases.isEmpty())
        for (c in cases) {
            val (w, h) = f(c["size"]?.array?.get(0)) to f(c["size"]?.array?.get(1))
            val (cx, cy) = f(c["centre"]?.array?.get(0)) to f(c["centre"]?.array?.get(1))
            val r = radialRadius(w, h, cx, cy)
            assertEquals(c["name"]?.string, f(c["radius"]), r, 1e-3f)
            for (s in c["samples"]?.array.orEmpty()) {
                val p = s.array.orEmpty()
                val t = if (r > 0f) kotlin.math.hypot(f(p[0]) - cx * w, f(p[1]) - cy * h) / r else 0f
                assertEquals("${c["name"]?.string} at ${p[0].number},${p[1].number}", f(p[2]), t, 1e-4f)
            }
        }
    }

    @Test
    fun picture() {
        Fixtures.require()
        for (c in Fixtures.json("box-backgrounds.json")["picture"]?.array.orEmpty()) {
            val size = c["size"]?.array.orEmpty()
            val image = c["image"]?.array.orEmpty()
            val focal = c["focal"]?.array.orEmpty()
            val got = pictureRect(c["fit"]?.string == "contain", f(size[0]), f(size[1]), f(image[0]), f(image[1]), f(focal[0]), f(focal[1]))
            val want = c["rect"]?.array.orEmpty()
            for (i in 0..3) assertEquals("${c["name"]?.string} [$i]", f(want[i]), got[i], 1e-4f)
        }
    }

    @Test
    fun visual() {
        Fixtures.require()
        for (c in Fixtures.json("box-backgrounds.json")["visual"]?.array.orEmpty()) {
            val name = c["name"]?.string
            val image = c["backgroundImage"]
            val picture = PaintStyle.parsePicture(image?.json)
            if (image == null) {
                assertNull(name, picture)
            } else {
                assertNotNull(name, picture)
                assertEquals(name, image["src"]?.string, picture!!.src)
                assertEquals(name, image["fit"]?.string == "contain", picture.contain)
                assertEquals(name, f(image["focalX"]), picture.focalX, 1e-6f)
                assertEquals(name, f(image["focalY"]), picture.focalY, 1e-6f)
                val scrim = image["scrim"]
                if (scrim == null) {
                    assertNull(name, picture.scrim)
                } else {
                    val want = parseHexColor(scrim["color"]?.string ?: "")!!
                    assertEquals(name, want.alpha * f(scrim["opacity"]), picture.scrim!!.alpha, 1e-3f)
                }
            }
            val gradient = c["backgroundGradient"]
            val paint = PaintStyle.parseGradient(gradient?.json)
            if (gradient == null) {
                assertNull(name, paint)
            } else {
                assertNotNull(name, paint)
                assertEquals(name, gradient["stops"]?.array?.size, paint!!.stops.size)
                val centre = gradient["centre"]?.array
                if (gradient["kind"]?.string == "radial") {
                    assertEquals(name, f(centre?.get(0)), paint.centre!!.first, 1e-6f)
                    assertEquals(name, f(centre?.get(1)), paint.centre!!.second, 1e-6f)
                } else {
                    assertNull(name, paint.centre)
                    assertEquals(name, f(gradient["angle"]), paint.angle, 1e-6f)
                }
            }
        }
    }
}
