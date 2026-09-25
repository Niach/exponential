package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity

/**
 * EXP-1075: live runs of the CALLER's, grouped by team — the input behind the
 * board switcher's "another team has your live runs" dot.
 *
 * Every session surface on the phone is already SELECTED-TEAM scoped
 * (AgentsViewModel.agentRows, AppViewModel.agentsRunning), which means a run
 * started in another team goes entirely dark once the switcher moves. The dot
 * is the one pointer back to it: a DOT, never a count — the number belongs to
 * the list you reach by switching, not to the control that switches.
 */
data class TeamLiveRuns(
    val count: Int,
    /** At least one of this team's live runs is parked on a question/plan. */
    val needsInput: Boolean,
)

/** Whether any team OTHER than the selected one has live runs, and its tone. */
data class OtherTeamsLive(
    val any: Boolean,
    val needsInput: Boolean,
)

/**
 * Own + live sessions grouped by `teamId`. Exactly the predicate
 * [AppViewModel.agentsRunning] applies for the selected team — a teammate's
 * run is never the caller's business (EXP-312: it can't be viewed or steered),
 * and ended/heartbeat-stale rows drop out (EXP-153).
 *
 * `needsInput` uses the DISPLAY state, not the raw flag: EXP-679 lets the
 * server set `needs_input` on every live status, and `in_review` masks it.
 */
fun liveRunsByTeam(
    sessions: List<CodingSessionEntity>,
    me: String?,
    now: Long,
): Map<String, TeamLiveRuns> {
    if (me == null) return emptyMap()
    val out = LinkedHashMap<String, TeamLiveRuns>()
    for (session in sessions) {
        if (session.userId != me) continue
        if (!CodingSessionLiveness.isLive(session, now)) continue
        val needsInput =
            codingSessionDisplayState(session, null) == CodingSessionDisplayState.NeedsInput
        val prev = out[session.teamId]
        out[session.teamId] = TeamLiveRuns(
            count = (prev?.count ?: 0) + 1,
            needsInput = (prev?.needsInput ?: false) || needsInput,
        )
    }
    return out
}

/**
 * Collapses [liveRunsByTeam] to the switcher control's single dot. The
 * SELECTED team is excluded — its runs already light the Agents tab, and a
 * second dot for them would read as "there is somewhere else to look".
 */
fun otherTeamsLive(
    byTeam: Map<String, TeamLiveRuns>,
    selectedTeamId: String?,
): OtherTeamsLive {
    var any = false
    var needsInput = false
    for ((teamId, runs) in byTeam) {
        if (teamId == selectedTeamId) continue
        if (runs.count <= 0) continue
        any = true
        if (runs.needsInput) needsInput = true
    }
    return OtherTeamsLive(any = any, needsInput = needsInput)
}
