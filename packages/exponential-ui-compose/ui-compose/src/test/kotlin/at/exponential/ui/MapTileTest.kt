package at.exponential.ui

import at.exponential.ui.host.HostPlugin
import at.exponential.ui.primitives.ImageSource
import at.exponential.ui.model.SurfaceModel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** genUI round 4: fixtures/map-tile.json against [SurfaceModel.imageSource]. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [34])
class MapTileTest {
    @Test
    fun everyCaseReplays() {
        Fixtures.require()
        for (c in Fixtures.json("map-tile.json")["cases"]?.array.orEmpty()) {
            val src = c["src"]!!.string!!
            val resolved = c["resolveUrl"]?.string
            val host = object : HostPlugin {
                override fun resolveUrl(src: String): String = if (resolved != null && src == c["src"]!!.string) resolved else src
            }
            val m = SurfaceModel("map", host = host, scope = CoroutineScope(Dispatchers.Unconfined), shaper = noShaper)
            val got = when (val v = m.imageSource(src, "Map")) {
                ImageSource.Load -> "load"
                is ImageSource.Text -> if (v.text.isEmpty()) "nothing" else "other"
                else -> "other"
            }
            assertEquals(c["name"]!!.string, c["paints"]!!.string, got)
        }
    }
}
