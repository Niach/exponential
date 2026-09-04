package com.exponential.app.data.db

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.builtins.nullable
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.intOrNull

/**
 * Nullable Int tolerant of both wire forms an integer column arrives in — a
 * bare JSON number (`12`, what tRPC and today's Electric shape rows send) and
 * its PostgreSQL TEXT form (`"12"`). Same lesson as [PgBoolSerializer]: a
 * value kotlinx can't parse throws and silently DROPS the whole row, which on
 * the coding_sessions shape would blank the Agents tab. The desktop decodes
 * every integer column through `tolerant_opt_i64` for exactly this reason.
 */
object PgIntSerializer : KSerializer<Int?> {
    private val delegate = Int.serializer().nullable
    override val descriptor: SerialDescriptor =
        PrimitiveSerialDescriptor("com.exponential.app.PgInt", PrimitiveKind.INT)

    override fun serialize(encoder: Encoder, value: Int?) {
        encoder.encodeSerializableValue(delegate, value)
    }

    override fun deserialize(decoder: Decoder): Int? {
        val jsonDecoder = decoder as? JsonDecoder
            ?: return decoder.decodeSerializableValue(delegate)
        val element = jsonDecoder.decodeJsonElement()
        if (element is JsonNull) return null
        val primitive = element as? JsonPrimitive
            ?: throw SerializationException("Expected a primitive int, got $element")
        primitive.intOrNull?.let { return it }
        return primitive.content.trim().toIntOrNull()
            ?: throw SerializationException("Cannot parse '${primitive.content}' as an int")
    }
}
