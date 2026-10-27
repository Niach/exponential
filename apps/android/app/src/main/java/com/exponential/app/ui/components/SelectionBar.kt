package com.exponential.app.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LocalMinimumInteractiveComponentSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassCard

// EXP-239 / #EXP-1388: THE floating multi-select bar's chrome, shared by the
// issue list's bulk bar and the Chats selection bar (chats.json SELECTION:
// "the issue list's bulk action bar, same capsule, placement and phone chrome
// swap"): the opaque glass card at radius 24 in the tab bar's slot and height,
// close, the count, then the caller's 32dp icon buttons spread over the rest
// of the slot. One placement ([SelectionBarSlot]), one selected-row paint
// ([selectedRowHighlight]) and one leading glyph cell ([SelectionGlyph]).

/** The bar's height IS the nav bar's pill height — it takes that slot. */
val SelectionBarHeight: Dp = 52.dp

@Composable
fun SelectionBarShell(
    count: String,
    onClear: () -> Unit,
    modifier: Modifier = Modifier,
    testTag: String = "bulk-selection-bar",
    content: @Composable RowScope.() -> Unit,
) {
    val neutral = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    // Suppress the 48dp minimum interactive inflation so the 32dp icon buttons
    // keep the bar at its own height instead of ballooning it.
    CompositionLocalProvider(LocalMinimumInteractiveComponentSize provides Dp.Unspecified) {
        Row(
            modifier = modifier
                // EXP-642: the styleguide capture addresses the bar directly
                // (iOS parity — `bulk-selection-bar`).
                .testTag(testTag)
                // EXP-698 r5: the ONE bulk-bar chrome on every client — the
                // opaque glass card at radius 24, not a hand-mixed capsule.
                // The bar takes the tab bar's slot and its height (52), so the
                // two never read as two different floating things.
                // #EXP-1388: the bar fills the tab bar's slot (the nav pill
                // fills it too), whatever the caller's action count.
                .fillMaxWidth()
                .height(SelectionBarHeight)
                .glassCard(opaque = true, cornerRadius = DesignTokens.Radius.Xl3)
                .padding(horizontal = 10.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            IconButton(onClick = onClear, modifier = Modifier.size(32.dp)) {
                Icon(
                    ExpIcons.uiClose,
                    contentDescription = "Clear selection",
                    modifier = Modifier.size(18.dp),
                    tint = neutral,
                )
            }
            Text(
                count,
                style = MaterialTheme.typography.titleSmall.copy(fontWeight = FontWeight.SemiBold),
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                modifier = Modifier.padding(horizontal = 4.dp),
            )
            // The actions spread over the rest of the bar, the last one on
            // its far edge (the issue bar's Start coding + Delete, the Chats
            // bar's Mark read … Delete alike).
            Row(
                modifier = Modifier.weight(1f),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.SpaceBetween,
                content = content,
            )
        }
    }
}

/** The list gutter a selectable row's card sits in (the issue list's). */
val SelectionRowGutter: Dp = 16.dp

/** THE bar's placement: bottom centre, in the tab bar's slot (the nav bar's
 *  own gutter, 8dp off the bottom edge). [modifier] = the caller's inset
 *  (a screen whose Box sits outside its Scaffold adds the nav-bar inset). */
@Composable
fun BoxScope.SelectionBarSlot(
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        modifier = Modifier
            .align(Alignment.BottomCenter)
            .then(modifier)
            .padding(horizontal = BottomNavDefaults.HorizontalInset)
            .padding(bottom = 8.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
        content = content,
    )
}

/** THE selected row's paint while selecting (the issue card's: the primary
 *  wash + the active 1dp border at the row radius), painted over the row's
 *  glass card. False = nothing. */
@Composable
fun Modifier.selectedRowHighlight(selected: Boolean): Modifier {
    if (!selected) return this
    val shape = RoundedCornerShape(GlassTokens.RowRadius)
    return this
        .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.12f), shape)
        .border(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.45f), shape)
}

/** One of the bar's 32dp icon buttons (neutral, or [tint], e.g. the error
 *  colour for a destructive action). */
@Composable
fun SelectionBarIconButton(
    icon: ImageVector,
    contentDescription: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    tint: Color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
) {
    IconButton(onClick = onClick, modifier = modifier.size(32.dp)) {
        Icon(icon, contentDescription = contentDescription, modifier = Modifier.size(18.dp), tint = tint)
    }
}

/** THE selection glyph cell a row leads with while selecting (the issue
 *  list's: ui-selected in the primary colour / ui-unselected muted, 20dp,
 *  10dp before the row's own lead). */
@Composable
fun SelectionGlyph(selected: Boolean, modifier: Modifier = Modifier) {
    Icon(
        if (selected) ExpIcons.uiSelected else ExpIcons.uiUnselected,
        contentDescription = if (selected) "Selected" else "Not selected",
        modifier = modifier.size(20.dp),
        tint = if (selected) {
            MaterialTheme.colorScheme.primary
        } else {
            MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
        },
    )
    Spacer(Modifier.width(10.dp))
}
