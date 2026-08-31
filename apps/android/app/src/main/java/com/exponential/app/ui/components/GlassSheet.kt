package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/** How tall a [GlassSheet] presents (EXP-687). */
enum class SheetHeight {
    /**
     * Wraps its content, capped at [GlassSheetDefaults.FittedMaxHeightFraction]
     * of the screen — every picker, the label editor, New board, Add repository.
     */
    Fitted,

    /** Fills the screen below the status bar — Start coding, New action, the diff. */
    Full,
}

/**
 * The one primary action a sheet may pin to its bottom edge (EXP-687). Never a
 * toolbar/top-right button: a phone sheet's reachable corner is the bottom one,
 * and one shape for "the button that commits this sheet" is the whole point.
 */
class SheetPrimaryAction(
    val label: String,
    val onClick: () -> Unit,
    val enabled: Boolean = true,
    val loading: Boolean = false,
    val icon: ImageVector? = null,
)

/**
 * The pinned values of the shared sheet chrome, JVM-testable like
 * [GlassMenuDefaults] and pinned by `GlassSheetDefaultsTest`. iOS mirrors these
 * in `GlassSheetTokens` — the two must not drift.
 */
object GlassSheetDefaults {
    /** Opaque #18181B: the sheet continues the app gradient where it ends. */
    val ContainerColor: Color = GlassTokens.BackgroundBottom

    /** 24dp top corners — the `xl3` rung, iOS `GlassSheetTokens.cornerRadius`, web `rounded-t-3xl`. M3's default is 28. */
    val CornerRadius = DesignTokens.Radius.Xl3

    /** A fitted sheet never eats more than 85 % of the screen — the cross-platform cap. */
    const val FittedMaxHeightFraction = 0.85f

    /** The drag handle is the ONLY dismiss affordance a sheet advertises (no ✕, no Cancel). */
    val DragHandleColor: Color = Color.White.copy(alpha = TextEmphasis.Quaternary)

    val TitleColor: Color = Color.White.copy(alpha = 0.9f)

    /** [GlassSheetRow]'s gutter, so a title lines up with the rows beneath it. */
    val HorizontalPadding = 20.dp
}

/**
 * The ONE bottom-sheet shell (EXP-240, reshaped by EXP-687): an opaque zinc
 * surface (the alpha-fill glass idiom needs the gradient beneath — a floating
 * sheet has none), a drag handle on every sheet, an optional left-aligned title
 * with an optional [headerAction] beside it, and an optional [primaryAction]
 * pinned full-width to the bottom. There is deliberately no close button and no
 * Cancel pill anywhere: swiping down (or back) dismisses, on all three mobile
 * clients.
 *
 * The [content] slot is bounded — [SheetHeight.Fitted] caps the whole column at
 * [GlassSheetDefaults.FittedMaxHeightFraction] of the screen — but never
 * scrolled here: the caller owns its own scroller (a `LazyColumn`, a
 * `verticalScroll` Column, a grid), so nesting one inside another can't happen.
 *
 * material3's [ModalBottomSheet] already applies `imePadding` and the system-bar
 * insets, so call sites must not add `navigationBarsPadding()`/`imePadding()`.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GlassSheet(
    title: String?,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
    height: SheetHeight = SheetHeight.Fitted,
    headerAction: (@Composable () -> Unit)? = null,
    primaryAction: SheetPrimaryAction? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val full = height == SheetHeight.Full
    val maxFittedHeight = (LocalConfiguration.current.screenHeightDp * GlassSheetDefaults.FittedMaxHeightFraction).dp
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        modifier = if (full) Modifier.statusBarsPadding() else Modifier,
        shape = RoundedCornerShape(topStart = GlassSheetDefaults.CornerRadius, topEnd = GlassSheetDefaults.CornerRadius),
        containerColor = GlassSheetDefaults.ContainerColor,
        dragHandle = { BottomSheetDefaults.DragHandle(color = GlassSheetDefaults.DragHandleColor) },
    ) {
        Column(
            modifier = modifier
                .fillMaxWidth()
                .then(if (full) Modifier.fillMaxHeight() else Modifier.heightIn(max = maxFittedHeight)),
        ) {
            if (title != null || headerAction != null) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(
                            start = GlassSheetDefaults.HorizontalPadding,
                            end = GlassSheetDefaults.HorizontalPadding,
                            bottom = 12.dp,
                        ),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        title.orEmpty(),
                        style = MaterialTheme.typography.titleMedium,
                        color = GlassSheetDefaults.TitleColor,
                        modifier = Modifier.weight(1f),
                    )
                    if (headerAction != null) headerAction()
                }
            }
            // fill = false on a fitted sheet: the slot takes what the content
            // needs, up to whatever the 85 % cap leaves over.
            Column(modifier = Modifier.weight(1f, fill = full)) {
                content()
            }
            if (primaryAction != null) {
                GlassSubmitButton(
                    label = primaryAction.label,
                    onClick = primaryAction.onClick,
                    enabled = primaryAction.enabled && !primaryAction.loading,
                    modifier = Modifier.padding(
                        start = GlassSheetDefaults.HorizontalPadding,
                        end = GlassSheetDefaults.HorizontalPadding,
                        top = 12.dp,
                        bottom = 16.dp,
                    ),
                    // EXP-694: both take the button's own content color — an
                    // enabled submit is now a solid near-white fill, so a white
                    // glyph would vanish on it.
                    icon = when {
                        primaryAction.loading -> ({
                            CircularProgressIndicator(
                                modifier = Modifier.size(16.dp),
                                strokeWidth = 2.dp,
                                color = LocalContentColor.current,
                            )
                        })
                        primaryAction.icon != null -> ({
                            Icon(
                                primaryAction.icon,
                                contentDescription = null,
                                modifier = Modifier.size(16.dp),
                            )
                        })
                        else -> null
                    },
                )
            } else {
                Spacer(Modifier.height(12.dp))
            }
        }
    }
}

/**
 * The secondary text action a sheet header may carry beside its title — "Clear
 * all", "No icon". Not a dismiss: those are the drag handle's job.
 */
@Composable
fun GlassSheetHeaderAction(label: String, onClick: () -> Unit) {
    TextButton(onClick = onClick) {
        Text(
            label,
            style = MaterialTheme.typography.labelLarge,
            color = Color.White.copy(alpha = TextEmphasis.Secondary),
        )
    }
}

// One sheet row: leading slot + label + trailing slot (a checkmark when
// [selected] and no explicit trailing). 44dp minimum touch height.
@Composable
fun GlassSheetRow(
    label: String,
    onClick: () -> Unit,
    selected: Boolean = false,
    enabled: Boolean = true,
    labelColor: Color = Color.White.copy(alpha = if (enabled) 0.9f else TextEmphasis.Quaternary),
    leading: (@Composable () -> Unit)? = null,
    trailing: (@Composable RowScope.() -> Unit)? = null,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 44.dp)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = GlassSheetDefaults.HorizontalPadding, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (leading != null) {
            Box(modifier = Modifier.width(30.dp), contentAlignment = Alignment.CenterStart) {
                leading()
            }
        }
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            color = labelColor,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (trailing != null) {
            trailing()
        } else if (selected) {
            Icon(
                ExpIcons.uiCheck,
                contentDescription = "Selected",
                modifier = Modifier.size(18.dp),
                tint = Color.White,
            )
        }
    }
}

// The inline search field the searchable sheets share — the exact styling the
// duplicate picker introduced (glass fill, no indicator line).
@Composable
fun GlassSheetSearchField(
    value: String,
    onValueChange: (String) -> Unit,
    placeholder: String,
    modifier: Modifier = Modifier,
) {
    GlassTextField(
        value = value,
        onValueChange = onValueChange,
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp),
        placeholder = placeholder,
        leadingIcon = {
            Icon(
                ExpIcons.navSearch,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
            )
        },
        singleLine = true,
    )
}
