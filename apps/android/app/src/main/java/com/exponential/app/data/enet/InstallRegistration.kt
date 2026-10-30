package com.exponential.app.data.enet

import at.exponential.net.ffi.endpointIdOf as ffiEndpointIdOf
import at.exponential.net.ffi.generateIdentity as ffiGenerateIdentity
import at.exponential.net.ffi.identitySign
import at.exponential.net.ffi.pushKeypairDerive
import com.exponential.app.data.api.RegisterInstallInput
import java.time.Instant
import java.time.temporal.ChronoUnit
import java.util.Base64

/** The eNet crypto an install registration needs (the UniFFI facade in the app, the same in tests). */
interface EnetCrypto {
    /** Lowercase hex of the identity's ed25519 public key = its iroh endpoint id. */
    fun endpointIdOf(identity: ByteArray): String

    /** `PushKeypair::derive(identity).public`: what peers seal to. */
    fun x25519PublicOf(identity: ByteArray): ByteArray

    /** 64-byte ed25519 signature by the identity. */
    fun sign(identity: ByteArray, message: ByteArray): ByteArray

    fun generateIdentity(): ByteArray
}

/** The facade (`exponential-net-ffi`, full build): every call loads JNA + the `.so` on first use, so never on the main thread. */
object EnetFfi : EnetCrypto {
    override fun endpointIdOf(identity: ByteArray): String = ffiEndpointIdOf(identity)
    override fun x25519PublicOf(identity: ByteArray): ByteArray = pushKeypairDerive(identity).public
    override fun sign(identity: ByteArray, message: ByteArray): ByteArray = identitySign(identity, message)
    override fun generateIdentity(): ByteArray = ffiGenerateIdentity()
}

internal fun base64Url(bytes: ByteArray): String = Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)

/** The UTF-8 string `installs.register` verifies (#VAPP-37; fixtures/install-register.json). */
internal fun installMessage(userId: String, endpointId: String, x25519Public: String, signedAt: String): String =
    "exp-install-v1\n$userId\n$endpointId\n$x25519Public\n$signedAt"

/** ISO-8601 UTC with milliseconds, the form the server's `Date.parse` and the fixture use. */
internal fun isoMillis(at: Instant): String = at.truncatedTo(ChronoUnit.MILLIS).toString().let {
    // Instant.toString drops a zero fraction ("…:00Z"); keep the fixed width.
    if (it.length == 20) it.dropLast(1) + ".000Z" else it
}

/** One signed `installs.register` input for [identity] on behalf of [userId]. */
internal fun signedRegistration(
    crypto: EnetCrypto,
    identity: ByteArray,
    userId: String,
    signedAt: Instant,
    label: String?,
    platform: String?,
): RegisterInstallInput {
    val endpointId = crypto.endpointIdOf(identity)
    val x25519Public = base64Url(crypto.x25519PublicOf(identity))
    val at = isoMillis(signedAt)
    val signature = crypto.sign(identity, installMessage(userId, endpointId, x25519Public, at).toByteArray(Charsets.UTF_8))
    return RegisterInstallInput(
        kind = "android",
        label = label?.take(64),
        platform = platform?.take(32),
        endpointId = endpointId,
        x25519Public = x25519Public,
        signedAt = at,
        signature = base64Url(signature),
    )
}
