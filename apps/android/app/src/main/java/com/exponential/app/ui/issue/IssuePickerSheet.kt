package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.theme.TextEmphasis

/**
 * A reusable bottom-sheet picker for issue properties (status, priority, assignee, ...).
 *
 * Glass chrome (EXP-240): [GlassSheet] title + circular ✕ over [GlassSheetRow]s
 * — leading icon, label, trailing check when selected. Tapping a row invokes
 * [onSelect] with the item and the sheet dismisses via [onDismiss]. The generic
 * signature is unchanged (CreateIssueScreen and the move-board picker reuse it);
 * [leadingContent] optionally replaces the plain [iconOf] glyph so status /
 * priority rows can render their colored icons.
 */
@Composable
fun <T> IssuePickerSheet(
    title: String,
    items: List<T>,
    selected: T?,
    keyOf: (T) -> Any = { it as Any },
    labelOf: (T) -> String,
    iconOf: ((T) -> ImageVector)? = null,
    leadingContent: (@Composable (T) -> Unit)? = null,
    onSelect: (T) -> Unit,
    onDismiss: () -> Unit,
) {
    val selectedKey = selected?.let(keyOf)

    GlassSheet(title = title, onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState()),
        ) {
            items.forEach { item ->
                GlassSheetRow(
                    label = labelOf(item),
                    selected = keyOf(item) == selectedKey,
                    leading = when {
                        leadingContent != null -> ({ leadingContent(item) })
                        iconOf != null -> ({
                            Icon(
                                iconOf(item),
                                contentDescription = null,
                                modifier = Modifier.size(18.dp),
                                tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                            )
                        })
                        else -> null
                    },
                    onClick = {
                        onSelect(item)
                        onDismiss()
                    },
                )
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}
