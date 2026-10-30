package com.exponential.app.data.enet

import android.os.Build
import android.util.Log
import com.exponential.app.data.api.InstallsApi
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.data.api.PendingWrap
import com.exponential.app.data.api.SessionKeysApi
import com.exponential.app.data.db.MultiAccountTeamRepository
import com.exponential.app.data.sealed.HolderApi
import com.exponential.app.data.sealed.SealedFfi
import com.exponential.app.data.sealed.SealedKeys
import com.exponential.app.data.sealed.TreeKeyHolder
import com.exponential.app.data.sealed.TreeKeyStore
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.selects.select
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull

/**
 * EXP-1457 step 1 / VAPP-40: every signed-in account runs one eNet node on
 * this install, registered with the install registry, admitting only the
 * installs of its teams.
 *
 * - Lazy and off the main thread: [start] only launches coroutines on
 *   Dispatchers.IO; JNA and the eNet `.so` load on the first facade call there.
 * - A node starts (and `installs.register`s) when its account signs in or at
 *   app start; its allowlist reloads then, on every team change and every
 *   [REFRESH_MS].
 * - The account's credentials dropping (sign-out, a dead session) stops its
 *   node; an explicit sign-out of this install also revokes it ([signOut]).
 * - EXP-1459: a registered install is a tree-key HOLDER ([TreeKeyHolder]):
 *   on start, every [HOLDER_MS] and whenever a screen met a key it lacks.
 *   An account's stored tree keys open content from app start, offline too.
 */
@Singleton
class EnetNodeManager @Inject constructor(
    private val auth: AuthRepository,
    private val teams: MultiAccountTeamRepository,
    private val api: InstallsApi,
    private val identities: EnetIdentityStore,
    private val treeKeys: TreeKeyStore,
    private val sessionKeys: SessionKeysApi,
) {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val lock = Mutex()
    private val wake = Channel<Unit>(Channel.CONFLATED)
    private val nodes = ConcurrentHashMap<String, InstallNode>()
    private val holders = ConcurrentHashMap<String, TreeKeyHolder>()
    private val installIds = ConcurrentHashMap<String, String>()
    private val holderWake = Channel<Unit>(Channel.CONFLATED)

    // accountId -> userId of the signed-in accounts, and their team ids.
    @Volatile private var accounts: Map<String, String> = emptyMap()
    @Volatile private var teamIds: Map<String, Set<String>> = emptyMap()

    // Accounts signing out: no node may (re)start for them until their
    // credentials are gone (a later sign-in starts afresh).
    private val suppressed = mutableSetOf<String>()
    @Volatile private var pass: Job? = null

    fun start() {
        scope.launch {
            val signedIn = auth.accounts
                .map { list -> list.filter { it.token != null && it.userId != null }.associate { it.id to it.userId!! } }
                .distinctUntilChanged()
            val teamSets = teams.serverGroups
                .map { groups -> groups.associate { g -> g.accountId to g.teams.map { it.id }.toSet() } }
                .distinctUntilChanged()
            combine(signedIn, teamSets) { a, t -> a to t }.collect { (a, t) ->
                synchronized(suppressed) { suppressed.retainAll(a.keys) }
                accounts = a
                teamIds = t
                wake.trySend(Unit)
            }
        }
        scope.launch { loop() }
        scope.launch { holderLoop() }
    }

    /** This install's registry id for [accountId] once its node registered (the sealed writers' `installId`). */
    fun installId(accountId: String): String? = installIds[accountId]

    /** [accountId]'s running eNet node, once it started (VAPP-33: the steer viewer's carrier). */
    fun peer(accountId: String): PeerNode? = nodes[accountId]?.node

    /** Runs a holder pass now (a key was just created, or a screen lacks one). */
    fun wakeHolders() {
        holderWake.trySend(Unit)
    }

    private suspend fun holderLoop() {
        while (true) {
            withTimeoutOrNull(HOLDER_MS) {
                select<Unit> {
                    holderWake.onReceive { }
                    SealedKeys.missing.onReceive { }
                }
            }
            for (holder in holders.values.toList()) {
                try {
                    holder.pass()
                } catch (err: CancellationException) {
                    throw err
                } catch (err: Throwable) {
                    Log.w(TAG, "tree-key holder failed: ${err.javaClass.simpleName}")
                }
            }
        }
    }

    private suspend fun loop() {
        var retry = false
        while (true) {
            withTimeoutOrNull(if (retry) RETRY_MS else REFRESH_MS) { wake.receive() }
            val job = scope.launch { lock.withLock { retry = reconcile() } }
            pass = job
            job.join()
        }
    }

    /** One pass over every account; true = something failed, retry soon. */
    private suspend fun reconcile(): Boolean {
        val wanted = accounts.filterKeys { id -> synchronized(suppressed) { id !in suppressed } }
        for (gone in nodes.keys - wanted.keys) stopQuietly(gone)
        var failed = false
        for ((accountId, userId) in wanted) {
            try {
                val node = nodes[accountId] ?: newNode(accountId, userId)?.also { nodes[accountId] = it }
                if (node == null) {
                    failed = true
                    continue
                }
                if (node.installId == null) startAndRegister(accountId, node)
                node.installId?.let { id ->
                    installIds[accountId] = id
                    if (!holders.containsKey(accountId)) newHolder(accountId, id)?.let {
                        holders[accountId] = it
                        holderWake.trySend(Unit)
                    }
                }
                if (!node.refreshAccess(teamIds[accountId].orEmpty())) failed = true
            } catch (err: CancellationException) {
                throw err
            } catch (err: Throwable) {
                failed = true
                Log.w(TAG, "eNet node for an account failed: ${err.javaClass.simpleName}")
            }
        }
        return failed
    }

    private fun newHolder(accountId: String, installId: String): TreeKeyHolder? {
        val identity = identities.vault.identity(accountId, create = false) ?: return null
        return TreeKeyHolder(
            accountId = accountId,
            installId = installId,
            installSecret = SealedFfi.installSecret(identity),
            crypto = SealedFfi,
            vault = treeKeys.vault,
            api = object : HolderApi {
                override suspend fun mine(installId: String) =
                    withTimeoutOrNull(CALL_TIMEOUT_MS) { sessionKeys.mine(accountId, installId) } ?: error("mine timed out")
                override suspend fun pending(installId: String) =
                    withTimeoutOrNull(CALL_TIMEOUT_MS) { sessionKeys.pending(accountId, installId) } ?: error("pending timed out")
                override suspend fun wrap(installId: String, wraps: List<PendingWrap>) {
                    withTimeoutOrNull(CALL_TIMEOUT_MS) { sessionKeys.wrap(accountId, installId, wraps) } ?: error("wrap timed out")
                }
            },
        )
    }

    private fun newNode(accountId: String, userId: String): InstallNode? {
        val identity = identities.vault.identity(accountId) ?: return null
        // The keys this account already holds open content before the node is up.
        SealedKeys.merge(treeKeys.vault.all(accountId))
        return InstallNode(
            userId = userId,
            identity = identity,
            crypto = EnetFfi,
            label = Build.MODEL,
            platform = "Android ${Build.VERSION.RELEASE}",
            register = { input -> withTimeoutOrNull(CALL_TIMEOUT_MS) { api.register(accountId, input) } ?: error("register timed out") },
            listTeam = { teamId ->
                withTimeoutOrNull(CALL_TIMEOUT_MS) { api.list(accountId, teamId).map { it.endpointId } } ?: error("list timed out")
            },
            startNode = { id -> FfiPeerNode.start(id) },
        )
    }

    private suspend fun startAndRegister(accountId: String, node: InstallNode) {
        try {
            node.start()
        } catch (err: TrpcException) {
            // Revoked (FORBIDDEN) or claimed by another account (CONFLICT):
            // this identity can never register again, so the install gets a
            // fresh one on the next pass.
            if (err.code == "FORBIDDEN" || err.code == "CONFLICT") {
                stopQuietly(accountId)
                identities.vault.forget(accountId)
            }
            throw err
        }
    }

    private suspend fun stopQuietly(accountId: String) {
        holders.remove(accountId)
        installIds.remove(accountId)
        val node = nodes.remove(accountId) ?: return
        try {
            node.stop()
        } catch (err: CancellationException) {
            throw err
        } catch (err: Throwable) {
            Log.w(TAG, "eNet node stop failed: ${err.javaClass.simpleName}")
        }
    }

    /**
     * Stops [accountId]'s node; [revoke] (an explicit sign-out of this
     * install) also revokes it in the registry; [forget] drops its identity.
     * Must be awaited BEFORE the account's credentials drop: the revoke
     * authenticates with them. Bounded, so sign-out never hangs on it.
     */
    suspend fun signOut(accountId: String, revoke: Boolean, forget: Boolean = revoke) {
        synchronized(suppressed) { suppressed.add(accountId) }
        pass?.cancelAndJoin()
        lock.withLock {
            val installId = nodes[accountId]?.installId
            stopQuietly(accountId)
            if (revoke) {
                withTimeoutOrNull(REVOKE_TIMEOUT_MS) {
                    try {
                        val id = installId ?: ownInstallId(accountId)
                        if (id != null) api.revoke(accountId, id)
                    } catch (err: CancellationException) {
                        throw err
                    } catch (err: Throwable) {
                        Log.w(TAG, "installs.revoke failed: ${err.javaClass.simpleName}")
                    }
                }
            }
            if (forget) {
                identities.vault.forget(accountId)
                SealedKeys.drop(treeKeys.vault.all(accountId).keys)
                treeKeys.vault.forget(accountId)
            }
        }
        wake.trySend(Unit)
    }

    /** This install's registry id for [accountId] when the node never registered in this process. */
    private suspend fun ownInstallId(accountId: String): String? {
        val identity = identities.vault.identity(accountId, create = false) ?: return null
        val endpointId = EnetFfi.endpointIdOf(identity)
        return api.mine(accountId).firstOrNull { it.endpointId == endpointId && it.revokedAt == null }?.installId
    }

    companion object {
        private const val TAG = "EnetNodes"
        /** The allowlist refresh cadence (#VAPP-37: on start + every 10 minutes). */
        const val REFRESH_MS = 10 * 60 * 1000L
        private const val RETRY_MS = 60 * 1000L
        /** The tree-key holder cadence (sealed-content.json: on start + every minute while online). */
        const val HOLDER_MS = 60 * 1000L
        private const val CALL_TIMEOUT_MS = 20_000L
        private const val REVOKE_TIMEOUT_MS = 3_000L
    }
}
