package com.exponential.app.data.steer

import com.exponential.app.domain.AgentFeedItem

/** #FEED-260: one synced message (body OPENED) a not-live transcript may append. */
data class SyncedTailMessage(
    val id: String,
    val authorKind: String,
    val consumedAt: String?,
    val createdAt: String,
    val createdAtMs: Long?,
    val body: String,
)

/** Synthetic row ids of the synced tail: far above any feed id, stable per message. */
private const val SYNCED_TAIL_ID_BASE = 1L shl 52

/** The feed's MAIN-lane row as [steerSyncedTail] reads it; null = a
 *  subagent's row (it renders inside its run, never the main lane). */
fun AgentFeedItem.tailFeedRow(): SteerTailFeedRow? = when (this) {
    is AgentFeedItem.Narration -> if (subagentId == null) SteerTailFeedRow("agent", text, at) else null
    is AgentFeedItem.UserMessage -> if (subagentId == null) SteerTailFeedRow("user", text, at) else null
    is AgentFeedItem.Tool -> if (subagentId == null) SteerTailFeedRow(null, null, settledAt.takeIf { settled }) else null
    is AgentFeedItem.ApiError -> if (subagentId == null) SteerTailFeedRow(null, null, null) else null
    else -> SteerTailFeedRow(null, null, null)
}

/**
 * #FEED-260 (fixtures/steer-enet.json SYNCED TAIL): the synced messages a
 * NOT-live feed draws after its last row, as ordinary transcript rows (the
 * agent's narration bubble, the person's input bubble, each at its
 * `createdAt`). [exclude] = message ids already drawn as the own tail.
 */
fun syncedTailItems(
    feed: List<AgentFeedItem>,
    live: Boolean,
    messages: List<SyncedTailMessage>,
    exclude: Set<String> = emptySet(),
    /** The live transcript cannot be loaded (the OFFLINE LINE's condition). */
    unreachable: Boolean = false,
    /** #EXP-1998: the cap (`Int.MAX_VALUE` = every drawable one; the caller
     *  cuts the shown stretch). */
    max: Int = STEER_SYNCED_TAIL_MAX,
): List<AgentFeedItem> {
    if (live || messages.isEmpty()) return emptyList()
    val picked = steerSyncedTail(
        live = false,
        feed = feed.mapNotNull { it.tailFeedRow() },
        messages = messages.filter { it.id !in exclude },
        max = max,
        unreachable = unreachable,
    ) { m ->
        SteerTailMessage(
            id = m.id,
            authorKind = m.authorKind,
            hasCard = false,
            consumedAt = m.consumedAt,
            createdAt = m.createdAt,
            createdAtMs = m.createdAtMs,
            body = m.body,
        )
    }
    return picked.map { m ->
        val id = SYNCED_TAIL_ID_BASE + (m.id.hashCode().toLong() and 0xFFFF_FFFFL)
        if (m.authorKind == "agent") {
            AgentFeedItem.Narration(id = id, text = m.body.trim(), messageId = "synced:${m.id}", at = m.createdAtMs)
        } else {
            AgentFeedItem.UserMessage(id = id, text = m.body.trim(), at = m.createdAtMs, messageId = "synced:${m.id}")
        }
    }
}
