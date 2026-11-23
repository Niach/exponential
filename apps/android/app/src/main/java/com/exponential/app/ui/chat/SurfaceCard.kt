package com.exponential.app.ui.chat

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import at.exponential.ui.compose.ExponentialSurface
import at.exponential.ui.model.SurfaceModel
import com.exponential.app.domain.ChatExpress
import com.exponential.app.domain.Chats
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.domain.PlanCard
import com.exponential.app.domain.SessionTree
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.LocalReduceMotion
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.motionDurationMs
import com.exponential.app.ui.theme.motionEasing
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** The surface card's frame fill: THE card surface (N-12 ONE gray ×4, the
 *  bubbles' and cards' fill), opaque so the children's stagger can veil
 *  their own regions in exactly the colour beneath them. */
private val SurfaceFrameFill: Color = GlassTokens.OpaqueCardFill

/**
 * Session 58cb32df (chats.json SURFACE CARD ×4): an agent's inline
 * Exponential UI preview (card kind `surface`), painted by the same renderer
 * as the plan card's surface ([rememberCardSurfaceModel] over
 * `ExponentialSurface`) in its own rounded glass frame, as wide as the
 * agent's column. It answers nothing.
 * #EXP-2067: every press posts an action card ([onAction]).
 *
 * ENTER ([animate] = the card ARRIVED while the chat was open; history, a
 * reopen or a scroll never animates): the frame fades in from
 * `frame.fromOpacity`, rises `fromTranslateY` and scales from `fromScale`
 * over `frame.duration` / `frame.ease`; its hairline starts at the primary
 * colour at `glow.fromOpacity` and settles to the normal stroke; after
 * `children.startAfterMs` the root's first-level children come in one by one
 * (opacity + a `fromTranslateY` rise, `staggerMs` apart, the ones past
 * `maxStaggered` together; the renderer's `SurfaceModel.nodeModifier` hook). Reduce motion =
 * a plain opacity fade of the frame.
 */
@Composable
fun SurfaceCard(
    row: SessionTree.SessionChatRow.Surface,
    animate: Boolean,
    /** #EXP-2067 (chat-express.json ACTIONS): EVERY press of the surface as
     *  [ChatExpress.surfaceAction] over its show origin (`show:<id>`); a
     *  [ChatExpress.ActionResult.TooLarge] = THE failed-action toast. */
    onAction: (ChatExpress.ActionResult) -> Unit,
    modifier: Modifier = Modifier,
    /** false = no title caption (the host names it). */
    showTitle: Boolean = true,
    /** #EXP-1432: false = THE renderer with NO card chrome (no frame fill,
     *  hairline, glow or inset). */
    chrome: Boolean = true,
) {
    val model = rememberCardSurfaceModel(row.surface) { name, context, userMessage ->
        ChatExpress.surfaceAction(name, context, userMessage, ChatExpress.showActionOrigin(row.id))?.let(onAction)
    }
    SurfaceCardFrame(
        key = row.id,
        model = model,
        animate = animate,
        modifier = modifier,
        title = row.title?.trim()?.takeIf { showTitle && it.isNotEmpty() },
        chrome = chrome,
    )
}

/**
 * THE surface card frame + painter ([SurfaceCard]'s, and #EXP-2073 every
 * Express block surface's): [model] painted in the glass frame, the ENTER
 * when [animate]; a null [model] = the empty frame; [placeholder] (no
 * model) = THE skeleton inside the frame.
 */
@Composable
fun SurfaceCardFrame(
    key: String,
    model: SurfaceModel?,
    animate: Boolean,
    modifier: Modifier = Modifier,
    title: String? = null,
    chrome: Boolean = true,
    placeholder: Boolean = false,
    /** #EXP-2067: a block's stream adds children live: no stagger. */
    stagger: Boolean = true,
) {
    val reduce = LocalReduceMotion.current
    // Decided once: a recomposition never re-plays the entrance.
    val frame = remember(key) { Animatable(if (animate) 0f else 1f) }
    val glow = remember(key) { Animatable(if (animate && !reduce) 0f else 1f) }
    val children = remember(key) { List(Chats.SurfaceCard.Children.MAX_STAGGERED) { Animatable(if (animate && !reduce && stagger) 0f else 1f) } }
    LaunchedEffect(key) {
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
            if (child.value >= 1f) return@forEachIndexed
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
    // The title = the plan card's surface caption (plan-card.json `card`:
    // 2xs medium muted, `surfaceLabelGap` ABOVE the frame) ×4; it enters
    // with the frame.
    Column(
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
            .testTag(Chats.TestIds.SURFACE_CARD),
        verticalArrangement = Arrangement.spacedBy(PlanCard.Look.SURFACE_LABEL_GAP.dp),
    ) {
        if (title != null) {
            Text(
                title,
                style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Medium),
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Box(
            modifier = if (!chrome) Modifier.fillMaxWidth() else Modifier
                .fillMaxWidth()
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
                .padding(PlanCard.Look.SURFACE_PADDING.dp),
        ) {
            if (model != null) {
                // The children's stagger through the renderer's paint-only node
                // hook: each first-level child of the root fades in AND rises
                // `children.fromTranslateY` px, `staggerMs` apart (the ones past
                // `maxStaggered` together), read per frame inside the layer.
                DisposableEffect(model) {
                    model.nodeModifier = { index ->
                        val node = model.node(index)
                        val root = model.node(0)
                        val position = if (node?.parent == 0) root?.children?.indexOf(index) ?: -1 else -1
                        if (position < 0) {
                            null
                        } else {
                            val anim = children[minOf(position, children.lastIndex)]
                            Modifier.graphicsLayer {
                                val p = anim.value
                                val from = Chats.SurfaceCard.Children.FROM_OPACITY
                                alpha = from + (1f - from) * p
                                translationY = (1f - p) * Chats.SurfaceCard.Children.FROM_TRANSLATE_Y * density
                            }
                        }
                    }
                    onDispose { model.nodeModifier = null }
                }
                ExponentialSurface(model)
            } else if (placeholder) {
                // An open block with no surface yet: THE skeleton (feedback-states.json).
                SkeletonRows(rows = 2, rowHeight = 24.dp, dot = false)
            } else {
                // A surface the core refuses: the empty frame.
                Box(Modifier.size(1.dp).offset(0.dp, 0.dp))
            }
        }
    }
}
