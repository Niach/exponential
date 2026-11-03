package com.exponential.app.domain

import java.time.Instant
import java.time.ZoneId

/**
 * VAPP-201 (packages/domain-contract/fixtures/team-zone.json): the TEAM TIME
 * ZONE every app host reads dates in. Replays the TS reference
 * `@exp/domain-contract/team-zone` (desktop `domain::team_zone`, iOS ExpCore
 * `TeamZone.swift`).
 */
object TeamZone {
    object Copy {
        const val ROW = "Time zone"
        const val DEVICE = "Device time zone"
    }

    const val TEST_ID_ROW = "team-time-zone"

    /** The zone a team's `time_zone` names, when the tz database knows it;
     *  null = the device's zone (no team zone, blank, or an unknown name). */
    fun resolveZone(teamZone: String?): String? {
        val zone = teamZone?.trim().orEmpty()
        if (zone.isEmpty()) return null
        return try {
            ZoneId.of(zone)
            zone
        } catch (_: Exception) {
            null
        }
    }

    /** A zone's offset from UTC at an instant, in minutes east. */
    fun zoneOffset(zone: String, ms: Long): Int =
        ZoneId.of(zone).rules.getOffset(Instant.ofEpochMilli(Math.floorDiv(ms, 1000L) * 1000L)).totalSeconds / 60

    /** The host's zone: the team zone when it resolves, else the device's. */
    fun hostZoneId(teamZone: String?): ZoneId = resolveZone(teamZone)?.let(ZoneId::of) ?: ZoneId.systemDefault()
}
