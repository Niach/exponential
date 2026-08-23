package com.exponential.app.data.steer

import com.exponential.app.data.api.IssueImagesApi
import com.exponential.app.data.api.SteerApi
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.db.DatabaseHolder
import io.ktor.client.HttpClient
import javax.inject.Inject
import javax.inject.Singleton
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json

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
    private val steerApi: SteerApi,
    private val issueImagesApi: IssueImagesApi,
    private val client: HttpClient,
    private val json: Json,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val lock = Any()
    private val connections = mutableMapOf<String, SteerConnection>()

    // Pending park (see [setForeground]). Cancelled when we come back inside
    // the grace window, so a quick app switch costs nothing — the SyncManager
    // gate's shape, and the same 30s window.
    private var parkJob: Job? = null

    /// Get (or open) the connection for a coding session and count the caller
    /// as attached. Every [acquire] must be paired with a [release].
    fun acquire(codingSessionId: String): SteerConnection {
        synchronized(lock) {
            val connection = connections.getOrPut(codingSessionId) {
                SteerConnection(
                    codingSessionId = codingSessionId,
                    auth = auth,
                    holder = holder,
                    steerApi = steerApi,
                    issueImagesApi = issueImagesApi,
                    client = client,
                    json = json,
                )
            }
            connection.refCount += 1
            return connection
        }
    }

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
     * after a grace window — a relay socket per open session must not keep a
     * radio warm for a process the user can't see — while keeping the feed,
     * the draft and the pending images; foregrounding revives them.
     */
    fun setForeground(foreground: Boolean) {
        synchronized(lock) {
            parkJob?.cancel()
            parkJob = if (foreground) {
                // A session that ended while nobody was watching has nothing
                // left to revive — drop it instead of redialing it.
                reapLocked()
                connections.values.toList().forEach { it.resume() }
                null
            } else {
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
        }
    }

    private fun reapLocked() {
        val done = connections.filterValues { it.refCount == 0 && it.isFinished }
        done.forEach { (id, connection) ->
            connection.close()
            connections.remove(id)
        }
    }
}

// Matches SyncManager's shape-loop park window: a quick app switch, a
// share-sheet or the photo picker must not cost the live session anything.
private const val BACKGROUND_PARK_DELAY_MS = 30_000L
