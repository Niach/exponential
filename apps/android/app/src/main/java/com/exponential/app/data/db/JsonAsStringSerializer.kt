package com.exponential.app.data.db

import kotlinx.serialization.KSerializer
import kotlinx.serialization.builtins.nullable
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive

// Postgres jsonb columns (e.g. issues.description) arrive from Electric as
// JSON values — sometimes as objects ({"text": "..."}), sometimes as quoted
// strings. Without this serializer, decoding a JsonObject into String? throws
// and the whole shape row gets dropped, so issues created on the web appear
// blank on Android.
object JsonAsStringSerializer : KSerializer<String?> {
    private val delegate = String.serializer().nullable
    override val descriptor: SerialDescriptor =
        PrimitiveSerialDescriptor("JsonAsString", PrimitiveKind.STRING)

    override fun deserialize(decoder: Decoder): String? {
        if (decoder !is JsonDecoder) return decoder.decodeSerializableValue(delegate)
        val element: JsonElement = decoder.decodeJsonElement()
        return when (element) {
            is JsonNull -> null
            is JsonPrimitive -> if (element.isString) element.content else element.toString()
            else -> element.toString()
        }
    }

    override fun serialize(encoder: Encoder, value: String?) {
        encoder.encodeSerializableValue(delegate, value)
    }
}

/** #EXP-1661: a jsonb string array stored by [JsonAsStringSerializer]
 *  (`["a","b"]`) as its strings; null, malformed or non-string = skipped. */
fun parseJsonStringList(raw: String?): List<String> {
    if (raw.isNullOrBlank()) return emptyList()
    val element = runCatching { kotlinx.serialization.json.Json.parseToJsonElement(raw) }.getOrNull()
    val array = element as? kotlinx.serialization.json.JsonArray ?: return emptyList()
    return array.mapNotNull { (it as? JsonPrimitive)?.takeIf { p -> p.isString }?.content }
}
