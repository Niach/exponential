package com.exponential.app.ui.repo

import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CenterAlignedTopAppBar
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import at.exponential.ui.primitives.EmptyStateSize
import com.exponential.app.data.api.RepoSettings
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.db.PullRequestEntity
import com.exponential.app.domain.MergeTrain
import com.exponential.app.domain.RepoCi
import com.exponential.app.domain.RepoPage
import com.exponential.app.ui.components.rememberTickingNow
import com.exponential.app.ui.components.EmptyState
import com.exponential.app.ui.components.FailedActionToast
import com.exponential.app.ui.components.GlassAlert
import com.exponential.app.ui.components.GlassAlertAction
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.GlassSegmentedControl
import com.exponential.app.ui.components.GroupDivider
import com.exponential.app.ui.components.PickerRow
import com.exponential.app.ui.components.PrList
import com.exponential.app.ui.components.PrListRow
import com.exponential.app.ui.components.PillMode
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.SwitchRow
import com.exponential.app.ui.components.TextFieldRow
import com.exponential.app.ui.components.TopBarBackButton
import com.exponential.app.ui.components.TopBarTitle
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.ExpMono
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup
import com.exponential.app.ui.work.MergeStatusPill
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/** The repo page's root test tag (`repo-page`). */
const val REPO_PAGE_TEST_ID = "repo-page"

/** repo-page.json `copy.checks[state].tone` in the theme's semantic colours. */
internal fun checksDotColor(state: String): Color = when (RepoPage.Copy.CHECKS[state]?.tone) {
    "green" -> DesignTokens.Semantic.Green
    "amber" -> DesignTokens.Semantic.Yellow
    "red" -> DesignTokens.Semantic.Red
    else -> DesignTokens.Semantic.Neutral
}

/** The provider `Badge` (repo-page.json `copy.provider`). */
@Composable
internal fun ProviderPill(provider: String?) {
    GlassPill(RepoPage.providerLabel(provider), size = PillSize.Sm, modifier = Modifier.testTag("repo-provider"))
}

/** A checks rollup's `Chip{dot}`: its label, its dot in the semantic tone. */
@Composable
internal fun ChecksPill(state: String, dotOnly: Boolean = false) {
    val label = RepoPage.Copy.CHECKS[state]?.label ?: state
    GlassPill(
        if (dotOnly) "" else label,
        size = PillSize.Sm,
        dot = checksDotColor(state),
        modifier = Modifier.testTag("repo-checks").semantics { contentDescription = label },
    )
}

/**
 * #EXP-1616 THE REPO PAGE (repo-page.json, view `repo`): the AppBar (back,
 * the repo's name, its provider pill) over the pill tabs Pull requests ·
 * Runs · Branches · Settings.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RepoPageScreen(
    onBack: () -> Unit,
    onOpenRun: (String) -> Unit = {},
    viewModel: RepoPageViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val prs by viewModel.prs.collectAsStateWithLifecycle()
    val isOwner by viewModel.isOwner.collectAsStateWithLifecycle()
    val failure by viewModel.failure.collectAsStateWithLifecycle()
    val isManager by viewModel.isManager.collectAsStateWithLifecycle()
    val ciRuns by viewModel.ciRuns.collectAsStateWithLifecycle()
    val ciJobs by viewModel.ciJobs.collectAsStateWithLifecycle()
    FailedActionToast(failure, onShown = viewModel::clearFailure)
    var tab by rememberSaveable { mutableStateOf(viewModel.initialTab) }
    val repo = state.repo

    Scaffold(
        containerColor = Color.Transparent,
        topBar = {
            CenterAlignedTopAppBar(
                title = {
                    // The bar holds the title only (repo-page.json): the
                    // provider lives in the Settings tab.
                    TopBarTitle(repo?.fullName.orEmpty())
                },
                navigationIcon = { TopBarBackButton(onClick = onBack) },
                colors = TopAppBarDefaults.centerAlignedTopAppBarColors(containerColor = Color.Transparent),
            )
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .padding(padding)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(16.dp)
                .testTag(REPO_PAGE_TEST_ID),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            GlassSegmentedControl(
                options = RepoPage.TABS.map { it.id },
                selected = tab,
                label = { id -> RepoPage.TABS.firstOrNull { it.id == id }?.label ?: id },
                onSelect = { tab = it },
                testTag = { id -> "repo-tab-$id" },
            )
            when {
                !state.loaded -> Column(Modifier.fillMaxWidth().glassGroup()) { SkeletonRows(rowHeight = 52.dp) }
                repo == null -> EmptyState(
                    title = "Repository not found",
                    icon = ExpIcons.settingsRepositories,
                    size = EmptyStateSize.List,
                )
                tab == RepoPage.TAB_PULLS -> PullsTab(prs)
                tab == RepoPage.TAB_RUNS -> {
                    // A pending run's duration ticks.
                    val now = rememberTickingNow(ciRuns.any { it.state == "pending" && it.startedAt != null })
                    val rows = remember(ciRuns, ciJobs, now) { RepoCi.runRows(ciRuns, ciJobs, now) }
                    RepoRunsTab(rows, onOpenRun)
                }
                tab == RepoPage.TAB_BRANCHES -> BranchesTab(repo, state.branches, prs)
                else -> SettingsTab(
                    repo = repo,
                    settings = state.settings,
                    branches = state.branches,
                    isOwner = isOwner,
                    busy = state.busy,
                    onSetDefaultBranch = viewModel::setDefaultBranch,
                    onSetMirror = viewModel::setMirror,
                    onRestore = viewModel::restore,
                    ci = {
                        RepoCiSection(
                            provider = repo.provider,
                            runs = ciRuns,
                            jobs = ciJobs,
                            ci = state.ci,
                            canManage = isManager,
                            onSetRequired = viewModel::setRequiredChecks,
                            onSetSwitch = viewModel::setCiSwitch,
                            onSaveSecret = viewModel::setSecret,
                            onDeleteSecret = viewModel::deleteSecret,
                        )
                    },
                )
            }
        }
    }
}

/** PULL REQUESTS: `repoPrTree` rows, nested with tree guides; each row's
 *  meta = the checks chip, then the merge-train status pill. */
@Composable
private fun PullsTab(prs: List<PullRequestEntity>) {
    val byId = remember(prs) { prs.associateBy { it.id } }
    val rows = remember(prs) { RepoPage.repoPrTree(prs.map { it.toRepoPr() }) }
    if (rows.isEmpty()) {
        EmptyState(title = RepoPage.Copy.Pulls.EMPTY, size = EmptyStateSize.List)
        return
    }
    val trainIds = remember(prs) { prs.map { it.mergeTrainId } }
    PrList(
        modifier = Modifier.glassGroup().testTag("repo-pulls"),
        rows = rows.map { row ->
            val pr = byId.getValue(row.id)
            PrListRow(
                key = row.id,
                title = row.title,
                identifier = row.identifier,
                depth = row.depth,
                subtitle = row.subtitle,
                trailing = {
                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                        // Phones: the dot alone; its label is the description.
                        row.checks?.let { ChecksPill(it, dotOnly = true) }
                        MergeStatusPill(
                            view = MergeTrain.statusPill(
                                MergeTrain.StatusPillInput(
                                    state = MergeTrain.prMergeState(pr.state, pr.mergeState),
                                    wait = pr.mergeWait,
                                    position = pr.mergePosition,
                                    batchSize = MergeTrain.batchSize(pr.mergeTrainId, trainIds),
                                    holdReason = pr.mergeHold,
                                    stack = false,
                                    checks = 0,
                                ),
                            ),
                            onClick = null,
                        )
                    }
                },
            )
        },
    )
}

/** BRANCHES: `repoBranches` rows: the name (mono), the short sha, the live
 *  PR's `#n`, the Default pill. */
@Composable
private fun BranchesTab(repo: TeamRepo, branches: List<Pair<String, String>>?, prs: List<PullRequestEntity>) {
    if (branches == null) {
        Column(Modifier.fillMaxWidth().glassGroup()) { SkeletonRows(rowHeight = 52.dp) }
        return
    }
    val rows = remember(branches, prs, repo.defaultBranch) {
        RepoPage.repoBranches(branches, repo.defaultBranch, prs.map { it.toRepoPr() })
    }
    if (rows.isEmpty()) {
        EmptyState(title = RepoPage.Copy.Branches.EMPTY, size = EmptyStateSize.List)
        return
    }
    val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)
    Column(Modifier.fillMaxWidth().glassGroup().testTag("repo-branches")) {
        rows.forEachIndexed { index, row ->
            if (index > 0) GroupDivider()
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Column(Modifier.weight(1f)) {
                    Text(
                        row.name,
                        style = MaterialTheme.typography.bodyMedium,
                        fontFamily = ExpMono,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    if (row.sha.isNotEmpty()) {
                        Text(row.sha, style = MaterialTheme.typography.labelSmall, fontFamily = ExpMono, color = tertiary)
                    }
                }
                row.prNumber?.let {
                    Text("#$it", style = MaterialTheme.typography.labelMedium, color = tertiary)
                }
                if (row.isDefault) GlassPill(RepoPage.Copy.Branches.DEFAULT, size = PillSize.Sm)
            }
        }
    }
}

/** SETTINGS: provider, default branch (owners), clone URL, backups newest
 *  first with Restore… (owners), the mirror (owners). */
@Composable
private fun SettingsTab(
    repo: TeamRepo,
    settings: RepoSettings?,
    branches: List<Pair<String, String>>?,
    isOwner: Boolean,
    busy: Boolean,
    onSetDefaultBranch: (String) -> Unit,
    onSetMirror: (String?, Boolean) -> Unit,
    onRestore: (String) -> Unit,
    /** #EXP-1617: the CI section, after the repo's own rows. */
    ci: @Composable () -> Unit = {},
) {
    val ecode = repo.provider == RepoPage.PROVIDER_ECODE
    if (ecode && settings == null) {
        Column(Modifier.fillMaxWidth().glassGroup()) { SkeletonRows(rowHeight = 52.dp) }
        return
    }
    val clipboard = LocalClipboardManager.current
    val focus = LocalFocusManager.current
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    var restoreId by remember { mutableStateOf<String?>(null) }
    val defaultBranch = settings?.defaultBranch ?: repo.defaultBranch
    val options = branches?.map { it.first }?.takeIf { it.isNotEmpty() } ?: listOf(defaultBranch)

    Column(Modifier.testTag("repo-settings"), verticalArrangement = Arrangement.spacedBy(20.dp)) {
        Column(Modifier.fillMaxWidth().glassGroup()) {
            Row(
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(RepoPage.Copy.Settings.PROVIDER, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
                ProviderPill(repo.provider)
            }
            GroupDivider()
            PickerRow(
                label = RepoPage.Copy.Settings.DEFAULT_BRANCH,
                value = defaultBranch,
                options = options,
                selected = defaultBranch,
                optionLabel = { it },
                onSelect = { if (it != defaultBranch) onSetDefaultBranch(it) },
                enabled = isOwner && ecode && !busy,
            )
            val remote = settings?.remoteUrl ?: repo.remoteUrl?.takeIf { ecode }
            if (remote != null) {
                GroupDivider()
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { clipboard.setText(AnnotatedString(remote)) }
                        .padding(horizontal = 16.dp, vertical = 12.dp)
                        .testTag("repo-clone-url"),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    Text(RepoPage.Copy.Settings.CLONE_URL, style = MaterialTheme.typography.bodyLarge)
                    Text(
                        remote,
                        style = MaterialTheme.typography.bodySmall,
                        fontFamily = ExpMono,
                        color = secondary,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                        textAlign = androidx.compose.ui.text.style.TextAlign.End,
                    )
                }
            }
        }

        if (ecode && settings != null) {
            Column {
                SectionHeader(RepoPage.Copy.Settings.BACKUPS)
                if (settings.backups.isEmpty()) {
                    EmptyState(title = RepoPage.Copy.Settings.NO_BACKUPS, size = EmptyStateSize.List)
                } else {
                    Column(Modifier.fillMaxWidth().glassGroup().testTag("repo-backups")) {
                        settings.backups.sortedByDescending { it.createdAt }.forEachIndexed { index, backup ->
                            if (index > 0) GroupDivider()
                            Row(
                                modifier = Modifier.fillMaxWidth().padding(start = 16.dp, end = 12.dp, top = 8.dp, bottom = 8.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                            ) {
                                Text(
                                    backupDate(backup.createdAt),
                                    style = MaterialTheme.typography.bodyMedium,
                                    modifier = Modifier.weight(1f),
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                Text(formatBytes(backup.bytes), style = MaterialTheme.typography.labelMedium, color = secondary)
                                if (isOwner) {
                                    GlassPill(
                                        RepoPage.Copy.Settings.RESTORE,
                                        size = PillSize.Sm,
                                        mode = PillMode.Action,
                                        enabled = !busy,
                                        onClick = { restoreId = backup.id },
                                    )
                                }
                            }
                        }
                    }
                }
            }
            Column {
                SectionHeader(RepoPage.Copy.Settings.MIRROR)
                var mirrorUrl by remember(settings.mirrorUrl) { mutableStateOf(settings.mirrorUrl.orEmpty()) }
                val saveUrl = {
                    val trimmed = mirrorUrl.trim()
                    if (trimmed != settings.mirrorUrl.orEmpty()) onSetMirror(trimmed.ifEmpty { null }, settings.mirrorEnabled)
                }
                Column(Modifier.fillMaxWidth().glassGroup()) {
                    SwitchRow(
                        title = RepoPage.Copy.Add.MIRROR,
                        checked = settings.mirrorEnabled,
                        onCheckedChange = { enabled -> onSetMirror(mirrorUrl.trim().ifEmpty { null }, enabled) },
                        enabled = isOwner && !busy && mirrorUrl.isNotBlank(),
                        modifier = Modifier.testTag("repo-mirror"),
                    )
                    GroupDivider()
                    TextFieldRow(
                        label = RepoPage.Copy.Settings.MIRROR_URL,
                        value = mirrorUrl,
                        onValueChange = { mirrorUrl = it.take(400) },
                        placeholder = "https://github.com/owner/name",
                        enabled = isOwner && !busy,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Done),
                        keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }),
                        onFocusLost = saveUrl,
                        textStyle = TextStyle(fontFamily = ExpMono),
                    )
                }
            }
        }
        ci()
    }

    restoreId?.let { backupId ->
        GlassAlert(
            title = RepoPage.Copy.Settings.RESTORE_TITLE,
            trailing = listOf(
                GlassAlertAction("Cancel", onClick = { restoreId = null }),
                GlassAlertAction(
                    RepoPage.Copy.Settings.RESTORE_CONFIRM,
                    onClick = {
                        restoreId = null
                        onRestore(backupId)
                    },
                    destructive = true,
                    testTag = "repo-restore-confirm",
                ),
            ),
            onDismiss = { restoreId = null },
        )
    }
}

private fun backupDate(iso: String): String = runCatching {
    DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM, FormatStyle.SHORT)
        .format(Instant.parse(iso).atZone(ZoneId.systemDefault()))
}.getOrDefault(iso)

private fun formatBytes(bytes: Long): String = when {
    bytes < 1024 -> "$bytes B"
    bytes < 1024 * 1024 -> String.format(java.util.Locale.US, "%.1f KB", bytes / 1024.0)
    else -> String.format(java.util.Locale.US, "%.1f MB", bytes / (1024.0 * 1024.0))
}
