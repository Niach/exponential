// #VAPP-265: `photos` (get, list, pick). Android: MediaStore.Images for list
// and get (READ_MEDIA_IMAGES on API 33+, READ_EXTERNAL_STORAGE before), the
// Photo Picker for pick (no permission). An id = the photo's content URI.
package at.exponential.engine.capabilities

/** A photo without its bytes. Unknown fields stay null; the capability fills defaults. */
data class PhotoRef(val id: String, val name: String?, val mimeType: String?, val takenAtMillis: Long?)

class PhotoFile(val ref: PhotoRef, val bytes: ByteArray)

/** The platform's photo library (MediaStore + the Photo Picker on Android). */
interface PhotoLibrary {
    /** Read access to the whole library (asks the person when needed); false = denied. */
    fun ensureReadAccess(): Boolean

    /** The library newest first: up to [limit] photos from [offset]. */
    fun query(offset: Int, limit: Int): List<PhotoRef>

    /** One photo with its bytes; null = no such photo. */
    fun read(id: String): PhotoFile?

    /** The platform picker (blocks until the person is done): the picked ids, empty = cancelled. */
    fun pick(limit: Int): List<String>
}

/** The opaque `cursor` of photos.list: an offset token. */
object PhotoCursor {
    fun encode(offset: Int): String = "o$offset"

    fun decode(cursor: String?): Int {
        if (cursor == null) return 0
        val n = cursor.takeIf { it.startsWith("o") }?.drop(1)?.toIntOrNull()
        return n?.takeIf { it >= 0 } ?: throw ToolFailure.failed("unknown cursor")
    }
}

class PhotosCapability(
    private val library: PhotoLibrary,
    mainThread: MainThreadCheck = AndroidMainThread,
) : NativeCapability("photos", mainThread) {
    override val toolNames = setOf("get", "list", "pick")

    override fun handle(tool: String, args: Args): Map<String, Any?> = when (tool) {
        "list" -> list(args.int("limit", 50).coerceIn(1, 100), PhotoCursor.decode(args.optString("cursor")))
        "get" -> mapOf("photo" to withBytes(library.read(args.string("id")) ?: throw ToolFailure.notFound()))
        else -> pick(args.int("limit", 1).coerceIn(1, 50))
    }

    private fun list(limit: Int, offset: Int): Map<String, Any?> {
        if (!library.ensureReadAccess()) throw ToolFailure.failed("permission denied")
        val page = library.query(offset, limit + 1)
        val photos = page.take(limit).map { reference(it) }
        val cursor = if (page.size > limit) PhotoCursor.encode(offset + limit) else null
        return linkedMapOf("photos" to photos, "cursor" to cursor)
    }

    private fun pick(limit: Int): Map<String, Any?> {
        val ids = library.pick(limit).distinct().take(limit)
        val photos = ids.map { id -> withBytes(library.read(id) ?: throw ToolFailure.failed("a picked photo cannot be read")) }
        return mapOf("photos" to photos)
    }

    private fun reference(ref: PhotoRef): Map<String, Any?> {
        val name = Text.name(ref.name, "photo")
        return linkedMapOf(
            "id" to ref.id,
            "name" to name,
            "mimeType" to Mime.pick(ref.mimeType, name, "image/jpeg"),
            "takenAt" to Times.rfc3339(ref.takenAtMillis),
        )
    }

    private fun withBytes(file: PhotoFile): Map<String, Any?> = reference(file.ref) + ("data" to Bytes.encode(file.bytes))
}
