package com.exponential.app.data.steer

import kotlinx.coroutines.channels.ReceiveChannel

// EXP-625: the feed itself, behind an interface, so [SteerConnection]'s
// dial/redial machinery is testable on the JVM without a node. The revival
// bugs it grew were all in the loop, and none of them was reachable from a
// test while the loop spoke to the transport directly.
//
// VAPP-43: the steer relay carries no session content. EXP-1529: eNet
// ([EnetSteerSocket]) stays the feed; the relay comes back ONLY as the sealed
// tunnel ([TunnelSteerSocket], ciphertext only) behind [FallbackSteerTransport].

/** One open viewer feed. TEXT frames only, the relay's wire frames unchanged. */
internal interface SteerSocket {
    val incoming: ReceiveChannel<String>
    suspend fun send(text: String)
    fun cancel()

    /** Why the feed ended, when it knows (EXP-656: the log line names who hung up). */
    suspend fun closeReason(): String?

    /** EXP-1529: when NEITHER carrier became active again after a drop (the lostBanner clock); null = unknown. */
    fun downSinceMs(): Long? = null

    /** EXP-1529: the connStats `path` of the carrier delivering now. */
    val path: String get() = "direct"

    /** EXP-1529: foreground: re-evaluate the carriers at once (no-op on a single-carrier feed). */
    fun wake() {}
}

internal interface SteerTransport {
    /**
     * VAPP-33/43: the install running (or holding the journal of) the session,
     * joined over eNet and bounded by its own join timeout. Null = NO FEED: no
     * node, no endpoint, every endpoint refused, or no `activity_synced` in
     * time (packages/domain-contract/fixtures/steer-enet.json).
     */
    suspend fun open(codingSessionId: String): SteerSocket?

    /** EXP-1529: why the last [open] found NO FEED, by connStats `errorClasses`. */
    val lastFailure: String? get() = null
}

internal class EnetSteerTransport(private val enet: EnetSteerDialer) : SteerTransport {
    override suspend fun open(codingSessionId: String): SteerSocket? = enet.open(codingSessionId)
    override val lastFailure: String? get() = enet.lastFailure
}
