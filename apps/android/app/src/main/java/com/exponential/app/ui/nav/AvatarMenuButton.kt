package com.exponential.app.ui.nav

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Logout
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.SwapHoriz
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.getValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.ServerWorkspaceGroup
import com.exponential.app.data.db.WorkspaceEntity

// Data the top-bar avatar button needs. Provided once by MainScaffold so
// every screen inside the NavHost can render the button without re-plumbing
// callbacks (workspace list, sign-out, etc.) through every layer.
@Immutable
data class AvatarMenuState(
    val email: String?,
    val serverGroups: List<ServerWorkspaceGroup>,
    val activeAccountId: String?,
    val selectedWorkspace: WorkspaceEntity?,
    val onSelectWorkspace: (accountId: String, workspaceId: String) -> Unit,
    val onOpenSettings: () -> Unit,
    val onSignOut: () -> Unit,
)

val LocalAvatarMenu = staticCompositionLocalOf<AvatarMenuState?> { null }

/**
 * Avatar IconButton for the top app bar action slot. Shows the current
 * user's initials in a circle and opens a DropdownMenu with:
 *   - Switch workspace -> sub-menu of available workspaces
 *   - Settings        -> navigates to the settings route
 *   - Sign out        -> invokes the existing sign-out callback
 *
 * Reads state from LocalAvatarMenu so callers don't have to forward props.
 * Renders nothing if LocalAvatarMenu hasn't been provided.
 */
@Composable
fun AvatarMenuButton() {
    val state = LocalAvatarMenu.current ?: return
    var menuOpen by remember { mutableStateOf(false) }
    var workspaceSubOpen by remember { mutableStateOf(false) }

    // Material spec minimum touch target is 48dp; IconButton already sizes
    // itself to 48dp, so we don't need extra padding here.
    IconButton(onClick = { menuOpen = true }) {
        AvatarCircle(state.email)
    }

    DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
        DropdownMenuItem(
            text = { Text("Switch workspace") },
            leadingIcon = { Icon(Icons.Filled.SwapHoriz, contentDescription = null) },
            onClick = {
                workspaceSubOpen = true
            },
        )
        DropdownMenuItem(
            text = { Text("Settings") },
            leadingIcon = { Icon(Icons.Filled.Settings, contentDescription = null) },
            onClick = {
                menuOpen = false
                state.onOpenSettings()
            },
        )
        HorizontalDivider()
        DropdownMenuItem(
            text = { Text("Sign out") },
            leadingIcon = { Icon(Icons.AutoMirrored.Filled.Logout, contentDescription = null) },
            onClick = {
                menuOpen = false
                state.onSignOut()
            },
        )
    }

    // Second menu anchored to the same IconButton; opens on "Switch workspace"
    // tap. Keeping it as a sibling DropdownMenu (rather than a nested submenu)
    // sidesteps Material 3's lack of a native cascading menu component.
    DropdownMenu(
        expanded = workspaceSubOpen,
        onDismissRequest = {
            workspaceSubOpen = false
            menuOpen = false
        },
    ) {
        state.serverGroups.forEachIndexed { idx, group ->
            if (idx > 0) HorizontalDivider()
            Column(
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
            ) {
                Text(
                    group.hostname,
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                if (!group.userEmail.isNullOrBlank()) {
                    Text(
                        group.userEmail,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            group.workspaces.forEach { ws ->
                val isActive = group.accountId == state.activeAccountId
                    && ws.id == state.selectedWorkspace?.id
                DropdownMenuItem(
                    text = { Text(ws.name) },
                    leadingIcon = {
                        if (isActive) {
                            Icon(Icons.Filled.Check, contentDescription = null)
                        } else {
                            Box(Modifier.size(20.dp))
                        }
                    },
                    onClick = {
                        state.onSelectWorkspace(group.accountId, ws.id)
                        workspaceSubOpen = false
                        menuOpen = false
                    },
                )
            }
        }
    }
}

@Composable
private fun AvatarCircle(email: String?) {
    val initials = remember(email) { initialsFor(email) }
    Box(
        modifier = Modifier
            .size(32.dp)
            .background(MaterialTheme.colorScheme.primary, CircleShape),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            initials,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onPrimary,
        )
    }
}

private fun initialsFor(email: String?): String {
    if (email.isNullOrBlank()) return "?"
    val local = email.substringBefore('@')
    // Try splitting on common separators first; fall back to first two chars
    // of the local-part so that "danny" becomes "DA" rather than just "D".
    val parts = local.split('.', '_', '-', '+').filter { it.isNotBlank() }
    return when {
        parts.size >= 2 -> "${parts[0].first()}${parts[1].first()}".uppercase()
        local.length >= 2 -> local.take(2).uppercase()
        else -> local.take(1).uppercase().ifEmpty { "?" }
    }
}
