// #VAPP-265: `files` (open, save). Android: the Storage Access Framework
// (ACTION_OPEN_DOCUMENT filtered by the accepted MIME types,
// ACTION_CREATE_DOCUMENT for save). No permission.
package at.exponential.engine.capabilities

/** A file the person picked, as the platform reports it. */
class PickedFile(val name: String?, val mimeType: String?, val bytes: ByteArray)

/** The platform's document dialogs (block until the person is done). */
interface DocumentPicker {
    // Picked files; empty = cancelled. `mimeTypes` = what to show (the wildcard = all).
    fun open(mimeTypes: List<String>, multiple: Boolean): List<PickedFile>

    /** A save dialog: false = cancelled. */
    fun save(name: String, mimeType: String, bytes: ByteArray): Boolean

    /** A platform lookup of a type by extension (MimeTypeMap), else null. */
    fun mimeForExtension(extension: String): String? = null
}

class FilesCapability(
    private val picker: DocumentPicker,
    mainThread: MainThreadCheck = AndroidMainThread,
) : NativeCapability("files", mainThread) {
    override val toolNames = setOf("open", "save")

    override fun handle(tool: String, args: Args): Map<String, Any?> = when (tool) {
        "open" -> open(args.strings("accept"), args.bool("multiple", false))
        else -> {
            val file = args.asFile()
            mapOf("saved" to picker.save(Text.name(file.name, "file"), file.mimeType, file.bytes))
        }
    }

    private fun open(accept: List<String>, multiple: Boolean): Map<String, Any?> {
        val picked = picker.open(Mime.acceptToMimeTypes(accept, picker::mimeForExtension), multiple)
        val files = picked.take(if (multiple) 100 else 1).map { f ->
            val name = Text.name(f.name, "file")
            linkedMapOf(
                "name" to name,
                "mimeType" to Mime.pick(f.mimeType, name, "application/octet-stream", picker::mimeForExtension),
                "data" to Bytes.encode(f.bytes),
            )
        }
        return mapOf("files" to files)
    }
}
