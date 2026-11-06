// #VAPP-265: files on Android: the Storage Access Framework (no permission).
package at.exponential.engine.capabilities

import android.net.Uri
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import androidx.activity.result.contract.ActivityResultContracts

class AndroidDocumentPicker(private val host: ActivityHost) : DocumentPicker {
    override fun open(mimeTypes: List<String>, multiple: Boolean): List<PickedFile> {
        val types = mimeTypes.toTypedArray()
        val uris: List<Uri> = if (multiple) {
            host.launch(ActivityResultContracts.OpenMultipleDocuments(), types)
        } else {
            listOfNotNull(host.launch(ActivityResultContracts.OpenDocument(), types))
        }
        return uris.map(::readFile)
    }

    override fun save(name: String, mimeType: String, bytes: ByteArray): Boolean {
        val uri = host.launch(ActivityResultContracts.CreateDocument(mimeType), name) ?: return false
        val out = host.context.contentResolver.openOutputStream(uri, "wt") ?: throw ToolFailure.failed("cannot write the file")
        out.use { it.write(bytes) }
        return true
    }

    override fun mimeForExtension(extension: String): String? = MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension)

    private fun readFile(uri: Uri): PickedFile {
        val resolver = host.context.contentResolver
        val name = resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
            if (c.moveToFirst() && !c.isNull(0)) c.getString(0) else null
        } ?: uri.lastPathSegment
        val bytes = resolver.openInputStream(uri)?.use { it.readBytes() } ?: throw ToolFailure.failed("cannot read a picked file")
        return PickedFile(name, resolver.getType(uri), bytes)
    }
}
