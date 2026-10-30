package com.exponential.app.data.enet

import at.exponential.net.ffi.EnetConfig
import at.exponential.net.ffi.EnetNode
import at.exponential.net.ffi.defaultConfig
import com.exponential.app.data.api.RegisterInstallInput
import java.time.Instant

/** What an install node needs from a running eNet node (the facade's [EnetNode] in the app). */
interface PeerNode {
    val endpointId: String
    fun allowedPeers(): Set<String>
    fun allow(peer: String)
    fun revoke(peer: String)
    suspend fun shutdown()
}

class FfiPeerNode(val node: EnetNode) : PeerNode {
    override val endpointId: String = node.endpointId()
    override fun allowedPeers(): Set<String> = node.allowedPeers().toSet()
    override fun allow(peer: String) = node.allowPeer(peer)
    override fun revoke(peer: String) = node.revokePeer(peer)
    override suspend fun shutdown() = node.shutdown()

    companion object {
        /** Our relay (https://relay.exponential.at), n0 relays off, nobody allowed until the registry says so. */
        fun productionConfig(): EnetConfig = defaultConfig().copy(n0Relays = false, allowAll = false, allowedPeers = emptyList())

        suspend fun start(identity: ByteArray, config: EnetConfig = productionConfig()): FfiPeerNode =
            FfiPeerNode(EnetNode.start(config, identity))
    }
}

/** The allowlist change that turns [current] into [desired] (never admitting [self]). */
internal data class AccessDiff(val allow: Set<String>, val revoke: Set<String>)

internal fun accessDiff(current: Set<String>, desired: Set<String>, self: String): AccessDiff {
    val want = desired - self
    return AccessDiff(allow = want - current, revoke = current - want)
}

/**
 * One account's eNet node on this install (EXP-1457 step 1): start it,
 * register it (`installs.register`, on every start), and keep its access
 * allowlist = the endpoint ids `installs.list` returns for every team the
 * user is in. No message content travels yet (EXP-1458 / EXP-1459).
 *
 * Not thread-safe: [EnetNodeManager] drives each one under its lock.
 */
class InstallNode(
    private val userId: String,
    private val identity: ByteArray,
    private val crypto: EnetCrypto,
    private val label: String?,
    private val platform: String?,
    private val register: suspend (RegisterInstallInput) -> String,
    private val listTeam: suspend (teamId: String) -> List<String>,
    private val startNode: suspend (identity: ByteArray) -> PeerNode,
    private val now: () -> Instant = Instant::now,
) {
    var node: PeerNode? = null
        private set
    var installId: String? = null
        private set

    // The last list each team answered: a failed refresh keeps a team's
    // previous ids instead of locking its members out.
    private val teamPeers = mutableMapOf<String, Set<String>>()

    /** Starts the node (once) and registers it (every call). Throws on failure; safe to call again. */
    suspend fun start() {
        val running = node ?: startNode(identity).also { node = it }
        check(running.endpointId == crypto.endpointIdOf(identity)) { "node runs another identity" }
        installId = register(signedRegistration(crypto, identity, userId, now(), label, platform))
    }

    /**
     * Reloads the allowlist for [teamIds]. Returns false when some team's
     * list failed (its previous ids stay allowed; try again later).
     */
    suspend fun refreshAccess(teamIds: Set<String>): Boolean {
        val running = node ?: return false
        teamPeers.keys.retainAll(teamIds)
        var complete = true
        for (teamId in teamIds.sorted()) {
            try {
                teamPeers[teamId] = listTeam(teamId).toSet()
            } catch (err: kotlin.coroutines.cancellation.CancellationException) {
                throw err
            } catch (_: Throwable) {
                complete = false
            }
        }
        val diff = accessDiff(running.allowedPeers(), teamPeers.values.flatten().toSet(), running.endpointId)
        diff.revoke.forEach(running::revoke)
        diff.allow.forEach(running::allow)
        return complete
    }

    suspend fun stop() {
        val running = node ?: return
        node = null
        teamPeers.clear()
        running.shutdown()
    }
}
