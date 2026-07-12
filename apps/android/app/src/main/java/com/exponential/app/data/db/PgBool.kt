package com.exponential.app.data.db

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull

/**
 * Boolean tolerant of every wire form a synced row's flag actually arrives in
 * (EXP-61): native JSON booleans (tRPC), the strings "true"/"false" (Electric
 * shape rows), and PostgreSQL's canonical text form "t"/"f" (observed from
 * Electric on staging — a bare `"f"` made kotlinx throw and silently DROP the
 * whole issue_subscribers row). Use for every Boolean on a @Serializable
 * entity that crosses the wire.
 */
typealias PgBool = @Serializable(PgBoolSerializer::class) Boolean

object PgBoolSerializer : KSerializer<Boolean> {
    override val descriptor: SerialDescriptor =
        PrimitiveSerialDescriptor("com.exponential.app.PgBool", PrimitiveKind.BOOLEAN)

    override fun serialize(encoder: Encoder, value: Boolean) = encoder.encodeBoolean(value)

    override fun deserialize(decoder: Decoder): Boolean {
        val jsonDecoder = decoder as? JsonDecoder ?: return decoder.decodeBoolean()
        val element = jsonDecoder.decodeJsonElement()
        val primitive = element as? JsonPrimitive
            ?: throw SerializationException("Expected a primitive boolean, got $element")
        primitive.booleanOrNull?.let { return it }
        return when (primitive.content.lowercase()) {
            "t", "true", "1" -> true
            "f", "false", "0" -> false
            else -> throw SerializationException("Cannot parse '${primitive.content}' as a boolean")
        }
    }
}
