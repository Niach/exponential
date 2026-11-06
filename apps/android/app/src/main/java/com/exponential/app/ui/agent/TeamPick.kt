package com.exponential.app.ui.agent

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.exponential.app.domain.TeamsPick
import com.exponential.app.ui.components.GlassPill
import com.exponential.app.ui.components.PillSize
import com.exponential.app.ui.components.TeamAvatar
import com.exponential.app.ui.components.picker.Picker
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerMode

// #EXP-1715 (composer-menu.json `teamsPick`, web `team-pick.tsx`): the
// viewer's Assistant's Teams pick, which teams a turn may touch. The "+" ›
// Teams page = THE multi picker rows (the team's own mark + name, the
// switcher's order); nothing picked = every team (null). While the pick is a
// subset, the picked teams ride as the composer's removable pill chips.

/** One team the pick offers (the synced `teams` row's mark + name). */
data class PickTeam(val id: String, val name: String, val iconUrl: String?)

/** The pick of a composer talking to the viewer's Assistant; null elsewhere. */
data class TeamsPickBinding(
    val teams: List<PickTeam>,
    /** null = every team. */
    val picked: List<String>?,
    val onChange: (List<String>?) -> Unit,
) {
    val ids: List<String> get() = teams.map { it.id }

    /** The Teams row's value ([TeamsPick.value]). */
    val value: String get() = TeamsPick.value(picked, teams.map { TeamsPick.Team(it.id, it.name) })
}

/** The Teams › page: THE multi picker's rows, inline in the pushed page. */
@Composable
internal fun ComposerTeamsPage(binding: TeamsPickBinding, close: () -> Unit) {
    val byId = binding.teams.associateBy { it.id }
    Column(modifier = Modifier.fillMaxWidth().testTag(TEAMS_PICKER_TEST_ID)) {
    Picker(
        items = binding.teams.map { PickerItem(value = it.id, label = it.name, keywords = listOf(it.name)) },
        mode = PickerMode.Multi,
        value = TeamsPick.pickedIds(binding.picked, binding.ids).toSet(),
        // The last checked team stays: a turn always touches one.
        onChange = { next -> if (next.isNotEmpty()) binding.onChange(TeamsPick.next(next, binding.ids)) },
        search = binding.teams.size > 6,
        title = "Teams",
        open = true,
        onOpenChange = { open -> if (!open) close() },
        renderItem = { item ->
            TeamAvatar(byId[item.value]?.iconUrl, size = 18.dp)
            Spacer(Modifier.width(12.dp))
            Text(
                item.label,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
        },
    )
    }
}

/** The Teams › page's rows (the styleguide capture names it). */
const val TEAMS_PICKER_TEST_ID = "agent-composer-teams-picker"

/** A team chip's tag (`agent-composer-chip-team-{id}`, web's). */
fun teamChipTestId(teamId: String): String = "agent-composer-chip-team-$teamId"

/** The picked teams as the composer's chips (a subset only): the team's
 *  mark + name + the remove ✕ (THE removable subject chip). */
@Composable
internal fun TeamPickChips(binding: TeamsPickBinding, enabled: Boolean, modifier: Modifier = Modifier) {
    val chosen = TeamsPick.chips(binding.picked, binding.teams) { it.id }
    if (chosen.isEmpty()) return
    FlowRow(
        modifier = modifier.fillMaxWidth().padding(bottom = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        chosen.forEach { team ->
            GlassPill(
                team.name,
                size = PillSize.Sm,
                leading = { TeamAvatar(team.iconUrl, size = 12.dp) },
                trailing = {
                    ChipClose(
                        contentDescription = "Remove ${team.name}",
                        testTag = "agent-composer-chip-team-${team.id}-remove",
                        onClick = { if (enabled) binding.onChange(TeamsPick.remove(binding.picked, binding.ids, team.id)) },
                    )
                },
                shrinkLabel = true,
                modifier = Modifier
                    .align(Alignment.CenterVertically)
                    .widthIn(max = 256.dp)
                    .testTag(teamChipTestId(team.id)),
            )
        }
    }
}

/** The synced teams as the pick's rows (the switcher's order). */
internal fun pickTeams(rows: List<com.exponential.app.data.db.TeamEntity>): List<PickTeam> =
    rows.map { PickTeam(it.id, it.name, it.iconUrl) }


/** The "+" menu's pushed pages with Teams › added while [binding] is set. */
internal fun teamsPagePlus(
    binding: TeamsPickBinding?,
    pages: Map<com.exponential.app.domain.ComposerMenuRowId, @Composable (close: () -> Unit) -> Unit>,
): Map<com.exponential.app.domain.ComposerMenuRowId, @Composable (close: () -> Unit) -> Unit> =
    if (binding == null) pages else pages + (com.exponential.app.domain.ComposerMenuRowId.Teams to { close -> ComposerTeamsPage(binding, close) })
