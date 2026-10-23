package com.exponential.app.ui.thread

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import at.exponential.ui.compose.ExponentialSurface
import com.exponential.app.domain.Chats
import com.exponential.app.domain.PlanCard
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.motionDurationMs
import com.exponential.app.ui.theme.motionEasing
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** The surface card's frame fill: opaque, so the children's stagger can veil
 *  their own regions in exactly the colour beneath them. */
private val SurfaceFrameFill: Color = DesignTokens.Palette.Background

/**
 * Thread 58cb32df (chats.json SURFACE CARD ×4): an agent's inline
 * Exponential UI preview (card kind `surface`), painted by the same renderer
 * as the plan card's surface ([rememberCardSurfaceModel] over
 * `ExponentialSurface`) in its own rounded glass frame, as wide as the
 * agent's column. It answers nothing: a press whose event is
 * [Chats.SurfaceCard.REPLY_EVENT] with a non-empty `context.text` posts that
 * text as the person's next message ([Chats.surfaceReply] → [onReply]).
 *
 * ENTER ([animate] = the card ARRIVED while the chat was open; history, a
 * reopen or a scroll never animates): the frame fades in from
 * `frame.fromOpacity`, rises `fromTranslateY` and scales from `fromScale`
 * over `frame.duration` / `frame.ease`; its hairline starts at the primary
 * colour at `glow.fromOpacity` and settles to the normal stroke; after
 * `children.startAfterMs` the root's first-level children come in one by one
 * (`staggerMs` apart, the ones past `maxStaggered` together). Reduce motion =
 * a plain opacity fade of the frame.
 */
@Composable
fun SurfaceCard(
    row: SessionTree.ThreadRow.Surface,
    animate: Boolean,
    onReply: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val reduce = LocalReduceMotion.current
    val model = rememberCardSurfaceModel(row.surface) { name, context ->
        Chats.surfaceReply(name, context)?.let(onReply)
    }
    // Decided once: a recomposition never re-plays the entrance.
    val frame = remember(row.id) { Animatable(if (animate) 0f else 1f) }
    val glow = remember(row.id) { Animatable(if (animate && !reduce) 0f else 1f) }
    val children = remember(row.id) { List(Chats.SurfaceCard.Children.MAX_STAGGERED) { Animatable(if (animate && !reduce) 0f else 1f) } }
    LaunchedEffect(row.id) {
        if (frame.value >= 1f) return@LaunchedEffect
        launch {
            frame.animateTo(
                1f,
                tween(
                    motionDurationMs(Chats.SurfaceCard.Frame.DURATION),
                    easing = motionEasing(Chats.SurfaceCard.Frame.EASE),
                ),
            )
        }
        if (reduce) return@LaunchedEffect
        launch {
            glow.animateTo(
                1f,
                tween(
                    motionDurationMs(Chats.SurfaceCard.Glow.DURATION),
                    easing = motionEasing(Chats.SurfaceCard.Glow.EASE),
                ),
            )
        }
        children.forEachIndexed { index, child ->
            launch {
                delay(Chats.SurfaceCard.childDelayMs(index).toLong())
                child.animateTo(
                    1f,
                    tween(
                        motionDurationMs(Chats.SurfaceCard.Children.DURATION),
                        easing = motionEasing(Chats.SurfaceCard.Children.EASE),
                    ),
                )
            }
        }
    }
    val shape = RoundedCornerShape(PlanCard.Look.SURFACE_RADIUS.dp)
    val stroke = GlassTokens.StrokeCard
    val primary = MaterialTheme.colorScheme.primary
    Box(
        modifier = modifier
            .fillMaxWidth()
            .graphicsLayer {
                val p = frame.value
                val from = Chats.SurfaceCard.Frame.FROM_OPACITY
                alpha = from + (1f - from) * p
                if (!reduce) {
                    translationY = (1f - p) * Chats.SurfaceCard.Frame.FROM_TRANSLATE_Y * density
                    val scale = Chats.SurfaceCard.Frame.FROM_SCALE + (1f - Chats.SurfaceCard.Frame.FROM_SCALE) * p
                    scaleX = scale
                    scaleY = scale
                }
            }
            .clip(shape)
            .background(SurfaceFrameFill, shape)
            .drawWithContent {
                drawContent()
                // The glow: the hairline starts at the primary colour and
                // settles to the normal stroke.
                val g = glow.value
                if (g < 1f) {
                    val start = primary.copy(alpha = Chats.SurfaceCard.Glow.FROM_OPACITY)
                    drawRoundRect(
                        color = lerp(start, stroke, g),
                        cornerRadius = androidx.compose.ui.geometry.CornerRadius(PlanCard.Look.SURFACE_RADIUS.dp.toPx()),
                        style = androidx.compose.ui.graphics.drawscope.Stroke(width = 1.dp.toPx()),
                    )
                }
            }
            .border(GlassTokens.Hairline, stroke, shape)
            .padding(PlanCard.Look.SURFACE_PADDING.dp)
            .testTag(Chats.TestIds.SURFACE_CARD),
    ) {
        if (model != null) {
            Box(
                Modifier.drawWithContent {
                    drawContent()
                    // The children's stagger: each first-level child of the
                    // root is veiled in the frame's own fill until its turn.
                    val root = model.node(0) ?: return@drawWithContent
                    val origin = model.frame(0)
                    root.children.forEachIndexed { index, child ->
                        val p = children[minOf(index, children.lastIndex)].value
                        if (p >= 1f) return@forEachIndexed
                        val rect = model.frame(child)
                        val rise = (1f - p) * Chats.SurfaceCard.Children.FROM_TRANSLATE_Y.dp.toPx()
                        drawRect(
                            color = SurfaceFrameFill.copy(alpha = 1f - p),
                            topLeft = Offset(
                                (rect.left - origin.left).dp.toPx(),
                                (rect.top - origin.top).dp.toPx() - rise,
                            ),
                            size = Size(rect.width.dp.toPx(), rect.height.dp.toPx() + rise),
                        )
                    }
                },
            ) {
                ExponentialSurface(model)
            }
        } else {
            // A surface the core refuses: the empty frame.
            Box(Modifier.size(1.dp).offset(0.dp, 0.dp))
        }
    }
}
