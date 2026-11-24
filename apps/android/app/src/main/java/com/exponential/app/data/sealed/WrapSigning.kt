package com.exponential.app.data.sealed

import at.exponential.net.ffi.EnetServedWrap
import at.exponential.net.ffi.EnetWrapStatement
import at.exponential.net.ffi.EnetWrapVerdict
import at.exponential.net.ffi.acceptWrap
import at.exponential.net.ffi.endpointIdOf as ffiEndpointIdOf
import at.exponential.net.ffi.signWrap
import com.exponential.app.data.enet.EnetFfi
import com.exponential.app.data.enet.base64Url

/**
 * #EXP-1950: signs a tree-key wrap THIS install made with its eNet identity
 * (packages/domain-contract/fixtures/wrap-signature.json; `enet_core::wrap_sig`
 * through the binding). The full eNet library only: the push-only build has
 * no signing and is never on this path.
 */
interface WrapSigner {
    /** Lowercase hex of the identity's ed25519 key (`installs.endpoint_id`). */
    fun endpointIdOf(identity: ByteArray): String

    /** base64url signature over (root, epoch, tree public key, recipient endpoint id, wrapped bytes); throws on a malformed field. */
    fun sign(identity: ByteArray, rootId: String, epoch: Int, treePublicKey: String, recipientEndpointId: String, wrapped: String): String

    /** #EXP-2119: the public half of a held tree [secret] (the server may not know an older epoch's); null = not derivable. */
    fun treePublicKey(identity: ByteArray, installSecret: ByteArray, rootId: String, epoch: Int, secret: ByteArray): String?
}

object WrapSignerFfi : WrapSigner {
    override fun endpointIdOf(identity: ByteArray): String = ffiEndpointIdOf(identity)

    override fun sign(identity: ByteArray, rootId: String, epoch: Int, treePublicKey: String, recipientEndpointId: String, wrapped: String): String =
        signWrap(identity, EnetWrapStatement(rootId, epoch.toUInt(), treePublicKey, recipientEndpointId, wrapped))

    /** The binding names a secret's public key only in a wrap's verdict: one made for this install itself, opened again. */
    override fun treePublicKey(identity: ByteArray, installSecret: ByteArray, rootId: String, epoch: Int, secret: ByteArray): String? = runCatching {
        val own = SealedFfi.wrapTreeKey(base64Url(EnetFfi.x25519PublicOf(identity)), secret)
        when (val verdict = acceptWrap(EnetServedWrap(rootId, epoch.toUInt(), own, null, null), identity, installSecret, emptyList(), false)) {
            is EnetWrapVerdict.Unsigned -> verdict.treePublicKey
            is EnetWrapVerdict.Verified, is EnetWrapVerdict.Refused -> null
        }
    }.getOrNull()
}

/** A reader a wrap names: its install id, ed25519 key (endpoint id) and X25519 key. */
data class WrapTarget(val installId: String, val endpointId: String, val x25519Public: String)

/** wrap-signature.json `resign.failureCases`: what a holder does after a failed `resign`. */
enum class ResignAfter { NotYet, Refused, Retry }

/** `resignAfter`: an older server has no such procedure (NOT_FOUND); FORBIDDEN / BAD_REQUEST refuse; anything else retries. */
fun resignAfter(code: String?): ResignAfter = when (code) {
    "NOT_FOUND" -> ResignAfter.NotYet
    "FORBIDDEN", "BAD_REQUEST" -> ResignAfter.Refused
    else -> ResignAfter.Retry
}
