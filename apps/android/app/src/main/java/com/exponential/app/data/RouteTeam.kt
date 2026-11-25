package com.exponential.app.data

import androidx.lifecycle.SavedStateHandle
import com.exponential.app.data.db.BoardEntity
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.IssueEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.domain.ARG_ACTION
import com.exponential.app.domain.ARG_ISSUES
import com.exponential.app.domain.ARG_PR
import com.exponential.app.domain.ARG_CHAT
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map

/**
 * EXP-1186: what a route names that OWNS a team — a saved prompt (the agent
 * route's `action` arg, NEW-2), else the issue a
 * `pr` / `issues` seed names, else the chat a `chat` seed starts into
 * (EXP-1284). Null = nothing team-owned (an empty composer,
 * a builtin): the screen keeps the SELECTED team.
 */
sealed interface RouteSubject {
    data class Prompt(val id: String) : RouteSubject
    data class Issue(val id: String) : RouteSubject
    data class Chat(val id: String) : RouteSubject
}

/** The [RouteSubject] off a route's args, [get] answering one key at a time. */
fun routeSubject(get: (String) -> String?): RouteSubject? {
    val promptId = get(ARG_ACTION)?.takeIf { it.isNotEmpty() }
    if (promptId != null && !promptId.startsWith("builtin:")) return RouteSubject.Prompt(promptId)
    val pr = get(ARG_PR)?.takeIf { it.isNotEmpty() }
    if (pr != null) return RouteSubject.Issue(pr)
    val issue = get(ARG_ISSUES)?.split(',')?.map { it.trim() }?.firstOrNull { it.isNotEmpty() }
    if (issue != null) return RouteSubject.Issue(issue)
    return get(ARG_CHAT)?.takeIf { it.isNotEmpty() }?.let { RouteSubject.Chat(it) }
}

/**
 * EXP-1186: the team a route-hosted screen works in. Cross-team lists
 * open rows of ANY member team, so a page reached from one
 * of those rows must work in the ROW's team — its owner gate, its pools, its
 * start — never silently in the selection. A route naming nothing team-owned
 * keeps [selectedId] (the Agent composer's Chat FAB). Until the subject's row
 * resolves (or if it never does) the selection stands in.
 */
@OptIn(ExperimentalCoroutinesApi::class)
fun routeTeamIdFlow(
    dbFlow: Flow<ExponentialDatabase?>,
    handle: SavedStateHandle,
    selectedId: Flow<String?>,
): Flow<String?> {
    val subject = routeSubject { handle.get<String>(it) } ?: return selectedId
    val subjectTeam: Flow<String?> = dbFlow.flatMapLatest { db ->
        if (db == null) {
            flowOf(null)
        } else {
            when (subject) {
                // NEW-2: an `action` route arg names a saved prompt.
                is RouteSubject.Prompt -> db.promptDao().observeById(subject.id).map { it?.teamId }
                is RouteSubject.Issue -> combine(
                    db.issueDao().observeById(subject.id),
                    db.boardDao().observeAll(),
                ) { issue, boards -> boards.firstOrNull { it.id == issue?.boardId }?.teamId }
                is RouteSubject.Chat -> db.sessionDao().observeById(subject.id).map { it?.teamId }
            }
        }
    }
    return combine(subjectTeam, selectedId) { own, selected -> own ?: selected }
        .distinctUntilChanged()
}

/** #EXP-2079 (agent-settings.json `assistant.chatTeams`): the synced teams
 *  in the cross-team order (by name, case-insensitive, then id). */
fun crossTeamOrder(teams: List<TeamEntity>): List<TeamEntity> =
    teams.sortedWith(compareBy<TeamEntity>({ it.name.lowercase() }, { it.id }))

/**
 * #EXP-2079 (`AgentSettings.Assistant.chatTeams`): the teams the lists of a
 * chat read (`#` issues, `@` members, the MCP pick, `/` prompts, Link / New
 * issue): [own] while it is a synced team, else the turn's teams ([picked] =
 * the conversation root's Teams pick ∩ the synced teams, else all of them),
 * in the cross-team order. It supersedes #EXP-1714's one browse team.
 */
@OptIn(ExperimentalCoroutinesApi::class)
fun chatTeamIdsFlow(
    dbFlow: Flow<ExponentialDatabase?>,
    own: Flow<String?>,
    picked: Flow<List<String>?> = flowOf(null),
): Flow<List<String>> {
    val teamIds: Flow<List<String>> = dbFlow.flatMapLatest { db ->
        db?.teamDao()?.observeAll()?.map { rows -> crossTeamOrder(rows).map { it.id } } ?: flowOf(emptyList())
    }
    return combine(own, teamIds, picked) { o, ids, pick ->
        com.exponential.app.domain.AgentSettings.Assistant.chatTeams(o, ids, pick)
    }.distinctUntilChanged()
}

/** #EXP-2079: the Teams pick of [session]'s conversation ROOT
 *  (`sessions.scope_team_ids`); null = none picked. */
@OptIn(ExperimentalCoroutinesApi::class)
fun rootTeamsPickFlow(dbFlow: Flow<ExponentialDatabase?>, session: Flow<SessionEntity?>): Flow<List<String>?> =
    session.flatMapLatest { row ->
        val rootId = row?.rootId?.takeIf { it.isNotEmpty() && it != row.id }
        if (row == null || rootId == null) {
            flowOf(row?.scopeTeamIdList)
        } else {
            dbFlow.flatMapLatest { db -> db?.sessionDao()?.observeById(rootId)?.map { it?.scopeTeamIdList } ?: flowOf(null) }
        }
    }.distinctUntilChanged()

/** #EXP-2079: [query] per chat team, joined team by team in [teamIds]' order. */
@OptIn(ExperimentalCoroutinesApi::class)
fun <T> teamByTeamFlow(
    dbFlow: Flow<ExponentialDatabase?>,
    teamIds: Flow<List<String>>,
    query: (ExponentialDatabase, String) -> Flow<List<T>>,
): Flow<List<T>> = combine(dbFlow, teamIds) { db, ids -> db to ids }.flatMapLatest { (db, ids) ->
    when {
        db == null || ids.isEmpty() -> flowOf(emptyList())
        ids.size == 1 -> query(db, ids.single())
        else -> combine(ids.map { query(db, it) }) { lists -> lists.flatMap { it } }
    }
}

/** #EXP-2079 (`chatTeams` (1)): the issues of the chat teams; an identifier
 *  found in several teams = the first team's in [teamIds]' order. */
fun chatTeamIssues(issues: List<IssueEntity>, boards: List<BoardEntity>, teamIds: List<String>): List<IssueEntity> {
    val rank = teamIds.withIndex().associate { it.value to it.index }
    val boardRank = boards.mapNotNull { board -> rank[board.teamId]?.let { board.id to it } }.toMap()
    val inTeams = issues.filter { it.boardId in boardRank }
    if (teamIds.size <= 1) return inTeams
    val first = HashMap<String, Int>()
    for (issue in inTeams) {
        val at = boardRank.getValue(issue.boardId)
        first.merge(issue.identifier.uppercase(), at, ::minOf)
    }
    return inTeams.filter { boardRank[it.boardId] == first[it.identifier.uppercase()] }
}
