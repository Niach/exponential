package com.exponential.app.ui.components

import androidx.compose.foundation.LocalIndication
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PlainTooltip
import androidx.compose.material3.Text
import androidx.compose.material3.TooltipBox
import androidx.compose.material3.TooltipDefaults
import androidx.compose.material3.rememberTooltipState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.VoiceCall
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.pressScale

// S-3 (fixtures/voice-call.json PHONE-CALL UI, styleguide entry
// `call-controls`, eUI `CallButton`'s twin): the phone call's ROUND
// ICON-ONLY buttons. Web `Button size="icon-lg"`: a 56 circle with a 24
// glyph; neutral on the secondary fill, toggled on the inverted neutral
// fill, the end button red (`destructive`). The label is the accessibility
// label and the long-press tooltip, shown UNDER the button only while
// ringing (the end button's `Cancel`).

/** The circle at the phone call's size (web `size-14`). */
val CallButtonSize: Dp = 56.dp

/** The glyph inside it (web `size-6`). */
private val CallGlyphSize: Dp = 24.dp

/** The compact row's circle (the desktop pill's, `size-8`). */
val CallButtonCompactSize: Dp = 32.dp

enum class CallButtonTone { Neutral, Toggled, End }

/**
 * ONE round call button: [icon] by concept, [label] as its accessibility
 * label and tooltip ([showLabel] = the label under it, the ringing Cancel).
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CallButton(
    icon: ImageVector,
    label: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    tone: CallButtonTone = CallButtonTone.Neutral,
    showLabel: Boolean = false,
    size: Dp = CallButtonSize,
) {
    val (fill, glyph) = when (tone) {
        CallButtonTone.Neutral -> DesignTokens.Palette.Secondary to DesignTokens.Palette.Foreground
        CallButtonTone.Toggled -> DesignTokens.Palette.Primary to DesignTokens.Palette.PrimaryForeground
        CallButtonTone.End -> DesignTokens.Palette.Destructive to Color.White
    }
    val interactionSource = remember { MutableInteractionSource() }
    val pressed by interactionSource.collectIsPressedAsState()
    val circle: @Composable () -> Unit = {
        Box(
            modifier = Modifier
                .pressScale(pressed)
                .size(size)
                .clip(CircleShape)
                .background(fill, CircleShape)
                .clickable(
                    interactionSource = interactionSource,
                    indication = LocalIndication.current,
                    role = Role.Button,
                    onClick = onClick,
                ),
            contentAlignment = Alignment.Center,
        ) {
            Icon(icon, contentDescription = label, modifier = Modifier.size(if (size < CallButtonSize) 16.dp else CallGlyphSize), tint = glyph)
        }
    }
    Column(modifier = modifier, horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (showLabel) {
            circle()
            Text(label, style = MaterialTheme.typography.labelMedium, color = DesignTokens.Palette.MutedForeground)
        } else {
            TooltipBox(
                positionProvider = TooltipDefaults.rememberPlainTooltipPositionProvider(),
                tooltip = { PlainTooltip { Text(label) } },
                state = rememberTooltipState(),
            ) { circle() }
        }
    }
}

/**
 * THE call's buttons in [VoiceCall.View.buttons] order: mute (toggled =
 * unmute on the inverted fill), speaker (phones: the loudspeaker; off =
 * the earpiece on the inverted fill) and the red end, labelled
 * [VoiceCall.View.end] (under it while ringing).
 */
@Composable
fun CallControls(
    view: VoiceCall.View,
    onMute: () -> Unit,
    onSpeaker: () -> Unit,
    onEnd: () -> Unit,
    modifier: Modifier = Modifier,
    size: Dp = CallButtonSize,
) {
    val ringing = view.activity == VoiceCall.Activity.Ringing
    val c = VoiceCall.Copy
    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(if (size < CallButtonSize) 8.dp else 32.dp),
        verticalAlignment = Alignment.Top,
    ) {
        for (button in view.buttons) {
            when (button) {
                VoiceCall.Button.Mute -> CallButton(
                    icon = if (view.muted) ExpIcons.callUnmute else ExpIcons.callMute,
                    label = if (view.muted) c.UNMUTE else c.MUTE,
                    onClick = onMute,
                    tone = if (view.muted) CallButtonTone.Toggled else CallButtonTone.Neutral,
                    size = size,
                    modifier = Modifier.testTag(VoiceCall.TestIds.MUTE),
                )
                VoiceCall.Button.Speaker -> CallButton(
                    icon = if (view.speaker) ExpIcons.callSpeaker else ExpIcons.callSpeakerOff,
                    label = if (view.speaker) c.SPEAKER else c.SPEAKER_OFF,
                    onClick = onSpeaker,
                    tone = if (view.speaker) CallButtonTone.Neutral else CallButtonTone.Toggled,
                    size = size,
                    modifier = Modifier.testTag(VoiceCall.TestIds.SPEAKER),
                )
                VoiceCall.Button.End -> CallButton(
                    icon = ExpIcons.callEnd,
                    label = view.end,
                    onClick = onEnd,
                    tone = CallButtonTone.End,
                    showLabel = ringing,
                    size = size,
                    modifier = Modifier.testTag(VoiceCall.TestIds.END),
                )
            }
        }
    }
}
