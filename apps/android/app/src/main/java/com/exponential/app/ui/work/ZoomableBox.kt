package com.exponential.app.ui.work

import androidx.compose.animation.core.animate
import androidx.compose.animation.core.tween
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.unit.IntSize
import kotlinx.coroutines.launch

// EXP-1149: pinch-to-zoom for the Results viewer. iOS previews a shot through
// Quick Look, which zooms natively; Android's dialog was a fixed Fit image.
// This box scales and pans its content around the box centre: pinch zooms
// about the fingers' centroid, a drag pans, a double tap toggles between fit
// and [DoubleTapScale] around the tapped point, and the picture never drifts
// off its own edges (a fitted picture cannot pan at all).

/** The zoom a double tap jumps to from the fitted picture. */
private const val DoubleTapScale = 2.5f
private const val MinScale = 1f
private const val MaxScale = 6f

/**
 * A box whose [content] pinch-zooms and pans. [contentAspect] (width ÷ height)
 * names the picture's fitted bounds inside the box so panning stops at ITS
 * edges rather than the box's letterbox; null treats the whole box as the
 * picture. [onTap] fires for a single tap on the FITTED picture only, so a
 * scrim-tap-to-close never fights a zoomed pan; zoomed, a single tap is inert
 * and a double tap returns to fit.
 */
@Composable
fun ZoomableBox(
    modifier: Modifier = Modifier,
    contentAspect: Float? = null,
    onTap: () -> Unit,
    content: @Composable BoxScope.() -> Unit,
) {
    var scale by remember { mutableFloatStateOf(MinScale) }
    var offset by remember { mutableStateOf(Offset.Zero) }
    var boxSize by remember { mutableStateOf(IntSize.Zero) }
    val scope = rememberCoroutineScope()

    // The picture's drawn size at 1×: FIT inside the box by its aspect.
    fun fittedSize(): Pair<Float, Float> {
        val w = boxSize.width.toFloat()
        val h = boxSize.height.toFloat()
        val aspect = contentAspect ?: return w to h
        if (w <= 0f || h <= 0f || aspect <= 0f) return w to h
        return if (w / h > aspect) (h * aspect) to h else w to (w / aspect)
    }

    // The translation range that keeps the scaled picture over the box: it
    // may slide by half its overflow on each axis, none when it fits.
    fun clamp(candidate: Offset, atScale: Float): Offset {
        val (fw, fh) = fittedSize()
        val maxX = maxOf(0f, (fw * atScale - boxSize.width) / 2f)
        val maxY = maxOf(0f, (fh * atScale - boxSize.height) / 2f)
        return Offset(candidate.x.coerceIn(-maxX, maxX), candidate.y.coerceIn(-maxY, maxY))
    }

    fun centre(): Offset = Offset(boxSize.width / 2f, boxSize.height / 2f)

    // With a centre origin a content point p draws at p·s + t, so holding the
    // screen point c fixed across s → s' means t' = c − (c − t)·s'/s.
    fun zoomAbout(anchor: Offset, from: Float, to: Float, fromOffset: Offset): Offset {
        val c = anchor - centre()
        return c - (c - fromOffset) * (to / from)
    }

    fun animateTo(targetScale: Float, targetOffset: Offset) {
        val startScale = scale
        val startOffset = offset
        scope.launch {
            animate(0f, 1f, animationSpec = tween(220)) { t, _ ->
                scale = startScale + (targetScale - startScale) * t
                offset = startOffset + (targetOffset - startOffset) * t
            }
        }
    }

    Box(
        modifier = modifier
            .clipToBounds()
            .onSizeChanged { boxSize = it }
            .pointerInput(contentAspect) {
                detectTapGestures(
                    onTap = { if (scale <= MinScale) onTap() },
                    onDoubleTap = { tap ->
                        if (scale > MinScale) {
                            animateTo(MinScale, Offset.Zero)
                        } else {
                            val target = zoomAbout(tap, scale, DoubleTapScale, offset)
                            animateTo(DoubleTapScale, clamp(target, DoubleTapScale))
                        }
                    },
                )
            }
            .pointerInput(contentAspect) {
                detectTransformGestures { centroid, pan, zoom, _ ->
                    val next = (scale * zoom).coerceIn(MinScale, MaxScale)
                    val anchored = zoomAbout(centroid, scale, next, offset)
                    offset = clamp(anchored + pan, next)
                    scale = next
                }
            },
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .graphicsLayer {
                    scaleX = scale
                    scaleY = scale
                    translationX = offset.x
                    translationY = offset.y
                },
            content = content,
        )
    }
}
