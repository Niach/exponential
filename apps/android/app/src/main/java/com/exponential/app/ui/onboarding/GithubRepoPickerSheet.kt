package com.exponential.app.ui.onboarding

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
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
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.graphics.vector.ImageVector
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
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

// Installed-repo picker (web github-repo-picker.tsx / iOS GithubRepoPicker): lists
// the repos the user's GitHub App is installed on and returns the chosen one. When
// the App isn't installed it offers an inline connect that opens the (mobile-marked)
// install URL in a Chrome Custom Tab; the server's post-install page fires
// exponential://github-connected, which closes the tab, returns here, and re-fetches
// (see GithubRepoPickerViewModel). Returning any other way (older server, tab
// dismissed by hand) still re-queries on lifecycle RESUME. The repo is connected
// server-side by `boards.create`'s `repository: { fullName }` path.
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GithubRepoPickerSheet(
    accountId: String,
    teamId: String,
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
    LifecycleResumeEffect(accountId, teamId) {
        viewModel.load(accountId, teamId, refresh = hasLoaded)
        hasLoaded = true
        onPauseOrDispose {}
    }

    // Search state is hoisted above the LazyColumn so the field (a header item)
    // survives recompositions of the repo rows.
    var query by remember { mutableStateOf("") }

    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
        containerColor = GlassTokens.BackgroundBottom,
    ) {
        // Lazy + height-capped so a hundreds-of-repos account scrolls instead of
        // clipping everything below the sheet fold (EXP-46) — heightIn(max) still
        // lets the short states (loading / connect prompt) wrap their content.
        val maxSheetHeight = (LocalConfiguration.current.screenHeightDp * 0.85f).dp
        LazyColumn(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(max = maxSheetHeight),
            contentPadding = PaddingValues(start = 20.dp, end = 20.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item(key = "title") {
                Text(
                    "Add repository",
                    style = MaterialTheme.typography.titleMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            }
            val data = result
            when {
                loading && data == null -> item(key = "loading") { LoadingRow() }
                data == null || !data.configured -> item(key = "not-configured") { NotConfigured() }
                !data.installed -> item(key = "connect") {
                    ConnectPrompt(
                        data = data,
                        message = "Connect the Exponential GitHub App to pick a repository. " +
                            "You'll come right back here.",
                        buttonLabel = "Connect GitHub",
                        buttonIcon = ExpIcons.uiGithub,
                        onRefresh = { viewModel.load(accountId, teamId, refresh = true) },
                    )
                }
                // A suspended installation lists no repos AND cannot be fixed by
                // a reconnect (REV2-29) — only unsuspending on GitHub can. Say
                // so instead of nudging the wrong fix (EXP-365).
                data.repos.isEmpty() && data.installations.any { it.suspended } -> item(key = "suspended") {
                    SuspendedNotice(data)
                }
                // Grant-scoped repos (see GithubInstallation): a pre-grant link —
                // or one whose grants were revoked — is `installed` but returns no
                // repos until the user re-runs the OAuth connect, so an empty list
                // gets the full reconnect prompt instead of a "No repositories"
                // dead-end. (A stale single account always lands here.) When SOME
                // repos are granted but another account is stale, the list stays
                // usable and the reconnect notice rides above it as a banner.
                data.repos.isEmpty() && data.installations.any { it.needsReauth && !it.suspended } -> item(key = "reconnect") {
                    ConnectPrompt(
                        data = data,
                        message = "Reconnect GitHub to load the repositories you can access" +
                            reauthAccountSuffix(data) + ".",
                        buttonLabel = "Reconnect GitHub",
                        buttonIcon = ExpIcons.uiRefresh,
                        // The reconnect hop returns here and re-queries by itself;
                        // only the not-installed state keeps a manual escape hatch.
                        onRefresh = null,
                    )
                }
                // Honestly empty: connected, granted, but no reachable repos.
                data.repos.isEmpty() -> item(key = "empty") {
                    Text(
                        "No repositories found for your connected GitHub accounts.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                        modifier = Modifier.padding(vertical = 8.dp),
                    )
                }
                else -> installedRepoItems(
                    data = data,
                    query = query,
                    onQueryChange = { query = it },
                    onPick = { onPick(it); onDismiss() },
                )
            }
            // Surfaced even with stale data on screen (EXP-365): the re-query
            // that runs on every return from the GitHub hop used to fail
            // silently, leaving an unexplained stale/short list.
            if (error != null) {
                item(key = "error") {
                    Text(
                        if (result == null) error ?: "" else "Couldn't refresh: ${error ?: ""}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
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

// " from a, b" when the stale accounts are known — names make the fix
// actionable when several accounts are linked (EXP-365).
private fun reauthAccountSuffix(data: GithubReposResult, preposition: String = "from"): String {
    val names = data.installations
        .filter { it.needsReauth && !it.suspended }
        .mapNotNull { it.accountLogin }
    return if (names.isEmpty()) "" else " $preposition ${names.joinToString(", ")}"
}

// GitHub-side App suspension (REV2-29): unsuspend on GitHub is the only fix.
@Composable
private fun SuspendedNotice(data: GithubReposResult) {
    val names = data.installations
        .filter { it.suspended }
        .joinToString(", ") { it.accountLogin ?: "a connected account" }
    Text(
        "GitHub suspended the Exponential app for $names — its repositories " +
            "can't be connected until you unsuspend it on GitHub.",
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.error,
        modifier = Modifier.padding(vertical = 8.dp),
    )
}

// Connect/reconnect prompt: not-installed and the needs-reauth/empty-grant
// states share the same Custom-Tab hop, differing in copy and in whether the
// manual "I've connected — refresh" escape hatch is offered ([onRefresh]).
@Composable
private fun ConnectPrompt(
    data: GithubReposResult,
    message: String,
    buttonLabel: String,
    buttonIcon: ImageVector,
    onRefresh: (() -> Unit)?,
) {
    val context = LocalContext.current
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(
            message,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        // Prefer the single-consent OAuth connect URL that claims the account for
        // the team AND captures the repo grants (the install page doesn't);
        // fall back to the App install page on older servers.
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
            Icon(buttonIcon, contentDescription = null, modifier = Modifier.size(16.dp))
            Spacer(Modifier.width(8.dp))
            Text(buttonLabel)
        }
        if (onRefresh != null) {
            OutlinedButton(onClick = onRefresh, modifier = Modifier.fillMaxWidth()) {
                Icon(ExpIcons.uiRefresh, contentDescription = null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.width(8.dp))
                Text("I've connected — refresh")
            }
        }
    }
}

// The installed-repos list as LazyColumn items (EXP-46: repo lists can run to
// hundreds of rows, so the rows are lazy and the sheet scrolls): reconnect
// banner + search field as header items, then one item per filtered repo. The
// old refresh/manage footer links are gone (EXP-329) — reconnecting is the
// banner's job and repo access is managed on GitHub itself.
private fun LazyListScope.installedRepoItems(
    data: GithubReposResult,
    query: String,
    onQueryChange: (String) -> Unit,
    onPick: (GithubPickerRepo) -> Unit,
) {
    val filtered = data.repos.filter {
        query.isBlank() || it.fullName.contains(query.trim(), ignoreCase = true)
    }
    // A suspended account contributes zero repos while the rest of the list
    // stays usable — explain why those repos are missing (EXP-365).
    if (data.installations.any { it.suspended }) {
        item(key = "suspended-banner") { SuspendedNotice(data) }
    }
    // Mixed-grant 2+-account case: some repos are granted (so the list stays
    // usable) but another linked account is stale — a small banner nudges a
    // reconnect without hiding the selectable repos.
    if (data.installations.any { it.needsReauth && !it.suspended }) {
        item(key = "reconnect-banner") {
            val context = LocalContext.current
            val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
            val reconnectUrl = data.connectUrl ?: data.installUrl
            Column(
                verticalArrangement = Arrangement.spacedBy(10.dp),
                modifier = Modifier
                    .fillMaxWidth()
                    .glassRow()
                    .padding(horizontal = 12.dp, vertical = 10.dp),
            ) {
                Text(
                    "Reconnect GitHub" + reauthAccountSuffix(data, preposition = "for") +
                        " to refresh — repos created or shared with you since " +
                        "your last connect won't appear until you do.",
                    style = MaterialTheme.typography.bodySmall,
                    color = secondary,
                )
                OutlinedButton(
                    onClick = {
                        reconnectUrl?.let {
                            CustomTabsIntent.Builder().build()
                                .launchUrl(context, android.net.Uri.parse(it))
                        }
                    },
                    enabled = reconnectUrl != null,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Icon(ExpIcons.uiRefresh, contentDescription = null, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(8.dp))
                    Text("Reconnect GitHub")
                }
            }
        }
    }
    item(key = "search") {
        OutlinedTextField(
            value = query,
            onValueChange = onQueryChange,
            singleLine = true,
            placeholder = { Text("Search repositories…") },
            leadingIcon = { Icon(ExpIcons.navSearch, contentDescription = null, modifier = Modifier.size(18.dp)) },
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None),
            modifier = Modifier.fillMaxWidth(),
        )
    }
    if (filtered.isEmpty()) {
        item(key = "no-results") {
            Text(
                "No repositories found.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                modifier = Modifier.padding(vertical = 8.dp),
            )
        }
    }
    items(filtered, key = { it.fullName }) { repo ->
        val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
        val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier
                .fillMaxWidth()
                .glassRow()
                .clickable { onPick(repo) }
                .padding(horizontal = 12.dp, vertical = 10.dp),
        ) {
            Icon(ExpIcons.uiRepository, contentDescription = null, modifier = Modifier.size(14.dp), tint = secondary)
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
                Icon(ExpIcons.uiPrivate, contentDescription = "Private", modifier = Modifier.size(14.dp), tint = tertiary)
            }
        }
    }
}
