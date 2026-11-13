package com.exponential.app.data.outbox

import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json

/**
 * #FEED-237 (message-delivery.json FILES): an outbox entry's picked file not
 * uploaded yet. Its bytes live at [path] (under the app's files dir,
 * `outbox/<clientId>/<n>`) so an app restart resumes the upload; the entry's
 * `pending_files` column holds the list, oldest pick first.
 */
@Serializable
data class OutboxFile(val filename: String, val contentType: String, val path: String)

/** The pure bookkeeping of an entry's files (no Android, no IO but [dir]). */
object OutboxFiles {
    private val json = Json { ignoreUnknownKeys = true }
    private val serializer = ListSerializer(OutboxFile.serializer())

    /** The column's wire (null = nothing pending). */
    fun encode(files: List<OutboxFile>): String? =
        files.takeIf { it.isNotEmpty() }?.let { json.encodeToString(serializer, it) }

    /** An unreadable column reads as nothing pending. */
    fun decode(wire: String?): List<OutboxFile> =
        if (wire.isNullOrBlank()) emptyList() else runCatching { json.decodeFromString(serializer, wire) }.getOrDefault(emptyList())

    /** Where an entry's files live: `<filesDir>/outbox/<clientId>`. */
    fun dir(filesDir: File, clientId: String): File = File(File(filesDir, "outbox"), clientId)

    /** The press: the pending picks as files `0`, `1`, … under [dir]. */
    fun plan(dir: File, picks: List<Pair<String, String>>): List<OutboxFile> =
        picks.mapIndexed { index, (filename, contentType) -> OutboxFile(filename, contentType, File(dir, "$index").path) }

    /** What an entry holds after its file [landed] uploaded as [id]: the id
     *  joins [landedIds] (in landing order), the file leaves [pending]. */
    data class Uploads(val attachmentIds: List<String>, val pending: List<OutboxFile>) {
        fun landed(file: OutboxFile, id: String): Uploads =
            Uploads(attachmentIds + id, pending.filterNot { it == file })

        val attachmentIdsWire: String? get() = attachmentIds.takeIf { it.isNotEmpty() }?.joinToString(",")
        val pendingWire: String? get() = encode(pending)
    }
}
