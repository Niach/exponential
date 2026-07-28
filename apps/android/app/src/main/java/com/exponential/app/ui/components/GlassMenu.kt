package com.exponential.app.ui.components

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MenuDefaults
import androidx.compose.material3.MenuItemColors
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.PopupProperties
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

/**
 * The one glass menu treatment (EXP-332). Android used to ship two different
 * menu looks: stock M3 [DropdownMenu] (surfaceContainer #171717, 4dp radius,
 * drop shadow) at seven call sites, and BlockTextField's hand-rolled `@`/`#`
 * autocomplete popup (surfaceContainerHigh #1F1F22, 8dp radius, 8dp shadow).
 * Both now render from these defaults, so a menu reads like the rest of the
 * app's glass chrome instead of like a different product.
 *
 * The recipe is `Modifier.glassButton(opaque = true)`'s (EXP-165): a menu floats
 * over arbitrary content, where the low-alpha fill alone bleeds through, so the
 * white glass tint is composited over an OPAQUE base. The base is
 * [DesignTokens.Palette.Popover] — the design system's own popover token, and
 * the same #171717 the opaque glass recipe uses. (GlassSheet's
 * `BackgroundBottom` is deliberately NOT reused: a bottom sheet continues the
 * app gradient where it ends, a menu has no such anchor.)
 *
 * Fill, stroke and radius all come from the CARD rung of the glass ladder — a
 * menu is an elevated container of rows — so the values stay derived from
 * [GlassTokens] rather than forking a second set of numbers.
 */
object GlassMenuDefaults {
    /** 12dp — iOS `UIMenu` is ~13pt; M3's 4dp `extraSmall` reads as a different product. */
    val Shape: Shape = RoundedCornerShape(GlassTokens.SectionRadius)

    /** White .06 over #171717 == opaque #252525 — `glassCard`'s fill on `glassButton(opaque)`'s base. */
    val ContainerColor: Color = GlassTokens.CardFill.compositeOver(DesignTokens.Palette.Popover)

    /** Hairline white .10. The floating bars hand-roll .12; indistinguishable at 0.5dp, so prefer the token. */
    val Border = BorderStroke(GlassTokens.Hairline, GlassTokens.StrokeCard)

    /**
     * No shadow, on purpose: separation in this app comes from the opaque fill
     * plus a hairline (BottomNavBar, IssueDetailBottomBar, the IssueListScreen
     * selection bar), never from elevation — and a black M3 shadow is nearly
     * invisible on a near-black theme anyway. Flip here if a menu ever needs
     * lifting off its backdrop.
     */
    val ShadowElevation = 0.dp

    /** M3's tonal overlay only applies when the color IS `colorScheme.surface`; keep it off explicitly. */
    val TonalElevation = 0.dp

    /** GlassSheetRow's label color. */
    val TextColor: Color = Color.White.copy(alpha = 0.9f)

    val IconColor: Color = Color.White.copy(alpha = TextEmphasis.Secondary)

    private val DisabledColor: Color = Color.White.copy(alpha = TextEmphasis.Quaternary)

    /** Hairline between groups of menu items — the same white .06 every other divider in the app uses. */
    val DividerColor: Color = GlassTokens.StrokeRow

    @Composable
    fun itemColors(): MenuItemColors = MenuDefaults.itemColors(
        textColor = TextColor,
        leadingIconColor = IconColor,
        trailingIconColor = IconColor,
        disabledTextColor = DisabledColor,
        disabledLeadingIconColor = DisabledColor,
        disabledTrailingIconColor = DisabledColor,
    )

    @Composable
    fun destructiveItemColors(): MenuItemColors = MenuDefaults.itemColors(
        textColor = MaterialTheme.colorScheme.error,
        leadingIconColor = MaterialTheme.colorScheme.error,
        trailingIconColor = MaterialTheme.colorScheme.error,
        disabledTextColor = DisabledColor,
        disabledLeadingIconColor = DisabledColor,
        disabledTrailingIconColor = DisabledColor,
    )
}

private val DefaultMenuPopupProperties = PopupProperties(focusable = true)

/**
 * [DropdownMenu] in the glass idiom. A drop-in for material3's — every parameter
 * a call site uses is forwarded, notably [properties] (the markdown toolbar's
 * attach menu MUST stay `focusable = false`, or the popup takes focus, drops the
 * IME, and tears its own menu out of composition) and [offset] / [scrollState].
 *
 * M3's enter/exit scale+fade, anchor positioning, edge flipping, outside-tap
 * dismissal and a11y semantics all live in `DropdownMenuContent` and are
 * untouched by the styling parameters, so they are preserved.
 */
@Composable
fun GlassDropdownMenu(
    expanded: Boolean,
    onDismissRequest: () -> Unit,
    modifier: Modifier = Modifier,
    offset: DpOffset = DpOffset(0.dp, 0.dp),
    scrollState: ScrollState = rememberScrollState(),
    properties: PopupProperties = DefaultMenuPopupProperties,
    content: @Composable ColumnScope.() -> Unit,
) {
    DropdownMenu(
        expanded = expanded,
        onDismissRequest = onDismissRequest,
        modifier = modifier,
        offset = offset,
        scrollState = scrollState,
        properties = properties,
        shape = GlassMenuDefaults.Shape,
        containerColor = GlassMenuDefaults.ContainerColor,
        tonalElevation = GlassMenuDefaults.TonalElevation,
        shadowElevation = GlassMenuDefaults.ShadowElevation,
        border = GlassMenuDefaults.Border,
        content = content,
    )
}

/**
 * One row of a [GlassDropdownMenu]. Mirrors [DropdownMenuItem]'s slot API so a
 * call site is a rename, and folds in the glass content colors. [destructive]
 * replaces the `Text("Delete", color = colorScheme.error)` the delete/remove
 * items used to hand-roll — it also tints the leading icon, which the manual
 * form never did.
 *
 * M3 pins items at `minHeight = 48.dp` with 12dp horizontal padding — already
 * past the glass 44dp touch target, and equal to [GlassTokens.RowPaddingH] — so
 * the metrics are deliberately left at their defaults.
 */
@Composable
fun GlassMenuItem(
    text: @Composable () -> Unit,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    leadingIcon: (@Composable () -> Unit)? = null,
    trailingIcon: (@Composable () -> Unit)? = null,
    enabled: Boolean = true,
    destructive: Boolean = false,
) {
    DropdownMenuItem(
        text = text,
        onClick = onClick,
        modifier = modifier,
        leadingIcon = leadingIcon,
        trailingIcon = trailingIcon,
        enabled = enabled,
        colors = if (destructive) {
            GlassMenuDefaults.destructiveItemColors()
        } else {
            GlassMenuDefaults.itemColors()
        },
    )
}

/**
 * The [GlassDropdownMenu] container as a bare surface, for menus that must
 * hand-roll their own `Popup` — BlockTextField's `@`/`#` autocomplete needs a
 * custom `PopupPositionProvider` (it tracks the caret rect above/below the IME)
 * that [DropdownMenu] gives no way to supply. Same fill, stroke and radius, so
 * the two menu families cannot drift apart again.
 */
@Composable
fun GlassMenuSurface(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    Surface(
        modifier = modifier,
        shape = GlassMenuDefaults.Shape,
        color = GlassMenuDefaults.ContainerColor,
        contentColor = GlassMenuDefaults.TextColor,
        tonalElevation = GlassMenuDefaults.TonalElevation,
        shadowElevation = GlassMenuDefaults.ShadowElevation,
        border = GlassMenuDefaults.Border,
        content = content,
    )
}
