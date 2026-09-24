package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.icons.ExpIcons

/**
 * EXP-1029 contract — sub-shell navigation for the settings shell
 * (`OptionGroup` rows, EXP-994). EXP-1020 implements it and uses it for
 * "Workflow settings" inside the device settings sheet.
 *
 * A [SubShell] is a ROW ENTRY inside a card. Opening it slides a child page
 * in place of the WHOLE card — not a nested card, not a pushed screen —
 * with a back button on top; the child page is the same shell (its own
 * `OptionGroup`s of rows), so a sub-shell may hold another sub-shell.
 * [SubShellHost] is the card boundary the page replaces: it renders its
 * card at rest and, once a row inside opened, that row's page.
 *
 * Siblings (same names): web `packages/ui/src/sub-shell.tsx`, IDE
 * `ui::sub_shell`, iOS `ExpUI/Sources/SubShell.swift`.
 *
 * This file is the CONTRACT: the row stub (never opens) and a host that
 * only ever renders its card. `SubShellContractTest` carries the behaviour,
 * ignored until EXP-1020.
 */

/** The card boundary a sub-shell page replaces. Contract stub: the card. */
@Composable
fun SubShellHost(content: @Composable () -> Unit) {
    content()
}

/**
 * A row entry that slides its child page in place of the whole card.
 * Contract stub: the row (label, description, value, chevron); opening does
 * nothing until EXP-1020.
 */
@Composable
fun SubShell(
    label: String,
    /** A muted second line under the label. */
    description: String? = null,
    /** A leading `ExpIcons` glyph. */
    icon: ImageVector? = null,
    /** A muted trailing summary (`opus · fable`). */
    value: String? = null,
    /** The child page's title; defaults to [label]. */
    title: String? = null,
    enabled: Boolean = true,
    /** The child page: the same shell — `OptionGroup`s of rows. */
    page: @Composable () -> Unit,
) {
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 12.dp)
            .alpha(if (enabled) 1f else 0.5f),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (icon != null) {
            Icon(imageVector = icon, contentDescription = null, tint = muted)
        }
        Column(modifier = Modifier.weight(1f)) {
            Text(label, color = LocalContentColor.current)
            if (description != null) {
                Text(description, style = MaterialTheme.typography.bodySmall, color = muted)
            }
        }
        if (value != null) {
            Text(value, color = muted)
        }
        Icon(imageVector = ExpIcons.uiChevronRight, contentDescription = null, tint = muted)
    }
}
