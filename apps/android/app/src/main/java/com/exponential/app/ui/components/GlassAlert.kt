package com.exponential.app.ui.components

import android.os.Build
import android.view.WindowManager
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens

/**
 * One answer of a [GlassAlert]. A trailing one is a [GlassPill] at
 * [PillSize.Md]: [primary] = the pill's primary paint (the ONE call to
 * action), [destructive] = the plain pill toned in the destructive colour
 * (never a solid red block). The leading answer is always quiet destructive
 * text, so both flags are ignored there.
 */
data class GlassAlertAction(
    val label: String,
    val onClick: () -> Unit,
    val primary: Boolean = false,
    val destructive: Boolean = false,
    val enabled: Boolean = true,
    val testTag: String? = null,
)

/**
 * The app's own centred alert (EXP-1212) — web `Dialog mobile="alert"` /
 * `AlertDialog` on a phone, never Material's `AlertDialog`: a dimmed, blurred
 * scrim and a centred glass card (24dp side margin, 16dp corners, the card
 * hairline, 20dp padding) with ONE question as its title (an optional muted
 * [body] under it) over ONE row of `md` [GlassPill]s (iOS `GlassPill .md`, web `Pill
 * size="md"`): [leading] set apart on the leading edge as quiet destructive
 * text lined up with the title, [trailing] packed
 * on the trailing edge in reading order (the last = the primary). When the
 * row does not fit the card (large font scales) the buttons stack full-width,
 * primary on top. [defaultAction] (an index into [trailing]) takes the
 * initial focus where there is one (a hardware keyboard: Enter answers it);
 * never a destructive one. No close ✕; system back and a scrim tap call
 * [onDismiss].
 */
@Composable
fun GlassAlert(
    title: String,
    trailing: List<GlassAlertAction>,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
    body: String? = null,
    leading: GlassAlertAction? = null,
    defaultAction: Int? = null,
) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false),
    ) {
        // The scrim is drawn here, so the window's own dim goes; the blur is
        // the platform's blur-behind where the device supports it (API 31+).
        val window = (LocalView.current.parent as? DialogWindowProvider)?.window
        SideEffect {
            window?.setDimAmount(0f)
            if (window != null && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                window.addFlags(WindowManager.LayoutParams.FLAG_BLUR_BEHIND)
                window.attributes = window.attributes.apply { blurBehindRadius = ScrimBlurPx }
            }
        }
        val noRipple = remember { MutableInteractionSource() }
        val focus = remember { FocusRequester() }
        LaunchedEffect(Unit) {
            // Touch mode has no focus to give; a keyboard does.
            if (defaultAction != null) runCatching { focus.requestFocus() }
        }
        Box(
            contentAlignment = Alignment.Center,
            modifier = Modifier
                .fillMaxSize()
                .background(ScrimColor)
                .clickable(interactionSource = noRipple, indication = null, onClick = onDismiss)
                .testTag("glass-alert-scrim"),
        ) {
            val shape = RoundedCornerShape(GlassTokens.CardRadius)
            Column(
                // The buttons lay out 48dp tall (their touch target) around a
                // 32dp paint: 8dp of gap + 8dp of target = 16dp to the title,
                // 12dp of padding + 8dp of target = 20dp to the bottom edge.
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = modifier
                    .padding(horizontal = 24.dp)
                    .widthIn(max = 384.dp)
                    .fillMaxWidth()
                    .clip(shape)
                    .background(DesignTokens.Glass.BackgroundBottom, shape)
                    .border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)
                    // Taps on the card are not scrim taps.
                    .clickable(interactionSource = noRipple, indication = null, onClick = {})
                    .padding(start = 20.dp, end = 20.dp, top = 20.dp, bottom = 12.dp)
                    .testTag("glass-alert"),
            ) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(
                        title,
                        style = MaterialTheme.typography.titleMedium.copy(
                            fontSize = 17.sp,
                            fontWeight = FontWeight.SemiBold,
                            letterSpacing = 0.sp,
                        ),
                        color = DesignTokens.Palette.Foreground,
                    )
                    if (!body.isNullOrBlank()) {
                        Text(
                            body,
                            style = MaterialTheme.typography.bodyMedium,
                            color = DesignTokens.Palette.MutedForeground,
                        )
                    }
                }
                AlertButtonRow(hasLeading = leading != null) {
                    leading?.let { AlertQuietButton(it) }
                    trailing.forEachIndexed { index, action ->
                        AlertButton(
                            action,
                            if (index == defaultAction) Modifier.focusRequester(focus) else Modifier,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun AlertButton(action: GlassAlertAction, modifier: Modifier = Modifier) {
    GlassPill(
        label = action.label,
        onClick = action.onClick,
        size = PillSize.Md,
        primary = action.primary,
        tint = if (action.destructive) DesignTokens.Palette.Destructive else null,
        enabled = action.enabled,
        // A 32dp capsule answering a 48dp touch target; the rest is layout.
        modifier = Modifier
            .minimumInteractiveComponentSize()
            .then(modifier)
            .then(if (action.testTag != null) Modifier.testTag(action.testTag) else Modifier),
    )
}

/** The leading answer: the destructive label alone, no fill, no hairline. */
@Composable
private fun AlertQuietButton(action: GlassAlertAction) {
    Box(
        contentAlignment = Alignment.Center,
        modifier = Modifier
            .minimumInteractiveComponentSize()
            .clickable(enabled = action.enabled, role = Role.Button, onClick = action.onClick)
            .then(if (action.testTag != null) Modifier.testTag(action.testTag) else Modifier),
    ) {
        Text(
            action.label,
            style = GlassPillDefaults.textStyle(PillSize.Md),
            color = DesignTokens.Palette.Destructive,
            maxLines = 1,
        )
    }
}

/**
 * ONE row: the leading text (when [hasLeading]) on the leading edge, in line
 * with the title, then
 * a flexible gap, then the rest packed trailing 8dp apart. When their natural
 * widths do not fit, every button goes full-width and stacks, the LAST
 * (primary) first and the leading one last.
 */
@Composable
private fun AlertButtonRow(hasLeading: Boolean, content: @Composable () -> Unit) {
    Layout(content = content, modifier = Modifier.fillMaxWidth()) { measurables, constraints ->
        val gap = 8.dp.roundToPx()
        val width = constraints.maxWidth
        val natural = measurables.map { it.maxIntrinsicWidth(constraints.maxHeight) }
        val trailingCount = measurables.size - if (hasLeading) 1 else 0
        val needed = natural.sum() + gap * (trailingCount - 1).coerceAtLeast(0) +
            (if (hasLeading) gap * 2 else 0)
        if (needed <= width) {
            val placeables = measurables.mapIndexed { i, m ->
                m.measure(Constraints(minWidth = natural[i], maxWidth = natural[i]))
            }
            val height = placeables.maxOfOrNull { it.height } ?: 0
            layout(width, height) {
                var x = width
                for (i in placeables.indices.reversed()) {
                    val p = placeables[i]
                    val y = (height - p.height) / 2
                    if (hasLeading && i == 0) {
                        p.placeRelative(0, y)
                    } else {
                        x -= p.width
                        p.placeRelative(x, y)
                        x -= gap
                    }
                }
            }
        } else {
            val placeables = measurables.map { it.measure(Constraints.fixedWidth(width)) }
            // Primary first: the trailing buttons in reverse, then the leading one.
            val order = placeables.indices.reversed().filter { !(hasLeading && it == 0) } +
                (if (hasLeading) listOf(0) else emptyList())
            val height = placeables.sumOf { it.height }
            layout(width, height) {
                var y = 0
                order.forEach { i ->
                    placeables[i].placeRelative(0, y)
                    y += placeables[i].height
                }
            }
        }
    }
}

/** Web's overlay `bg-black/60`. */
private val ScrimColor = Color.Black.copy(alpha = 0.6f)

/** Web's overlay `backdrop-blur-sm` (4px) at a phone's ~3x density. */
private const val ScrimBlurPx = 12
