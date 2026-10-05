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
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.ui.window.DialogWindowProvider
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens

/** One button of a [GlassAlert], top to bottom in the order given. */
data class GlassAlertAction(
    val label: String,
    val role: GlassButtonRole,
    val onClick: () -> Unit,
    val enabled: Boolean = true,
    val testTag: String? = null,
)

/**
 * The app's own centred alert (EXP-1212) — web `Dialog mobile="alert"` /
 * `AlertDialog` on a phone, never Material's `AlertDialog`: a dimmed, blurred
 * scrim and a centred glass card (24dp side margin, 16dp corners, the card
 * hairline, 20dp padding) with a left-aligned title + muted body over
 * FULL-WIDTH stacked [GlassSubmitButton]s (40dp, 8dp apart). No close ✕;
 * system back and a scrim tap call [onDismiss]. Nothing takes initial focus,
 * so a destructive action is never the default.
 */
@Composable
fun GlassAlert(
    title: String,
    body: String?,
    actions: List<GlassAlertAction>,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
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
                verticalArrangement = Arrangement.spacedBy(16.dp),
                modifier = modifier
                    .padding(horizontal = 24.dp)
                    .widthIn(max = 384.dp)
                    .fillMaxWidth()
                    .clip(shape)
                    .background(DesignTokens.Glass.BackgroundBottom, shape)
                    .border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)
                    // Taps on the card are not scrim taps.
                    .clickable(interactionSource = noRipple, indication = null, onClick = {})
                    .padding(20.dp)
                    .testTag("glass-alert"),
            ) {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(
                        title,
                        style = MaterialTheme.typography.titleMedium.copy(
                            fontSize = 18.sp,
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
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    actions.forEach { action ->
                        GlassSubmitButton(
                            label = action.label,
                            onClick = action.onClick,
                            enabled = action.enabled,
                            role = action.role,
                            compact = true,
                            modifier = if (action.testTag != null) Modifier.testTag(action.testTag) else Modifier,
                        )
                    }
                }
            }
        }
    }
}

/** Web's overlay `bg-black/60`. */
private val ScrimColor = Color.Black.copy(alpha = 0.6f)

/** Web's overlay `backdrop-blur-sm` (4px) at a phone's ~3x density. */
private const val ScrimBlurPx = 12
