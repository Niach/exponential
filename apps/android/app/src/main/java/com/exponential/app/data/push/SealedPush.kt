package com.exponential.app.data.push

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

/** The FCM data key the push relay delivers a sealed payload under (data-only message). */
internal const val SEALED_PUSH_DATA_KEY = "s"

/**
 * What the server seals per device (EXP-1435, fixtures/push-seal.json):
 * UTF-8 JSON `{title, body, data}`, data = the same string map a plaintext
 * push carries (userId, notificationId, type, issueId, sessionId…).
 */
internal data class SealedPushPayload(
    val title: String?,
    val body: String?,
    val data: Map<String, String>,
)

/** Null when [plaintext] is not that JSON object (the push is then dropped). */
internal fun parseSealedPushPayload(plaintext: ByteArray): SealedPushPayload? {
    val obj = runCatching {
        Json.parseToJsonElement(plaintext.toString(Charsets.UTF_8)) as? JsonObject
    }.getOrNull() ?: return null
    val title = obj.stringOrNull("title")
    val body = obj.stringOrNull("body")
    val rawData = obj["data"]
    val data = when (rawData) {
        null -> emptyMap()
        is JsonObject -> rawData.mapNotNull { (key, value) ->
            (value as? JsonPrimitive)?.takeIf { it.isString }?.content?.let { key to it }
        }.toMap()
        else -> if (rawData is JsonPrimitive && rawData.contentOrNull == null) emptyMap() else return null
    }
    return SealedPushPayload(title = title, body = body, data = data)
}

/**
 * Opens the relay's `s` value: base64url → [open] (the facade's pushOpen with
 * this install's secret) → the payload. Null at any step = drop silently.
 */
internal fun openSealedPush(sealedBase64Url: String, open: (ByteArray) -> ByteArray?): SealedPushPayload? {
    val sealed = base64UrlDecode(sealedBase64Url) ?: return null
    if (sealed.isEmpty()) return null
    val plaintext = open(sealed) ?: return null
    return parseSealedPushPayload(plaintext)
}

private fun JsonObject.stringOrNull(key: String): String? =
    (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.contentOrNull
