package at.exponential.ui.primitives

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * The drawn switch's palette, per state. A host that keeps a platform
 * switch (an M3 `Switch`) can map these onto its colours so both read
 * the same; [DrawnSwitch] paints them directly.
 */
@Immutable
data class DrawnSwitchColors(
    /** Track while on. */
    val trackOn: Color,
    /** Track while off. */
    val trackOff: Color,
    /** Thumb while off. */
    val thumb: Color,
    /** Thumb while on. */
    val thumbOn: Color,
    /** Track while on and disabled. */
    val trackOnDisabled: Color = trackOn.copy(alpha = trackOn.alpha * 0.5f),
    /** Track while off and disabled. */
    val trackOffDisabled: Color = trackOff.copy(alpha = trackOff.alpha * 0.5f),
    /** Thumb while off and disabled. */
    val thumbDisabled: Color = thumb.copy(alpha = thumb.alpha * 0.5f),
    /** Thumb while on and disabled. */
    val thumbOnDisabled: Color = thumbOn.copy(alpha = thumbOn.alpha * 0.5f),
    /** Track hairline while off (null = none). */
    val trackStroke: Color? = null,
    /** Track hairline while on (null = none). */
    val trackStrokeOn: Color? = null,
) {
    /** The track colour in a state. */
    fun track(checked: Boolean, enabled: Boolean): Color = when {
        checked && enabled -> trackOn
        checked -> trackOnDisabled
        enabled -> trackOff
        else -> trackOffDisabled
    }

    /** The thumb colour in a state. */
    fun thumb(checked: Boolean, enabled: Boolean): Color = when {
        checked && enabled -> thumbOn
        checked -> thumbOnDisabled
        enabled -> thumb
        else -> thumbDisabled
    }

    /** Factories over the tokens. */
    companion object {
        /** On = `primary` track with a `primaryForeground` thumb; off = `input` track, white thumb. */
        fun from(tokens: PrimitiveTokens): DrawnSwitchColors = DrawnSwitchColors(
            trackOn = tokens.primary,
            trackOff = tokens.input,
            thumb = Color.White,
            thumbOn = tokens.primaryForeground,
        )
    }
}

/**
 * A drawn switch: a capsule track with a travelling thumb, animated
 * ([animationMillis], 0 = none). With [onCheckedChange] it is its own
 * `Role.Switch` toggle; null = a pure indicator inside a row that toggles.
 */
@Composable
fun DrawnSwitch(
    checked: Boolean,
    onCheckedChange: ((Boolean) -> Unit)?,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    colors: DrawnSwitchColors = DrawnSwitchColors.from(LocalPrimitiveTokens.current),
    trackWidth: Dp = 32.dp,
    trackHeight: Dp = 20.dp,
    thumbSize: Dp = 16.dp,
    trackStrokeWidth: Dp = LocalPrimitiveTokens.current.hairline,
    animationMillis: Int = 150,
) {
    val inset = ((trackHeight - thumbSize) / 2).coerceAtLeast(0.dp)
    val target = if (checked) trackWidth - thumbSize - inset else inset
    val spec = tween<Dp>(durationMillis = animationMillis)
    val x by animateDpAsState(target, spec, label = "thumb")
    val track by animateColorAsState(colors.track(checked, enabled), tween(animationMillis), label = "track")
    val thumb by animateColorAsState(colors.thumb(checked, enabled), tween(animationMillis), label = "thumbColor")
    val hairline = if (checked) colors.trackStrokeOn else colors.trackStroke
    val shape = RoundedCornerShape(percent = 50)
    Box(
        modifier = modifier
            .then(
                if (onCheckedChange != null) {
                    Modifier.toggleable(value = checked, enabled = enabled, role = Role.Switch, onValueChange = onCheckedChange)
                } else {
                    Modifier
                },
            )
            .size(trackWidth, trackHeight)
            .background(track, shape)
            .then(if (hairline != null) Modifier.border(trackStrokeWidth, hairline, shape) else Modifier),
        contentAlignment = Alignment.CenterStart,
    ) {
        Box(
            Modifier
                .offset(x = x)
                .size(thumbSize)
                .background(thumb, CircleShape),
        )
    }
}
