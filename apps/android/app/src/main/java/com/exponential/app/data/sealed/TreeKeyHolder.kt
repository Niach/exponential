package com.exponential.app.data.sealed

import com.exponential.app.data.api.HeldTreeKey
import com.exponential.app.data.api.PendingTreeKey
import com.exponential.app.data.api.PendingWrap
import kotlin.coroutines.cancellation.CancellationException

/** What a holder pass needs from tRPC `sessionKeys` (the API in the app, a fake in tests). */
interface HolderApi {
    suspend fun mine(installId: String): List<HeldTreeKey>
    suspend fun pending(installId: String): List<PendingTreeKey>
    suspend fun wrap(installId: String, wraps: List<PendingWrap>)
}

/**
 * EXP-1459: one account's install as a tree-key HOLDER (sealed-content.json
 * KEY API), run on node start and every minute while online:
 *
 * 1. `mine`: every wrap this install holds that it has not unwrapped yet is
 *    opened with the install's X25519 secret (`PushKeypair::derive(identity)`)
 *    and kept in the account's vault + [SealedKeys].
 * 2. `pending`: every reader still missing the current epoch's wrap on a
 *    tree this install holds gets one (`wrap`), automatically, no approval.
 */
class TreeKeyHolder(
    private val accountId: String,
    private val installId: String,
    private val installSecret: ByteArray,
    private val crypto: SealedCrypto,
    private val vault: TreeKeyVault,
    private val api: HolderApi,
    private val publish: (TreeKeys) -> Unit = SealedKeys::merge,
) {
    /** One pass; false when a step failed (the next pass retries). */
    suspend fun pass(): Boolean {
        val fetched = step { fetchMine() }
        val wrapped = step { answerPending() }
        return fetched && wrapped
    }

    /** Unwraps what `mine` lists and this install does not hold yet. */
    internal suspend fun fetchMine() {
        val held = vault.all(accountId)
        val fresh = mutableMapOf<String, MutableMap<Int, ByteArray>>()
        for (wrap in api.mine(installId)) {
            if (held[wrap.rootId]?.containsKey(wrap.epoch) == true) continue
            // A wrap that does not open (another identity's, tampered) is skipped.
            val secret = runCatching { crypto.unwrapTreeKey(installSecret, wrap.wrapped) }.getOrNull() ?: continue
            fresh.getOrPut(wrap.rootId) { mutableMapOf() }[wrap.epoch] = secret
        }
        if (fresh.isEmpty()) return
        vault.put(accountId, fresh)
        publish(fresh)
    }

    /** Wraps the current epoch for every reader `pending` lists (batches of its limit). */
    internal suspend fun answerPending() {
        val held = vault.all(accountId)
        val pairs = api.pending(installId)
        val wraps = pairs.mapNotNull { pair ->
            val secret = held[pair.rootId]?.get(pair.epoch) ?: return@mapNotNull null
            val wrapped = runCatching { crypto.wrapTreeKey(pair.targetX25519Public, secret) }.getOrNull() ?: return@mapNotNull null
            PendingWrap(pair.rootId, pair.epoch, pair.targetInstallId, wrapped)
        }
        if (wraps.isNotEmpty()) api.wrap(installId, wraps)
    }

    private inline fun step(block: () -> Unit): Boolean = try {
        block()
        true
    } catch (err: CancellationException) {
        throw err
    } catch (_: Throwable) {
        false
    }
}
