package com.exponential.app.ui.issue

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.PersonOff
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.UserEntity
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.GlassSheetSearchField
import com.exponential.app.ui.components.UserAvatar
import com.exponential.app.ui.components.userDisplayName
import com.exponential.app.ui.theme.TextEmphasis

/**
 * Searchable assignee sheet (EXP-240): an inline search field, a pinned
 * Unassigned row, then member rows (avatar + name) filtered by name/email.
 * Selecting dismisses.
 */
@Composable
fun AssigneePickerSheet(
    users: List<UserEntity>,
    selectedUserId: String?,
    onSelect: (String?) -> Unit,
    onDismiss: () -> Unit,
) {
    var query by remember { mutableStateOf("") }

    val filtered = remember(users, query) {
        val q = query.trim()
        if (q.isEmpty()) {
            users
        } else {
            users.filter {
                (it.name ?: "").contains(q, ignoreCase = true) ||
                    it.email.contains(q, ignoreCase = true)
            }
        }
    }

    GlassSheet(title = "Assignee", onDismiss = onDismiss) {
        GlassSheetSearchField(
            value = query,
            onValueChange = { query = it },
            placeholder = "Search members",
        )
        Spacer(Modifier.height(4.dp))
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(max = 420.dp)
                .verticalScroll(rememberScrollState()),
        ) {
            // Unassigned stays pinned above the filtered members.
            GlassSheetRow(
                label = "Unassigned",
                selected = selectedUserId == null,
                leading = {
                    Icon(
                        Icons.Filled.PersonOff,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp),
                        tint = Color.White.copy(alpha = TextEmphasis.Tertiary),
                    )
                },
                onClick = {
                    onSelect(null)
                    onDismiss()
                },
            )
            filtered.forEach { user ->
                val name = userDisplayName(user, user.id)
                GlassSheetRow(
                    label = name,
                    selected = user.id == selectedUserId,
                    leading = { UserAvatar(user = user, nameOrEmail = name, size = 24.dp) },
                    onClick = {
                        onSelect(user.id)
                        onDismiss()
                    },
                )
            }
            if (filtered.isEmpty()) {
                Text(
                    "No matching members",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 20.dp, vertical = 12.dp),
                )
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}
