package com.exponential.app.data.sealed

import android.util.Log
import com.exponential.app.data.api.HeldTreeKey
import com.exponential.app.data.api.InstallTrustApi
import com.exponential.app.data.api.RootClaimApproval
import com.exponential.app.data.api.TrpcException
import io.ktor.http.HttpStatusCode
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.serialization.json.Json

// #EXP-1950 (fixtures/install-trust.json; iOS `InstallTrustKeeper.swift`, web
// `lib/enet/install-trust.ts`): this install's side of install approval and
// the four rules, per account, around the pure [InstallTrust]. It follows
// `installs.trust` into the pin store (the Keystore vault beside the tree
// keys, [TrustVault]), claims its account's root when there is no chain (with
// the approvals of the installs that existed with it, decision e), asks ONCE
// to be approved when it waits, and answers for the tree-key holder whether a
// wrap is kept (R1), a key sealed to (R2, R3) and a target wrapped for (R4).
// Release 1 is RECORD-ONLY ([TrustMode.BUILD]) for keys; the one thing it
// refuses that a released build did is a wrap for an install that ASKED (or
// claims a root) and is not approved, or an own install nobody pinned (and a
// waiting install wraps for nobody but itself).
// It APPROVES a new install only when a person presses Approve; by itself
// only the installs that existed with it once (`approveExisting`).
// An older server without `installs.trust` (NOT_FOUND) = `unavailable`:
// every target is wrapped as before and recorded `legacy`.

/** The persistent side of the pin store: one sealed slot per account. */
interface TrustStorePersistence {
    /** null = no store for the account (decision e: an existing install);
     *  THROWS when one is there but cannot be read: never taken for none. */
    fun load(accountId: String): String?
    fun save(accountId: String, json: String): Boolean
    fun remove(accountId: String)

    /** In memory (tests). */
    class Memory : TrustStorePersistence {
        private val items = mutableMapOf<String, String>()

        @Synchronized override fun load(accountId: String): String? = items[accountId]

        @Synchronized override fun save(accountId: String, json: String): Boolean {
            items[accountId] = json
            return true
        }

        @Synchronized override fun remove(accountId: String) {
            items.remove(accountId)
        }
    }
}

/** `trusted` / `waiting` / `claimRoot` ([TrustStatus]), `unknown` before the
 *  first answer, `unavailable` on a server without `installs.trust`. */
enum class InstallTrustState { Unknown, Unavailable, Trusted, Waiting, ClaimRoot }

/** What the UI draws: the rows (Needs you, the phone's Devices) and this
 *  install's own waiting state with its code. */
data class InstallTrustSnapshot(
    val state: InstallTrustState,
    val rows: List<TrustRow>,
    /** This install's own code (shown while it waits). */
    val code: String?,
) {
    /** Waiting for approval: the waiting EmptyState instead of the rows. */
    val waiting: Boolean get() = state == InstallTrustState.Waiting && code != null

    /** The installs Needs you counts: 0 while this install itself waits. */
    val installs: Int get() = if (waiting) 0 else rows.size

    companion object {
        val NONE = InstallTrustSnapshot(InstallTrustState.Unknown, emptyList(), null)
    }
}

/** One recorded decision (the newest last, bounded). */
data class TrustDecision(val rule: Rule, val subject: String, val verdict: String) {
    enum class Rule { R1, R3, R4, Call }
}

class InstallTrustKeeper(
    private val api: InstallTrustApi,
    private val crypto: TrustCrypto,
    private val persistence: TrustStorePersistence,
    /** The account's eNet identity on this install (never created here). */
    private val identity: (accountId: String) -> ByteArray?,
    val mode: TrustMode = TrustMode.BUILD,
) {
    private class Account(
        var store: TrustStore,
        var installId: String? = null,
        var state: InstallTrustState = InstallTrustState.Unknown,
        var rows: List<TrustRow> = emptyList(),
        var served: Map<String, ServedInstall> = emptyMap(),
        var roots: Set<String> = emptySet(),
        var asked: Boolean = false,
        /** The kept store could not be read: never cached, never synced (a write would replace it). */
        val unreadable: Boolean = false,
    )

    private val lock = Any()
    private val accounts = mutableMapOf<String, Account>()
    private val endpoints = mutableMapOf<String, Pair<ByteArray, String>>()
    private val decisionLog = ArrayDeque<TrustDecision>()

    private val _snapshots = MutableStateFlow<Map<String, InstallTrustSnapshot>>(emptyMap())

    /** Per account: what the UI draws. */
    val snapshots: StateFlow<Map<String, InstallTrustSnapshot>> = _snapshots.asStateFlow()

    val decisions: List<TrustDecision> get() = synchronized(lock) { decisionLog.toList() }

    /** This install's endpoint id for [accountId] (its identity's), null without one. */
    fun endpointId(accountId: String): String? {
        val seed = identity(accountId) ?: return null
        synchronized(lock) {
            endpoints[accountId]?.let { (cached, id) -> if (cached.contentEquals(seed)) return id }
        }
        val id = runCatching { crypto.endpointId(seed) }.getOrNull() ?: return null
        synchronized(lock) {
            // A rotated identity: the store kept for the old one is another install's.
            if (endpoints.put(accountId, seed to id) != null) accounts.remove(accountId)
        }
        return id
    }

    /**
     * The account's pin store as kept (its own endpoint's). None at all =
     * decision (e): this install EXISTED (a build that mints the identity
     * writes the store first, [markNew]), so it may adopt at its first
     * reconcile. A store kept for another endpoint (a rotated identity) or
     * that does not decode is a new install's; one that cannot be read is
     * never none.
     */
    private fun account(accountId: String): Account? {
        val endpoint = endpointId(accountId) ?: return null
        synchronized(lock) {
            accounts[accountId]?.let { if (it.store.selfEndpointId == endpoint) return it }
            val data = try {
                persistence.load(accountId)
            } catch (err: Exception) {
                Log.w(TAG, "the pin store could not be read: ${err.javaClass.simpleName}")
                return Account(TrustStore(endpoint, ""), unreadable = true)
            }
            val kept = data?.let { runCatching { json.decodeFromString(TrustStore.serializer(), it) }.getOrNull() }
                ?.takeIf { it.selfEndpointId == endpoint }
            val fresh = Account(kept ?: TrustStore(endpoint, "", existing = data == null))
            accounts[accountId] = fresh
            return fresh
        }
    }

    private fun persist(accountId: String) {
        val store = synchronized(lock) { accounts[accountId]?.takeIf { !it.unreadable }?.store } ?: return
        persistence.save(accountId, json.encodeToString(TrustStore.serializer(), store))
    }

    fun snapshot(accountId: String): InstallTrustSnapshot {
        val found = account(accountId) ?: return InstallTrustSnapshot.NONE
        return synchronized(lock) { InstallTrustSnapshot(found.state, found.rows, InstallCode.of(found.store.selfEndpointId)) }
    }

    /** The pin store (tests, the holder's records). */
    fun store(accountId: String): TrustStore? = account(accountId)?.let { synchronized(lock) { it.store } }

    private fun publish(accountId: String) {
        val next = snapshot(accountId)
        _snapshots.update { if (it[accountId] == next) it else it + (accountId to next) }
    }

    private fun set(accountId: String, state: InstallTrustState, rows: List<TrustRow>? = null) {
        synchronized(lock) {
            val found = accounts[accountId] ?: return
            found.state = state
            if (rows != null) found.rows = rows
        }
        publish(accountId)
    }

    // ── following the server ──────────────────────────────────────────────

    /** Follows `installs.trust` once: claims the root when the account has no
     *  chain (with the existing installs' approvals), approves the existing
     *  ones its chain does not reach yet, asks once while it waits. 3 rounds:
     *  a CONFLICT (another install claimed first) is read next round. */
    suspend fun sync(accountId: String, installId: String) {
        val seed = identity(accountId) ?: return
        val current = account(accountId) ?: return
        if (current.unreadable) return
        synchronized(lock) { current.installId = installId }
        repeat(ROUNDS) {
            val answer = try {
                api.trust(accountId, installId)
            } catch (err: CancellationException) {
                throw err
            } catch (err: Exception) {
                if (err is TrpcException && (err.code == "NOT_FOUND" || (err.code == null && err.status == HttpStatusCode.NotFound))) {
                    set(accountId, InstallTrustState.Unavailable, emptyList())
                }
                return
            }
            val view: TrustView
            val found: Account
            synchronized(lock) {
                found = accounts[accountId] ?: return
                // The account's user, from this install's own row.
                val me = answer.installs.firstOrNull { it.installId == installId }
                if (me != null && me.userId != found.store.userId) {
                    found.store = if (found.store.userId.isEmpty()) {
                        found.store.copy(userId = me.userId)
                    } else {
                        TrustStore(found.store.selfEndpointId, me.userId)
                    }
                }
                if (found.store.userId.isEmpty()) return
                view = InstallTrust.reconcile(found.store, answer, crypto)
                found.store = view.store
                found.served = answer.installs.associateBy { it.endpointId }
                found.roots = answer.approvals.filter { it.kind == ApprovalKind.Root && it.approverEndpointId == it.subjectEndpointId }.map { it.subjectEndpointId }.toSet()
            }
            persist(accountId)
            set(accountId, InstallTrustState.valueOf(view.status.name), view.rows)
            val me = found.served[view.store.selfEndpointId]
            when (view.status) {
                TrustStatus.ClaimRoot -> {
                    me ?: return
                    val signature = runCatching { InstallApproval.sign(ApprovalKind.Root, me, seed, crypto) }.getOrNull() ?: return
                    val approvals = existingApprovals(view.approveExisting, found, seed)
                    try {
                        api.claimRoot(accountId, installId, signature, approvals)
                        Log.i(TAG, "claimed the root of this account's install chain")
                    } catch (err: CancellationException) {
                        throw err
                    } catch (_: Exception) {
                        // A CONFLICT (another install started the chain meanwhile) is read next round.
                    }
                    return@repeat
                }
                TrustStatus.Trusted -> for (approval in existingApprovals(view.approveExisting, found, seed)) {
                    try {
                        api.approve(accountId, installId, approval.subjectInstallId, ApprovalKind.Approve, approval.signature)
                    } catch (err: CancellationException) {
                        throw err
                    } catch (_: Exception) {
                    }
                }
                TrustStatus.Waiting -> if (!found.asked && me?.approvalRequested != true) {
                    found.asked = try {
                        api.requestApproval(accountId, installId)
                        true
                    } catch (err: CancellationException) {
                        throw err
                    } catch (_: Exception) {
                        false
                    }
                }
            }
            return
        }
    }

    /** The `approve`s of the installs that existed with this one (decision e), over the key it pinned them with. */
    private fun existingApprovals(endpointIds: List<String>, account: Account, seed: ByteArray): List<RootClaimApproval> = endpointIds.mapNotNull { endpointId ->
        val row = account.served[endpointId] ?: return@mapNotNull null
        val pin = account.store.pins[endpointId] ?: return@mapNotNull null
        val message = InstallApproval.message(ApprovalKind.Approve, pin.userId, endpointId, pin.x25519Public) ?: return@mapNotNull null
        val signature = runCatching { crypto.sign(seed, message) }.getOrNull() ?: return@mapNotNull null
        RootClaimApproval(row.installId, signature)
    }

    /** A person pressed Approve on a row. */
    suspend fun approve(accountId: String, row: TrustRow) {
        val found = account(accountId)
        val installId = found?.takeIf { it.state == InstallTrustState.Trusted }?.installId ?: throw InstallTrustException("not trusted")
        val seed = identity(accountId) ?: throw InstallTrustException("install not found")
        val subject = found.served[row.endpointId]?.takeIf { it.installId == row.installId } ?: throw InstallTrustException("install not found")
        val kind = if (row.kind == TrustRow.Kind.Reset) ApprovalKind.Reset else ApprovalKind.Approve
        val signature = InstallApproval.sign(kind, subject, seed, crypto)
        api.approve(accountId, installId, row.installId, kind, signature)
        sync(accountId, installId)
    }

    /** A person pressed Remove: an own install is revoked (its wraps go, its
     *  trees rotate); a team-mate's reset is declined. */
    suspend fun remove(accountId: String, row: TrustRow) {
        val found = account(accountId)
        val installId = found?.takeIf { it.state == InstallTrustState.Trusted }?.installId ?: throw InstallTrustException("not trusted")
        when (row.kind) {
            TrustRow.Kind.NewInstall -> api.revoke(accountId, row.installId)
            TrustRow.Kind.Reset -> {
                val seed = identity(accountId) ?: throw InstallTrustException("install not found")
                val subject = found.served[row.endpointId]?.takeIf { it.installId == row.installId } ?: throw InstallTrustException("install not found")
                api.approve(accountId, installId, row.installId, ApprovalKind.Decline, InstallApproval.sign(ApprovalKind.Decline, subject, seed, crypto))
            }
        }
        sync(accountId, installId)
    }

    // ── the rules, for the key holder ─────────────────────────────────────

    /**
     * #EXP-2123 (install-trust.json `keyMaker`): only a JOINED install makes a
     * chat key ([InstallTrustState.Unavailable], an older server without
     * `installs.trust`, counts as joined). A store not reconciled yet
     * ([InstallTrustState.Unknown]) is reconciled first.
     */
    suspend fun makesKeys(accountId: String, installId: String): Boolean {
        val before = account(accountId) ?: return false
        if (synchronized(lock) { before.state } == InstallTrustState.Unknown) sync(accountId, installId)
        val found = account(accountId) ?: return false
        return synchronized(lock) { found.state == InstallTrustState.Unavailable || found.store.joined }
    }

    private fun log(rule: TrustDecision.Rule, subject: String, verdict: String) = synchronized(lock) {
        decisionLog.addLast(TrustDecision(rule, subject, verdict))
        while (decisionLog.size > DECISION_LOG) decisionLog.removeFirst()
    }

    /** R1: whether a served wrap is kept, recorded in the store. */
    fun keep(accountId: String, wrap: HeldTreeKey, installSecret: ByteArray): InstallTrust.KeepVerdict {
        val seed = identity(accountId) ?: return InstallTrust.KeepVerdict.Refuse(WrapRefusal.Malformed)
        val found = account(accountId) ?: return InstallTrust.KeepVerdict.Refuse(WrapRefusal.Malformed)
        val store = synchronized(lock) { found.store }
        val verdict = InstallTrust.keepWrap(wrap, seed, installSecret, store, mode, crypto)
        when (verdict) {
            is InstallTrust.KeepVerdict.Keep -> {
                log(TrustDecision.Rule.R1, "${wrap.rootId}#${wrap.epoch}", verdict.record.wire)
                record(accountId, wrap.rootId, wrap.epoch, verdict.record)
            }
            is InstallTrust.KeepVerdict.Refuse -> log(TrustDecision.Rule.R1, "${wrap.rootId}#${wrap.epoch}", "refused:${verdict.reason.wire}")
        }
        return verdict
    }

    /** What this install recorded about a key it holds (`legacy` = held before, decision c). */
    fun recordOf(accountId: String, rootId: String, epoch: Int): KeyRecord = store(accountId)?.record(rootId, epoch) ?: KeyRecord.Legacy

    /** Records a key (its own new keys are `verified`); persisted. */
    fun record(accountId: String, rootId: String, epoch: Int, record: KeyRecord) {
        val found = account(accountId) ?: return
        if (found.unreadable) return
        synchronized(lock) {
            if (found.store.roots[rootId.lowercase()]?.epochs?.get(epoch.toString()) == record) return
            found.store = found.store.recordKey(rootId, epoch, record)
        }
        persist(accountId)
    }

    /** Decision (c): every key held before the store knew it stays, `legacy`. */
    fun recordLegacy(accountId: String, held: List<Pair<String, Int>>) {
        val found = account(accountId) ?: return
        if (found.unreadable) return
        var changed = false
        synchronized(lock) {
            for ((rootId, epoch) in held) {
                if (found.store.roots[rootId.lowercase()]?.epochs?.get(epoch.toString()) != null) continue
                found.store = found.store.recordKey(rootId, epoch, KeyRecord.Legacy)
                changed = true
            }
        }
        if (changed) persist(accountId)
    }

    /** R2 + R3: whether this install seals to (root, epoch, key) as the server names it. */
    fun seal(accountId: String, rootId: String, publicKey: String, epoch: Int, vouch: Vouch?, held: ByteArray?): InstallTrust.SealVerdict {
        val found = account(accountId) ?: return InstallTrust.SealVerdict.Seal(KeyRecord.Legacy)
        val store = synchronized(lock) { found.store }
        val verdict = InstallTrust.sealTo(
            rootId.lowercase(), epoch, publicKey, held?.let { it to store.record(rootId, epoch) }, vouch, store, mode, crypto,
        )
        when (verdict) {
            is InstallTrust.SealVerdict.Seal -> log(TrustDecision.Rule.R3, "$rootId#$epoch", verdict.record.wire)
            is InstallTrust.SealVerdict.Refuse -> log(TrustDecision.Rule.R3, "$rootId#$epoch", "refused:${verdict.reason.wire}")
        }
        return verdict
    }

    /** R4: whether this install wraps a key for a target the server names
     *  (`pending`'s pair, a reader of a new key, a rotation, a resign). */
    fun target(accountId: String, installId: String, x25519Public: String, endpointId: String?, userId: String?): InstallTrust.TargetVerdict {
        val found = account(accountId) ?: return InstallTrust.TargetVerdict.Refuse(InstallTrust.TargetRefusal.NotTrusted)
        val (state, own, served, roots, store) = synchronized(lock) { Snap(found.state, found.installId, found.served, found.roots, found.store) }
        if (state == InstallTrustState.Unavailable) return InstallTrust.TargetVerdict.Wrap(KeyRecord.Legacy)
        if (installId == own) return InstallTrust.TargetVerdict.Wrap(KeyRecord.Verified)
        val known = endpointId?.let { served[it] } ?: served.values.firstOrNull { it.installId == installId }
        val target = ServedInstall(
            installId = installId,
            userId = userId ?: known?.userId ?: "",
            endpointId = endpointId ?: known?.endpointId ?: "",
            x25519Public = x25519Public,
            registrationSignature = known?.registrationSignature,
            registrationSignedAt = known?.registrationSignedAt,
            approvalRequested = known?.approvalRequested ?: false,
        )
        // The registration binds the key `installs.trust` lists; a target
        // served elsewhere with another key is checked against it.
        val verdict = if (known != null && known.x25519Public != x25519Public && store.pins[target.endpointId] == null) {
            InstallTrust.TargetVerdict.Refuse(InstallTrust.TargetRefusal.KeyMismatch)
        } else {
            InstallTrust.wrapTarget(target, store, mode, roots, crypto)
        }
        when (verdict) {
            is InstallTrust.TargetVerdict.Wrap -> log(TrustDecision.Rule.R4, installId, verdict.record.wire)
            is InstallTrust.TargetVerdict.Refuse -> log(TrustDecision.Rule.R4, installId, "refused:${verdict.reason.wire}")
        }
        return verdict
    }

    private data class Snap(
        val state: InstallTrustState,
        val installId: String?,
        val served: Map<String, ServedInstall>,
        val roots: Set<String>,
        val store: TrustStore,
    )

    /** A call's key: the keyring's secret of (root, epoch), by the rule; null = `no_key`. */
    fun callKey(accountId: String, rootId: String, epoch: Int, held: ByteArray?): ByteArray? {
        val found = account(accountId) ?: return held
        val store = synchronized(lock) { found.store }
        return when (val verdict = InstallTrust.callKey(rootId, epoch, held?.let { it to store.record(rootId, epoch) }, store, mode)) {
            is InstallTrust.CallKeyVerdict.Key -> {
                log(TrustDecision.Rule.Call, "$rootId#$epoch", verdict.record.wire)
                verdict.secret
            }
            InstallTrust.CallKeyVerdict.NoKey -> {
                log(TrustDecision.Rule.Call, "$rootId#$epoch", "noKey")
                null
            }
            InstallTrust.CallKeyVerdict.Rollback -> {
                log(TrustDecision.Rule.Call, "$rootId#$epoch", "rollback")
                null
            }
        }
    }

    /** The account signed out of this install (its identity forgotten): its store goes with it. */
    fun forget(accountId: String) {
        synchronized(lock) {
            accounts.remove(accountId)
            endpoints.remove(accountId)
        }
        persistence.remove(accountId)
        _snapshots.update { it - accountId }
    }

    companion object {
        private const val TAG = "InstallTrust"
        const val ROUNDS = 3
        const val DECISION_LOG = 200
        internal val json = Json { ignoreUnknownKeys = true; encodeDefaults = true; explicitNulls = false }

        /**
         * Decision (e): writes the account's empty store (`existing` false)
         * BEFORE this install registers for it: an identity this build just
         * minted for the account. A store this endpoint already keeps stays;
         * one that cannot be read is left alone.
         */
        fun markNew(accountId: String, identity: ByteArray, crypto: TrustCrypto, persistence: TrustStorePersistence) {
            val endpoint = runCatching { crypto.endpointId(identity) }.getOrNull() ?: return
            val kept = try {
                persistence.load(accountId)
            } catch (_: Exception) {
                return
            }
            val store = kept?.let { runCatching { json.decodeFromString(TrustStore.serializer(), it) }.getOrNull() }
            if (store?.selfEndpointId == endpoint) return
            persistence.save(accountId, json.encodeToString(TrustStore.serializer(), TrustStore(endpoint, "")))
        }
    }
}
