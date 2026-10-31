package com.exponential.app.data.enet

import at.exponential.net.ffi.EnetConfig
import at.exponential.net.ffi.EnetNode
import at.exponential.net.ffi.defaultConfig
import at.exponential.net.ffi.EnetEvent
import com.exponential.app.data.api.RegisterInstallInput
import java.time.Instant
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.launch

/** One chat message a peer sent this node: its endpoint id and the body. */
class PeerChat(val from: String, val body: ByteArray)

/** What an install node needs from a running eNet node (the facade's [EnetNode] in the app). */
interface PeerNode {
    val endpointId: String
    fun allowedPeers(): Set<String>
    fun allow(peer: String)
    fun revoke(peer: String)

    /** Sends one acked chat message (throws on a refusal or a failed send). */
    suspend fun sendChat(peer: String, body: ByteArray)

    /** Every chat message from now on (VAPP-33: consumers filter by the body's tag). */
    val chats: SharedFlow<PeerChat>
    suspend fun shutdown()
}

class FfiPeerNode(val node: EnetNode) : PeerNode {
    override val endpointId: String = node.endpointId()
    override fun allowedPeers(): Set<String> = node.allowedPeers().toSet()
    /** EXP-1471: an allowed peer is also taught its address on our relay (else every dial fails). */
    override fun allow(peer: String) {
        node.allowPeer(peer)
        RelayTicket.encode(peer)?.let { runCatching { node.addTicket(it) } }
    }
    override fun revoke(peer: String) = node.revokePeer(peer)

    override suspend fun sendChat(peer: String, body: ByteArray) {
        node.sendChat(peer, body)
    }

    private val pump = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val _chats = MutableSharedFlow<PeerChat>()
    override val chats: SharedFlow<PeerChat> = _chats

    init {
        // The ONE consumer of nextEvent: a chat goes to whoever collects
        // [chats] now (nobody = dropped); every other event is not ours yet.
        pump.launch {
            while (true) {
                val event = node.nextEvent() ?: break
                if (event is EnetEvent.Chat) _chats.emit(PeerChat(event.from, event.message.body))
            }
        }
    }

    override suspend fun shutdown() {
        pump.cancel()
        node.shutdown()
    }

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
    @Volatile var node: PeerNode? = null
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
