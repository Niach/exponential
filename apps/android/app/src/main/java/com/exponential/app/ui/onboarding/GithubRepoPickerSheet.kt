package com.exponential.app.ui.onboarding

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.browser.customtabs.CustomTabsIntent
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.GithubPickerRepo
import com.exponential.app.data.api.GithubReposResult
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

// Installed-repo picker (web github-repo-picker.tsx / iOS GithubRepoPicker): lists
// the repos the user's GitHub App is installed on and returns the chosen one. When
// the App isn't installed it offers an inline connect that opens the (mobile-marked)
// install URL in a Chrome Custom Tab; the server's post-install page fires
// exp://github-connected, which closes the tab, returns here, and re-fetches
// (see GithubRepoPickerViewModel). Returning any other way (older server, tab
// dismissed by hand) still re-queries on lifecycle RESUME. The repo is connected
// server-side by `projects.create`'s `repository: { fullName }` path.
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GithubRepoPickerSheet(
    accountId: String,
    workspaceId: String,
    onPick: (GithubPickerRepo) -> Unit,
    onDismiss: () -> Unit,
    viewModel: GithubRepoPickerViewModel = hiltViewModel(),
) {
    val result by viewModel.result.collectAsStateWithLifecycle()
    val loading by viewModel.loading.collectAsStateWithLifecycle()
    val error by viewModel.error.collectAsStateWithLifecycle()

    // Re-query on every resume so returning from the GitHub install Custom Tab
    // (new repos granted) refreshes without a manual tap. The first load isn't a
    // forced refresh; later resumes bypass the server cache.
    var hasLoaded by remember { mutableStateOf(false) }
    LifecycleResumeEffect(accountId, workspaceId) {
        viewModel.load(accountId, workspaceId, refresh = hasLoaded)
        hasLoaded = true
        onPauseOrDispose {}
    }

    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = GlassTokens.BackgroundBottom,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .padding(bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                "Add repository",
                style = MaterialTheme.typography.titleMedium,
                color = MaterialTheme.colorScheme.onSurface,
            )
            val data = result
            when {
                loading && data == null -> LoadingRow()
                data == null || !data.configured -> NotConfigured()
                !data.installed -> NotInstalled(
                    data = data,
                    onConnect = { viewModel.load(accountId, workspaceId, refresh = true) },
                )
                else -> InstalledList(
                    data = data,
                    onPick = { onPick(it); onDismiss() },
                )
            }
            if (error != null && data == null) {
                Text(
                    error ?: "",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
    }
}

@Composable
private fun LoadingRow() {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        modifier = Modifier.padding(vertical = 20.dp),
    ) {
        CircularProgressIndicator(modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
        Text(
            "Loading your GitHub repositories…",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
    }
}

@Composable
private fun NotConfigured() {
    Text(
        "GitHub isn't configured on this server, so repositories can't be connected.",
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        modifier = Modifier.padding(vertical = 8.dp),
    )
}

@Composable
private fun NotInstalled(data: GithubReposResult, onConnect: () -> Unit) {
    val context = LocalContext.current
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(
            "Connect the Exponential GitHub App to pick a repository. You'll be brought back here when it's done.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        // Prefer the single-consent OAuth connect URL that claims the account for
        // the workspace; fall back to the App install page on older servers.
        val connectUrl = data.connectUrl ?: data.installUrl
        Button(
            onClick = {
                connectUrl?.let {
                    CustomTabsIntent.Builder().build()
                        .launchUrl(context, android.net.Uri.parse(it))
                }
            },
            enabled = connectUrl != null,
            modifier = Modifier.fillMaxWidth(),
        ) {
            Icon(Icons.Filled.Code, contentDescription = null, modifier = Modifier.size(16.dp))
            Spacer(Modifier.width(8.dp))
            Text("Connect GitHub")
        }
        OutlinedButton(onClick = onConnect, modifier = Modifier.fillMaxWidth()) {
            Icon(Icons.Filled.Refresh, contentDescription = null, modifier = Modifier.size(16.dp))
            Spacer(Modifier.width(8.dp))
            Text("I've connected — refresh")
        }
    }
}

@Composable
private fun InstalledList(data: GithubReposResult, onPick: (GithubPickerRepo) -> Unit) {
    var query by remember { mutableStateOf("") }
    val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val filtered = data.repos.filter {
        query.isBlank() || it.fullName.contains(query.trim(), ignoreCase = true)
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(
            value = query,
            onValueChange = { query = it },
            singleLine = true,
            placeholder = { Text("Search repositories…") },
            leadingIcon = { Icon(Icons.Filled.Search, contentDescription = null, modifier = Modifier.size(18.dp)) },
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None),
            modifier = Modifier.fillMaxWidth(),
        )
        if (filtered.isEmpty()) {
            Text(
                "No repositories found.",
                style = MaterialTheme.typography.bodySmall,
                color = tertiary,
                modifier = Modifier.padding(vertical = 8.dp),
            )
        }
        filtered.forEach { repo ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier
                    .fillMaxWidth()
                    .glassRow()
                    .clickable { onPick(repo) }
                    .padding(horizontal = 12.dp, vertical = 10.dp),
            ) {
                Icon(Icons.Filled.Code, contentDescription = null, modifier = Modifier.size(14.dp), tint = secondary)
                Spacer(Modifier.width(10.dp))
                Text(
                    repo.fullName,
                    style = MaterialTheme.typography.bodyMedium,
                    fontFamily = FontFamily.Monospace,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                if (repo.isPrivate) {
                    Icon(Icons.Filled.Lock, contentDescription = "Private", modifier = Modifier.size(14.dp), tint = tertiary)
                }
            }
        }
        if (data.hasMore) {
            val context = LocalContext.current
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable {
                        data.installUrl?.let {
                            CustomTabsIntent.Builder().build()
                                .launchUrl(context, android.net.Uri.parse(it))
                        }
                    }
                    .padding(vertical = 6.dp),
            ) {
                Text(
                    "Don't see your repo? Manage repositories on GitHub.",
                    style = MaterialTheme.typography.bodySmall,
                    color = tertiary,
                )
            }
        }
    }
}
