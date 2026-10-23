package com.exponential.app.data

import androidx.lifecycle.SavedStateHandle
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.domain.ARG_ACTION
import com.exponential.app.domain.ARG_ISSUES
import com.exponential.app.domain.ARG_PR
import com.exponential.app.domain.ARG_THREAD
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
 * `pr` / `issues` seed names, else the thread a `thread` seed starts into
 * (EXP-1284). Null = nothing team-owned (an empty composer,
 * a builtin): the screen keeps the SELECTED team.
 */
sealed interface RouteSubject {
    data class Prompt(val id: String) : RouteSubject
    data class Issue(val id: String) : RouteSubject
    data class Thread(val id: String) : RouteSubject
}

/** The [RouteSubject] off a route's args, [get] answering one key at a time. */
fun routeSubject(get: (String) -> String?): RouteSubject? {
    val promptId = get(ARG_ACTION)?.takeIf { it.isNotEmpty() }
    if (promptId != null && !promptId.startsWith("builtin:")) return RouteSubject.Prompt(promptId)
    val pr = get(ARG_PR)?.takeIf { it.isNotEmpty() }
    if (pr != null) return RouteSubject.Issue(pr)
    val issue = get(ARG_ISSUES)?.split(',')?.map { it.trim() }?.firstOrNull { it.isNotEmpty() }
    if (issue != null) return RouteSubject.Issue(issue)
    return get(ARG_THREAD)?.takeIf { it.isNotEmpty() }?.let { RouteSubject.Thread(it) }
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
                is RouteSubject.Thread -> db.sessionDao().observeById(subject.id).map { it?.teamId }
            }
        }
    }
    return combine(subjectTeam, selectedId) { own, selected -> own ?: selected }
        .distinctUntilChanged()
}
