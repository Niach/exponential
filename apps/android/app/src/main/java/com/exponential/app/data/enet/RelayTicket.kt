package com.exponential.app.data.enet

import java.net.URI

/**
 * EXP-1471 on Android: the node has no discovery, so it only reaches a peer it
 * has an address for. A teammate's install row gives the endpoint id; every
 * install homes on the same relay, so the address is `enet1…` with that one
 * relay (the web's relay-ticket.ts, the daemon's `apply_allowlist`).
 * Encoding = crates/enet-core/src/ticket.rs: `enet1` + lowercase unpadded
 * base32 of [32 id][u8 relays, u8 len + url…][u8 addrs].
 */
object RelayTicket {
    const val RELAY_URL = "https://relay.exponential.at"

    private const val ALPHABET = "abcdefghijklmnopqrstuvwxyz234567"
    private val HEX_ID = Regex("^[0-9a-fA-F]{64}$")

    /** `enet1…` for the hex [endpointId] homed on [relayUrl]; null for a malformed id. */
    fun encode(endpointId: String, relayUrl: String = RELAY_URL): String? {
        if (!HEX_ID.matches(endpointId)) return null
        val uri = runCatching { URI(relayUrl) }.getOrNull() ?: return null
        if (uri.host == null) return null
        // `new URL(…).href` / iroh's RelayUrl: an empty path prints as `/`.
        val href = if (uri.rawPath.isNullOrEmpty()) "$relayUrl/" else relayUrl
        val relay = href.toByteArray(Charsets.UTF_8)
        if (relay.size > 255) return null
        val bytes = ByteArray(32 + 2 + relay.size + 1)
        for (i in 0 until 32) bytes[i] = endpointId.substring(i * 2, i * 2 + 2).toInt(16).toByte()
        bytes[32] = 1
        bytes[33] = relay.size.toByte()
        relay.copyInto(bytes, 34)
        bytes[34 + relay.size] = 0
        return "enet1" + base32(bytes)
    }

    private fun base32(bytes: ByteArray): String {
        val out = StringBuilder()
        var buffer = 0
        var bits = 0
        for (b in bytes) {
            buffer = (buffer shl 8) or (b.toInt() and 0xff)
            bits += 8
            while (bits >= 5) {
                bits -= 5
                out.append(ALPHABET[(buffer shr bits) and 31])
            }
            buffer = buffer and ((1 shl bits) - 1)
        }
        if (bits > 0) out.append(ALPHABET[(buffer shl (5 - bits)) and 31])
        return out.toString()
    }
}
