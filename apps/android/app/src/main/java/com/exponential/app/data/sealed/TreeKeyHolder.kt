package com.exponential.app.data.sealed

import com.exponential.app.data.api.CreateSessionKeyInput
import com.exponential.app.data.api.DueTreeKey
import com.exponential.app.data.api.HeldTreeKey
import com.exponential.app.data.api.PendingTreeKey
import com.exponential.app.data.api.PendingWrap
import com.exponential.app.data.api.RESIGN_MAX
import com.exponential.app.data.api.ResignResult
import com.exponential.app.data.api.ResignWrap
import com.exponential.app.data.api.TreeKeyWrap
import com.exponential.app.data.api.TreePublicKey
import com.exponential.app.data.api.TrpcException
import io.ktor.http.HttpStatusCode
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

    /** #EXP-1950 `sessionKeys.publicKeys`: the current key of each root. */
    suspend fun publicKeys(rootIds: List<String>): List<TreePublicKey> = emptyList()

    /** #EXP-1950 `sessionKeys.resign` (default: a server without it). */
    suspend fun resign(installId: String, wraps: List<ResignWrap>): ResignResult =
        throw TrpcException("No procedure sessionKeys.resign", HttpStatusCode.NotFound, code = "NOT_FOUND")
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
 * 3. `pending`: every reader still missing the wrap of an epoch this install
 *    holds (the current one; #EXP-2119: an older one too) gets one (`wrap`), automatically, no approval:
 *    the server lists readers and nobody else.
 * 4. #EXP-1950 (wrap-signature.json): every wrap this holder MAKES carries the
 *    install's signature ([identity] through [signer]): a rotation's own
 *    wrap and each `pending` answer. `resign` (decision c): this install's
 *    own unsigned row of a root's current epoch, and a new root's inline
 *    seal ([queueResign]: unsigned while its root id was unknown), get a
 *    fresh signed wrap, once per root, at most [RESIGN_MAX] per pass; an
 *    older server (NOT_FOUND) is asked again after [RESIGN_NOT_YET_MS].
 * 5. #EXP-1950 (install-trust.json): with an [InstallTrustKeeper] ([trust])
 *    it follows `installs.trust` first on every pass, keeps a wrap only by R1
 *    (recorded), wraps only for targets R4 allows (an install that asked and
 *    is not approved, or an own install nobody pinned, gets nothing; a
 *    waiting install wraps for itself only), and answers R2 + R3 for a send
 *    ([sealAllowed]) and a call's key from its keyring ([callSecret]).
 *    Release 1 records; it refuses only what the fixture refuses now. None =
 *    as before.
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
    /** #EXP-1950: the install's eNet identity; null (or no [signer]) = it signs nothing. */
    private val identity: ByteArray? = null,
    private val signer: WrapSigner? = null,
    /** #EXP-1950: the pin store and the four rules (null = off, as before). */
    val trust: InstallTrustKeeper? = null,
    private val now: () -> Long = System::currentTimeMillis,
    private val publish: (TreeKeys) -> Unit = SealedKeys::merge,
) {
    /** #EXP-1951: rotations the server refused this install, (root, epoch). */
    private val left = mutableSetOf<Pair<String, Int>>()

    /** #EXP-1950: this install's endpoint id, when it signs. */
    private val endpointId: String? by lazy { identity?.let { id -> signer?.let { runCatching { it.endpointIdOf(id) }.getOrNull() } } }

    /** An adopted new root whose seal went unsigned. */
    private class FreshRoot(val rootId: String, val epoch: Int, val treePublicKey: String, var targets: List<WrapTarget>)

    private val resignLock = Any()
    /** Roots whose own row of the highest held epoch is unsigned (`mine`). */
    private val unsignedOwn = mutableMapOf<String, Int>()
    private val fresh = mutableListOf<FreshRoot>()
    /** (root, epoch) signed again or refused: never asked again. */
    private val resigned = mutableSetOf<Pair<String, Int>>()
    /** Roots a refused batch named: tried one per call. */
    private val alone = mutableSetOf<String>()
    private var notYetUntil = 0L

    /** One pass; false when a step failed (the next pass retries). */
    suspend fun pass(): Boolean {
        step { trust?.sync(accountId, installId) }
        val fetched = step { fetchMine() }
        val rotated = step { rotateDue() }
        val wrapped = step { answerPending() }
        step { resignDue() }
        return fetched && rotated && wrapped
    }

    /** #EXP-1950: the signature of a wrap THIS holder just made; null = unsigned (no identity, no recipient, a malformed field). */
    private fun sign(rootId: String, epoch: Int, treePublicKey: String, recipientEndpointId: String?, wrapped: String): String? {
        val id = identity ?: return null
        val recipient = recipientEndpointId ?: return null
        return runCatching { signer?.sign(id, rootId, epoch, treePublicKey, recipient, wrapped) }.getOrNull()
    }

    /** Whether [secret] is the secret half of [publicKey] (a seal to it opens). */
    private fun opens(secret: ByteArray, publicKey: String, epoch: Int): Boolean = runCatching {
        crypto.openBody(crypto.sealBody(publicKey, epoch, KEY_CHECK), mapOf(epoch to secret)).text == KEY_CHECK
    }.getOrDefault(false)

    /**
     * #EXP-1950: a new root's inline seal went unsigned (its root id was
     * unknown while sealing); now that [rootId] exists its wraps for
     * [targets] are made again, signed, on the next pass.
     */
    fun queueResign(rootId: String, epoch: Int, treePublicKey: String, targets: List<WrapTarget>) {
        if (endpointId == null) return
        synchronized(resignLock) { fresh += FreshRoot(rootId, epoch, treePublicKey, targets) }
    }

    /**
     * #EXP-1941: a call's pull of ONE root, not throttled (the call waits on
     * it, and `mine` nudges the devices that hold the key): the secret of
     * ([rootId], [epoch]), null while no wrap for it opened.
     */
    suspend fun pull(rootId: String, epoch: Int): ByteArray? {
        if (trust != null) return callSecret(rootId, epoch)
        return pullHeld(rootId, epoch)
    }

    private suspend fun pullHeld(rootId: String, epoch: Int): ByteArray? {
        vault.all(accountId)[rootId]?.get(epoch)?.let { return it }
        fetchMine(listOf(rootId))
        return vault.all(accountId)[rootId]?.get(epoch)
    }

    /** Unwraps what `mine` lists and this install does not hold yet. */
    internal suspend fun fetchMine(rootIds: List<String>? = null) {
        val held = vault.all(accountId)
        val fresh = mutableMapOf<String, MutableMap<Int, ByteArray>>()
        val mine = api.mine(installId, rootIds)
        noteUnsigned(mine)
        val before = mutableListOf<Pair<String, Int>>()
        for (wrap in mine) {
            if (held[wrap.rootId]?.containsKey(wrap.epoch) == true) {
                before += wrap.rootId to wrap.epoch
                continue
            }
            val secret = if (trust != null) {
                // R1 (install-trust.json `keepCases`): kept and recorded, or refused.
                when (val verdict = trust.keep(accountId, wrap, installSecret)) {
                    is InstallTrust.KeepVerdict.Keep -> verdict.secret
                    is InstallTrust.KeepVerdict.Refuse -> continue
                }
            } else {
                // A wrap that does not open (another identity's, tampered) is skipped.
                runCatching { crypto.unwrapTreeKey(installSecret, wrap.wrapped) }.getOrNull() ?: continue
            }
            fresh.getOrPut(wrap.rootId) { mutableMapOf() }[wrap.epoch] = secret
        }
        // Decision (c): a key held before the pin store knew it stays, `legacy`.
        if (before.isNotEmpty()) trust?.recordLegacy(accountId, before)
        if (fresh.isEmpty()) return
        vault.put(accountId, fresh)
        publish(fresh)
    }

    /** #EXP-1950 decision (c): the roots whose own row of the highest held epoch is unsigned. */
    private fun noteUnsigned(mine: List<HeldTreeKey>) {
        if (endpointId == null) return
        val highest = mine.groupBy { it.rootId }.mapValues { (_, rows) -> rows.maxBy { it.epoch } }
        synchronized(resignLock) {
            for ((root, row) in highest) {
                if (row.signature == null && (root to row.epoch) !in resigned) unsignedOwn[root] = row.epoch else unsignedOwn.remove(root)
            }
        }
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
            val wrapped = crypto.wrapTreeKey(own, key.secret)
            val signature = sign(root.rootId, epoch, key.publicKey, endpointId, wrapped)
            val failure = try {
                api.create(CreateSessionKeyInput(root.rootId, installId, epoch, key.publicKey, listOf(TreeKeyWrap(installId, wrapped, signature))))
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
                    // It made the key itself: verified.
                    trust?.record(accountId, root.rootId, epoch, KeyRecord.Verified)
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

    private fun heldPublic(rootId: String, epoch: Int, secret: ByteArray): String? =
        identity?.let { id -> signer?.treePublicKey(id, installSecret, rootId, epoch, secret) }

    /** Wraps every pair `pending` lists (batches of its limit): the held secret of exactly (root, epoch), not held = skipped. */
    internal suspend fun answerPending() {
        val held = vault.all(accountId)
        val pairs = api.pending(installId)
        val wraps = pairs.mapNotNull { pair ->
            val secret = held[pair.rootId]?.get(pair.epoch) ?: return@mapNotNull null
            // R4: never for a target that asked and is not approved, or whose key is not the pinned one.
            if (trust?.target(accountId, pair.targetInstallId, pair.targetX25519Public, pair.targetEndpointId, null)?.wraps == false) return@mapNotNull null
            val wrapped = runCatching { crypto.wrapTreeKey(pair.targetX25519Public, secret) }.getOrNull() ?: return@mapNotNull null
            // #EXP-1950: signed over the key this install HOLDS; a root whose
            // stored key is another goes unsigned (the server would refuse the batch).
            // #EXP-2119: an older epoch the server knows no key of names the held secret's own.
            val publicKey = pair.publicKey.ifEmpty { heldPublic(pair.rootId, pair.epoch, secret) }
            val signature = if (endpointId != null && pair.targetEndpointId != null && publicKey != null && opens(secret, publicKey, pair.epoch)) {
                sign(pair.rootId, pair.epoch, publicKey, pair.targetEndpointId, wrapped)
            } else {
                null
            }
            PendingWrap(pair.rootId, pair.epoch, pair.targetInstallId, wrapped, signature)
        }
        if (wraps.isNotEmpty()) api.wrap(installId, wraps)
    }

    /** One root's wraps to sign again. */
    private class ResignJob(val rootId: String, val epoch: Int, val treePublicKey: String?, val targets: List<WrapTarget>)

    /**
     * #EXP-1950 (wrap-signature.json `resign`): fresh signed wraps of a root's
     * current epoch replacing unsigned rows: an adopted new root's (its
     * seal's targets) and this install's own rows of keys held from before.
     * One batch of at most [RESIGN_MAX] wraps plus [RESIGN_ALONE_PER_PASS]
     * single roots per pass; returns how many rows the server replaced.
     */
    internal suspend fun resignDue(): Int {
        val ownTarget = endpointId?.let { endpoint -> installPublic?.let { WrapTarget(installId, endpoint, it) } } ?: return 0
        val jobs = synchronized(resignLock) {
            if (now() < notYetUntil) return 0
            val queued = fresh.map { ResignJob(it.rootId, it.epoch, it.treePublicKey, it.targets) }
            queued + unsignedOwn.filterKeys { root -> queued.none { it.rootId == root } }.map { (root, epoch) -> ResignJob(root, epoch, null, listOf(ownTarget)) }
        }
        if (jobs.isEmpty()) return 0
        val (single, batched) = synchronized(resignLock) { jobs.partition { it.rootId in alone } }
        // Own rows: the root's current key, checked against the held secret.
        val needKeys = (batched.take(RESIGN_MAX) + single.take(RESIGN_ALONE_PER_PASS)).filter { it.treePublicKey == null }.map { it.rootId }
        val keys = needKeys.chunked(KEYS_CHUNK).flatMap { api.publicKeys(it) }.associateBy { it.rootId }
        val calls = mutableListOf<List<Pair<ResignJob, List<ResignWrap>>>>()
        val packed = mutableListOf<Pair<ResignJob, List<ResignWrap>>>()
        var count = 0
        for (job in batched) {
            val wraps = resignWraps(job, keys) ?: continue
            if (count + wraps.size > RESIGN_MAX) continue
            count += wraps.size
            packed += job to wraps
        }
        if (packed.isNotEmpty()) calls += packed
        for (job in single.take(RESIGN_ALONE_PER_PASS)) resignWraps(job, keys)?.let { calls += listOf(job to it) }
        var replaced = 0
        for (call in calls) {
            val after = try {
                replaced += api.resign(installId, call.flatMap { it.second }).replaced
                null
            } catch (err: CancellationException) {
                throw err
            } catch (err: TrpcException) {
                resignAfter(err.code ?: if (err.status == HttpStatusCode.NotFound) "NOT_FOUND" else null)
            } catch (_: Exception) {
                ResignAfter.Retry
            }
            synchronized(resignLock) {
                when (after) {
                    null -> call.forEach { finishResign(it.first) }
                    ResignAfter.NotYet -> notYetUntil = now() + RESIGN_NOT_YET_MS
                    ResignAfter.Refused -> if (call.size > 1) {
                        call.forEach { alone += it.first.rootId }
                    } else {
                        // A new root's target may be no reader any more: its own row alone next.
                        val job = call.single().first
                        val queued = fresh.firstOrNull { it.rootId == job.rootId && it.epoch == job.epoch }
                        if (queued != null && queued.targets != listOf(ownTarget)) queued.targets = listOf(ownTarget) else finishResign(job)
                    }
                    ResignAfter.Retry -> Unit
                }
            }
            if (after == ResignAfter.NotYet) break
        }
        return replaced
    }

    /** Fresh signed wraps of [job]'s key, never the bytes of a wrap this holder did not make; null = nothing to sign. */
    private fun resignWraps(job: ResignJob, keys: Map<String, TreePublicKey>): List<ResignWrap>? {
        val secret = vault.all(accountId)[job.rootId]?.get(job.epoch)
        val publicKey = job.treePublicKey ?: keys[job.rootId]?.takeIf { it.epoch == job.epoch }?.publicKey
        val wraps = if (secret == null || publicKey == null || !opens(secret, publicKey, job.epoch)) {
            emptyList()
        } else {
            job.targets.mapNotNull { target ->
                // R4 over a new root's readers: this install always, the others only where the rule wraps for them.
                if (target.installId != installId && trust?.target(accountId, target.installId, target.x25519Public, target.endpointId, null)?.wraps == false) {
                    return@mapNotNull null
                }
                val wrapped = runCatching { crypto.wrapTreeKey(target.x25519Public, secret) }.getOrNull() ?: return@mapNotNull null
                val signature = sign(job.rootId, job.epoch, publicKey, target.endpointId, wrapped) ?: return@mapNotNull null
                ResignWrap(job.rootId, job.epoch, target.installId, wrapped, signature)
            }
        }
        if (wraps.isEmpty()) {
            // Not its current key any more, or nothing it can sign: not again.
            synchronized(resignLock) { finishResign(job) }
            return null
        }
        return wraps
    }

    /**
     * #EXP-1950 R2 + R3 (install-trust.json `sealCases`): whether a send seals
     * to [publicKey] at [epoch] of the existing root [rootId]: only to the key
     * of a held secret of that epoch, or one a pinned install vouched for;
     * release 1 seals an unvouched key and records it, never a refused one.
     */
    fun sealAllowed(rootId: String, publicKey: String, epoch: Int, vouch: Vouch?): Boolean {
        val trust = trust ?: return true
        val held = vault.all(accountId)[rootId]?.get(epoch)
        return trust.seal(accountId, rootId, publicKey, epoch, vouch, held) is InstallTrust.SealVerdict.Seal
    }

    /**
     * #EXP-1950 (install-trust.json `callCases`): a sealed call's key, from
     * this install's keyring ONLY (one pull when missing); null = `no_key`.
     * Release 1 takes any kept key, its record says what it is.
     */
    suspend fun callSecret(rootId: String, epoch: Int): ByteArray? {
        val held = pullHeld(rootId, epoch)
        val trust = trust ?: return held
        return trust.callKey(accountId, rootId, epoch, held)
    }

    /** #EXP-1950 R4 over a writer's readers ([WrapTarget] + its user when known): this install always, the others only where the rule wraps for them. */
    fun allowed(targets: List<Pair<WrapTarget, String?>>): List<WrapTarget> {
        val trust = trust ?: return targets.map { it.first }
        return targets.filter { (target, userId) ->
            target.installId == installId || trust.target(accountId, target.installId, target.x25519Public, target.endpointId, userId).wraps
        }.map { it.first }
    }

    /** #EXP-1950 `newRoot.makerRecord`: a key this install made (a new root, a key it created) is `verified`. */
    fun recordMade(rootId: String, epoch: Int) {
        trust?.record(accountId, rootId, epoch, KeyRecord.Verified)
    }

    private fun finishResign(job: ResignJob) {
        resigned += job.rootId to job.epoch
        fresh.removeAll { it.rootId == job.rootId && it.epoch == job.epoch }
        if (unsignedOwn[job.rootId] == job.epoch) unsignedOwn.remove(job.rootId)
        alone.remove(job.rootId)
    }

    private companion object {
        /** #EXP-1950: how long a server without `resign` is left alone. */
        const val RESIGN_NOT_YET_MS = 30 * 60 * 1000L
        /** The most single-root `resign` calls one pass makes. */
        const val RESIGN_ALONE_PER_PASS = 8
        const val KEYS_CHUNK = 100
        const val KEY_CHECK = "k"
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
