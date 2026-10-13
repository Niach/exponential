package com.exponential.app.domain

import com.exponential.app.data.db.SessionEntity

/**
 * EXP-1075: live sessions of the CALLER's, grouped by team — the input behind
 * the board switcher SHEET's per-team dots. A DOT, never a count.
 * EXP-1291: a session is live until `ended`; agents (never ended) count only
 * while busy or waiting on a person, so an idle agent never lights a dot.
 */
data class TeamLiveRuns(
    val count: Int,
    /** At least one of this team's live sessions waits on a person. */
    val needsInput: Boolean,
)

/** Whether [session] lights the live dots (own sessions only, upstream). */
fun lightsLiveDot(session: SessionEntity): Boolean = when {
    session.isEnded -> false
    session.isAgent -> session.busy || session.needsInput
    else -> true
}

/** Own + live sessions grouped by `teamId`. */
fun liveRunsByTeam(sessions: List<SessionEntity>, me: String?): Map<String, TeamLiveRuns> {
    if (me == null) return emptyMap()
    val out = LinkedHashMap<String, TeamLiveRuns>()
    for (session in sessions) {
        if (session.userId != me || !lightsLiveDot(session)) continue
        val prev = out[session.teamId]
        out[session.teamId] = TeamLiveRuns(
            count = (prev?.count ?: 0) + 1,
            needsInput = (prev?.needsInput ?: false) || session.needsInput,
        )
    }
    return out
}
