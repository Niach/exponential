package com.exponential.app.ui.onboarding

import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.GithubPickerRepo
import com.exponential.app.data.api.GithubReposResult
import com.exponential.app.domain.isRepoFullName
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassPillDefaults
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.icons.ExpIcons
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
    val connectError by viewModel.connectError.collectAsStateWithLifecycle()
    val lookupBusy by viewModel.lookupBusy.collectAsStateWithLifecycle()
    val lookupError by viewModel.lookupError.collectAsStateWithLifecycle()
    val context = LocalContext.current

    // FEED-30: the footer's "Add by name" field, hoisted like the search so it
    // survives recompositions of the rows.
    var lookupName by remember { mutableStateOf("") }

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

    GlassSheet(title = "Add repository", onDismiss = onDismiss) {
        // Lazy so a hundreds-of-repos account scrolls instead of clipping
        // everything below the sheet fold (EXP-46) — the shell's fitted cap
        // still lets the short states (loading / connect prompt) wrap.
        LazyColumn(
            modifier = Modifier.fillMaxWidth(),
            contentPadding = PaddingValues(start = 20.dp, end = 20.dp, bottom = 12.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
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
                        onConnectStarted = viewModel::clearConnectError,
                    )
                }
                else -> {
                    when {
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
                        // dead-end. (`needsReauth` is viewer-scoped since EXP-557 — only
                        // YOUR grant-less accounts nudge here; team-wide STALE accounts
                        // get their Disconnect affordance in settings instead.) When SOME
                        // repos are granted but another account needs reauth, the list
                        // stays usable and the reconnect notice rides above it as a banner.
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
                                onConnectStarted = viewModel::clearConnectError,
                            )
                        }
                        // Honestly empty: connected, granted, but no reachable repos.
                        data.repos.isEmpty() -> item(key = "empty") {
                            Text(
                                "None of your connected GitHub accounts grants a repository yet.",
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
                            onConnectStarted = viewModel::clearConnectError,
                        )
                    }
                    // FEED-30: the footer explains the list (the empty one too):
                    // per-account GitHub configure links, Refresh, Install on
                    // another account, the page-cap note and the by-name field.
                    item(key = "footer") {
                        PickerFooter(
                            data = data,
                            lookupName = lookupName,
                            onLookupNameChange = {
                                lookupName = it
                                viewModel.clearLookupError()
                            },
                            lookupBusy = lookupBusy,
                            lookupError = lookupError,
                            // On OAuth instances the list IS the viewer's grant
                            // snapshot, which only the re-auth (or the webhook)
                            // rewrites — Refresh runs the re-auth hop there and a
                            // forced re-list where there is no OAuth.
                            onRefresh = {
                                val connectUrl = data.connectUrl
                                if (connectUrl != null) {
                                    viewModel.clearConnectError()
                                    CustomTabsIntent.Builder().build()
                                        .launchUrl(context, android.net.Uri.parse(connectUrl))
                                } else {
                                    viewModel.load(accountId, teamId, refresh = true)
                                }
                            },
                            onLookup = {
                                viewModel.lookup(lookupName) { repo ->
                                    lookupName = ""
                                    onPick(repo)
                                    onDismiss()
                                }
                            },
                            onConnectStarted = viewModel::clearConnectError,
                        )
                    }
                }
            }
            // A FAILED connect hop's outcome (EXP-390): the deep link's error
            // slug used to be dropped, making every failure a silent no-op.
            if (connectError != null) {
                item(key = "connect-error") {
                    Text(
                        connectError ?: "",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
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
        "GitHub suspended the Exponential app for $names. Its repositories " +
            "can't be connected until you unsuspend it on GitHub.",
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.error,
        modifier = Modifier.padding(vertical = 8.dp),
    )
}

// FEED-30 (web github-repo-picker.tsx footer): the list explains itself. A
// missing repo is (almost) always an installation whose repo selection doesn't
// include it, or a repo on an account that isn't installed at all — say so,
// link the exact GitHub page per account, offer the two fixes, and the by-name
// escape hatch backed by integrations.github.lookupRepo (its error names the
// real reason). Rendered in EVERY installed state, the empty one included.
@Composable
private fun PickerFooter(
    data: GithubReposResult,
    lookupName: String,
    onLookupNameChange: (String) -> Unit,
    lookupBusy: Boolean,
    lookupError: String?,
    onRefresh: () -> Unit,
    onLookup: () -> Unit,
    onConnectStarted: () -> Unit,
) {
    val context = LocalContext.current
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val manageLinks = data.installations.filter { it.manageUrl.isNotEmpty() }
    Column(
        verticalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier
            .fillMaxWidth()
            .glassRow()
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        Text(
            "Only repositories your GitHub installation grants appear here. " +
                "Missing one? Grant it on GitHub, then refresh.",
            style = MaterialTheme.typography.bodySmall,
            color = secondary,
        )
        if (manageLinks.isNotEmpty()) {
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                manageLinks.forEach { inst ->
                    GlassPill(
                        inst.accountLogin ?: "installation",
                        size = PillSize.Sm,
                        trailing = {
                            Icon(
                                ExpIcons.uiExternalLink,
                                contentDescription = "Configure on GitHub",
                                modifier = Modifier.size(GlassPillDefaults.SmGlyphSize),
                            )
                        },
                        onClick = {
                            CustomTabsIntent.Builder().build()
                                .launchUrl(context, android.net.Uri.parse(inst.manageUrl))
                        },
                    )
                }
            }
        }
        if (data.hasMore) {
            Text(
                "Showing the first 500 repositories per account — use the field below for the rest.",
                style = MaterialTheme.typography.bodySmall,
                color = secondary,
            )
        }
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            GlassPill("Refresh", icon = ExpIcons.uiRefresh, onClick = onRefresh)
            // GitHub's account picker (installations/new) — the ONLY way to a
            // second account/org once one is linked (the OAuth hop just
            // re-links what the viewer already controls).
            val installUrl = data.installUrl
            if (installUrl != null) {
                GlassPill(
                    "Install on another account",
                    icon = ExpIcons.uiAdd,
                    onClick = {
                        onConnectStarted()
                        CustomTabsIntent.Builder().build()
                            .launchUrl(context, android.net.Uri.parse(installUrl))
                    },
                )
            }
        }
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            GlassTextField(
                value = lookupName,
                onValueChange = onLookupNameChange,
                singleLine = true,
                placeholder = "owner/name",
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.None,
                    autoCorrectEnabled = false,
                    imeAction = ImeAction.Go,
                ),
                keyboardActions = KeyboardActions(onGo = { onLookup() }),
                textStyle = LocalTextStyle.current.copy(fontFamily = FontFamily.Monospace),
                modifier = Modifier.weight(1f),
            )
            GlassPill(
                "Look up",
                enabled = isRepoFullName(lookupName.trim()) && !lookupBusy,
                loading = lookupBusy,
                onClick = onLookup,
            )
        }
        if (lookupError != null) {
            Text(
                lookupError,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.error,
            )
        }
    }
}

// Connect/reconnect prompt: not-installed and the needs-reauth/empty-grant
// states share the same Custom-Tab hop, differing in copy and in whether the
// manual "I've connected" escape hatch is offered ([onRefresh]).
@Composable
private fun ConnectPrompt(
    data: GithubReposResult,
    message: String,
    buttonLabel: String,
    buttonIcon: ImageVector,
    onRefresh: (() -> Unit)?,
    // A fresh attempt clears the previous hop's failure message (EXP-390).
    onConnectStarted: () -> Unit = {},
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
                    onConnectStarted()
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
                Text("I've connected")
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
    onConnectStarted: () -> Unit = {},
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
                        " to refresh. Repos created or shared with you since " +
                        "your last connect won't appear until you do.",
                    style = MaterialTheme.typography.bodySmall,
                    color = secondary,
                )
                OutlinedButton(
                    onClick = {
                        reconnectUrl?.let {
                            onConnectStarted()
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
        GlassTextField(
            value = query,
            onValueChange = onQueryChange,
            singleLine = true,
            placeholder = "Search repositories…",
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
