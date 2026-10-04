package com.exponential.app.ui.issue

import android.content.Intent
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.net.toUri
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.AppConstants
import com.exponential.app.domain.CodingReadiness
import com.exponential.app.domain.CodingReadiness.Fix
import com.exponential.app.domain.CodingReadiness.StepKey
import com.exponential.app.domain.CodingReadiness.StepState
import com.exponential.app.domain.CodingReadinessRepoPicker
import com.exponential.app.ui.components.AddDeviceSheet
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GlassSheet
import com.exponential.app.ui.components.GlassSheetDefaults
import com.exponential.app.ui.components.GlassSheetRow
import com.exponential.app.ui.components.GlassSheetSearchField
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.onboarding.GithubRepoPickerSheet
import com.exponential.app.ui.theme.DesignTokens
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis

// EXP-1121: the "Ready to code?" checklist — the three readiness steps as
// rows (met = green tick, current = amber with its fixes, pending = dashed),
// a 3-segment progress bar under the summary, and the ONE Start coding
// footer that enables once every step is met (it never auto-starts). The
// rows tick live: the host's model observes the synced board and devices.
// Choose repository is an inline second page (the team's repositories,
// `boards.setRepository` on a pick); the navigating fixes are the host's.
// iOS `CodingReadinessSheet` draws the same layout.

private object ReadinessSheetStyle {
    val Green: Color = DesignTokens.Semantic.Green
    val Amber: Color = StartReadinessStyle.Amber
    val PendingSegment: Color = Color.White.copy(alpha = 0.10f)
    val PendingRing: Color = Color.White.copy(alpha = 0.30f)
    val RowPaddingH = 16.dp
    val RowPaddingV = 14.dp
    val IconSize = 24.dp
    val FixHeight = 36.dp
    val FixShape = RoundedCornerShape(10.dp)
}

private enum class ReadinessPage { Checklist, Picker }

/** The step's concept glyph (the met row draws a tick instead). */
private fun stepIcon(key: StepKey): ImageVector = when (key) {
    StepKey.GITHUB -> ExpIcons.settingsRepositories
    StepKey.REPOSITORY -> ExpIcons.actionRepository
    StepKey.DEVICE -> ExpIcons.uiDevice
}

/**
 * @param availableFixes the fixes this viewer can actually perform here — a
 *   fix outside it never renders (no dead buttons).
 * @param onNavigateFix a fix that leaves the sheet (Board settings, Open
 *   Devices, and Connect GitHub without a board); the host closes the sheet
 *   and navigates.
 * @param onStart the ready footer's tap; the host closes the sheet and opens
 *   the composer.
 */
@Composable
fun CodingReadinessSheet(
    viewModel: CodingReadinessViewModel,
    availableFixes: Set<Fix>,
    onNavigateFix: (Fix) -> Unit,
    onStart: () -> Unit,
    onDismiss: () -> Unit,
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val setting by viewModel.setting.collectAsStateWithLifecycle()
    val setError by viewModel.setError.collectAsStateWithLifecycle()
    val readiness = state.readiness
    var page by rememberSaveable { mutableStateOf(ReadinessPage.Checklist) }
    var addFromGithub by remember { mutableStateOf(false) }
    // EXP-1169: "Set up a server" opens the shared Add device sheet (the
    // install snippet's copy pill gates on the instance origin there).
    var addDevice by remember { mutableStateOf(false) }
    val context = LocalContext.current
    val fixes = availableFixes
    // A board that got its repository (here or elsewhere) has nothing to pick.
    LaunchedEffect(state.board?.repositoryId) {
        if (state.board?.repositoryId != null) page = ReadinessPage.Checklist
    }

    val onFix: (Fix) -> Unit = { fix ->
        when (fix) {
            Fix.CHOOSE_REPOSITORY -> {
                viewModel.clearSetError()
                page = ReadinessPage.Picker
            }
            Fix.GET_DESKTOP_APP -> runCatching {
                context.startActivity(Intent(Intent.ACTION_VIEW, AppConstants.DESKTOP_RELEASES_URL.toUri()))
            }
            Fix.SET_UP_SERVER -> addDevice = true
            // SLOP-26: the Add-repository picker IS the guided flow on the
            // phone — it names the missing prerequisite (not linked / expired
            // / not installed) with its one fix, then lists the live
            // repositories, and a pick adds the repo AND points this board at
            // it. Without a board to point at, team settings' block is the fix.
            Fix.CONNECT_GITHUB -> {
                if (state.accountId != null && state.board != null) addFromGithub = true else onNavigateFix(fix)
            }
            Fix.BOARD_SETTINGS, Fix.OPEN_DEVICES -> onNavigateFix(fix)
        }
    }

    GlassSheet(title = null, onDismiss = onDismiss) {
        when (page) {
            ReadinessPage.Checklist -> Checklist(
                readiness = readiness,
                fixes = fixes,
                onFix = onFix,
                onStart = onStart,
            )
            ReadinessPage.Picker -> {
                BackHandler { page = ReadinessPage.Checklist }
                RepositoryPicker(
                    state = state,
                    settingId = setting,
                    error = setError,
                    canAdd = state.accountId != null && state.board != null,
                    onBack = { page = ReadinessPage.Checklist },
                    onPick = { repo -> viewModel.setRepository(repo.id) { page = ReadinessPage.Checklist } },
                    onAddFromGithub = { addFromGithub = true },
                )
            }
        }
    }

    val accountId = state.accountId
    val teamId = state.board?.teamId
    if (addDevice) {
        AddDeviceSheet(onDismiss = { addDevice = false })
    }
    if (addFromGithub && accountId != null && teamId != null) {
        GithubRepoPickerSheet(
            accountId = accountId,
            teamId = teamId,
            onAdd = { repo -> viewModel.addRepository(repo.fullName, repo.defaultBranch, repo.isPrivate) },
            onDismiss = { addFromGithub = false },
        )
    }
}

@Composable
private fun ColumnScope.Checklist(
    readiness: CodingReadiness.Readiness,
    fixes: Set<Fix>,
    onFix: (Fix) -> Unit,
    onStart: () -> Unit,
) {
    Column(modifier = Modifier.padding(horizontal = GlassSheetDefaults.HorizontalPadding)) {
        Text(
            CodingReadiness.Copy.TITLE,
            fontSize = 17.sp,
            fontWeight = FontWeight.SemiBold,
            color = Color.White,
        )
        Spacer(Modifier.height(2.dp))
        Text(
            readiness.summary,
            fontSize = 15.sp,
            color = Color.White.copy(alpha = TextEmphasis.Secondary),
        )
        Row(
            modifier = Modifier.fillMaxWidth().padding(top = 12.dp),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            readiness.steps.forEach { step ->
                Box(
                    modifier = Modifier
                        .weight(1f)
                        .height(3.dp)
                        .clip(RoundedCornerShape(percent = 50))
                        .background(
                            when (step.state) {
                                StepState.MET -> ReadinessSheetStyle.Green
                                StepState.CURRENT -> ReadinessSheetStyle.Amber
                                StepState.PENDING -> ReadinessSheetStyle.PendingSegment
                            },
                        ),
                )
            }
        }
    }
    Spacer(Modifier.height(16.dp))
    Column(modifier = Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState())) {
        HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
        readiness.steps.forEach { step ->
            StepRow(
                step = step,
                fixes = step.fixes.filter { it in fixes },
                onFix = onFix,
            )
            HorizontalDivider(thickness = GlassTokens.Hairline, color = GlassTokens.StrokeRow)
        }
    }
    StartFooter(ready = readiness.ready, onStart = onStart)
}

@Composable
private fun StepIcon(step: CodingReadiness.Step) {
    val size = ReadinessSheetStyle.IconSize
    when (step.state) {
        StepState.MET -> Box(
            modifier = Modifier
                .size(size)
                .clip(CircleShape)
                .background(ReadinessSheetStyle.Green.copy(alpha = 0.18f)),
            contentAlignment = Alignment.Center,
        ) {
            Icon(ExpIcons.uiCheck, contentDescription = null, modifier = Modifier.size(12.dp), tint = ReadinessSheetStyle.Green)
        }
        StepState.CURRENT -> Box(
            modifier = Modifier
                .size(size)
                .border(1.5.dp, ReadinessSheetStyle.Amber, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Icon(stepIcon(step.key), contentDescription = null, modifier = Modifier.size(13.dp), tint = ReadinessSheetStyle.Amber)
        }
        StepState.PENDING -> Box(
            modifier = Modifier
                .size(size)
                .dashedOutline(CircleShape, ReadinessSheetStyle.PendingRing),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                stepIcon(step.key),
                contentDescription = null,
                modifier = Modifier.size(13.dp),
                tint = Color.White.copy(alpha = TextEmphasis.Tertiary),
            )
        }
    }
}

@Composable
private fun StepRow(
    step: CodingReadiness.Step,
    fixes: List<Fix>,
    onFix: (Fix) -> Unit,
) {
    val current = step.state == StepState.CURRENT
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(if (current) ReadinessSheetStyle.Amber.copy(alpha = 0.08f) else Color.Transparent)
            .padding(horizontal = ReadinessSheetStyle.RowPaddingH, vertical = ReadinessSheetStyle.RowPaddingV),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalAlignment = if (step.state == StepState.MET) Alignment.CenterVertically else Alignment.Top,
    ) {
        StepIcon(step)
        if (step.state == StepState.MET) {
            MetRowText(step)
        } else {
            OpenRowText(step, current, fixes, onFix)
        }
    }
}

/** A met row: the muted one-line title and its end-aligned detail. */
@Composable
private fun RowScope.MetRowText(step: CodingReadiness.Step) {
    Text(
        step.title,
        fontSize = 16.sp,
        color = Color.White.copy(alpha = TextEmphasis.Secondary),
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.weight(1f),
    )
    step.detail?.let { detail ->
        Text(
            detail,
            fontSize = 14.sp,
            color = Color.White.copy(alpha = TextEmphasis.Tertiary),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.End,
            modifier = Modifier.widthIn(max = 200.dp),
        )
    }
}

/** A current or pending row: title, body, and (current only) its fixes. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun RowScope.OpenRowText(
    step: CodingReadiness.Step,
    current: Boolean,
    fixes: List<Fix>,
    onFix: (Fix) -> Unit,
) {
    Column(modifier = Modifier.weight(1f)) {
        Text(
            step.title,
            fontSize = 16.sp,
            fontWeight = if (current) FontWeight.Medium else FontWeight.Normal,
            color = Color.White,
        )
        step.body?.takeIf { it.isNotEmpty() }?.let { body ->
            Spacer(Modifier.height(2.dp))
            Text(
                body,
                fontSize = 14.sp,
                color = Color.White.copy(alpha = TextEmphasis.Secondary),
            )
        }
        if (current && fixes.isNotEmpty()) {
            FlowRow(
                modifier = Modifier.padding(top = 10.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                fixes.forEachIndexed { index, fix ->
                    FixButton(label = fix.label, primary = index == 0, onClick = { onFix(fix) })
                }
            }
        }
    }
}

/** The first fix = white-filled; the rest = glass fill + hairline, same size. */
@Composable
private fun FixButton(label: String, primary: Boolean, onClick: () -> Unit) {
    val shape = ReadinessSheetStyle.FixShape
    Box(
        modifier = Modifier
            .height(ReadinessSheetStyle.FixHeight)
            .clip(shape)
            .then(
                if (primary) {
                    Modifier.background(Color.White, shape)
                } else {
                    Modifier
                        .background(GlassTokens.CardFill, shape)
                        .border(GlassTokens.Hairline, GlassTokens.StrokeStrong, shape)
                },
            )
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 14.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            label,
            fontSize = 15.sp,
            fontWeight = FontWeight.Medium,
            color = if (primary) Color.Black else Color.White,
            maxLines = 1,
        )
    }
}

/** Full-width at the button corner (EXP-1176: a text button is the row-radius
 * rectangle, never a capsule): glass + quaternary until ready, then white/black. */
@Composable
private fun StartFooter(ready: Boolean, onStart: () -> Unit) {
    val shape = GlassTokens.ButtonShape
    val content = if (ready) Color.Black else Color.White.copy(alpha = TextEmphasis.Quaternary)
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = 16.dp, end = 16.dp, top = 16.dp)
            .height(50.dp)
            .clip(shape)
            .then(
                if (ready) {
                    Modifier.background(Color.White, shape)
                } else {
                    Modifier
                        .background(GlassTokens.CardFill, shape)
                        .border(GlassTokens.Hairline, GlassTokens.StrokeCard, shape)
                },
            )
            .clickable(enabled = ready, role = Role.Button, onClick = onStart),
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(ExpIcons.actionRun, contentDescription = null, modifier = Modifier.size(18.dp), tint = content)
        Text(
            CodingReadiness.Copy.START,
            fontSize = 16.sp,
            fontWeight = FontWeight.Medium,
            color = content,
        )
    }
}

@Composable
private fun ColumnScope.RepositoryPicker(
    state: CodingReadinessViewModel.UiState,
    settingId: String?,
    error: String?,
    canAdd: Boolean,
    onBack: () -> Unit,
    onPick: (CodingReadinessRepoPicker.Repo) -> Unit,
    onAddFromGithub: () -> Unit,
) {
    var query by rememberSaveable { mutableStateOf("") }
    val board = state.board
    val repos = state.repos
    val rows = remember(repos, board?.id, board?.name, board?.slug, query) {
        CodingReadinessRepoPicker.rows(
            repos = repos.orEmpty().map { repo ->
                CodingReadinessRepoPicker.Repo(
                    id = repo.id,
                    fullName = repo.fullName,
                    boards = repo.boards.map { CodingReadinessRepoPicker.Board(it.id, it.name) },
                )
            },
            boardId = board?.id.orEmpty(),
            boardName = board?.name.orEmpty(),
            boardSlug = board?.slug.orEmpty(),
            query = query,
        )
    }
    Row(
        modifier = Modifier.fillMaxWidth().padding(start = 8.dp, end = GlassSheetDefaults.HorizontalPadding, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        CircleIconButton(
            ExpIcons.uiChevronLeft,
            contentDescription = "Back",
            onClick = onBack,
            borderless = true,
        )
        Text(
            Fix.CHOOSE_REPOSITORY.label,
            fontSize = 17.sp,
            fontWeight = FontWeight.SemiBold,
            color = Color.White,
        )
    }
    GlassSheetSearchField(
        value = query,
        onValueChange = { query = it },
        placeholder = CodingReadiness.Copy.PICKER_SEARCH,
    )
    Spacer(Modifier.height(8.dp))
    error?.let { message ->
        Text(
            message,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.error,
            modifier = Modifier.padding(horizontal = GlassSheetDefaults.HorizontalPadding, vertical = 4.dp),
        )
    }
    when {
        repos == null -> Box(
            modifier = Modifier.fillMaxWidth().padding(vertical = 24.dp),
            contentAlignment = Alignment.Center,
        ) {
            CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp, color = Color.White)
        }
        repos.isEmpty() -> Text(
            CodingReadiness.Copy.PICKER_EMPTY,
            style = MaterialTheme.typography.bodyMedium,
            color = Color.White.copy(alpha = TextEmphasis.Secondary),
            modifier = Modifier.padding(horizontal = GlassSheetDefaults.HorizontalPadding, vertical = 12.dp),
        )
        else -> LazyColumn(modifier = Modifier.weight(1f, fill = false)) {
            items(rows, key = { it.repo.id }) { row ->
                GlassSheetRow(
                    label = row.repo.fullName,
                    onClick = { onPick(row.repo) },
                    enabled = settingId == null,
                    labelColor = Color.White.copy(alpha = 0.9f),
                    leading = {
                        Icon(
                            ExpIcons.uiRepository,
                            contentDescription = null,
                            modifier = Modifier.size(16.dp),
                            tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                        )
                    },
                    trailing = {
                        if (settingId == row.repo.id) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(16.dp),
                                strokeWidth = 2.dp,
                                color = Color.White,
                            )
                        } else {
                            row.tag?.let { tag ->
                                Text(
                                    tag,
                                    fontSize = 13.sp,
                                    color = Color.White.copy(alpha = TextEmphasis.Tertiary),
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier.padding(start = 8.dp).widthIn(max = 140.dp),
                                )
                            }
                        }
                    },
                )
            }
        }
    }
    if (canAdd) {
        HorizontalDivider(
            thickness = GlassTokens.Hairline,
            color = GlassTokens.StrokeRow,
            modifier = Modifier.padding(top = 4.dp),
        )
        GlassSheetRow(
            label = CodingReadiness.Copy.PICKER_ADD_FROM_GITHUB,
            onClick = onAddFromGithub,
            enabled = settingId == null,
            leading = {
                Icon(
                    ExpIcons.uiGithub,
                    contentDescription = null,
                    modifier = Modifier.size(16.dp),
                    tint = Color.White.copy(alpha = TextEmphasis.Secondary),
                )
            },
        )
    }
}
