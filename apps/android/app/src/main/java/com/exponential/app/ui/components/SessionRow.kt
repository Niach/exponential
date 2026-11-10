package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import com.exponential.app.ui.theme.ExpMono
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.ListItem
import com.exponential.app.domain.TreeGuide
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow

// Session 20b7fdc2: THE session row, x4 (web `@exp/ui` SessionRow, desktop
// `run_rows::run_row`, iOS ExpUI `SessionRow.swift`; geometry =
// `list-item.json`). ONE line: [tree guides][lead at 12 + 14·depth]
// [mono id · title]. The lead is the session's run mark (the host hands it
// in); a bold title = unread. No fold chevron, no trailing chevron.

const val SESSION_ROW_TAG = "session-row"

/** One session row: 32dp (`list-item.json` small). */
@Composable
fun SessionRow(
    title: String,
    modifier: Modifier = Modifier,
    identifier: String? = null,
    depth: Int = 0,
    /** This row's connector geometry (`TreeGuides.compute`). */
    guide: TreeGuide? = null,
    active: Boolean = false,
    unread: Boolean = false,
    onClick: (() -> Unit)? = null,
    testTag: String = SESSION_ROW_TAG,
    lead: @Composable () -> Unit,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .height(ListItem.SMALL_DP.dp)
            .testTag(testTag)
            .treeGuides(guide)
            .flatRow(active)
            .then(if (onClick == null) Modifier else Modifier.clickable(onClick = onClick))
            .padding(start = ListItem.leadX(depth).dp, end = ListItem.BASE_DP.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(ListItem.GAP_DP.dp),
    ) {
        Box(Modifier.size(ListItem.MARK_DP.dp), contentAlignment = Alignment.Center) { lead() }
        Row(
            modifier = Modifier.weight(1f),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            if (!identifier.isNullOrEmpty()) {
                Text(
                    identifier,
                    style = MaterialTheme.typography.labelMedium,
                    fontFamily = ExpMono,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 1,
                    softWrap = false,
                )
            }
            Text(
                title,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = if (unread) FontWeight.SemiBold else null,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}
