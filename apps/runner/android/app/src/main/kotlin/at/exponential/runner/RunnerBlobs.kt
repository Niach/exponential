package at.exponential.runner

import at.exponential.engine.ffi.BlobStore
import at.exponential.engine.ffi.EngineException
import at.exponential.ui.json.JsonValue
import java.io.File
import java.io.IOException
import java.util.UUID

/**
 * VAPP-271: the runner's app files (the Engine's BlobStore), handed to the
 * Engine so the painter reads the SAME store: an `app-file:<fileId>` image
 * src (catalog/host.json media.appFile) is these bytes. On disk it keeps the
 * Engine's own FsBlobStore layout under `<dataDir>/blobs` (`<app>/<uuid>` +
 * `<app>/<uuid>.json` = {name, mime}), so files written before stay
 * readable; [root] null = in memory (tests).
 */
class RunnerBlobs(private val root: File?) : BlobStore {
    private val memory = HashMap<String, ByteArray>()

    private fun dir(app: String): File? {
        if (app.isEmpty() || app.contains('/') || app.contains('\\') || app.startsWith(".")) {
            throw EngineException.Failure("BAD_REQUEST", "not an app id")
        }
        return root?.let { File(it, app) }
    }

    /** FsBlobStore's ids are uuids; anything else names no file. */
    private fun isId(id: String): Boolean = runCatching { UUID.fromString(id) }.isSuccess && id.length == 36

    private fun <T> io(block: () -> T): T = try {
        block()
    } catch (e: IOException) {
        throw EngineException.Failure("IO", e.message ?: "io")
    }

    override fun `put`(`app`: String, `name`: String, `mime`: String, `bytes`: ByteArray): String {
        val id = UUID.randomUUID().toString()
        val dir = dir(app) ?: return synchronized(memory) { memory["$app/$id"] = bytes; id }
        io {
            dir.mkdirs()
            File(dir, id).writeBytes(bytes)
            File(dir, "$id.json").writeText(JsonValue.Obj(mapOf("mime" to JsonValue.Str(mime), "name" to JsonValue.Str(name))).json)
        }
        return id
    }

    override fun `get`(`app`: String, `id`: String): ByteArray? {
        if (!isId(id)) return null
        val dir = dir(app) ?: return synchronized(memory) { memory["$app/$id"] }
        val file = File(dir, id)
        return if (file.isFile) io { file.readBytes() } else null
    }

    override fun `delete`(`app`: String, `id`: String) {
        if (!isId(id)) return
        val dir = dir(app) ?: return synchronized(memory) { memory.remove("$app/$id"); Unit }
        io { listOf(File(dir, id), File(dir, "$id.json")).forEach { if (it.exists() && !it.delete()) throw IOException("could not delete $it") } }
    }
}
