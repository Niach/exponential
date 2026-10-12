package com.exponential.app.ui.actions

import com.exponential.app.ui.components.EmptyState
import com.exponential.app.domain.EmptyStateCopy
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
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
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.exponential.app.data.api.ActionDto
import com.exponential.app.domain.AgentComposerSeed
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.FeedbackStates
import com.exponential.app.domain.TriggerBadge
import com.exponential.app.domain.TriggerBadges
import com.exponential.app.domain.triggerBadges
import com.exponential.app.ui.components.BottomBarInset
import com.exponential.app.ui.components.CircleIconButton
import com.exponential.app.ui.components.GlassDropdownMenu
import com.exponential.app.ui.components.GlassMenuItem
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.LocalToaster
import com.exponential.app.ui.components.SkeletonRows
import com.exponential.app.ui.components.SectionHeader
import com.exponential.app.ui.components.TeamSectionHeader
import com.exponential.app.ui.components.teamBands
import com.exponential.app.ui.components.actionGlyph
import com.exponential.app.ui.icons.ExpIcons
import com.exponential.app.ui.theme.GlassTokens
import com.exponential.app.ui.theme.TextEmphasis
import com.exponential.app.ui.theme.flatRow
import com.exponential.app.ui.theme.glassRow

// The Actions screen (EXP-253): every member team's action prompts (EXP-1186),
// banded per team once there are several, each with
// a Run affordance. EXP-825: Run NAVIGATES to the Agent page composer with the
// action preselected (the ONE launcher — typed inputs, free text and the
// agent/model/device options live there), the "Actions" header's "New action"
// (web-parity placement per EXP-574) lands there on the "Create action"
// builtin. Neither client builtin is listed here: "Create
// action" left in EXP-431 and "Fix merge conflicts" in EXP-686 (both pick from
// the composer's ▶ menu).
//
// EXP-686 gave it its own bottom-bar tab (it used to be a push off the Agents
// header, now Devices), so the screen is a ROOT surface: a plain header row,
// no back button, and its lists clear the floating bar.
//
// EXP-1270: the Suggestions segment is gone (the seeds left every client), so
// the screen is the one list. SLOP-2: an action CARRIES its triggers — a row
// wears a glyph per trigger kind beside its name, and tapping it opens the
// action page ([ActionDetailScreen]: Prompt | Triggers | Runs).

@Composable
fun ActionsScreen(
    // EXP-825: Run / New action navigate to the composer.
    onOpenAgent: (AgentComposerSeed) -> Unit,
    // SLOP-2: a row opens its action's page.
    onOpenAction: (actionId: String) -> Unit,
    viewModel: ActionsViewModel = hiltViewModel(),
) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    val selectedTeamId by viewModel.selectedTeamId.collectAsStateWithLifecycle()
    // EXP-1186: cross-team — one band per team once there are several.
    val teams by viewModel.teams.collectAsStateWithLifecycle()
    val multiTeam = teams.size > 1
    val toaster = LocalToaster.current
    LaunchedEffect(state.error) {
        state.error?.let { toaster.error(FeedbackStates.errorTitle("load", "actions"), it) }
    }
    // EXP-1186: with several teams each team band carries its own "New
    // action", creating in THAT team (web + iOS parity).
    val newActionPill: @Composable (teamId: String?) -> Unit = { teamId ->
        GlassPill(
            "New action",
            icon = ExpIcons.actionCreate,
            enabled = (teamId ?: selectedTeamId) != null,
            onClick = {
                teamId?.let(viewModel::selectTeam)
                onOpenAgent(AgentComposerSeed(actionId = DomainContract.builtinCreateActionId))
            },
            modifier = Modifier.testTag(if (teamId == null) "new-action" else "new-action-$teamId"),
        )
    }

    Scaffold(containerColor = Color.Transparent) { padding ->
        Column(modifier = Modifier.padding(padding).fillMaxSize()) {
            // The root screens' header (AgentsScreen / SearchScreen parity):
            // a plain large title, no back button — this is a tab now.
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = 16.dp, end = 16.dp, top = 8.dp, bottom = 12.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    "Actions",
                    style = MaterialTheme.typography.headlineLarge,
                    color = MaterialTheme.colorScheme.onSurface,
                    modifier = Modifier.weight(1f),
                )
            }
            Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
                when {
                    // EXP-1275: the first load is skeleton rows in
                    // the action row's geometry, never a spinner.
                    state.actions.isEmpty() && state.loading ->
                        SkeletonRows(modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp))
                    // FEED-79: a failed load is the empty state plus a toast.
                    state.actions.isEmpty() -> EmptyState(EmptyStateCopy.actions)
                    else -> LazyColumn(
                        modifier = Modifier.fillMaxSize(),
                        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = BottomBarInset),
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        // EXP-574 (web parity): the "Actions" header with
                        // the "New action" entry (EXP-431) as its trailing
                        // control.
                        if (!multiTeam) {
                            item(key = "__actions_header__") {
                                SectionHeader(title = "Actions") { newActionPill(null) }
                            }
                        }
                        // Server order (sort_order, then name) — since
                        // EXP-686 the list carries no client builtins to
                        // pin above it. EXP-1186: one band per team (the
                        // team avatar + name) with several teams.
                        teamBands(state.actions, teams) { it.teamId }.forEach { band ->
                            band.team?.let { team ->
                                item(key = "__actions_team_${team.id}__") {
                                    TeamSectionHeader(team) { newActionPill(team.id) }
                                }
                            }
                            items(band.items, key = { it.id }) { action ->
                                ActionRow(
                                    action = action,
                                    // EXP-1186: the composer resolves the
                                    // ACTION's team off the seed.
                                    onRun = { onOpenAgent(AgentComposerSeed(actionId = action.id)) },
                                    // SLOP-2: the row (and its menu's Edit)
                                    // opens the action page, which is
                                    // read-only for non-owners.
                                    onOpen = { onOpenAction(action.id) },
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

// One action: its curated glyph, name with a glyph per trigger kind it carries
// (SLOP-2), optional description, a trailing play button and (EXP-694) the row
// overflow with Edit. Tapping the row opens the action page.
@Composable
private fun ActionRow(
    action: ActionDto,
    onRun: () -> Unit,
    onOpen: () -> Unit,
) {
    var menuOpen by remember { mutableStateOf(false) }
    val badges = remember(action.triggers) { triggerBadges(action.parsedTriggers) }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("action-row")
            .flatRow()
            .clickable(onClick = onOpen)
            .padding(horizontal = GlassTokens.RowPaddingH, vertical = GlassTokens.RowPaddingV),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            actionGlyph(action),
            contentDescription = null,
            modifier = Modifier.size(18.dp),
            tint = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Secondary),
        )
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            // EXP-697: no repo glyph beside the name — the row says what the
            // action is, not where it runs.
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                Text(
                    action.name,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    // Yield to the glyphs instead of pushing them off the row.
                    modifier = Modifier.weight(1f, fill = false),
                )
                TriggerBadgeGlyphs(badges)
            }
            val description = action.description
            if (!description.isNullOrBlank()) {
                Text(
                    description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = TextEmphasis.Tertiary),
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        // EXP-615: an icon-only play button, like every other Run affordance.
        // EXP-694: on the shared glass circle (iOS `CircleIconButton` parity),
        // not a bare primary-tinted glyph in an M3 touch box.
        CircleIconButton(
            ExpIcons.actionRun,
            contentDescription = "Run",
            onClick = onRun,
            modifier = Modifier.padding(start = 8.dp),
        )
        // Builtins are shipped prompts with no team row to edit (the list
        // carries none today — the guard keeps it that way).
        if (!action.isBuiltin) {
            Box {
                // EXP-698: every trailing row action is the one 32dp glass
                // circle — a bare glyph in an M3 touch box read as a hitbox,
                // not a control.
                CircleIconButton(
                    ExpIcons.uiMore,
                    contentDescription = "Action options",
                    onClick = { menuOpen = true },
                    modifier = Modifier.padding(start = 8.dp),
                    borderless = true,
                )
                GlassDropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                    GlassMenuItem(
                        text = { Text("Edit") },
                        leadingIcon = { Icon(ExpIcons.uiEdit, contentDescription = null) },
                        onClick = {
                            menuOpen = false
                            onOpen()
                        },
                    )
                }
            }
        }
    }
}

/**
 * The trigger glyphs beside a name (SLOP-2): the schedule clock and/or the
 * event bolt, each drawn when the subject has a trigger of that kind and
 * MUTED while none of that kind is enabled. No text, no count.
 */
@Composable
private fun TriggerBadgeGlyphs(badges: TriggerBadges) {
    badges.schedule?.let { TriggerBadgeGlyph(ExpIcons.triggerSchedule, it, "trigger-badge-schedule") }
    badges.event?.let { TriggerBadgeGlyph(ExpIcons.triggerEvent, it, "trigger-badge-event") }
}

@Composable
private fun TriggerBadgeGlyph(icon: ImageVector, badge: TriggerBadge, tag: String) {
    Icon(
        icon,
        contentDescription = null,
        modifier = Modifier.size(12.dp).testTag(tag),
        tint = MaterialTheme.colorScheme.onSurface.copy(
            alpha = if (badge.active) TextEmphasis.Secondary else TextEmphasis.Quaternary,
        ),
    )
}
