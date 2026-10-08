package at.exponential.ui.kitchensink

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Picture
import android.os.Build
import android.util.Log
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithCache
import androidx.compose.ui.graphics.drawscope.draw
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import java.io.File
import java.io.FileOutputStream
import androidx.compose.ui.graphics.Canvas as ComposeCanvas

/**
 * The `dump` extra: records every frame of the modified content (the
 * whole surface, at its full height, not the viewport the scroller shows)
 * into a [Picture], and [write]s the latest one as a PNG.
 */
class SurfaceDump {
    private val picture = Picture()
    private var recorded = false

    /** Record the content while drawing it as usual. */
    val modifier: Modifier = Modifier.drawWithCache {
        val w = size.width.toInt().coerceAtLeast(1)
        val h = size.height.toInt().coerceAtLeast(1)
        onDrawWithContent {
            val canvas = ComposeCanvas(picture.beginRecording(w, h))
            draw(this, layoutDirection, canvas, size) { this@onDrawWithContent.drawContent() }
            picture.endRecording()
            recorded = true
            drawIntoCanvas { it.nativeCanvas.drawPicture(picture) }
        }
    }

    /** Write the last recorded frame to [path]; false when nothing was drawn yet. */
    fun write(path: String): Boolean {
        if (!recorded) return false
        val bitmap = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            Bitmap.createBitmap(picture).copy(Bitmap.Config.ARGB_8888, false)
        } else {
            Bitmap.createBitmap(picture.width, picture.height, Bitmap.Config.ARGB_8888).also { Canvas(it).drawPicture(picture) }
        }
        return runCatching {
            File(path).parentFile?.mkdirs()
            FileOutputStream(path).use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
            Log.i(LOG_TAG, "dump ${bitmap.width}x${bitmap.height} → $path")
            true
        }.onFailure { Log.w(LOG_TAG, "dump $path: ${it.message}") }.getOrDefault(false)
    }
}
