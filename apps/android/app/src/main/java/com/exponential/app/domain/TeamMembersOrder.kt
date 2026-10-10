package com.exponential.app.domain

import com.exponential.app.data.db.TeamMemberEntity

/**
 * Settings › Members lists the roster in JOIN order (EXP-1267 ×4, web
 * `members-section.tsx` `membersInJoinOrder`): by the member row's
 * `created_at`, ties (and an unparsable stamp, which sorts last) broken by
 * id. Parsed through [WireTimestamps] because the column arrives as Postgres
 * text OR ISO, which do not string-sort against each other.
 */
fun membersInJoinOrder(members: List<TeamMemberEntity>): List<TeamMemberEntity> =
    members.sortedWith(
        compareBy<TeamMemberEntity>({ WireTimestamps.parseEpochMs(it.createdAt) ?: Long.MAX_VALUE }, { it.id }),
    )
