package com.exponential.app.data.db

import kotlinx.serialization.KSerializer
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive

/**
 * Non-null String tolerant of a JSON `null` on the wire: decodes it to "".
 * kotlinx.serialization throws on `null` for a non-nullable property (the
 * shared Json has no `coerceInputValues`), and a throw silently DROPS the
 * whole shape row. Use when a server column went nullable but the Room column
 * stays NOT NULL TEXT (no migration), "" standing in for "none".
 */
object NullAsEmptyStringSerializer : KSerializer<String> {
    override val descriptor: SerialDescriptor =
        PrimitiveSerialDescriptor("com.exponential.app.NullAsEmptyString", PrimitiveKind.STRING)

    override fun serialize(encoder: Encoder, value: String) = encoder.encodeString(value)

    override fun deserialize(decoder: Decoder): String {
        val jsonDecoder = decoder as? JsonDecoder ?: return decoder.decodeString()
        return when (val element = jsonDecoder.decodeJsonElement()) {
            is JsonNull -> ""
            is JsonPrimitive -> element.content
            else -> element.toString()
        }
    }
}
