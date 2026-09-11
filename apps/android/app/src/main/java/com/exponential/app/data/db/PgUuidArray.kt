package com.exponential.app.data.db

import androidx.room.TypeConverter
import kotlinx.serialization.KSerializer
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive

/**
 * A Postgres `uuid[]` column as it reaches a synced row (FEED-33,
 * `devices.shared_team_ids`). Electric ships the cell as the Postgres text
 * array literal inside a JSON string (`"{a,b}"`, `"{}"` when empty); tRPC
 * and tests hand over a native JSON array. Anything else (null, absent,
 * garbage) reads as EMPTY — a row must never be dropped over its share list.
 */
object PgUuidArraySerializer : KSerializer<List<String>> {
    private val delegate = ListSerializer(String.serializer())
    override val descriptor: SerialDescriptor = delegate.descriptor

    override fun serialize(encoder: Encoder, value: List<String>) =
        encoder.encodeSerializableValue(delegate, value)

    override fun deserialize(decoder: Decoder): List<String> {
        val jsonDecoder = decoder as? JsonDecoder ?: return decoder.decodeSerializableValue(delegate)
        return fromElement(jsonDecoder.decodeJsonElement())
    }

    fun fromElement(element: JsonElement): List<String> = when (element) {
        is JsonNull -> emptyList()
        is JsonArray -> element.mapNotNull { (it as? JsonPrimitive)?.content }.clean()
        is JsonPrimitive -> if (element.isString) parseText(element.content) else emptyList()
        else -> emptyList()
    }

    /** The text literal (`{a,b}`, elements optionally double-quoted) or a JSON array as text. */
    fun parseText(raw: String): List<String> {
        val text = raw.trim()
        return when {
            text.startsWith("{") && text.endsWith("}") ->
                text.substring(1, text.length - 1).split(",").clean()
            text.startsWith("[") ->
                runCatching { fromElement(Json.parseToJsonElement(text)) }.getOrDefault(emptyList())
            else -> emptyList()
        }
    }

    private fun List<String>.clean(): List<String> =
        map { it.trim().trim('"').trim() }.filter { it.isNotEmpty() }.distinct()
}

/**
 * Room storage for the list columns typed as `List<String>` (only
 * `devices.shared_team_ids` so far): JSON array text, the same shape the raw
 * jsonb text columns keep.
 */
object StringListConverters {
    private val json = Json

    @TypeConverter
    fun fromList(value: List<String>): String =
        json.encodeToString(ListSerializer(String.serializer()), value)

    @TypeConverter
    fun toList(value: String): List<String> = PgUuidArraySerializer.parseText(value)
}
