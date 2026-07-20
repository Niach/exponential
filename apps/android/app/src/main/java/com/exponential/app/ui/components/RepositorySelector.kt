package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.outlined.Circle
import androidx.compose.material3.CircularProgressIndicator
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
import com.exponential.app.data.api.GithubPickerRepo
import com.exponential.app.data.api.BoardRepositoryChoice
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.ui.onboarding.GithubRepoPickerSheet
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassButton
import com.exponential.app.ui.theme.glassRow

// The (optional) repository picker for board creation (masterplan v4 §6 —
// boards no longer require a repo; coding features gate on presence). Lists
// the team's already-connected registry repos AND lets the user add a
// brand-new repo by name via the installed-repos picker — that path connects the
// repo inline through `boards.create`'s `repository: { fullName }`. Binds a
// [BoardRepositoryChoice]. Shared by the create-board sheet and onboarding.
@Composable
fun RepositorySelector(
    accountId: String,
    teamId: String,
    repos: List<TeamRepo>,
    loading: Boolean,
    selection: BoardRepositoryChoice?,
    onSelect: (BoardRepositoryChoice) -> Unit,
    modifier: Modifier = Modifier,
) {
    val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)

    // A repo added by name in this session (not yet in the registry) — shown as a
    // selectable row and connected inline on create.
    var addedRepo by remember { mutableStateOf<GithubPickerRepo?>(null) }
    var showPicker by remember { mutableStateOf(false) }

    Column(modifier = modifier, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (loading) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.padding(vertical = 6.dp),
            ) {
                CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                Text("Loading repositories…", style = MaterialTheme.typography.bodySmall, color = tertiary)
            }
            return@Column
        }

        repos.forEach { repo ->
            RepoRow(
                fullName = repo.fullName,
                isPrivate = repo.isPrivate,
                selected = selection == BoardRepositoryChoice.Registry(repo.id),
                secondary = secondary,
                tertiary = tertiary,
            ) { onSelect(BoardRepositoryChoice.Registry(repo.id)) }
        }

        addedRepo?.let { added ->
            val selected = (selection as? BoardRepositoryChoice.Inline)?.fullName == added.fullName
            RepoRow(
                fullName = added.fullName,
                isPrivate = added.isPrivate,
                selected = selected,
                secondary = secondary,
                tertiary = tertiary,
            ) {
                onSelect(
                    BoardRepositoryChoice.Inline(
                        fullName = added.fullName,
                        defaultBranch = added.defaultBranch,
                        isPrivate = added.isPrivate,
                    )
                )
            }
        }

        if (repos.isEmpty() && addedRepo == null) {
            Text(
                "Connect a GitHub repository to back this board.",
                style = MaterialTheme.typography.bodySmall,
                color = tertiary,
                modifier = Modifier.padding(vertical = 4.dp),
            )
        }

        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier
                .glassButton()
                .clickable { showPicker = true }
                .padding(horizontal = 12.dp, vertical = 8.dp),
        ) {
            Icon(Icons.Filled.Add, contentDescription = null, modifier = Modifier.size(14.dp), tint = secondary)
            Spacer(Modifier.width(6.dp))
            Text(
                if (repos.isEmpty() && addedRepo == null) "Add a repository from GitHub…" else "Add another repository…",
                style = MaterialTheme.typography.labelMedium,
                color = secondary,
            )
        }
    }

    if (showPicker) {
        GithubRepoPickerSheet(
            accountId = accountId,
            teamId = teamId,
            onPick = { repo ->
                addedRepo = repo
                onSelect(
                    BoardRepositoryChoice.Inline(
                        fullName = repo.fullName,
                        defaultBranch = repo.defaultBranch,
                        isPrivate = repo.isPrivate,
                    )
                )
            },
            onDismiss = { showPicker = false },
        )
    }
}

@Composable
private fun RepoRow(
    fullName: String,
    isPrivate: Boolean,
    selected: Boolean,
    secondary: androidx.compose.ui.graphics.Color,
    tertiary: androidx.compose.ui.graphics.Color,
    onClick: () -> Unit,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .glassRow(active = selected)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        Icon(
            if (selected) Icons.Filled.CheckCircle else Icons.Outlined.Circle,
            contentDescription = null,
            modifier = Modifier.size(16.dp),
            tint = if (selected) MaterialTheme.colorScheme.primary else tertiary,
        )
        Spacer(Modifier.width(10.dp))
        Icon(Icons.Filled.Code, contentDescription = null, modifier = Modifier.size(14.dp), tint = secondary)
        Spacer(Modifier.width(8.dp))
        Text(
            fullName,
            style = MaterialTheme.typography.bodyMedium,
            fontFamily = FontFamily.Monospace,
            color = MaterialTheme.colorScheme.onSurface,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        if (isPrivate) {
            Icon(Icons.Filled.Lock, contentDescription = "Private", modifier = Modifier.size(14.dp), tint = tertiary)
        }
    }
}
