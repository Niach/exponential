package com.exponential.app.ui.steer

import com.exponential.app.data.api.SteerDevice
import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.data.db.ExponentialDatabase
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.TeamEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.domain.AgentSettings
import com.exponential.app.domain.DeviceLiveness
import com.exponential.app.ui.session.composeDeviceList
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

// EXP-485: the synced devices shape is the ONLY device source. The remote-start
// hosts (the Actions tab, the issue list's selection bar, the issue detail's
// start circle and the Reviews/Changes delegate) used to poll `devices.list`
// for presence — a network round trip per screen resume, per account and per
// team switch, for state the shape already carries. They share this builder
// now, so every surface resolves machines, online-ness and shared-server
// ownership identically to the Agents tab.

/**
 * The caller's machines plus (EXP-432) [teamIdFlow]'s shared servers as
 * [SteerDevice] rows, online-ness derived from `last_seen_at` freshness on the
 * 30s ticker (Room flows only re-emit on writes, so the clock has to tick
 * separately). null until the devices shape's initial snapshot has landed —
 * the "still loading" the hosts' `null` device list has always meant, so a
 * cold start can't read as "no machines".
 */
fun steerDeviceFlow(
    dbFlow: Flow<ExponentialDatabase?>,
    teamIdFlow: Flow<String?>,
    userIdFlow: Flow<String?>,
    /** EXP-1186: the Devices page — servers shared with ANY member team. */
    allTeams: Boolean = false,
): Flow<List<SteerDevice>?> = combine(
    dbFlow.scopedQuery(emptyList<DeviceEntity>()) { it.deviceDao().observeAll() },
    dbFlow.scopedQuery(emptyList<UserEntity>()) { it.userDao().observeAll() },
    dbFlow.scopedQuery(null as Boolean?) { it.electricOffsetDao().observeIsLive("devices") },
    combine(teamIdFlow, userIdFlow) { teamId, userId -> teamId to userId },
    DeviceLiveness.ticker(),
) { rows, users, snapshotLive, (teamId, userId), now ->
    if (snapshotLive != true && rows.isEmpty()) {
        null
    } else {
        composeDeviceList(rows, users, teamId, userId, now, allTeams)
    }
}

/**
 * #EXP-2079 (agent-settings.json `assistant.chatTeams` `deviceCases`): the
 * team whose rule lists a session's devices ([steerDeviceFlow]'s team). null
 * = the viewer's OWN machines, whatever teams they are shared with, and no
 * teammate's server: a session in a team the viewer does not sync that they
 * started (the Assistant's conversation; its personal team never syncs).
 * No synced teams yet = not decided (the team rule).
 */
fun sessionDeviceTeamFlow(
    dbFlow: Flow<ExponentialDatabase?>,
    session: Flow<SessionEntity?>,
    userIdFlow: Flow<String?>,
): Flow<String?> = combine(
    session.map { row -> row?.let { it.teamId to it.userId } }.distinctUntilChanged(),
    dbFlow.scopedQuery(emptyList<TeamEntity>()) { it.teamDao().observeAll() }.map { rows -> rows.map { it.id } }.distinctUntilChanged(),
    userIdFlow,
) { row, teamIds, me ->
    val teamId = row?.first?.takeIf { it.isNotEmpty() } ?: return@combine null
    val own = teamIds.isNotEmpty() && AgentSettings.Assistant.ownDevices(
        sessionTeamSynced = teamId in teamIds,
        starterIsViewer = !me.isNullOrEmpty() && row.second == me,
    )
    if (own) null else teamId
}.distinctUntilChanged()

/**
 * What the remote-start surfaces actually gate on: the ONLINE machines a start
 * can be delivered to right now, or null while either steer availability or
 * the shape snapshot is still resolving. A known-off relay lists nothing —
 * every caller reads an empty list as "no machine online", and an offline
 * registry row is not a start target.
 */
fun onlineStartTargets(
    devices: List<SteerDevice>?,
    steerEnabled: Boolean?,
): List<SteerDevice>? = when {
    steerEnabled == null -> null
    !steerEnabled -> emptyList()
    else -> devices?.filter { it.online }
}
