package com.exponential.app.data.sealed

import at.exponential.net.ffi.EnetServedWrap
import at.exponential.net.ffi.EnetVouchVerdict
import at.exponential.net.ffi.EnetWrapRefusal
import at.exponential.net.ffi.EnetWrapStatement
import at.exponential.net.ffi.EnetWrapVerdict
import at.exponential.net.ffi.acceptVouch as ffiAcceptVouch
import at.exponential.net.ffi.acceptWrap as ffiAcceptWrap
import at.exponential.net.ffi.endpointIdOf as ffiEndpointIdOf
import at.exponential.net.ffi.identitySign
import at.exponential.net.ffi.identityVerify
import com.exponential.app.data.api.HeldTreeKey
import com.exponential.app.data.enet.base64Url
import com.exponential.app.data.enet.installMessage
import java.security.MessageDigest
import java.util.Base64
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

// #EXP-1950 (packages/domain-contract/fixtures/install-trust.json, read its
// `$comment`; TS reference `src/install-trust.ts`, iOS `InstallTrust.swift`):
// which installs this install TRUSTS, and what it does with a key the server
// hands it.
//
// - THE CODE an install shows while it waits for approval: [InstallCode].
// - THE APPROVALS, signed by an install's eNet identity: [InstallApproval].
// - THE PIN STORE of one install per account and how it follows
//   `installs.trust`: [InstallTrust.reconcile]. The server only ever SHRINKS
//   the pins; they grow only by a signature this install checks itself.
// - THE FOUR RULES, record-only in release 1 ([TrustMode.BUILD]): `keepWrap`
//   (R1), `sealEpochOk` (R2), `sealTo` (R3), `wrapTarget` (R4) and `callKey`.
//   Each answers what a build does NOW (and records) and once signing is
//   mandatory (decision d).
// The ed25519 runs through the full eNet binding ([EnetTrustCrypto], strict
// verify = `identityVerify`); the push-only library is never on this path.

/** Release 1 = `record`: nothing a released build makes is refused;
 *  `mandatory` = release 2 of decision (d). A BUILD constant, never a server flag. */
enum class TrustMode(val wire: String) {
    Record("record"),
    Mandatory("mandatory"),
    ;

    companion object {
        val BUILD = Record
    }
}

/** What this install recorded about a key it holds. */
@Serializable
enum class KeyRecord(val wire: String) {
    @SerialName("verified") Verified("verified"),
    @SerialName("legacy") Legacy("legacy"),
    @SerialName("unsigned") Unsigned("unsigned"),
    @SerialName("refused-would-be") RefusedWouldBe("refused-would-be"),
    ;

    companion object {
        fun of(wire: String): KeyRecord? = entries.firstOrNull { it.wire == wire }
    }
}

/** Why an install is pinned: `self` = this install; `chain` = on the path
 *  from its account's root when it joined; `approved` = an `approve` by a
 *  pinned install of the same account; `first` = a team-mate's root at first
 *  contact; `reset` = a team-mate's new root a pinned install confirmed;
 *  `existing` = an install of the own account served at this EXISTING
 *  install's first reconcile (decision e, once). */
@Serializable
enum class PinVia(val wire: String) {
    @SerialName("self") Self("self"),
    @SerialName("chain") Chain("chain"),
    @SerialName("approved") Approved("approved"),
    @SerialName("first") First("first"),
    @SerialName("reset") Reset("reset"),
    @SerialName("existing") Existing("existing"),
    ;

    companion object {
        fun of(wire: String): PinVia? = entries.firstOrNull { it.wire == wire }
    }
}

@Serializable
data class TrustPin(
    val userId: String,
    val x25519Public: String,
    val via: PinVia,
    /** The approver (`approved`, `reset`); null otherwise. */
    val by: String? = null,
)

@Serializable
data class TrustRootRecord(
    val verifiedEpoch: Int = 0,
    /** epoch (decimal) → record. */
    val epochs: Map<String, KeyRecord> = emptyMap(),
)

/** The pin store of one install for one account (the Keystore vault beside the tree keys: [TrustVault]). */
@Serializable
data class TrustStore(
    /** This install's endpoint id. */
    @SerialName("self") val selfEndpointId: String,
    val userId: String,
    /** Placed in its account's chain (a root, approved, or existing). */
    val joined: Boolean = false,
    /** Decision (e): this install existed before approval shipped (its store
     *  was made for an install an earlier build had registered for this
     *  account; a build that makes an identity writes that account's store
     *  first, `false`). Consumed by the first reconcile that serves this install. */
    val existing: Boolean = false,
    val pins: Map<String, TrustPin> = emptyMap(),
    /** Every team-mate ever pinned: a new root of one is a Reset, not a first contact. */
    val members: List<String> = emptyList(),
    /** Endpoint ids this install's account turned down (a Reset row's Remove). */
    val declined: List<String> = emptyList(),
    /** Per root (lowercase): the highest VERIFIED epoch and what each held epoch is. */
    val roots: Map<String, TrustRootRecord> = emptyMap(),
) {
    /** The record of a held key; a key the store never saw = `legacy` (decision c). */
    fun record(rootId: String, epoch: Int): KeyRecord = roots[rootId.lowercase()]?.epochs?.get(epoch.toString()) ?: KeyRecord.Legacy

    /** Records a kept key (`verified` raises the root's verified epoch). */
    fun recordKey(rootId: String, epoch: Int, record: KeyRecord): TrustStore {
        val root = rootId.lowercase()
        val entry = roots[root] ?: TrustRootRecord()
        val next = TrustRootRecord(
            verifiedEpoch = if (record == KeyRecord.Verified) maxOf(entry.verifiedEpoch, epoch) else entry.verifiedEpoch,
            epochs = entry.epochs + (epoch.toString() to record),
        )
        return copy(roots = roots + (root to next))
    }
}

// ── what the server serves (`installs.trust`) ─────────────────────────────

/** A live install of the viewer's account or of a team-mate. */
@Serializable
data class ServedInstall(
    val installId: String,
    val userId: String,
    val endpointId: String,
    val x25519Public: String,
    val registrationSignature: String? = null,
    val registrationSignedAt: String? = null,
    /** A build that follows the rule asked to be approved (`installs.requestApproval`). */
    val approvalRequested: Boolean = false,
    val label: String? = null,
)

/** What an approval says (`approval.kinds`). */
@Serializable
enum class ApprovalKind(val wire: String, val byte: Int) {
    @SerialName("root") Root("root", 1),
    @SerialName("approve") Approve("approve", 2),
    @SerialName("reset") Reset("reset", 3),
    @SerialName("decline") Decline("decline", 4),
    ;

    companion object {
        fun of(wire: String): ApprovalKind? = entries.firstOrNull { it.wire == wire }
    }
}

@Serializable
data class ServedApproval(
    val kind: ApprovalKind,
    val subjectEndpointId: String,
    val approverEndpointId: String,
    val signature: String,
)

/** `installs.trust`'s answer. */
@Serializable
data class TrustAnswer(
    val installs: List<ServedInstall> = emptyList(),
    val approvals: List<ServedApproval> = emptyList(),
)

/** A row of Needs you: approve or remove an install. */
data class TrustRow(
    val kind: Kind,
    val installId: String,
    val endpointId: String,
    val userId: String,
    val label: String?,
    /** Derived HERE from [endpointId], never read from the server. */
    val code: String,
) {
    enum class Kind(val wire: String) { NewInstall("newInstall"), Reset("reset") }
}

/** `trusted` = joined; `waiting` = shows "Waiting for approval" and its code;
 *  `claimRoot` = its account has no chain: it signs `root` for itself (a
 *  joined existing install with its `approveExisting`, in one claim). */
enum class TrustStatus(val wire: String) { Trusted("trusted"), Waiting("waiting"), ClaimRoot("claimRoot") }

data class TrustView(
    val store: TrustStore,
    val status: TrustStatus,
    val rows: List<TrustRow>,
    /** The `existing` pins this install signs `approve` for (with its root
     *  claim, or once it is on the chain). Sorted by endpoint id. */
    val approveExisting: List<String> = emptyList(),
)

// ── the crypto (the eNet binding; a fake never stands in for the replays) ─

/** Why a wrap (or a vouch) is not kept: wrap-signature.json `refusals`. */
enum class WrapRefusal(val wire: String) {
    Malformed("malformed"), Unsigned("unsigned"), Unpinned("unpinned"), Unopened("unopened"), Signature("signature"),
}

sealed interface WrapAcceptance {
    class Verified(val secret: ByteArray, val treePublicKey: String) : WrapAcceptance
    class Unsigned(val secret: ByteArray, val treePublicKey: String) : WrapAcceptance
    data class Refused(val reason: WrapRefusal) : WrapAcceptance
}

/** A wrap statement a vouch signs (wrap-signature.json). */
data class WrapStatement(val rootId: String, val epoch: Int, val treePublicKey: String, val recipientEndpointId: String, val wrapped: String)

interface TrustCrypto {
    /** Strict ed25519: [signature] (base64url, 86 chars) by [endpointId] over [message]. Never throws. */
    fun verify(signature: String, message: ByteArray, endpointId: String): Boolean

    /** The identity's signature over [message], base64url without padding. */
    fun sign(identity: ByteArray, message: ByteArray): String
    fun endpointId(identity: ByteArray): String

    /** wrap-signature.json `acceptWrap` (the binding's). */
    fun acceptWrap(wrap: HeldTreeKey, identity: ByteArray, installSecret: ByteArray, pins: List<String>, required: Boolean): WrapAcceptance

    /** wrap-signature.json `acceptVouch`: null = verified. */
    fun acceptVouch(statement: WrapStatement, signature: String?, signerEndpointId: String?, pins: List<String>): WrapRefusal?

    /** Whether [secret] is the secret half of the tree key [publicKey]. */
    fun treeKeyMatches(secret: ByteArray, publicKey: String, epoch: Int): Boolean
}

/** The full eNet binding (`exponential-net-ffi`); loads JNA + the `.so` on first use, never on the main thread. */
object EnetTrustCrypto : TrustCrypto {
    override fun verify(signature: String, message: ByteArray, endpointId: String): Boolean {
        if (!InstallTrust.isSignature(signature) || !InstallTrust.isEndpointId(endpointId)) return false
        val bytes = InstallTrust.b64urlDecode(signature)?.takeIf { it.size == 64 } ?: return false
        return runCatching { identityVerify(endpointId, message, bytes) }.getOrDefault(false)
    }

    override fun sign(identity: ByteArray, message: ByteArray): String = base64Url(identitySign(identity, message))

    override fun endpointId(identity: ByteArray): String = ffiEndpointIdOf(identity)

    private fun refusal(reason: EnetWrapRefusal): WrapRefusal = when (reason) {
        EnetWrapRefusal.MALFORMED -> WrapRefusal.Malformed
        EnetWrapRefusal.UNSIGNED -> WrapRefusal.Unsigned
        EnetWrapRefusal.UNPINNED -> WrapRefusal.Unpinned
        EnetWrapRefusal.UNOPENED -> WrapRefusal.Unopened
        EnetWrapRefusal.SIGNATURE -> WrapRefusal.Signature
    }

    override fun acceptWrap(wrap: HeldTreeKey, identity: ByteArray, installSecret: ByteArray, pins: List<String>, required: Boolean): WrapAcceptance {
        if (wrap.epoch < 0) return WrapAcceptance.Refused(WrapRefusal.Malformed)
        val verdict = runCatching {
            ffiAcceptWrap(
                EnetServedWrap(wrap.rootId, wrap.epoch.toUInt(), wrap.wrapped, wrap.signature, wrap.signerEndpointId),
                identity,
                installSecret,
                pins,
                required,
            )
        }.getOrNull() ?: return WrapAcceptance.Refused(WrapRefusal.Malformed)
        return when (verdict) {
            is EnetWrapVerdict.Verified -> WrapAcceptance.Verified(verdict.secret, verdict.treePublicKey)
            is EnetWrapVerdict.Unsigned -> WrapAcceptance.Unsigned(verdict.secret, verdict.treePublicKey)
            is EnetWrapVerdict.Refused -> WrapAcceptance.Refused(refusal(verdict.reason))
        }
    }

    override fun acceptVouch(statement: WrapStatement, signature: String?, signerEndpointId: String?, pins: List<String>): WrapRefusal? {
        if (statement.epoch < 0) return WrapRefusal.Malformed
        val verdict = runCatching {
            ffiAcceptVouch(
                EnetWrapStatement(statement.rootId, statement.epoch.toUInt(), statement.treePublicKey, statement.recipientEndpointId, statement.wrapped),
                signature,
                signerEndpointId,
                pins,
            )
        }.getOrNull() ?: return WrapRefusal.Malformed
        return when (verdict) {
            is EnetVouchVerdict.Verified -> null
            is EnetVouchVerdict.Refused -> refusal(verdict.reason)
        }
    }

    override fun treeKeyMatches(secret: ByteArray, publicKey: String, epoch: Int): Boolean = runCatching {
        val crypto = SealedFfi
        crypto.openBody(crypto.sealBody(publicKey, maxOf(epoch, 1), KEY_CHECK), mapOf(maxOf(epoch, 1) to secret)).text == KEY_CHECK
    }.getOrDefault(false)

    private const val KEY_CHECK = "k"
}

// ── the code ──────────────────────────────────────────────────────────────

object InstallCode {
    const val LABEL = "exp/enet/install-code/1"
    const val ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"

    /** The 10 characters (two groups of five) an install shows to be
     *  approved: the first 50 bits of SHA-256(label ‖ 0x00 ‖ endpointId[32])
     *  in Crockford base32. Both ends derive it; null for a malformed id. */
    fun of(endpointId: String): String? {
        if (!InstallTrust.isEndpointId(endpointId)) return null
        val key = InstallTrust.hexBytes(endpointId) ?: return null
        val digest = sha256(LABEL.toByteArray(Charsets.UTF_8) + byteArrayOf(0) + key)
        var bits = 0L
        for (i in 0 until 7) bits = (bits shl 8) or (digest[i].toLong() and 0xff)
        bits = bits ushr 6
        val out = StringBuilder()
        for (i in 9 downTo 0) out.append(ALPHABET[((bits ushr (5 * i)) and 31).toInt()])
        return "${out.substring(0, 5)} ${out.substring(5)}"
    }
}

internal fun sha256(bytes: ByteArray): ByteArray = MessageDigest.getInstance("SHA-256").digest(bytes)

// ── the approvals ─────────────────────────────────────────────────────────

object InstallApproval {
    const val LABEL = "exp/enet/install-approval/1"
    const val MESSAGE_BYTES = 125

    /** label ‖ 0x00 ‖ kind u8 ‖ SHA-256(userId utf8)[32] ‖ endpointId[32] ‖
     *  x25519Public[32]; null when malformed. */
    fun message(kind: ApprovalKind, userId: String, endpointId: String, x25519Public: String): ByteArray? {
        if (!InstallTrust.isEndpointId(endpointId) || userId.isEmpty()) return null
        val endpoint = InstallTrust.hexBytes(endpointId) ?: return null
        val x = InstallTrust.b64urlDecode(x25519Public)?.takeIf { it.size == 32 } ?: return null
        return LABEL.toByteArray(Charsets.UTF_8) + byteArrayOf(0, kind.byte.toByte()) + sha256(userId.toByteArray(Charsets.UTF_8)) + endpoint + x
    }

    fun message(kind: ApprovalKind, subject: ServedInstall): ByteArray? = message(kind, subject.userId, subject.endpointId, subject.x25519Public)

    fun sign(kind: ApprovalKind, subject: ServedInstall, identity: ByteArray, crypto: TrustCrypto): String {
        val message = message(kind, subject) ?: throw InstallTrustException("malformed approval subject")
        return crypto.sign(identity, message)
    }

    fun verify(kind: ApprovalKind, subject: ServedInstall, signature: String, approverEndpointId: String, crypto: TrustCrypto): Boolean {
        val message = message(kind, subject) ?: return false
        return crypto.verify(signature, message, approverEndpointId)
    }
}

class InstallTrustException(message: String) : Exception(message)

// ── reconcile + the four rules ────────────────────────────────────────────

object InstallTrust {
    private val HEX = Regex("^[0-9a-f]{64}$")
    private val SIGNATURE = Regex("^[A-Za-z0-9_-]{85}[AQgw]$")

    fun isEndpointId(text: String): Boolean = HEX.matches(text)

    /** base64url of exactly 64 bytes, canonical (`[A-Za-z0-9_-]{85}[AQgw]`). */
    fun isSignature(text: String): Boolean = SIGNATURE.matches(text)

    fun b64urlDecode(text: String): ByteArray? = runCatching { Base64.getUrlDecoder().decode(text.trimEnd('=')) }.getOrNull()

    fun hexBytes(hex: String): ByteArray? {
        if (hex.length % 2 != 0) return null
        return runCatching { ByteArray(hex.length / 2) { i -> hex.substring(2 * i, 2 * i + 2).toInt(16).toByte() } }.getOrNull()
    }

    /** install-register.json's message. */
    fun registrationMessage(userId: String, endpointId: String, x25519Public: String, signedAt: String): String =
        installMessage(userId, endpointId, x25519Public, signedAt)

    /** Whether the row's stored registration binds its user, endpoint id and X25519 key. */
    fun registrationValid(install: ServedInstall, crypto: TrustCrypto): Boolean {
        val signature = install.registrationSignature ?: return false
        val signedAt = install.registrationSignedAt ?: return false
        val text = registrationMessage(install.userId, install.endpointId, install.x25519Public, signedAt)
        return crypto.verify(signature, text.toByteArray(Charsets.UTF_8), install.endpointId)
    }

    /**
     * Follows the server's answer. The server may only SHRINK the pins (a pin
     * missing from the answer, or served with another user or key, is
     * dropped); a pin is ADDED only for a signature this install verified, by
     * an install already pinned (or, once, at joining, at an existing
     * install's first reconcile and at first contact).
     */
    fun reconcile(before: TrustStore, served: TrustAnswer, crypto: TrustCrypto): TrustView {
        val pins = LinkedHashMap(before.pins)
        val members = before.members.toMutableList()
        val declined = before.declined.toMutableList()
        var joined = before.joined
        var existing = before.existing
        val self = before.selfEndpointId
        val userId = before.userId

        // `valid` = the registration verifies (a row, a first contact);
        // `counted` adds rows that store none (registered before #3394).
        val valid = LinkedHashMap<String, ServedInstall>()
        val counted = LinkedHashMap<String, ServedInstall>()
        for (install in served.installs) {
            if (registrationValid(install, crypto)) valid[install.endpointId] = install
            if (valid.containsKey(install.endpointId) || install.registrationSignature == null) counted[install.endpointId] = install
        }
        val me = counted[self]

        // Shrink: what the server no longer lists (or binds) is no longer pinned.
        for (endpointId in pins.keys.toList()) {
            if (endpointId == self) continue
            val row = counted[endpointId]
            val pin = pins.getValue(endpointId)
            if (row == null || row.userId != pin.userId || row.x25519Public != pin.x25519Public) pins.remove(endpointId)
        }
        if (me != null) pins[self] = TrustPin(userId, me.x25519Public, PinVia.Self, null)

        // The approvals that verify, against the subject as its registration binds it.
        val approvals = served.approvals.filter { approval ->
            val subject = counted[approval.subjectEndpointId] ?: return@filter false
            if (approval.kind == ApprovalKind.Root && approval.approverEndpointId != approval.subjectEndpointId) return@filter false
            InstallApproval.verify(approval.kind, subject, approval.signature, approval.approverEndpointId, crypto)
        }
        val rootSet = approvals.filter { it.kind == ApprovalKind.Root }.map { it.subjectEndpointId }.toSet()
        val isRoot = { endpointId: String -> endpointId in rootSet }
        val userOf = { endpointId: String -> counted[endpointId]?.userId }
        val ownChain = approvals.any { (it.kind == ApprovalKind.Root || it.kind == ApprovalKind.Approve) && userOf(it.subjectEndpointId) == userId }

        // Joining: its own root claim, the chain from its account's root to
        // it, or (decision e) an existing install of an account with no chain.
        if (!joined && me != null) {
            if (isRoot(self)) {
                joined = true
            } else {
                val path = chain(self, userId, approvals, isRoot, userOf)
                if (path != null) {
                    for (endpointId in path) {
                        if (endpointId == self) continue
                        val row = counted.getValue(endpointId)
                        pins[endpointId] = TrustPin(row.userId, row.x25519Public, PinVia.Chain, null)
                    }
                    joined = true
                } else if (existing && !ownChain) {
                    for (install in counted.values) {
                        if (install.userId != userId || install.endpointId == self || install.approvalRequested) continue
                        pins[install.endpointId] = TrustPin(install.userId, install.x25519Public, PinVia.Existing, null)
                    }
                    joined = true
                }
            }
        }
        // Only the FIRST reconcile that serves this install adopts.
        if (me != null) existing = false

        // Declines by this account's pinned installs.
        for (approval in approvals) {
            if (approval.kind != ApprovalKind.Decline) continue
            val approver = pins[approval.approverEndpointId]
            if (approver?.userId == userId && approval.subjectEndpointId !in declined) declined += approval.subjectEndpointId
        }

        if (joined) {
            // First contact with a team-mate: their root(s) as served, once.
            for (install in counted.values) {
                if (install.userId == userId || install.userId in members) continue
                if (pins.values.any { it.userId == install.userId }) continue
                for (root in counted.values) {
                    if (root.userId == install.userId && isRoot(root.endpointId) && root.endpointId !in declined) {
                        pins[root.endpointId] = TrustPin(root.userId, root.x25519Public, PinVia.First, null)
                    }
                }
            }
            // Grow by signatures of pinned installs, to a fixpoint.
            var changed = true
            while (changed) {
                changed = false
                for (approval in approvals) {
                    val subject = counted.getValue(approval.subjectEndpointId)
                    if (pins.containsKey(subject.endpointId) || subject.endpointId in declined) continue
                    val approver = pins[approval.approverEndpointId] ?: continue
                    val pinned = (approval.kind == ApprovalKind.Approve && approver.userId == subject.userId) ||
                        (approval.kind == ApprovalKind.Reset && approver.userId != subject.userId && subject.userId != userId && isRoot(subject.endpointId))
                    if (!pinned) continue
                    pins[subject.endpointId] = TrustPin(
                        subject.userId,
                        subject.x25519Public,
                        if (approval.kind == ApprovalKind.Approve) PinVia.Approved else PinVia.Reset,
                        approval.approverEndpointId,
                    )
                    changed = true
                }
            }
            for (pin in pins.values) if (pin.userId != userId && pin.userId !in members) members += pin.userId
        }

        val rows = mutableListOf<TrustRow>()
        if (joined) {
            for (install in valid.values) {
                if (pins.containsKey(install.endpointId) || install.endpointId in declined) continue
                // Own: every install nobody pinned is new (decision e), any
                // build. A team-mate's: only a new root of a member pinned before.
                val kind = when {
                    install.userId == userId -> TrustRow.Kind.NewInstall
                    isRoot(install.endpointId) && install.userId in members -> TrustRow.Kind.Reset
                    else -> null
                } ?: continue
                rows += TrustRow(kind, install.installId, install.endpointId, install.userId, install.label, InstallCode.of(install.endpointId)!!)
            }
            rows.sortWith(compareBy<TrustRow> { if (it.kind == TrustRow.Kind.NewInstall) 0 else 1 }.thenBy { it.endpointId })
        }

        // A new install claims a root only ALONE (decision e).
        val alone = served.installs.none { it.userId == userId && it.endpointId != self }
        val status = when {
            joined -> if (ownChain) TrustStatus.Trusted else TrustStatus.ClaimRoot
            ownChain || !alone -> TrustStatus.Waiting
            else -> TrustStatus.ClaimRoot
        }

        val reached = { endpointId: String ->
            isRoot(endpointId) || approvals.any { it.kind == ApprovalKind.Approve && it.subjectEndpointId == endpointId && pins.containsKey(it.approverEndpointId) }
        }
        val onChain = reached(self)
        val approveExisting = if (joined && (status == TrustStatus.ClaimRoot || onChain)) {
            pins.filter { (endpointId, pin) -> pin.via == PinVia.Existing && !reached(endpointId) }.keys.sorted()
        } else {
            emptyList()
        }
        val store = before.copy(joined = joined, existing = existing, pins = pins, members = members, declined = declined)
        return TrustView(store, status, rows, approveExisting)
    }

    /** The approvals from a root of [userId] down to [target] (`approve`
     *  only, every approver of [userId]), shortest first; null when none. */
    private fun chain(
        target: String,
        userId: String,
        approvals: List<ServedApproval>,
        isRoot: (String) -> Boolean,
        userOf: (String) -> String?,
    ): List<String>? {
        val parents = HashMap<String, String>()
        val roots = approvals.filter { it.kind == ApprovalKind.Root && userOf(it.subjectEndpointId) == userId }.map { it.subjectEndpointId }.distinct().sorted()
        val queue = ArrayDeque(roots)
        val seen = roots.toMutableSet()
        while (queue.isNotEmpty()) {
            val at = queue.removeFirst()
            if (at == target) {
                val path = mutableListOf(at)
                while (parents.containsKey(path.first())) path.add(0, parents.getValue(path.first()))
                return path
            }
            for (approval in approvals) {
                if (approval.kind != ApprovalKind.Approve || approval.approverEndpointId != at || approval.subjectEndpointId in seen) continue
                if (userOf(approval.subjectEndpointId) != userId || isRoot(approval.subjectEndpointId)) continue
                seen += approval.subjectEndpointId
                parents[approval.subjectEndpointId] = at
                queue.addLast(approval.subjectEndpointId)
            }
        }
        return null
    }

    // ── R1 ────────────────────────────────────────────────────────────────

    sealed interface KeepVerdict {
        class Keep(val record: KeyRecord, val secret: ByteArray, val treePublicKey: String) : KeepVerdict
        data class Refuse(val reason: WrapRefusal) : KeepVerdict
    }

    /** R1: whether this install keeps a served wrap (of ANY epoch). A
     *  signature that fails, a malformed wrap and one that does not open are
     *  refused in every mode; `record` keeps an unsigned wrap (`unsigned`)
     *  and one signed by an UNPINNED install that verifies (`refused-would-be`). */
    fun keepWrap(wrap: HeldTreeKey, identity: ByteArray, installSecret: ByteArray, store: TrustStore, mode: TrustMode, crypto: TrustCrypto): KeepVerdict =
        when (val verdict = crypto.acceptWrap(wrap, identity, installSecret, store.pins.keys.toList(), mode == TrustMode.Mandatory)) {
            is WrapAcceptance.Verified -> KeepVerdict.Keep(KeyRecord.Verified, verdict.secret, verdict.treePublicKey)
            is WrapAcceptance.Unsigned -> KeepVerdict.Keep(KeyRecord.Unsigned, verdict.secret, verdict.treePublicKey)
            is WrapAcceptance.Refused -> if (verdict.reason == WrapRefusal.Unpinned && mode == TrustMode.Record) {
                // Is it at least its signer's? Then it is kept, and recorded.
                val signer = wrap.signerEndpointId
                if (signer == null) {
                    KeepVerdict.Refuse(WrapRefusal.Signature)
                } else {
                    when (val again = crypto.acceptWrap(wrap, identity, installSecret, listOf(signer), false)) {
                        is WrapAcceptance.Verified -> KeepVerdict.Keep(KeyRecord.RefusedWouldBe, again.secret, again.treePublicKey)
                        is WrapAcceptance.Refused -> KeepVerdict.Refuse(again.reason)
                        is WrapAcceptance.Unsigned -> KeepVerdict.Refuse(WrapRefusal.Signature)
                    }
                }
            } else {
                KeepVerdict.Refuse(verdict.reason)
            }
        }

    // ── R2 ────────────────────────────────────────────────────────────────

    /** R2: false (= `rollback`, every mode) when an answer names an epoch
     *  below the highest one this install verified for the root. */
    fun sealEpochOk(store: TrustStore, rootId: String, epoch: Int): Boolean = epoch >= (store.roots[rootId.lowercase()]?.verifiedEpoch ?: 0)

    // ── R3 ────────────────────────────────────────────────────────────────

    enum class SealRefusal(val wire: String) {
        Malformed("malformed"), Unsigned("unsigned"), Unpinned("unpinned"), Unopened("unopened"), Signature("signature"),
        Rollback("rollback"), KeyMismatch("keyMismatch"),
    }

    sealed interface SealVerdict {
        data class Seal(val record: KeyRecord) : SealVerdict
        data class Refuse(val reason: SealRefusal) : SealVerdict
    }

    private fun sealRefusal(refusal: WrapRefusal): SealRefusal = SealRefusal.entries.first { it.wire == refusal.wire }

    /** R3: whether this install seals to (root, epoch, public key) as the
     *  server names it. With a held secret of that epoch: only to ITS public
     *  key. Without: a vouch by a pinned install = `verified`; none =
     *  `unsigned` and one by an unpinned install that verifies =
     *  `refused-would-be`, both sealed in `record` mode, refused once
     *  mandatory; a vouch whose signature fails is refused in every mode. */
    fun sealTo(
        rootId: String,
        epoch: Int,
        publicKey: String,
        held: Pair<ByteArray, KeyRecord>?,
        vouch: Vouch?,
        store: TrustStore,
        mode: TrustMode,
        crypto: TrustCrypto,
    ): SealVerdict {
        if (!sealEpochOk(store, rootId, epoch)) return SealVerdict.Refuse(SealRefusal.Rollback)
        if (held != null) {
            val (secret, record) = held
            if (!crypto.treeKeyMatches(secret, publicKey, epoch)) return SealVerdict.Refuse(SealRefusal.KeyMismatch)
            if (mode == TrustMode.Mandatory && record == KeyRecord.Unsigned) return SealVerdict.Refuse(SealRefusal.Unsigned)
            if (mode == TrustMode.Mandatory && record == KeyRecord.RefusedWouldBe) return SealVerdict.Refuse(SealRefusal.Unpinned)
            return SealVerdict.Seal(record)
        }
        if (vouch == null) return if (mode == TrustMode.Record) SealVerdict.Seal(KeyRecord.Unsigned) else SealVerdict.Refuse(SealRefusal.Unsigned)
        val statement = WrapStatement(rootId, epoch, publicKey, vouch.recipientEndpointId, vouch.wrapped)
        return when (val refusal = crypto.acceptVouch(statement, vouch.signature, vouch.signerEndpointId, store.pins.keys.toList())) {
            null -> SealVerdict.Seal(KeyRecord.Verified)
            WrapRefusal.Unsigned -> if (mode == TrustMode.Record) SealVerdict.Seal(KeyRecord.Unsigned) else SealVerdict.Refuse(SealRefusal.Unsigned)
            WrapRefusal.Unpinned -> {
                val own = crypto.acceptVouch(statement, vouch.signature, vouch.signerEndpointId, listOf(vouch.signerEndpointId))
                when {
                    own != null -> SealVerdict.Refuse(sealRefusal(own))
                    mode == TrustMode.Record -> SealVerdict.Seal(KeyRecord.RefusedWouldBe)
                    else -> SealVerdict.Refuse(SealRefusal.Unpinned)
                }
            }
            else -> SealVerdict.Refuse(sealRefusal(refusal))
        }
    }

    // ── R4 ────────────────────────────────────────────────────────────────

    enum class TargetRefusal(val wire: String) { Unapproved("unapproved"), KeyMismatch("keyMismatch"), Registration("registration"), NotTrusted("notTrusted") }

    sealed interface TargetVerdict {
        val wraps: Boolean get() = this is Wrap

        data class Wrap(val record: KeyRecord) : TargetVerdict
        data class Refuse(val reason: TargetRefusal) : TargetVerdict
    }

    /** R4: whether this install wraps a key for a target the server names.
     *  Only a joined install wraps for others; a pinned target with its pinned
     *  user and key = `verified`. An install of the OWN account nobody pinned
     *  (a New install, decision e), and a team-mate's that ASKED (or claims a
     *  root), are refused in every mode (the waiting state); a team-mate's
     *  that never asked and claims no root is an older build: `legacy`,
     *  wrapped in `record` mode, refused once mandatory. A registration that
     *  does not verify, or a pinned endpoint served with another key, is
     *  refused always. */
    fun wrapTarget(target: ServedInstall, store: TrustStore, mode: TrustMode, roots: Set<String> = emptySet(), crypto: TrustCrypto): TargetVerdict {
        if (target.endpointId == store.selfEndpointId) return TargetVerdict.Wrap(KeyRecord.Verified)
        if (!store.joined) return TargetVerdict.Refuse(TargetRefusal.NotTrusted)
        store.pins[target.endpointId]?.let { pin ->
            return if (pin.x25519Public == target.x25519Public && pin.userId == target.userId) {
                TargetVerdict.Wrap(KeyRecord.Verified)
            } else {
                TargetVerdict.Refuse(TargetRefusal.KeyMismatch)
            }
        }
        // An older build that registered before #EXP-1950 kept no signature: legacy too.
        if (target.registrationSignature != null && !registrationValid(target, crypto)) return TargetVerdict.Refuse(TargetRefusal.Registration)
        // Decision (e): every own install that existed is pinned (`existing`);
        // one nobody pinned is a New install row, any build: unapproved in every mode.
        if (target.userId == store.userId) return TargetVerdict.Refuse(TargetRefusal.Unapproved)
        if (target.approvalRequested || target.endpointId in roots) return TargetVerdict.Refuse(TargetRefusal.Unapproved)
        return if (mode == TrustMode.Record) TargetVerdict.Wrap(KeyRecord.Legacy) else TargetVerdict.Refuse(TargetRefusal.Unapproved)
    }

    // ── a call's key ──────────────────────────────────────────────────────

    sealed interface CallKeyVerdict {
        class Key(val secret: ByteArray, val record: KeyRecord) : CallKeyVerdict
        data object NoKey : CallKeyVerdict
        data object Rollback : CallKeyVerdict
    }

    /** A call's key (attack 4): the secret of the (root, epoch) `sendTarget`
     *  names, from this install's KEYRING only. `record` uses any held key;
     *  `mandatory` only `verified` and `legacy` ones; else `no_key`. */
    fun callKey(rootId: String, epoch: Int, held: Pair<ByteArray, KeyRecord>?, store: TrustStore, mode: TrustMode): CallKeyVerdict {
        if (!sealEpochOk(store, rootId, epoch)) return CallKeyVerdict.Rollback
        held ?: return CallKeyVerdict.NoKey
        if (mode == TrustMode.Mandatory && held.second != KeyRecord.Verified && held.second != KeyRecord.Legacy) return CallKeyVerdict.NoKey
        return CallKeyVerdict.Key(held.first, held.second)
    }
}

/** A signed wrap of that epoch for another install (`acceptVouch`): R3's
 *  evidence for a key this install holds no secret of. */
@Serializable
data class Vouch(val recipientEndpointId: String, val wrapped: String, val signature: String, val signerEndpointId: String)
