package com.exponential.app.ui.components

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import com.exponential.app.data.api.BoardRepositoryChoice
import com.exponential.app.data.api.RepositoriesApi
import com.exponential.app.data.api.TeamRepo
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.domain.BoardRepoLabel
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.onboarding.GithubRepoPickerSheet
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.glassGroup
import dagger.hilt.android.lifecycle.HiltViewModel
import javax.inject.Inject
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * EXP-712: the ONE explanatory line under the board form's repository + branch
 * block. Byte-identical on web, iOS, Android and desktop — the fields are
 * otherwise self-explanatory, so nothing else is said there.
 * `apps/web/src/lib/board-copy.test.ts` greps this tree for the literal.
 */
const val BOARD_REPO_NOTE = "Coding sessions start from here."

private const val NO_REPOSITORY = "No repository"

/**
 * The branch lists behind a board's Branch picker (EXP-712), keyed by
 * repository id so the two hosts (create form, board settings) share one cache
 * and a repo switch never shows the previous repo's branches. Server-only data
 * — `repositories.listBranches` reads GitHub through the installation token.
 */
@HiltViewModel
class BoardBranchesViewModel @Inject constructor(
    private val repositoriesApi: RepositoriesApi,
) : ViewModel() {

    data class Branches(
        val loading: Boolean = false,
        val branches: List<String> = emptyList(),
        val error: String? = null,
    )

    private val _byRepo = MutableStateFlow<Map<String, Branches>>(emptyMap())
    val byRepo: StateFlow<Map<String, Branches>> = _byRepo.asStateFlow()

    /** Loads once per repo; a previous FAILURE is retried, a success isn't refetched. */
    fun load(accountId: String, repositoryId: String, force: Boolean = false) {
        val current = _byRepo.value[repositoryId]
        if (!force && current != null && (current.loading || current.error == null)) return
        put(repositoryId, Branches(loading = true))
        viewModelScope.launch {
            runCatching { repositoriesApi.listBranches(accountId, repositoryId) }
                .onSuccess { put(repositoryId, Branches(branches = it)) }
                .onFailure {
                    put(repositoryId, Branches(error = trpcErrorMessage(it, "Couldn't load branches")))
                }
        }
    }

    private fun put(repositoryId: String, value: Branches) {
        _byRepo.value = _byRepo.value + (repositoryId to value)
    }
}

/**
 * A board's repository + branch block (EXP-712) — web `board-repo-field.tsx`,
 * shared by the create-board form and the per-board settings sheet.
 *
 * The repository behaves like ONE select: the current value on a field-shaped
 * row, and a sheet offering "No repository", the team's connected repos, and a
 * trailing "Connect another repository…" that opens the installed-repos
 * picker. A repo picked there becomes the selection immediately; whether that
 * CONNECTS it (settings) or waits for submit (create, via `boards.create`'s
 * inline `{ fullName }` path) is the host's call.
 *
 * Below it — only once a repo is selected — the branch its coding sessions
 * start from: the repo's default unless the board pins another. A not-yet-
 * connected repo has no id to list branches from, so that case is a plain
 * field placeheld with the repo's GitHub default.
 *
 * Nothing here persists: the host owns the writes.
 */
@Composable
fun BoardRepoField(
    accountId: String,
    teamId: String,
    repos: List<TeamRepo>,
    loading: Boolean,
    selection: BoardRepositoryChoice?,
    onSelect: (BoardRepositoryChoice?) -> Unit,
    branch: String?,
    onBranchChange: (String?) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    error: String? = null,
    onRetry: (() -> Unit)? = null,
    // FEED-32: the host's one-shot re-list for a Registry selection [repos]
    // doesn't know is in flight — the field reads "Loading repository…"
    // instead of "Repository unavailable" meanwhile.
    resolving: Boolean = false,
    viewModel: BoardBranchesViewModel = hiltViewModel(),
) {
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    val tertiary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary)

    var repoSheet by remember { mutableStateOf(false) }
    var branchSheet by remember { mutableStateOf(false) }
    var showPicker by remember { mutableStateOf(false) }

    val registry = (selection as? BoardRepositoryChoice.Registry)
        ?.let { picked -> repos.firstOrNull { it.id == picked.repositoryId } }
    val inline = selection as? BoardRepositoryChoice.Inline
    // The branch that means "follow the repo": `repositories.list` already
    // folds the team's pin into `defaultBranch`; a not-yet-connected repo only
    // knows GitHub's default.
    val repoDefault = inline?.defaultBranch ?: registry?.defaultBranch
    val selectedName = inline?.fullName ?: registry?.fullName
    val hasRepos = repos.isNotEmpty() || inline != null

    val branchState by viewModel.byRepo.collectAsStateWithLifecycle()
    LaunchedEffect(registry?.id) {
        registry?.id?.let { viewModel.load(accountId, it) }
    }

    Column(modifier = modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        // EXP-862: PICKER ROWS in one glass group — label inside the row, value
        // and chevron at its trailing edge, like every other field of the board
        // form (web `GlassPickerRow`, desktop's board form, iOS the same).
        Column(modifier = Modifier.fillMaxWidth().glassGroup()) {
            // FEED-32: never blank and never "No repository" for a linked
            // board — an unresolved Registry id reads "Loading repository…"
            // while a list is in flight, "Repository unavailable" after.
            SelectRow(
                label = "Repository",
                value = BoardRepoLabel.trigger(
                    selectedName = selectedName,
                    repositoryId = (selection as? BoardRepositoryChoice.Registry)?.repositoryId,
                    loading = loading,
                    resolving = resolving,
                ),
                monospace = selectedName != null,
                placeholder = selectedName == null,
                enabled = enabled && !loading,
                leading = if (selectedName == null) null else ExpIcons.uiRepository,
                onClick = { repoSheet = true },
            )
            if (repoDefault != null) {
                GroupDivider()
                if (registry != null) {
                    SelectRow(
                        label = "Branch",
                        value = branch ?: repoDefault,
                        monospace = true,
                        enabled = enabled,
                        leading = ExpIcons.uiBranch,
                        onClick = {
                            viewModel.load(accountId, registry.id)
                            branchSheet = true
                        },
                    )
                } else {
                    // Inline (not yet connected) repo: no id to list branches
                    // from, so the board's branch is typed. Empty = follow the
                    // repo's default, which is what the placeholder shows.
                    TextFieldRow(
                        label = "Branch",
                        value = branch ?: "",
                        onValueChange = { onBranchChange(it.trim().ifEmpty { null }) },
                        enabled = enabled,
                        placeholder = repoDefault,
                        textStyle = LocalTextStyle.current.copy(fontFamily = FontFamily.Monospace),
                    )
                }
            }
        }

        Text(BOARD_REPO_NOTE, style = MaterialTheme.typography.bodySmall, color = tertiary)

        if (error != null) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    error,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.weight(1f),
                )
                if (onRetry != null) {
                    TextButton(onClick = onRetry) { Text("Retry") }
                }
            }
        }
    }

    if (repoSheet) {
        GlassSheet(title = "Repository", onDismiss = { repoSheet = false }) {
            // Lazy so a many-repos team scrolls instead of clipping the
            // connect row below the sheet fold (the GithubRepoPickerSheet
            // lesson).
            LazyColumn(
                modifier = Modifier.fillMaxWidth(),
                contentPadding = PaddingValues(bottom = 12.dp),
            ) {
                item {
                    GlassSheetRow(
                        label = NO_REPOSITORY,
                        selected = selection == null,
                        onClick = {
                            repoSheet = false
                            onSelect(null)
                        },
                    )
                }
                items(repos, key = { it.id }) { repo ->
                    GlassSheetRow(
                        label = repo.fullName,
                        selected = registry?.id == repo.id,
                        leading = { RowGlyph(ExpIcons.uiRepository) },
                        trailing = if (!repo.isPrivate) null else ({
                            if (registry?.id == repo.id) {
                                Icon(
                                    ExpIcons.uiCheck,
                                    contentDescription = "Selected",
                                    modifier = Modifier.size(18.dp),
                                )
                                Spacer(Modifier.width(8.dp))
                            }
                            Icon(
                                ExpIcons.uiPrivate,
                                contentDescription = "Private",
                                modifier = Modifier.size(14.dp),
                                tint = tertiary,
                            )
                        }),
                        onClick = {
                            repoSheet = false
                            onSelect(BoardRepositoryChoice.Registry(repo.id))
                        },
                    )
                }
                inline?.let { picked ->
                    item {
                        GlassSheetRow(
                            label = picked.fullName,
                            selected = true,
                            leading = { RowGlyph(ExpIcons.uiRepository) },
                            onClick = { repoSheet = false },
                        )
                    }
                }
                item {
                    Column {
                        HorizontalDivider(
                            color = GlassTokens.StrokeRow,
                            modifier = Modifier.padding(vertical = 4.dp),
                        )
                        GlassSheetRow(
                            label = if (hasRepos) {
                                "Connect another repository…"
                            } else {
                                "Connect a GitHub repository…"
                            },
                            leading = { RowGlyph(if (hasRepos) ExpIcons.uiAdd else ExpIcons.uiGithub) },
                            onClick = {
                                repoSheet = false
                                showPicker = true
                            },
                        )
                    }
                }
            }
        }
    }

    if (branchSheet && registry != null && repoDefault != null) {
        val state = branchState[registry.id] ?: BoardBranchesViewModel.Branches(loading = true)
        // GitHub's list may not include the repo's default (a fresh repo the
        // token can't enumerate yet), and it must always be pickable.
        val options = if (state.branches.contains(repoDefault)) {
            state.branches
        } else {
            listOf(repoDefault) + state.branches
        }
        GlassSheet(title = "Branch", onDismiss = { branchSheet = false }) {
            LazyColumn(
                modifier = Modifier.fillMaxWidth(),
                contentPadding = PaddingValues(bottom = 12.dp),
            ) {
                if (state.loading) {
                    item { SheetStatusRow(loading = true, text = "Loading branches…", color = tertiary) }
                }
                state.error?.let { message ->
                    item {
                        Column {
                            SheetStatusRow(
                                loading = false,
                                text = message,
                                color = MaterialTheme.colorScheme.error,
                            )
                            GlassSheetRow(
                                label = "Retry",
                                onClick = { viewModel.load(accountId, registry.id, force = true) },
                            )
                        }
                    }
                }
                items(options, key = { it }) { name ->
                    val isDefault = name == repoDefault
                    val current = (branch ?: repoDefault) == name
                    GlassSheetRow(
                        label = name,
                        selected = current,
                        leading = { RowGlyph(ExpIcons.uiBranch) },
                        trailing = if (!isDefault) null else ({
                            Text(
                                "default",
                                style = MaterialTheme.typography.labelSmall,
                                color = tertiary,
                            )
                            if (current) {
                                Spacer(Modifier.width(8.dp))
                                Icon(
                                    ExpIcons.uiCheck,
                                    contentDescription = "Selected",
                                    modifier = Modifier.size(18.dp),
                                )
                            }
                        }),
                        onClick = {
                            branchSheet = false
                            // Picking the repo's default CLEARS the pin, so the
                            // board keeps following the repo (a later repo-side
                            // rename or team pin carries over).
                            onBranchChange(if (isDefault) null else name)
                        },
                    )
                }
            }
        }
    }

    if (showPicker) {
        GithubRepoPickerSheet(
            accountId = accountId,
            teamId = teamId,
            onPick = { repo ->
                // An ALREADY-connected repo picked here is the registry row,
                // not a second inline connect.
                val known = repos.firstOrNull { it.fullName == repo.fullName }
                onSelect(
                    if (known != null) {
                        BoardRepositoryChoice.Registry(known.id)
                    } else {
                        BoardRepositoryChoice.Inline(
                            fullName = repo.fullName,
                            defaultBranch = repo.defaultBranch,
                            isPrivate = repo.isPrivate,
                        )
                    },
                )
            },
            onDismiss = { showPicker = false },
        )
    }
}

/**
 * EXP-862: a picker ROW inside a glass group — the label, the current value and
 * a chevron, with the group owning the chrome. It replaced the captioned
 * field-shaped select this form used to stack.
 */
@Composable
private fun SelectRow(
    label: String,
    value: String,
    onClick: () -> Unit,
    monospace: Boolean = false,
    placeholder: Boolean = false,
    enabled: Boolean = true,
    leading: ImageVector? = null,
) {
    val alpha = when {
        !enabled -> TextEmphasis.Quaternary
        placeholder -> TextEmphasis.Tertiary
        else -> TextEmphasis.Secondary
    }
    val secondary = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Text(
            label,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface.copy(
                alpha = if (enabled) TextEmphasis.Primary else TextEmphasis.Quaternary,
            ),
            maxLines = 1,
        )
        if (leading != null) {
            Spacer(Modifier.width(8.dp))
            Icon(leading, contentDescription = null, modifier = Modifier.size(14.dp), tint = secondary)
        }
        Text(
            value,
            style = MaterialTheme.typography.bodyMedium,
            fontFamily = if (monospace) FontFamily.Monospace else null,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.End,
            modifier = Modifier.weight(1f).padding(start = 8.dp),
        )
        Icon(
            ExpIcons.uiChevronRight,
            contentDescription = null,
            modifier = Modifier.size(20.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(
                alpha = if (enabled) TextEmphasis.Tertiary else TextEmphasis.Quaternary,
            ),
        )
    }
}

/** The 14dp glyph a [GlassSheetRow] leading slot takes. */
@Composable
private fun RowGlyph(icon: ImageVector) {
    Icon(
        icon,
        contentDescription = null,
        modifier = Modifier.size(14.dp),
        tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
    )
}

/** Loading / error line inside an option sheet, aligned with its rows. */
@Composable
private fun SheetStatusRow(
    loading: Boolean,
    text: String,
    color: Color,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 20.dp, vertical = 10.dp),
    ) {
        if (loading) {
            Box(modifier = Modifier.width(30.dp), contentAlignment = Alignment.CenterStart) {
                CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
            }
        }
        Text(text, style = MaterialTheme.typography.bodySmall, color = color)
    }
}
