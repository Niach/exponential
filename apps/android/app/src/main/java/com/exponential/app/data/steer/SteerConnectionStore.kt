package com.exponential.app.data.steer

import android.content.Context
import android.os.SystemClock
import android.util.Log
import com.exponential.app.AppConstants
import com.exponential.app.BuildConfig
import com.exponential.app.data.api.InstallsApi
import com.exponential.app.data.api.STEER_CLIENT
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.DatabaseHolder
import com.exponential.app.data.db.accountDatabaseFlow
import com.exponential.app.data.db.scopedQuery
import com.exponential.app.data.electric.SyncManager
import com.exponential.app.data.enet.EnetNodeManager
import com.exponential.app.data.sealed.SealedFfi
import com.exponential.app.data.sealed.SealedContent
import com.exponential.app.data.sealed.SealedKeys
import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.foldStoredTail
import dagger.hilt.android.qualifiers.ApplicationContext
import java.io.File
import io.ktor.client.HttpClient
import javax.inject.Inject
import javax.inject.Named
import javax.inject.Singleton
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json

private const val TAG = "SteerConnectionStore"

/// One live [SteerConnection] per coding session, held by the app rather than
/// by a screen (EXP-621). The steer screen's ViewModel is nav-entry scoped, so
/// a back-tap used to take the socket, the whole activity feed and a
/// half-typed message with it — reopening the session replayed the entire log
/// behind a "Connecting…" spinner. Acquiring here instead means reopening
/// re-attaches to what is already running, instantly.
@Singleton
class SteerConnectionStore @Inject constructor(
    private val auth: AuthRepository,
    private val holder: DatabaseHolder,
    private val issueImagesApi: IssueImagesApi,
    private val json: Json,
    private val syncManager: SyncManager,
    private val enetNodes: EnetNodeManager,
    private val installsApi: InstallsApi,
    private val steerApi: SteerApi,
    @Named(STEER_CLIENT) private val steerClient: HttpClient,
    @ApplicationContext context: Context,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    /** #PERF-9 (steer-enet.json STORED TAIL): the tails' reads and writes, never on the main thread. */
    private val ioScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val tailFiles = SteerTailFiles(File(context.filesDir, "steer-tail"))

    /**
     * EXP-1522: work that must outlive the screen that started it. A send
     * the user backs out of right away used to die with its ViewModel
     * (upload or `sessions.send` cancelled mid-flight, the draft never
     * cleared); it finishes here instead.
     */
    fun launchDetached(block: suspend CoroutineScope.() -> Unit): Job = scope.launch(block = block)
    private val lock = Any()
    private val connections = mutableMapOf<String, SteerConnection>()

    // Pending park (see [setForeground]). Cancelled when we come back inside
    // the grace window, so a quick app switch costs nothing.
    private var parkJob: Job? = null

    /// Whether the process is foregrounded, as [setForeground] last said. The
    /// sync-kick fan-out is gated on it: a push wakes a BACKGROUND process and
    /// kicks the shapes, and reviving parked sockets from there would undo the
    /// park the moment it happened (EXP-625).
    private var foreground = false

    /// EXP-1529: connection analytics, posted under the active account
    /// (the events name no account, user, team or session).
    private val connStats = SteerConnStatsClient(post = { events ->
        val accountId = auth.activeAccountId.value
        if (accountId != null) steerApi.recordConnStats(accountId, events)
    })

    init {
        // Every reason the shapes have to suspect they're behind (app
        // foreground, a network coming back, a DNS change, a push, a
        // pull-to-refresh) is a reason to suspect the steer feed died the
        // same way (EXP-625). The connection decides what that means for it;
        // most kicks are no-ops on a healthy socket.
        scope.launch {
            syncManager.lastKickAt.drop(1).collect {
                val live = synchronized(lock) { revivableLocked() }
                if (live.isEmpty()) return@collect
                Log.i(TAG, "sync-kick -> ${live.size} connection(s)")
                live.forEach { it.kick("sync-kick") }
            }
        }
        // EXP-1524: a session opened before the account's eNet node was up
        // (a notification cold start) dials the moment it is, instead of
        // waiting out its Starting backoff.
        scope.launch {
            combine(auth.activeAccountId, enetNodes.ready) { id, ready -> id != null && id in ready }
                .distinctUntilChanged()
                .filter { it }
                .collect {
                    val live = synchronized(lock) { revivableLocked() }
                    if (live.isEmpty()) return@collect
                    Log.i(TAG, "enet-ready -> ${live.size} connection(s)")
                    live.forEach { it.kick("enet-ready") }
                }
        }
    }

    /// Get (or open) the connection for a coding session and count the caller
    /// as attached. Every [acquire] must be paired with a [release].
    fun acquire(codingSessionId: String): SteerConnection {
        synchronized(lock) {
            val connection = connections.getOrPut(codingSessionId) {
                val accountId = auth.activeAccountId.value
                SteerConnection(
                    codingSessionId = codingSessionId,
                    // EXP-1529: eNet first, the sealed relay tunnel as its fallback.
                    transport = FallbackSteerTransport(
                        enet = EnetSteerTransport(enetDialer()),
                        relay = tunnelDialer(),
                        nowMs = { SystemClock.elapsedRealtime() },
                    ),
                    sessionFlow = accountDatabaseFlow(auth, holder)
                        .scopedQuery<SessionEntity?>(null) {
                            it.sessionDao().observeById(codingSessionId)
                        },
                    json = json,
                    issueImagesApi = issueImagesApi,
                    auth = auth,
                    // EXP-1522: a long replay folds off the main thread.
                    foldDispatcher = Dispatchers.Default,
                    stats = connStats,
                    version = AppConstants.VERSION_NAME,
                    tail = accountId?.let { tailRecorder(it, codingSessionId) },
                ).also { opened -> if (accountId != null) paintStoredTail(opened, accountId, codingSessionId) }
            }
            connection.setBackground(!foreground)
            connection.refCount += 1
            return connection
        }
    }

    /** #PERF-9: the connection's tail, written (sealed for a keyed tree) off the main thread. */
    private fun tailRecorder(accountId: String, sessionId: String): SteerTailRecorder {
        val generation = tailFiles.generation(accountId)
        return SteerTailRecorder(ioScope, persist = { events ->
            // An unknown tree is never written: it could be a keyed one.
            val tree = tailTree(accountId, sessionId) ?: return@SteerTailRecorder
            val stored = StoredTailCodec.seal(StoredTailCodec.encode(sessionId, events), tree, SealedFfi)
                ?: return@SteerTailRecorder
            // PRUNING: checked off the main thread whenever a tail is written.
            if (tailFiles.write(accountId, sessionId, stored, generation)) tailFiles.prune(accountId)
        })
    }

    /** #PERF-9: read, open and fold the stored tail off the main thread, then paint it. */
    private fun paintStoredTail(connection: SteerConnection, accountId: String, sessionId: String) {
        scope.launch {
            val state = withContext(Dispatchers.IO) {
                runCatching { loadTail(accountId, sessionId) }.getOrNull()
            } ?: return@launch
            connection.seedStoredTail(state)
        }
    }

    private suspend fun loadTail(accountId: String, sessionId: String): ActivityFeedState? {
        // PRUNING: opening the session stamps its tail's last use.
        tailFiles.touch(accountId, sessionId)
        val stored = tailFiles.read(accountId, sessionId) ?: return null
        val tree = tailTree(accountId, sessionId) ?: return null
        val clear = StoredTailCodec.open(stored, tree, SealedKeys.of(tree.rootId), SealedFfi)
        if (clear == null) {
            // A clear tail of a keyed tree is never shown, nor kept.
            if (!SealedContent.isSealedBody(stored)) tailFiles.delete(accountId, sessionId)
            return null
        }
        val events = StoredTailCodec.decode(clear, sessionId) ?: return null
        return foldStoredTail(events).takeIf { it.feed.isNotEmpty() }
    }

    private suspend fun tailTree(accountId: String, sessionId: String): TailTree? {
        val sessions = holder.database(accountId).sessionDao()
        val row = sessions.getById(sessionId) ?: return null
        val rootId = row.rootId.ifEmpty { row.id }
        val root = if (rootId == row.id) row else sessions.getById(rootId) ?: return null
        return TailTree(rootId, root.sealPublicKey, root.sealEpoch ?: 1)
    }

    /** #PERF-9: signing out of [accountId] deletes its stored tails. */
    suspend fun forgetTails(accountId: String) {
        withContext(Dispatchers.IO) { tailFiles.forget(accountId) }
    }

    /// VAPP-33: steer over the active account's eNet node, the PRIMARY
    /// carrier; the relay carries only the sealed fallback (EXP-1529).
    private fun enetDialer() = EnetSteerDialer(
        node = { auth.activeAccountId.value?.let(enetNodes::peer) },
        discover = { sessionId ->
            val accountId = auth.activeAccountId.value ?: error("No active account")
            installsApi.forSession(accountId, sessionId).map { it.endpointId }
        },
    )

    /// EXP-1529: the sealed relay leg (fixtures/steer-enet.json `fallback`
    /// AVAILABILITY): the root's seal key + the tree secret of its current
    /// epoch this install holds, then a `steer.tunnelTicket`.
    private fun tunnelDialer() = RelayTunnelDialer(
        sealer = ::sealerFor,
        ticket = { sessionId ->
            val accountId = auth.activeAccountId.value ?: error("No active account")
            val result = steerApi.tunnelTicket(accountId, sessionId)
            val ticket = result.ticket
            val url = result.url
            when {
                result.disabled -> TunnelTicket.Disabled
                ticket != null && url != null -> TunnelTicket.Granted(ticket, dialUrl(url))
                else -> TunnelTicket.Unavailable
            }
        },
        wire = KtorTunnelWire(steerClient),
    )

    private suspend fun sealerFor(sessionId: String): SteerSealer? {
        val accountId = auth.activeAccountId.value ?: return null
        val sessions = holder.database(accountId).sessionDao()
        val row = sessions.getById(sessionId) ?: return null
        val rootId = row.rootId.ifEmpty { row.id }
        val root = if (rootId == row.id) row else sessions.getById(rootId) ?: return null
        val publicKey = root.sealPublicKey ?: return null
        val epoch = root.sealEpoch ?: 1
        if (SealedKeys.of(rootId)[epoch] == null) return null
        return TreeSteerSealer(SealedFfi, publicKey, epoch) { SealedKeys.of(rootId) }
    }

    /// The capture-only authority rewrite, exactly like the voice call's dial.
    private fun dialUrl(url: String): String =
        SteerTestHooks.relayAuthority
            ?.takeIf { BuildConfig.DEBUG }
            ?.let { withRelayAuthority(url, it) }
            ?: url

    /// Detach one screen. The connection itself keeps running — that IS the
    /// point — unless it has nothing left to do, which is the moment its feed
    /// stops being worth holding.
    fun release(codingSessionId: String) {
        synchronized(lock) {
            val connection = connections[codingSessionId] ?: return
            connection.refCount = (connection.refCount - 1).coerceAtLeast(0)
            reapLocked()
        }
    }

    /// The connection a screen already holds for a session, without
    /// attaching (EXP-1342: the chat header's Stop hands the shown page's
    /// queue back to its composer before `sessions.stop`). Null = no screen
    /// has this session open, so there is no queue bar to hand back.
    fun peek(codingSessionId: String): SteerConnection? =
        synchronized(lock) { connections[codingSessionId] }

    /// Drop the connections of sessions that are over and that no screen is
    /// showing. A finished session someone is still LOOKING at keeps its feed:
    /// closing it out from under the screen would blank the transcript the
    /// user opened it to read.
    fun closeEnded() {
        synchronized(lock) { reapLocked() }
    }

    /**
     * App visibility, driven by ExponentialApp's ProcessLifecycleOwner
     * observer alongside the shape loops. Backgrounding parks every socket
     * after a grace window — a steer feed per open session must not keep a
     * radio warm for a process the user can't see — while keeping the feed,
     * the draft and the pending images; foregrounding revives them.
     */
    fun setForeground(foreground: Boolean) {
        synchronized(lock) {
            this.foreground = foreground
            Log.i(TAG, "foreground=$foreground (${connections.size} connection(s))")
            connections.values.forEach { it.setBackground(!foreground) }
            parkJob?.cancel()
            parkJob = if (foreground) {
                // A session that ended while nobody was watching has nothing
                // left to revive — drop it instead of redialing it.
                reapLocked()
                // #EXP-1678: only what a screen shows comes back. A connection
                // nobody is attached to stays parked until a screen acquires
                // it (its attach kicks it): reviving every session opened
                // this process lifetime redialled them all in one burst.
                revivableLocked().forEach { it.resume() }
                null
            } else {
                // EXP-1529: the analytics go out before the process may sleep.
                connStats.flush()
                scope.launch {
                    delay(BACKGROUND_PARK_DELAY_MS)
                    // Commit the park under the same lock the foreground path
                    // cancels under, re-checking liveness inside it — the
                    // SyncManager gate's race (a cancel landing exactly at the
                    // grace deadline can no longer stop this coroutine).
                    synchronized(lock) {
                        if (!isActive) return@launch
                        connections.values.toList().forEach { it.park() }
                    }
                    // The parks just closed their views (`ended`).
                    connStats.flush()
                }
            }
        }
    }

    /// Sign-out / account switch: nothing survives it. The sockets carry that
    /// account's ticket and the feeds are that account's data.
    fun closeAll() {
        synchronized(lock) {
            connections.values.forEach { it.close() }
            connections.clear()
            connStats.flush()
        }
    }

    /** #EXP-1678: the connections a revival may dial: foreground, attached. */
    private fun revivableLocked(): List<SteerConnection> =
        if (foreground) connections.values.filter { it.refCount > 0 } else emptyList()

    private fun reapLocked() {
        val done = connections.filterValues { it.refCount == 0 && it.isFinished }
        done.forEach { (id, connection) ->
            Log.i(TAG, "reap ${id.take(8)}")
            connection.close()
            connections.remove(id)
        }
    }
}

// A quick app switch, a share sheet or the photo picker must not cost the live
// session anything. Deliberately NOT the shape loops' behaviour, which park
// instantly since EXP-656: a shape loop resumes from a persisted cursor for the
// price of one GET, while a steer feed resumes by re-joining over eNet and
// taking a full replay. But it parks BEFORE Android 14+'s cached-apps freezer
// (debounce 10 s, `dumpsys activity settings`): the 30 s this used to be never
// fired on time; the frozen feed died under us (the host's QUIC idle timeout is
// 10 s) and every overdue redial and relay ticket failed at the next thaw.
internal const val BACKGROUND_PARK_DELAY_MS = 5_000L
