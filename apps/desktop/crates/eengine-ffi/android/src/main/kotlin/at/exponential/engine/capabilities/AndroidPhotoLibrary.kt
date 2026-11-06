// #VAPP-265: photos on Android: MediaStore.Images (list, get) and the Photo
// Picker (pick; the AndroidX contract falls back to ACTION_OPEN_DOCUMENT where
// the picker is missing). Ids = content URIs (MediaStore or picker ones; a
// picker URI stays readable while the app process holds its grant).
package at.exponential.engine.capabilities

import android.Manifest
import android.content.ContentResolver
import android.content.ContentUris
import android.database.Cursor
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.MediaStore
import android.provider.OpenableColumns
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import java.io.FileNotFoundException

class AndroidPhotoLibrary(private val host: ActivityHost) : PhotoLibrary {
    private val resolver: ContentResolver get() = host.context.contentResolver

    override fun ensureReadAccess(): Boolean = host.requestAnyPermission(readPermissions())

    override fun query(offset: Int, limit: Int): List<PhotoRef> {
        val collection = MediaStore.Images.Media.EXTERNAL_CONTENT_URI
        val projection = arrayOf(
            MediaStore.Images.Media._ID, MediaStore.Images.Media.DISPLAY_NAME, MediaStore.Images.Media.MIME_TYPE,
            MediaStore.Images.Media.DATE_TAKEN, MediaStore.Images.Media.DATE_MODIFIED,
        )
        val sort = "${MediaStore.Images.Media.DATE_TAKEN} DESC, ${MediaStore.Images.Media.DATE_MODIFIED} DESC, ${MediaStore.Images.Media._ID} DESC"
        val paged = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
        val args = Bundle().apply {
            putString(ContentResolver.QUERY_ARG_SQL_SORT_ORDER, sort)
            if (paged) {
                putInt(ContentResolver.QUERY_ARG_LIMIT, limit)
                putInt(ContentResolver.QUERY_ARG_OFFSET, offset)
            }
        }
        val out = ArrayList<PhotoRef>()
        resolver.query(collection, projection, args, null)?.use { c ->
            // Before API 30 MediaStore ignores LIMIT/OFFSET: page the cursor itself.
            if (!paged && !c.moveToPosition(offset - 1) && offset > 0) return out
            while (out.size < limit && c.moveToNext()) {
                val id = c.getLong(0)
                out.add(
                    PhotoRef(
                        id = ContentUris.withAppendedId(collection, id).toString(),
                        name = c.getStringOrNull(1),
                        mimeType = c.getStringOrNull(2),
                        takenAtMillis = takenAt(c.getLongOrNull(3), c.getLongOrNull(4)),
                    ),
                )
            }
        }
        return out
    }

    override fun read(id: String): PhotoFile? {
        val uri = Uri.parse(id)
        if (uri.scheme != ContentResolver.SCHEME_CONTENT) return null
        val ref = try {
            describe(uri, id)
        } catch (_: SecurityException) {
            if (!ensureReadAccess()) throw ToolFailure.failed("permission denied")
            describe(uri, id)
        } ?: return null
        val bytes = try {
            resolver.openInputStream(uri)?.use { it.readBytes() }
        } catch (_: FileNotFoundException) {
            null
        } ?: return null
        return PhotoFile(ref, bytes)
    }

    override fun pick(limit: Int): List<String> {
        val request = PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)
        val uris: List<Uri> = if (limit <= 1) {
            listOfNotNull(host.launch(ActivityResultContracts.PickVisualMedia(), request))
        } else {
            val max = limit.coerceAtMost(maxPickable())
            if (max <= 1) listOfNotNull(host.launch(ActivityResultContracts.PickVisualMedia(), request))
            else host.launch(ActivityResultContracts.PickMultipleVisualMedia(max), request)
        }
        return uris.map { it.toString() }
    }

    /** Name, type and time of one photo; null = it does not exist. */
    private fun describe(uri: Uri, id: String): PhotoRef? {
        val withTimes = arrayOf(OpenableColumns.DISPLAY_NAME, MediaStore.MediaColumns.DATE_TAKEN, MediaStore.MediaColumns.DATE_MODIFIED)
        val cursor = try {
            resolver.query(uri, withTimes, null, null, null)
        } catch (_: IllegalArgumentException) {
            resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
        } catch (_: UnsupportedOperationException) {
            null
        }
        val ref = cursor?.use { c ->
            if (!c.moveToFirst()) return null
            val taken = c.columnIndex(MediaStore.MediaColumns.DATE_TAKEN)?.let { c.getLongOrNull(it) }
            val modified = c.columnIndex(MediaStore.MediaColumns.DATE_MODIFIED)?.let { c.getLongOrNull(it) }
            PhotoRef(id, c.columnIndex(OpenableColumns.DISPLAY_NAME)?.let { c.getStringOrNull(it) }, null, takenAt(taken, modified))
        } ?: PhotoRef(id, uri.lastPathSegment, null, null)
        return ref.copy(mimeType = resolver.getType(uri))
    }

    private fun maxPickable(): Int =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU || isPhotoPickerExtension()) MediaStore.getPickImagesMaxLimit() else Int.MAX_VALUE

    private fun isPhotoPickerExtension(): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && android.os.ext.SdkExtensions.getExtensionVersion(Build.VERSION_CODES.R) >= 2

    companion object {
        /** The library read permission per API level (34+ also takes "selected photos" access). */
        fun readPermissions(): List<String> = when {
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE -> listOf(Manifest.permission.READ_MEDIA_IMAGES, Manifest.permission.READ_MEDIA_VISUAL_USER_SELECTED)
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU -> listOf(Manifest.permission.READ_MEDIA_IMAGES)
            else -> listOf(Manifest.permission.READ_EXTERNAL_STORAGE)
        }

        /** DATE_TAKEN (ms) when known, else DATE_MODIFIED (s). */
        fun takenAt(dateTakenMillis: Long?, dateModifiedSeconds: Long?): Long? =
            dateTakenMillis?.takeIf { it > 0 } ?: dateModifiedSeconds?.takeIf { it > 0 }?.let { it * 1000 }
    }
}

private fun Cursor.columnIndex(name: String): Int? = getColumnIndex(name).takeIf { it >= 0 }
private fun Cursor.getStringOrNull(i: Int): String? = if (isNull(i)) null else getString(i)
private fun Cursor.getLongOrNull(i: Int): Long? = if (isNull(i)) null else getLong(i)
