package com.exponential.app.data.sealed

import at.exponential.net.ffi.EnetEpochKey
import at.exponential.net.ffi.EnetSealStatus
import at.exponential.net.ffi.generateTreeKey as ffiGenerateTreeKey
import at.exponential.net.ffi.openBody as ffiOpenBody
import at.exponential.net.ffi.openCard as ffiOpenCard
import at.exponential.net.ffi.pushKeypairDerive
import at.exponential.net.ffi.sealBody as ffiSealBody
import at.exponential.net.ffi.sealCard as ffiSealCard
import at.exponential.net.ffi.unwrapTreeKey as ffiUnwrapTreeKey
import at.exponential.net.ffi.wrapTreeKey as ffiWrapTreeKey

/** fixtures/sealed-content.json open statuses, in the fixture's spelling. */
enum class SealStatus(val wire: String) {
    Plain("plain"),
    Opened("opened"),
    NoKey("noKey"),
    Unsupported("unsupported"),
    Malformed("malformed"),
    Refused("refused"),
}

/** A body (or sealed title) opened: [text] is set for plain and opened only. */
data class OpenedBody(val status: SealStatus, val epoch: Int?, val text: String?)

/** A card opened: [cardJson] = the full card (plain / opened) or the clear part alone. */
data class OpenedCard(val status: SealStatus, val epoch: Int?, val cardJson: String)

/** A fresh tree keypair: [publicKey] = `sessions.seal_public_key`. */
class TreeKey(val secret: ByteArray, val publicKey: String)

/**
 * EXP-1458 / EXP-1459: the sealed-content primitives (`enet-core::sealed`
 * through exponential-net-ffi). The wire format lives in Rust alone; this
 * app never re-implements it.
 */
interface SealedCrypto {
    fun generateTreeKey(): TreeKey
    fun wrapTreeKey(installPublic: String, treeSecret: ByteArray): String
    fun unwrapTreeKey(installSecret: ByteArray, wrapped: String): ByteArray
    fun sealBody(treePublicKey: String, epoch: Int, text: String): String
    fun openBody(body: String, keys: Map<Int, ByteArray>): OpenedBody
    fun sealCard(treePublicKey: String, epoch: Int, cardJson: String): String
    fun openCard(cardJson: String, keys: Map<Int, ByteArray>): OpenedCard

    /** `PushKeypair::derive(identity).secret`: the install's X25519 secret the wraps open with. */
    fun installSecret(identity: ByteArray): ByteArray
}

/** The facade: loads JNA + the `.so` on first use, so never on the main thread for the first call. */
object SealedFfi : SealedCrypto {
    override fun generateTreeKey(): TreeKey = ffiGenerateTreeKey().let { TreeKey(it.secret, it.publicKey) }
    override fun wrapTreeKey(installPublic: String, treeSecret: ByteArray): String = ffiWrapTreeKey(installPublic, treeSecret)
    override fun unwrapTreeKey(installSecret: ByteArray, wrapped: String): ByteArray = ffiUnwrapTreeKey(installSecret, wrapped)
    override fun sealBody(treePublicKey: String, epoch: Int, text: String): String = ffiSealBody(treePublicKey, epoch.toUInt(), text)
    override fun sealCard(treePublicKey: String, epoch: Int, cardJson: String): String = ffiSealCard(treePublicKey, epoch.toUInt(), cardJson)

    override fun openBody(body: String, keys: Map<Int, ByteArray>): OpenedBody =
        ffiOpenBody(body, keys.epochKeys()).let { OpenedBody(it.status.status(), it.epoch?.toInt(), it.text) }

    override fun openCard(cardJson: String, keys: Map<Int, ByteArray>): OpenedCard =
        ffiOpenCard(cardJson, keys.epochKeys()).let { OpenedCard(it.status.status(), it.epoch?.toInt(), it.cardJson) }

    override fun installSecret(identity: ByteArray): ByteArray = pushKeypairDerive(identity).secret

    private fun Map<Int, ByteArray>.epochKeys() = map { (epoch, secret) -> EnetEpochKey(epoch.toUInt(), secret) }

    private fun EnetSealStatus.status(): SealStatus = when (this) {
        EnetSealStatus.PLAIN -> SealStatus.Plain
        EnetSealStatus.OPENED -> SealStatus.Opened
        EnetSealStatus.NO_KEY -> SealStatus.NoKey
        EnetSealStatus.UNSUPPORTED -> SealStatus.Unsupported
        EnetSealStatus.MALFORMED -> SealStatus.Malformed
        EnetSealStatus.REFUSED -> SealStatus.Refused
    }
}
