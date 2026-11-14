package com.exponential.app.ui.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.exponential.app.data.sidebar.SidebarFoldersStore
import com.exponential.app.data.sidebar.isFolderOpen
import com.exponential.app.domain.SidebarFolders
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.TextEmphasis
import dagger.hilt.android.EntryPointAccessors
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flowOf

// #EXP-1832 (fixtures/sidebar-folders.json): the member's folders, READ-ONLY on
// the phone (the board pickers, the board menu, the Apps list). A folder row
// is the list's own row with the folder glyph, its name, the count while
// folded and the fold chevron (web `sidebar-folders.tsx`, the IDE's sidebar,
// iOS); a press folds it, it never navigates. Its items sit one tree level in
// under the tree guides. No drag, no create, no rename on phones.

/** The rows a list shows for [ids] (the section's visible targets in DEFAULT
 *  order) + the fold toggle; flat rows when nothing is arranged. */
data class SidebarFolderLayout(
    val rows: List<SidebarFolders.Row>,
    val toggle: (folderId: String) -> Unit,
) {
    /** Only items, all at the top level: nothing to fold. */
    val isFlat: Boolean get() = rows.none { it.group }
}

@Composable
fun rememberSidebarFolders(teamId: String?, section: String, ids: List<String>): SidebarFolderLayout {
    val context = LocalContext.current
    val store = remember(context) {
        // A host without the Hilt graph (a specimen) simply shows flat rows.
        runCatching {
            EntryPointAccessors.fromApplication(context.applicationContext, SidebarFoldersStore.Access::class.java)
                .sidebarFoldersStore()
        }.getOrNull()
    }
    val entries by remember(store, teamId, section) {
        if (store == null || teamId == null) flowOf(emptyList()) else store.entries(teamId, section)
    }.collectAsState(emptyList())
    val flags by remember(store) { store?.openFlags ?: MutableStateFlow(emptyMap()) }.collectAsState()
    val rows = remember(ids, entries, flags, teamId) {
        SidebarFolders.rows(SidebarFolders.tree(ids, entries)) { folder -> teamId == null || flags.isFolderOpen(teamId, folder) }
    }
    return remember(rows, store, teamId) {
        SidebarFolderLayout(rows) { folder -> if (store != null && teamId != null) store.toggle(teamId, folder) }
    }
}

/** A folder row's leading glyph (muted, like web's `ui-folder`). */
@Composable
fun SidebarFolderGlyph(size: Dp = 18.dp, modifier: Modifier = Modifier) {
    Icon(
        ExpIcons.uiFolder,
        contentDescription = null,
        modifier = modifier.size(size),
        tint = Color.White.copy(alpha = TextEmphasis.Secondary),
    )
}

/** A folder row's trailing: the item count while folded, then the chevron
 *  (right = folded, down = open). */
@Composable
fun SidebarFolderTrailing(open: Boolean, count: Int, modifier: Modifier = Modifier) {
    Row(
        modifier = modifier.testTag(if (open) "sidebar-folder-open" else "sidebar-folder-closed"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        if (!open) {
            Text(
                count.toString(),
                style = MaterialTheme.typography.bodySmall,
                color = Color.White.copy(alpha = TextEmphasis.Tertiary),
            )
        }
        Icon(
            ExpIcons.uiChevronRight,
            contentDescription = if (open) "Collapse" else "Expand",
            modifier = Modifier.size(16.dp).rotate(if (open) 90f else 0f),
            tint = Color.White.copy(alpha = TextEmphasis.Tertiary),
        )
    }
}
