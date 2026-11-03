package com.exponential.app.ui.components

import android.content.Context
import androidx.compose.runtime.staticCompositionLocalOf
import org.json.JSONObject

/**
 * EXP-1654: the committed sample pictures a styleguide specimen paints
 * (`packages/exponential-ui/samples`, read its `samples.json` `$comment`).
 * A picture view asks [LocalSpecimenPicture] first: a non-null result (a
 * PNG's bytes, any Coil model) replaces its attachment src. Null outside a
 * specimen (the product never provides it).
 */
val LocalSpecimenPicture = staticCompositionLocalOf<((String) -> Any?)?> { null }

/** The sample set, bundled into DEBUG assets only (`syncSpecimenSamples`). */
object SpecimenSamples {
    private const val PREFIX = "/api/attachments/"
    @Volatile private var attachments: Map<String, String>? = null
    private val bytes = HashMap<String, ByteArray>()

    /** An attachment id (or its `/api/attachments/<id>` src) → its sample PNG, else null. */
    fun resolver(context: Context): (String) -> Any? {
        val assets = context.applicationContext.assets
        val table = attachments ?: runCatching {
            val json = JSONObject(assets.open("samples/samples.json").bufferedReader().use { it.readText() })
            val map = json.getJSONObject("attachments")
            map.keys().asSequence().associateWith { map.getString(it) }
        }.getOrDefault(emptyMap()).also { attachments = it }
        return { src ->
            val file = table[src.removePrefix(PREFIX)]
            file?.let { name ->
                synchronized(bytes) {
                    bytes.getOrPut(name) { assets.open("samples/$name").use { it.readBytes() } }
                }
            }
        }
    }
}
