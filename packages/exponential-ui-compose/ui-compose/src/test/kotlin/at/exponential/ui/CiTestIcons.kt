package at.exponential.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.unit.dp

/**
 * #EXP-2204: the test host's registry for the `ci-*` status concepts
 * (packages/icons/icons.json → Lucide 0.544.0 geometry: circle-check,
 * circle-x, loader-circle, circle-dashed, circle-slash), which are not
 * among the painter's built-in glyphs; every other name falls back to the
 * built-ins (null). What the Exponential app's own registry draws.
 */
object CiTestIcons {
    private const val CIRCLE = "M22 12a10 10 0 1 1-20 0a10 10 0 1 1 20 0z"
    private val paths = mapOf(
        "ci-passed" to listOf(CIRCLE, "M9 12l2 2 4-4"),
        "ci-failed" to listOf(CIRCLE, "M15 9l-6 6", "M9 9l6 6"),
        "ci-running" to listOf("M21 12a9 9 0 1 1-6.219-8.56"),
        "ci-queued" to listOf(
            "M10.1 2.182a10 10 0 0 1 3.8 0", "M13.9 21.818a10 10 0 0 1-3.8 0", "M17.609 3.721a10 10 0 0 1 2.69 2.7",
            "M2.182 13.9a10 10 0 0 1 0-3.8", "M20.279 17.609a10 10 0 0 1-2.7 2.69", "M21.818 10.1a10 10 0 0 1 0 3.8",
            "M3.721 6.391a10 10 0 0 1 2.7-2.69", "M6.391 20.279a10 10 0 0 1-2.69-2.7",
        ),
        "ci-skipped" to listOf(CIRCLE, "M9 15l6-6"),
    )
    private val vectors = HashMap<String, ImageVector>()

    private fun vector(name: String, d: List<String>): ImageVector = vectors.getOrPut(name) {
        val b = ImageVector.Builder(name, 24.dp, 24.dp, 24f, 24f)
        for (p in d) b.addPath(PathParser().parsePathString(p).toNodes(), stroke = SolidColor(Color.Black), strokeLineWidth = 2f, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round)
        b.build()
    }

    /** `HostPlugin.icon`: a `ci-*` glyph in the content colour, else null (the built-ins). */
    fun icon(name: String): (@Composable () -> Unit)? {
        val d = paths[name] ?: return null
        val v = vector(name, d)
        return { Image(rememberVectorPainter(v), null, Modifier.fillMaxSize(), colorFilter = ColorFilter.tint(LocalContentColor.current)) }
    }
}
