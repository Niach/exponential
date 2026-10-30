package com.exponential.app.data.steer

import kotlinx.coroutines.channels.ReceiveChannel

// EXP-625: the feed itself, behind an interface, so [SteerConnection]'s
// dial/redial machinery is testable on the JVM without a node. The revival
// bugs it grew were all in the loop, and none of them was reachable from a
// test while the loop spoke to the transport directly.
//
// VAPP-43: the steer relay carries no session content any more, so the only
// viewer feed is the one joined over eNet ([EnetSteerSocket]); there is no
// ticket mint and no relay socket behind it.

/** One open viewer feed. TEXT frames only, the relay's wire frames unchanged. */
internal interface SteerSocket {
    val incoming: ReceiveChannel<String>
    suspend fun send(text: String)
    fun cancel()

    /** Why the feed ended, when it knows (EXP-656: the log line names who hung up). */
    suspend fun closeReason(): String?
}

internal interface SteerTransport {
    /**
     * VAPP-33/43: the install running (or holding the journal of) the session,
     * joined over eNet and bounded by its own join timeout. Null = NO FEED: no
     * node, no endpoint, every endpoint refused, or no `activity_synced` in
     * time (packages/domain-contract/fixtures/steer-enet.json).
     */
    suspend fun open(codingSessionId: String): SteerSocket?
}

internal class EnetSteerTransport(private val enet: EnetSteerDialer) : SteerTransport {
    override suspend fun open(codingSessionId: String): SteerSocket? = enet.open(codingSessionId)
}
