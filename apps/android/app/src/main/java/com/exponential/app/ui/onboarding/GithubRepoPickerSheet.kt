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
import com.exponential.app.data.api.GithubPrerequisite
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

// The Add-repository picker (SLOP-7/SLOP-26 canonical form — web
// github-repo-picker.tsx, desktop add_repository_dialog.rs, iOS
// GithubRepoPicker): lists the repos the VIEWER can push to, LIVE off GitHub,
// and a TAP ADDS through the host's [onAdd] (which throws on failure). The
// sheet stays open while the add runs and on failure (the error renders
// inline), and dismisses on success. When a prerequisite is missing it names
// which one and offers the ONE fix — Connect GitHub (the link-ticket hop in a
// Custom Tab, [GithubRepoPickerViewModel.connectGithub]), Reconnect GitHub
// (the same hop) or Install the app (`installUrl` in a Custom Tab) — plus
// "I’ve done that" to re-list. Every return path re-queries (see the VM) and
// lifecycle RESUME is the fallback.
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
    val connecting by viewModel.connecting.collectAsStateWithLifecycle()
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
                        if (failure.forbidden) {
                            GlassPill(
                                GithubCopy.RECONNECT_GITHUB,
                                icon = ExpIcons.uiRefresh,
                                size = PillSize.Sm,
                                enabled = !connecting,
                                onClick = { viewModel.connectGithub(openUrl) },
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
                // A prerequisite is missing: one sentence naming it, ONE button
                // fixing it, and the "I’ve done that" re-list.
                data.prerequisite == GithubPrerequisite.NOT_LINKED -> item(key = "connect") {
                    PrerequisiteBox(
                        text = GithubCopy.PICKER_NOT_LINKED,
                        fix = GithubCopy.CONNECT_GITHUB,
                        busy = connecting,
                        onFix = { viewModel.connectGithub(openUrl) },
                        onDone = { viewModel.load(accountId, teamId, refresh = true) },
                    )
                }
                data.prerequisite == GithubPrerequisite.EXPIRED -> item(key = "reconnect") {
                    PrerequisiteBox(
                        text = GithubCopy.PICKER_RECONNECT_BANNER,
                        fix = GithubCopy.RECONNECT_GITHUB,
                        busy = connecting,
                        onFix = { viewModel.connectGithub(openUrl) },
                        onDone = { viewModel.load(accountId, teamId, refresh = true) },
                    )
                }
                data.prerequisite == GithubPrerequisite.NOT_INSTALLED -> item(key = "install") {
                    val installUrl = data.installUrl
                    PrerequisiteBox(
                        text = GithubCopy.PICKER_NOT_INSTALLED,
                        fix = GithubCopy.INSTALL_APP,
                        enabled = installUrl != null,
                        onFix = {
                            installUrl?.let {
                                viewModel.clearConnectError()
                                openUrl(it)
                            }
                        },
                        onDone = { viewModel.load(accountId, teamId, refresh = true) },
                    )
                }
                else -> {
                    installedItems(
                        data = data,
                        query = query,
                        onQueryChange = { query = it },
                        adding = adding,
                        onPick = add,
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
                            // The list is live off GitHub: Refresh is a forced
                            // re-list past the server's discovery cache.
                            onRefresh = { viewModel.load(accountId, teamId, refresh = true) },
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
            // A FAILED connect hop's outcome.
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

/**
 * A missing prerequisite (web `repo-picker-prerequisite`): the dashed notice,
 * the ONE primary fix (the app's one primary button, never a system tint) and
 * the "I’ve done that" re-list.
 */
@Composable
private fun PrerequisiteBox(
    text: String,
    fix: String,
    onFix: () -> Unit,
    onDone: () -> Unit,
    busy: Boolean = false,
    enabled: Boolean = true,
) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        GithubBox(text)
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
            itemVerticalAlignment = Alignment.CenterVertically,
        ) {
            Button(onClick = onFix, enabled = enabled && !busy, shape = GlassTokens.ButtonShape) {
                if (busy) {
                    CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                } else {
                    Icon(ExpIcons.uiGithub, contentDescription = null, modifier = Modifier.size(16.dp))
                }
                Spacer(Modifier.width(8.dp))
                Text(fix)
            }
            GlassPill(
                GithubCopy.PICKER_CONNECTED_CHECK,
                icon = ExpIcons.uiRefresh,
                size = PillSize.Sm,
                onClick = onDone,
            )
        }
    }
}

/** Not configured / a prerequisite: a dashed box with the GitHub glyph. */
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

// Installed: the suspended banner (REV2-29), then the search + rows (tap
// adds), or the honest empty state.
private fun LazyListScope.installedItems(
    data: GithubReposResult,
    query: String,
    onQueryChange: (String) -> Unit,
    adding: Boolean,
    onPick: (GithubPickerRepo) -> Unit,
) {
    val suspended = data.installations.filter { it.suspended }
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
                    ExpIcons.uiWarning,
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
    } else if (suspended.isEmpty()) {
        item(key = "empty") {
            Text(
                GithubCopy.NONE_PUSHABLE,
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
            // GitHub's account picker (installations/new) — the way to a
            // second account/org once the first one is installed.
            data.installUrl?.let { installUrl ->
                GlassPill(
                    GithubCopy.INSTALL_ANOTHER,
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
