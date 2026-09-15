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
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
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
import com.exponential.app.domain.GithubCopy
import com.exponential.app.domain.isRepoFullName
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassTextField
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassRow

// The Add-repository picker (FEED-42 canonical form — web github-repo-picker.tsx,
// desktop add_repository_dialog.rs, iOS GithubRepoPicker): lists the repos the
// user's linked GitHub accounts grant, and a TAP ADDS through the host's
// [onAdd] (which throws on failure). The sheet stays open while the add runs
// and on failure (the error renders inline), and dismisses on success. When the
// App isn't installed it offers the connect hop in a Chrome Custom Tab; the
// server's post-connect page fires exponential://github-connected, which
// re-fetches (see GithubRepoPickerViewModel). Returning any other way still
// re-queries on lifecycle RESUME.
@Composable
fun GithubRepoPickerSheet(
    accountId: String,
    teamId: String,
    onAdd: suspend (GithubPickerRepo) -> Unit,
    onDismiss: () -> Unit,
    viewModel: GithubRepoPickerViewModel = hiltViewModel(),
) {
    val result by viewModel.result.collectAsStateWithLifecycle()
    val loading by viewModel.loading.collectAsStateWithLifecycle()
    val error by viewModel.error.collectAsStateWithLifecycle()
    val connectError by viewModel.connectError.collectAsStateWithLifecycle()
    val lookupBusy by viewModel.lookupBusy.collectAsStateWithLifecycle()
    val lookupError by viewModel.lookupError.collectAsStateWithLifecycle()
    val adding by viewModel.adding.collectAsStateWithLifecycle()
    val addError by viewModel.addError.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val openUrl: (String) -> Unit = { url ->
        CustomTabsIntent.Builder().build().launchUrl(context, android.net.Uri.parse(url))
    }

    // The VM outlives one presentation (screen-scoped): start each clean.
    LaunchedEffect(Unit) { viewModel.resetTransient() }

    // FEED-30: the footer's "Add by name" field, hoisted like the search so it
    // survives recompositions of the rows.
    var lookupName by remember { mutableStateOf("") }

    // Re-query on every resume so returning from the GitHub Custom Tab (new
    // repos granted) refreshes without a manual tap. The first load isn't a
    // forced refresh; later resumes bypass the server cache.
    var hasLoaded by remember { mutableStateOf(false) }
    LifecycleResumeEffect(accountId, teamId) {
        viewModel.load(accountId, teamId, refresh = hasLoaded)
        hasLoaded = true
        onPauseOrDispose {}
    }

    var query by remember { mutableStateOf("") }
    val listState = rememberLazyListState()
    LaunchedEffect(addError) {
        if (addError != null) listState.animateScrollToItem(0)
    }
    val add: (GithubPickerRepo) -> Unit = { repo ->
        viewModel.add(repo, onAdd) {
            lookupName = ""
            onDismiss()
        }
    }

    GlassSheet(title = GithubCopy.PICKER_TITLE, onDismiss = onDismiss) {
        // Lazy so a hundreds-of-repos account scrolls instead of clipping
        // everything below the sheet fold (EXP-46).
        LazyColumn(
            state = listState,
            modifier = Modifier.fillMaxWidth(),
            contentPadding = PaddingValues(start = 20.dp, end = 20.dp, bottom = 12.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            // FEED-42: a failed add stays in front of the user, inline, at the
            // top (the list scrolls back to it).
            addError?.let { failure ->
                item(key = "add-error") {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        ErrorText(failure.message)
                        val reconnectUrl = result?.let { it.connectUrl ?: it.installUrl }
                        if (failure.forbidden && reconnectUrl != null) {
                            GlassPill(
                                GithubCopy.RECONNECT_GITHUB,
                                icon = ExpIcons.uiRefresh,
                                size = PillSize.Sm,
                                onClick = {
                                    viewModel.clearConnectError()
                                    openUrl(reconnectUrl)
                                },
                            )
                        }
                    }
                }
            }
            val data = result
            when {
                loading && data == null -> item(key = "loading") { LoadingBox() }
                data == null || !data.configured -> item(key = "not-configured") {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        GithubBox(GithubCopy.PICKER_NOT_CONFIGURED)
                        error?.let { ErrorText(it) }
                    }
                }
                !data.installed -> item(key = "connect") {
                    val connectUrl = data.connectUrl ?: data.installUrl
                    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        GithubBox(GithubCopy.PICKER_NOT_INSTALLED)
                        FlowRow(
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                            verticalArrangement = Arrangement.spacedBy(8.dp),
                            itemVerticalAlignment = Alignment.CenterVertically,
                        ) {
                            Button(
                                onClick = {
                                    connectUrl?.let {
                                        viewModel.clearConnectError()
                                        openUrl(it)
                                    }
                                },
                                enabled = connectUrl != null,
                            ) {
                                Icon(ExpIcons.uiGithub, contentDescription = null, modifier = Modifier.size(16.dp))
                                Spacer(Modifier.width(8.dp))
                                Text(GithubCopy.CONNECT_GITHUB)
                            }
                            GlassPill(
                                GithubCopy.I_HAVE_CONNECTED,
                                icon = ExpIcons.uiRefresh,
                                size = PillSize.Sm,
                                onClick = { viewModel.load(accountId, teamId, refresh = true) },
                            )
                        }
                    }
                }
                else -> {
                    installedItems(
                        data = data,
                        query = query,
                        onQueryChange = { query = it },
                        adding = adding,
                        onPick = add,
                        onReconnect = {
                            (data.connectUrl ?: data.installUrl)?.let {
                                viewModel.clearConnectError()
                                openUrl(it)
                            }
                        },
                    )
                    item(key = "footer") {
                        PickerFooter(
                            data = data,
                            loading = loading,
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
                                    openUrl(connectUrl)
                                } else {
                                    viewModel.load(accountId, teamId, refresh = true)
                                }
                            },
                            onInstall = { url ->
                                viewModel.clearConnectError()
                                openUrl(url)
                            },
                            onLookup = { viewModel.lookup(lookupName) { repo -> add(repo) } },
                        )
                    }
                    // A refresh that failed with data on screen (EXP-365).
                    error?.let { message -> item(key = "error") { ErrorText(message) } }
                }
            }
            // A FAILED connect hop's outcome (EXP-390).
            connectError?.let { message -> item(key = "connect-error") { ErrorText(message) } }
        }
    }
}

@Composable
private fun ErrorText(message: String) {
    Text(message, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
}

/** The picker's boxed state container: solid ([dashed] = false) or dashed hairline. */
private fun Modifier.pickerBox(dashed: Boolean, color: Color): Modifier = this
    .fillMaxWidth()
    .drawBehind {
        drawRoundRect(
            color = color,
            cornerRadius = CornerRadius(GlassTokens.RowRadius.toPx()),
            style = Stroke(
                width = 1.dp.toPx(),
                pathEffect = if (dashed) PathEffect.dashPathEffect(floatArrayOf(6f, 6f)) else null,
            ),
        )
    }

@Composable
private fun LoadingBox() {
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        modifier = Modifier
            .pickerBox(dashed = false, color = GlassTokens.StrokeRow)
            .padding(horizontal = 12.dp, vertical = 20.dp),
    ) {
        CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
        Text(GithubCopy.PICKER_LOADING, style = MaterialTheme.typography.bodyMedium, color = secondary)
    }
}

/** Not configured / not installed: a dashed box with the GitHub glyph. */
@Composable
private fun GithubBox(message: String) {
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Row(
        verticalAlignment = Alignment.Top,
        modifier = Modifier
            .pickerBox(dashed = true, color = GlassTokens.StrokeRow)
            .padding(horizontal = 12.dp, vertical = 12.dp),
    ) {
        Icon(
            ExpIcons.uiGithub,
            contentDescription = null,
            modifier = Modifier.padding(top = 2.dp).size(16.dp),
            tint = secondary,
        )
        Spacer(Modifier.width(8.dp))
        Text(message, style = MaterialTheme.typography.bodyMedium, color = secondary)
    }
}

// Installed: the suspended and re-auth banners (INDEPENDENT — both can show),
// then the search + rows (tap adds), or the honest empty state.
private fun LazyListScope.installedItems(
    data: GithubReposResult,
    query: String,
    onQueryChange: (String) -> Unit,
    adding: Boolean,
    onPick: (GithubPickerRepo) -> Unit,
    onReconnect: () -> Unit,
) {
    val suspended = data.installations.filter { it.suspended }
    val reauthLogins = data.installations
        .filter { it.needsReauth && !it.suspended }
        .mapNotNull { it.accountLogin?.takeIf { login -> login.isNotEmpty() } }
    val needsReauth = data.installations.any { it.needsReauth && !it.suspended }
    val empty = data.repos.isEmpty()

    // A suspended installation lists no repos and cannot be fixed by a
    // reconnect (REV2-29) — only unsuspending on GitHub can. No button.
    if (suspended.isNotEmpty()) {
        item(key = "suspended-banner") {
            Row(
                verticalAlignment = Alignment.Top,
                modifier = Modifier
                    .pickerBox(dashed = false, color = DesignTokens.Semantic.Red.copy(alpha = 0.5f))
                    .padding(horizontal = 12.dp, vertical = 10.dp),
            ) {
                Icon(
                    ExpIcons.uiGithub,
                    contentDescription = null,
                    modifier = Modifier.padding(top = 2.dp).size(16.dp),
                    tint = MaterialTheme.colorScheme.error,
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    GithubCopy.pickerSuspended(suspended.map { it.accountLogin }),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
        }
    }
    if (needsReauth) {
        item(key = "reconnect-banner") {
            Column(
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier
                    .pickerBox(dashed = false, color = DesignTokens.Semantic.Yellow.copy(alpha = 0.4f))
                    .padding(horizontal = 12.dp, vertical = 10.dp),
            ) {
                Row(verticalAlignment = Alignment.Top) {
                    Icon(
                        ExpIcons.uiWarning,
                        contentDescription = null,
                        modifier = Modifier.padding(top = 2.dp).size(14.dp),
                        tint = DesignTokens.Semantic.Yellow,
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        GithubCopy.reauthBanner(empty = empty, logins = reauthLogins),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                    )
                }
                GlassPill(
                    GithubCopy.RECONNECT_GITHUB,
                    icon = ExpIcons.uiRefresh,
                    size = PillSize.Sm,
                    enabled = data.connectUrl != null || data.installUrl != null,
                    onClick = onReconnect,
                )
            }
        }
    }
    if (!empty) {
        val filtered = data.repos.filter {
            query.isBlank() || it.fullName.contains(query.trim(), ignoreCase = true)
        }
        item(key = "search") {
            GlassTextField(
                value = query,
                onValueChange = onQueryChange,
                singleLine = true,
                placeholder = GithubCopy.SEARCH_PLACEHOLDER,
                leadingIcon = { Icon(ExpIcons.navSearch, contentDescription = null, modifier = Modifier.size(18.dp)) },
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None),
                modifier = Modifier.fillMaxWidth(),
            )
        }
        if (filtered.isEmpty()) {
            item(key = "no-results") {
                Text(
                    GithubCopy.NO_RESULTS,
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
                    .clickable(enabled = !adding) { onPick(repo) }
                    .padding(horizontal = 12.dp, vertical = 10.dp),
            ) {
                Icon(ExpIcons.uiGithub, contentDescription = null, modifier = Modifier.size(16.dp), tint = secondary)
                Spacer(Modifier.width(10.dp))
                Text(
                    repo.fullName,
                    style = MaterialTheme.typography.bodyMedium,
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
    } else if (!needsReauth && suspended.isEmpty()) {
        item(key = "empty") {
            Text(
                GithubCopy.NONE_GRANTED,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
                modifier = Modifier
                    .pickerBox(dashed = false, color = GlassTokens.StrokeRow)
                    .padding(horizontal = 12.dp, vertical = 20.dp),
            )
        }
    }
}

// FEED-30/42 footer, in a dashed container: the sentence with comma-separated
// Configure links per account, the page-cap note, Refresh + Install on another
// account, and the by-name escape hatch (its error names the real reason).
@Composable
private fun PickerFooter(
    data: GithubReposResult,
    loading: Boolean,
    lookupName: String,
    onLookupNameChange: (String) -> Unit,
    lookupBusy: Boolean,
    lookupError: String?,
    onRefresh: () -> Unit,
    onInstall: (String) -> Unit,
    onLookup: () -> Unit,
) {
    val context = LocalContext.current
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val manageLinks = data.installations.filter { it.manageUrl.isNotEmpty() }
    Column(
        verticalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier
            .pickerBox(dashed = true, color = GlassTokens.StrokeRow)
            .padding(horizontal = 12.dp, vertical = 10.dp),
    ) {
        FlowRow(itemVerticalAlignment = Alignment.CenterVertically) {
            Text(GithubCopy.FOOTER, style = MaterialTheme.typography.bodySmall, color = secondary)
            manageLinks.forEachIndexed { index, inst ->
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.clickable {
                        CustomTabsIntent.Builder().build()
                            .launchUrl(context, android.net.Uri.parse(inst.manageUrl))
                    },
                ) {
                    Text(
                        GithubCopy.installationLabel(inst.accountLogin, inst.installationId),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface,
                    )
                    Spacer(Modifier.width(2.dp))
                    Icon(
                        ExpIcons.uiExternalLink,
                        contentDescription = GithubCopy.CONFIGURE,
                        modifier = Modifier.size(12.dp),
                        tint = MaterialTheme.colorScheme.onSurface,
                    )
                }
                if (index < manageLinks.size - 1) {
                    Text(", ", style = MaterialTheme.typography.bodySmall, color = secondary)
                }
            }
        }
        if (data.hasMore) {
            Text(GithubCopy.CAP_NOTE, style = MaterialTheme.typography.bodySmall, color = secondary)
        }
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            GlassPill(
                GithubCopy.REFRESH,
                icon = ExpIcons.uiRefresh,
                size = PillSize.Sm,
                enabled = !loading,
                onClick = onRefresh,
            )
            // GitHub's account picker (installations/new) — the ONLY way to a
            // second account/org once one is linked.
            data.installUrl?.let { installUrl ->
                GlassPill(
                    GithubCopy.INSTALL_ON_ANOTHER_ACCOUNT,
                    icon = ExpIcons.uiAdd,
                    size = PillSize.Sm,
                    onClick = { onInstall(installUrl) },
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
                placeholder = GithubCopy.LOOKUP_PLACEHOLDER,
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.None,
                    autoCorrectEnabled = false,
                    imeAction = ImeAction.Go,
                ),
                keyboardActions = KeyboardActions(onGo = { onLookup() }),
                textStyle = LocalTextStyle.current.copy(fontFamily = FontFamily.Monospace),
                modifier = Modifier
                    .weight(1f)
                    .semantics { contentDescription = GithubCopy.LOOKUP_A11Y },
            )
            GlassPill(
                GithubCopy.LOOK_UP,
                size = PillSize.Sm,
                enabled = isRepoFullName(lookupName.trim()) && !lookupBusy,
                loading = lookupBusy,
                onClick = onLookup,
            )
        }
        lookupError?.let { ErrorText(it) }
    }
}
