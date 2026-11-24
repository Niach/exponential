package com.exponential.app.data.sealed

import com.exponential.app.data.api.CreateSessionKeyInput
import com.exponential.app.data.api.DueTreeKey
import com.exponential.app.data.api.HeldTreeKey
import com.exponential.app.data.api.PendingTreeKey
import com.exponential.app.data.api.PendingWrap
import com.exponential.app.data.api.TreeKeyWrap
import com.exponential.app.data.api.TrpcException
import kotlin.coroutines.cancellation.CancellationException

/** What a holder pass needs from tRPC `sessionKeys` (the API in the app, a fake in tests). */
interface HolderApi {
    /** [rootIds] = only these trees (#EXP-1941: a call pulls its one root). */
    suspend fun mine(installId: String, rootIds: List<String>? = null): List<HeldTreeKey>
    suspend fun pending(installId: String): List<PendingTreeKey>
    suspend fun wrap(installId: String, wraps: List<PendingWrap>)

    /** #EXP-1951 `sessionKeys.due`: the roots this install rotates. */
    suspend fun due(installId: String): List<DueTreeKey> = emptyList()

    /** `sessionKeys.create` (a rotation); throws the server's refusal. */
    suspend fun create(input: CreateSessionKeyInput) {
        error("create unsupported")
    }
}

/**
 * EXP-1459: one account's install as a tree-key HOLDER (sealed-content.json
 * KEY API), run on node start and every minute while online:
 *
 * 1. `mine`: every wrap this install holds that it has not unwrapped yet is
 *    opened with the install's X25519 secret (`PushKeypair::derive(identity)`)
 *    and kept in the account's vault + [SealedKeys].
 * 2. `due` (#EXP-1951, `rotation`): a root whose rotation is due (a reader
 *    was dropped) and whose current secret this install holds gets a fresh
 *    key at the next epoch, wrapped for THIS install only.
 * 3. `pending`: every reader still missing the current epoch's wrap on a
 *    tree this install holds gets one (`wrap`), automatically, no approval:
 *    the server lists readers and nobody else.
 */
class TreeKeyHolder(
    private val accountId: String,
    private val installId: String,
    private val installSecret: ByteArray,
    private val crypto: SealedCrypto,
    private val vault: TreeKeyVault,
    private val api: HolderApi,
    /** The install's own X25519 public key (base64url); null = it never rotates. */
    private val installPublic: String? = null,
    private val publish: (TreeKeys) -> Unit = SealedKeys::merge,
) {
    /** #EXP-1951: rotations the server refused this install, (root, epoch). */
    private val left = mutableSetOf<Pair<String, Int>>()

    /** One pass; false when a step failed (the next pass retries). */
    suspend fun pass(): Boolean {
        val fetched = step { fetchMine() }
        val rotated = step { rotateDue() }
        val wrapped = step { answerPending() }
        return fetched && rotated && wrapped
    }

    /**
     * #EXP-1941: a call's pull of ONE root, not throttled (the call waits on
     * it, and `mine` nudges the devices that hold the key): the secret of
     * ([rootId], [epoch]), null while no wrap for it opened.
     */
    suspend fun pull(rootId: String, epoch: Int): ByteArray? {
        vault.all(accountId)[rootId]?.get(epoch)?.let { return it }
        fetchMine(listOf(rootId))
        return vault.all(accountId)[rootId]?.get(epoch)
    }

    /** Unwraps what `mine` lists and this install does not hold yet. */
    internal suspend fun fetchMine(rootIds: List<String>? = null) {
        val held = vault.all(accountId)
        val fresh = mutableMapOf<String, MutableMap<Int, ByteArray>>()
        for (wrap in api.mine(installId, rootIds)) {
            if (held[wrap.rootId]?.containsKey(wrap.epoch) == true) continue
            // A wrap that does not open (another identity's, tampered) is skipped.
            val secret = runCatching { crypto.unwrapTreeKey(installSecret, wrap.wrapped) }.getOrNull() ?: continue
            fresh.getOrPut(wrap.rootId) { mutableMapOf() }[wrap.epoch] = secret
        }
        if (fresh.isEmpty()) return
        vault.put(accountId, fresh)
        publish(fresh)
    }

    /** #EXP-1951: rotates every due root whose current secret this install holds; returns how many. */
    internal suspend fun rotateDue(): Int {
        val own = installPublic ?: return 0
        val due = api.due(installId)
        if (due.isEmpty()) return 0
        // The server holds a wrap of each for this install: unwrap what is missing.
        val missing = vault.all(accountId).let { held -> due.filter { held[it.rootId]?.containsKey(it.epoch) != true }.map { it.rootId } }
        if (missing.isNotEmpty()) fetchMine(missing)
        var rotated = 0
        for (root in due) {
            val held = vault.all(accountId)[root.rootId].orEmpty()
            val epoch = rotationEpoch(
                flagged = root.epoch,
                current = maxOf(root.epoch, held.keys.maxOrNull() ?: 0),
                held = held.containsKey(root.epoch),
                left = (root.rootId to root.epoch) in left,
            ) ?: continue
            val key = crypto.generateTreeKey()
            val failure = try {
                api.create(CreateSessionKeyInput(root.rootId, installId, epoch, key.publicKey, listOf(TreeKeyWrap(installId, crypto.wrapTreeKey(own, key.secret)))))
                null
            } catch (err: CancellationException) {
                throw err
            } catch (err: TrpcException) {
                createFailure(err.code, err.message)
            } catch (_: Exception) {
                CreateFailure.Transient
            }
            when (rotationAfter(failure)) {
                RotationAfter.Adopt -> {
                    val fresh = mapOf(root.rootId to mapOf(epoch to key.secret))
                    vault.put(accountId, fresh)
                    publish(fresh)
                    rotated++
                }
                RotationAfter.Reread -> runCatching { fetchMine(listOf(root.rootId)) }.onFailure { if (it is CancellationException) throw it }
                RotationAfter.Leave -> left += root.rootId to root.epoch
                RotationAfter.Retry -> Unit
            }
        }
        return rotated
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
