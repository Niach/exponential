package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.data.api.WorkspaceRepo
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

// The required repository picker for project creation (masterplan v4 §6). Lists
// ONLY already-connected repos — connecting new repos stays web-only on Android,
// so the empty state points the user at the web app. Shared by the create-project
// sheet and the onboarding wizard.
@Composable
fun RepositorySelector(
    repos: List<WorkspaceRepo>,
    loading: Boolean,
    selectedId: String?,
    onSelect: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)

    if (!loading && repos.isEmpty()) {
        Text(
            "Connect a repository in the web app first",
            style = MaterialTheme.typography.bodySmall,
            color = tertiary,
            modifier = modifier.padding(vertical = 4.dp),
        )
        return
    }

    val selected = repos.firstOrNull { it.id == selectedId }
    var menuOpen by remember { mutableStateOf(false) }
    Box(modifier = modifier) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier
                .fillMaxWidth()
                .glassRow()
                .clickable(enabled = !loading) { menuOpen = true }
                .padding(horizontal = 12.dp, vertical = 10.dp),
        ) {
            Icon(Icons.Filled.Code, contentDescription = null, modifier = Modifier.size(14.dp), tint = secondary)
            Spacer(Modifier.width(8.dp))
            Text(
                when {
                    loading -> "Loading repositories…"
                    selected != null -> selected.fullName
                    else -> "Select a repository"
                },
                style = MaterialTheme.typography.bodyMedium,
                fontFamily = if (selected != null) FontFamily.Monospace else FontFamily.Default,
                color = if (selected != null) MaterialTheme.colorScheme.onSurface else tertiary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Icon(Icons.Filled.ExpandMore, contentDescription = null, modifier = Modifier.size(18.dp), tint = tertiary)
        }
        DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
            repos.forEach { repo ->
                DropdownMenuItem(
                    text = {
                        Text(repo.fullName, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodyMedium)
                    },
                    leadingIcon = { Icon(Icons.Filled.Code, contentDescription = null, modifier = Modifier.size(14.dp)) },
                    onClick = {
                        menuOpen = false
                        onSelect(repo.id)
                    },
                )
            }
        }
    }
}
