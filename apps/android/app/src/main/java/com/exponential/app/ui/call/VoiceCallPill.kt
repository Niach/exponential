package com.exponential.app.ui.call

import android.content.Context
import android.os.SystemClock
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.VoiceCall
import com.exponential.app.ui.components.CallButton
import com.exponential.app.ui.components.CallButtonCompactSize
import com.exponential.app.ui.components.CallButtonTone
import com.exponential.app.ui.components.ExponentialMark
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.thread.AgentGlyph
import kotlinx.coroutines.delay
import kotlin.math.roundToInt

// EXP-1681 (voice-call.json, styleguide entry `call-pill`): THE floating
// call pill, the desktop's (`voice_overlay::call_pill`) and web's
// `VoiceCallPill` row: the drag handle, the agent's avatar small, the name
// over the timer (ringing: the status), VoiceWave sm, then mute and the red
// end at the compact size. A minimized call draws it ([VoiceCallPillOverlay]);
// the styleguide specimens draw the same composable.

/** The avatar disc (web `CallAvatar` sm, desktop `AVATAR_SM`). */
private val PILL_AVATAR = 28.dp

/** VoiceWave sm. */
private const val PILL_WAVE = 32f

/** One pill: [timer] = the ticking `callTimer` text while [VoiceCall.View.status] is null. */
@Composable
fun VoiceCallPill(
    agent: String,
    view: VoiceCall.View,
    timer: String?,
    level: Float,
    bands: List<Double>,
    onMute: () -> Unit,
    onEnd: () -> Unit,
    modifier: Modifier = Modifier,
    agentIcon: String? = null,
    agentColor: String? = null,
    width: Dp = VoiceCall.Pill.WIDTH.dp,
    still: Boolean = false,
) {
    Row(
        modifier = modifier
            .width(width)
            .height(VoiceCall.Pill.HEIGHT.dp)
            .shadow(12.dp, CircleShape)
            .background(GlassTokens.OpaqueCardFill, CircleShape)
            .border(GlassTokens.Hairline, GlassTokens.StrokeCard, CircleShape)
            .padding(start = 12.dp, end = 10.dp)
            .testTag(VoiceCall.TestIds.PILL),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        DragHandle()
        PillAvatar(view, agent, agentIcon, agentColor)
        Column(Modifier.weight(1f)) {
            Text(
                agent,
                style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.Medium),
                color = DesignTokens.Palette.Foreground,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            val status = view.status
            Text(
                status ?: timer.orEmpty(),
                style = MaterialTheme.typography.labelMedium,
                color = DesignTokens.Palette.MutedForeground,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = if (status == null) Modifier.testTag(VoiceCall.TestIds.TIMER) else Modifier,
            )
        }
        val quiet = view.activity == VoiceCall.Activity.Ringing || view.activity == VoiceCall.Activity.Muted
        Box(Modifier.alpha(if (quiet) 0.35f else 1f)) {
            VoiceCallWave(activity = view.activity, level = level, bands = bands, still = still, size = PILL_WAVE)
        }
        // Mute and end only: the speaker stays on the full screen.
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            for (button in view.buttons) {
                when (button) {
                    VoiceCall.Button.Mute -> CallButton(
                        icon = if (view.muted) ExpIcons.callUnmute else ExpIcons.callMute,
                        label = if (view.muted) VoiceCall.Copy.UNMUTE else VoiceCall.Copy.MUTE,
                        onClick = onMute,
                        tone = if (view.muted) CallButtonTone.Toggled else CallButtonTone.Neutral,
                        size = CallButtonCompactSize,
                        modifier = Modifier.testTag(VoiceCall.TestIds.MUTE),
                    )
                    VoiceCall.Button.End -> CallButton(
                        icon = ExpIcons.callEnd,
                        label = view.end,
                        onClick = onEnd,
                        tone = CallButtonTone.End,
                        size = CallButtonCompactSize,
                        modifier = Modifier.testTag(VoiceCall.TestIds.END),
                    )
                    VoiceCall.Button.Speaker -> Unit
                }
            }
        }
    }
}

/** Two columns of three dots: a texture, not a glyph. */
@Composable
private fun DragHandle() {
    val dot = DesignTokens.Palette.MutedForeground.copy(alpha = 0.7f)
    Canvas(Modifier.padding(horizontal = 2.dp).size(width = 8.dp, height = 13.dp)) {
        val r = 1.5.dp.toPx()
        val step = 5.dp.toPx()
        for (column in 0..1) for (row in 0..2) {
            drawCircle(dot, radius = r, center = Offset(r + column * step, r + row * step))
        }
    }
}

@Composable
private fun PillAvatar(view: VoiceCall.View, agent: String, icon: String?, color: String?) {
    Box(
        modifier = Modifier
            .size(PILL_AVATAR)
            .alpha(if (view.activity == VoiceCall.Activity.Thinking) 0.6f else 1f)
            .background(DesignTokens.Palette.Secondary, CircleShape)
            .then(
                if (view.activity == VoiceCall.Activity.Listening) {
                    Modifier.border(2.dp, DesignTokens.Semantic.Green.copy(alpha = 0.8f), CircleShape)
                } else {
                    Modifier
                },
            )
            .semantics {
                contentDescription = agent
                stateDescription = view.activity.label
            },
        contentAlignment = Alignment.Center,
    ) {
        if (icon == null) ExponentialMark(size = 16.dp) else AgentGlyph(icon = icon, color = color, size = 16.dp)
    }
}

/**
 * A minimized call's pill over every screen (the root overlay, under the
 * toasts): dragged by any point outside its buttons, kept `pill.marginPx`
 * inside the safe area, its centre remembered on the device as a fraction
 * of the safe area; a tap opens the call screen again.
 */
@Composable
fun VoiceCallPillOverlay(
    agent: String,
    agentIcon: String?,
    agentColor: String?,
    view: VoiceCall.View,
    answeredAt: Long?,
    level: Float,
    onMute: () -> Unit,
    onEnd: () -> Unit,
    onExpand: () -> Unit,
) {
    val prefs = LocalContext.current.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    val now by produceState(SystemClock.elapsedRealtime(), answeredAt) {
        while (answeredAt != null) {
            value = SystemClock.elapsedRealtime()
            delay(1_000L - (value - answeredAt) % 1_000L)
        }
    }
    BoxWithConstraints(Modifier.fillMaxSize().safeDrawingPadding()) {
        val density = LocalDensity.current
        val margin = with(density) { VoiceCall.Pill.MARGIN_PX.dp.toPx() }
        val areaW = constraints.maxWidth.toFloat()
        val areaH = constraints.maxHeight.toFloat()
        val widthDp = minOf(VoiceCall.Pill.WIDTH.dp, maxWidth - (VoiceCall.Pill.MARGIN_PX * 2).dp)
        val w = with(density) { widthDp.toPx() }
        val h = with(density) { VoiceCall.Pill.HEIGHT.dp.toPx() }
        fun clamp(p: Offset) = Offset(
            p.x.coerceIn(margin + w / 2, maxOf(margin + w / 2, areaW - margin - w / 2)),
            p.y.coerceIn(margin + h / 2, maxOf(margin + h / 2, areaH - margin - h / 2)),
        )
        val saved = Offset(prefs.getFloat(KEY_X, -1f), prefs.getFloat(KEY_Y, -1f))
        val start = if (saved.x >= 0f && saved.y >= 0f) {
            Offset(saved.x * areaW, saved.y * areaH)
        } else {
            Offset(areaW / 2, areaH - with(density) { VoiceCall.Pill.PHONE_BOTTOM_PX.dp.toPx() } - h / 2)
        }
        var center by remember { mutableStateOf(clamp(start)) }
        VoiceCallPill(
            agent = agent,
            agentIcon = agentIcon,
            agentColor = agentColor,
            view = view,
            timer = answeredAt?.let { VoiceCall.callTimer(now - it) },
            level = level,
            bands = emptyList(),
            onMute = onMute,
            onEnd = onEnd,
            width = widthDp,
            modifier = Modifier
                .offset { IntOffset((center.x - w / 2).roundToInt(), (center.y - h / 2).roundToInt()) }
                .pointerInput(areaW, areaH) {
                    detectDragGestures(
                        onDragEnd = {
                            prefs.edit().putFloat(KEY_X, center.x / areaW).putFloat(KEY_Y, center.y / areaH).apply()
                        },
                    ) { change, delta ->
                        change.consume()
                        center = clamp(center + delta)
                    }
                }
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = onExpand),
        )
    }
}

private const val PREFS = "voice_call_pill"
private const val KEY_X = "x"
private const val KEY_Y = "y"
