package com.exponential.app.domain

import com.exponential.app.data.db.CodingSessionEntity

/**
 * EXP-1075: live runs of the CALLER's, grouped by team — the input behind the
 * board switcher SHEET's per-team dots (EXP-1210: the switcher pill itself
 * wears none). A DOT, never a count — the number belongs to the list you
 * reach by switching, not to the control that switches.
 */
data class TeamLiveRuns(
    val count: Int,
    /** At least one of this team's live runs is parked on a question/plan. */
    val needsInput: Boolean,
)

/**
 * Own + live sessions grouped by `teamId`. Exactly the predicate
 * [AppViewModel.agentsRunning] applies for the selected team — a teammate's
 * run is never the caller's business (EXP-312: it can't be viewed or steered),
 * and ended/heartbeat-stale rows drop out (EXP-153).
 *
 * `needsInput` uses the DISPLAY state (EXP-1184: `needs_input` wins on every
 * live status, an open PR included), so the dot reads like every list.
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
