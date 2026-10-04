package com.exponential.app.ui.components

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.exponential.app.data.db.TeamEntity

/**
 * EXP-1186: one team's slice of a CROSS-TEAM list (Agent page runs, Recent,
 * Actions). [team] is null when the user is in at most one team: the list
 * then renders exactly as it did before, with no team band.
 */
data class TeamBand<T>(val team: TeamEntity?, val items: List<T>)

/**
 * Splits [items] into one [TeamBand] per team, in [teams] order (the name
 * order the teams DAO returns), keeping each band's rows in input order. With
 * at most one team the whole list is ONE band with a null team. Teams with no
 * rows get no band; rows of a team missing from [teams] trail at the end.
 */
fun <T> teamBands(
    items: List<T>,
    teams: List<TeamEntity>,
    teamIdOf: (T) -> String?,
): List<TeamBand<T>> {
    if (teams.size <= 1) return listOf(TeamBand(null, items))
    val byTeam = items.groupBy(teamIdOf)
    val known = teams.mapNotNull { team ->
        byTeam[team.id]?.takeIf { it.isNotEmpty() }?.let { TeamBand(team, it) }
    }
    val knownIds = teams.mapTo(HashSet()) { it.id }
    val orphans = items.filter { teamIdOf(it) !in knownIds }
    return if (orphans.isEmpty()) known else known + TeamBand(null, orphans)
}

/** EXP-1186: the team band — THE section header, led by the team avatar. */
@Composable
fun TeamSectionHeader(
    team: TeamEntity,
    modifier: Modifier = Modifier,
    trailing: (@Composable () -> Unit)? = null,
) {
    SectionHeader(
        team.name,
        modifier = modifier.testTag("team-band"),
        leading = { TeamAvatar(team, size = 16.dp) },
        trailing = trailing,
    )
}
