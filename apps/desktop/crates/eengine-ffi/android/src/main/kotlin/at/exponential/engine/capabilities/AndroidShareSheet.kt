// #VAPP-265: share on Android: an ACTION_SEND chooser. A file is written under
// cacheDir/exponential-engine-share/<random>/ and served read-only by the
// library's ShareFileProvider (`<applicationId>.exponential.engine.share`,
// not exported, per-URI read grants only); share files older than a day are
// deleted on the next share. `shared` = the chooser's chosen-component
// callback (Intent.createChooser with an IntentSender) fired: a dismissed
// sheet answers false. (A chooser action such as Copy never reports a target,
// so it reads as false.)
package at.exponential.engine.capabilities

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.ClipData
import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.Build
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import androidx.activity.result.contract.ActivityResultContracts
import java.io.File
import java.io.FileNotFoundException
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class AndroidShareSheet(private val host: ActivityHost) : ShareSheet {
    override fun send(content: ShareContent): Boolean {
        val context = host.context
        val token = UUID.randomUUID().toString()
        val send = Intent(Intent.ACTION_SEND)
        content.text?.let { send.putExtra(Intent.EXTRA_TEXT, it) }
        content.title?.let {
            send.putExtra(Intent.EXTRA_SUBJECT, it)
            send.putExtra(Intent.EXTRA_TITLE, it)
        }
        val file = content.file
        if (file != null) {
            val uri = ShareFileProvider.store(context, file)
            send.type = file.mimeType
            send.putExtra(Intent.EXTRA_STREAM, uri)
            send.clipData = ClipData.newUri(context.contentResolver, file.name, uri)
            send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        } else {
            send.type = "text/plain"
        }
        val chosen = CountDownLatch(1)
        ShareChosenReceiver.waiting[token] = chosen
        try {
            val callback = Intent(context, ShareChosenReceiver::class.java)
                .setData(Uri.parse("exponential-engine-share:$token"))
                .putExtra(ShareChosenReceiver.EXTRA_TOKEN, token)
            val mutable = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) PendingIntent.FLAG_MUTABLE else 0
            val pending = PendingIntent.getBroadcast(context, token.hashCode(), callback, mutable or PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_ONE_SHOT)
            val chooser = Intent.createChooser(send, content.title, pending.intentSender)
            host.launch(ActivityResultContracts.StartActivityForResult(), chooser)
            // The chosen-component broadcast may land just after the chooser closes.
            return chosen.count == 0L || chosen.await(CALLBACK_GRACE_MS, TimeUnit.MILLISECONDS)
        } finally {
            ShareChosenReceiver.waiting.remove(token)
        }
    }

    companion object {
        const val CALLBACK_GRACE_MS = 1500L
    }
}

/** The chooser's chosen-component callback (declared in the library manifest, not exported). */
class ShareChosenReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        intent.getStringExtra(EXTRA_TOKEN)?.let { waiting[it]?.countDown() }
    }

    companion object {
        const val EXTRA_TOKEN = "at.exponential.engine.SHARE_TOKEN"
        internal val waiting = ConcurrentHashMap<String, CountDownLatch>()
    }
}

/** Read-only access to the files shared through [AndroidShareSheet] (not exported; URI grants only). */
class ShareFileProvider : ContentProvider() {
    override fun onCreate(): Boolean = true

    override fun getType(uri: Uri): String? = fileFor(uri)?.let { File(it.parentFile, TYPE_FILE).takeIf(File::exists)?.readText() }

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        if (mode != "r") throw FileNotFoundException("read-only")
        val file = fileFor(uri)?.takeIf(File::isFile) ?: throw FileNotFoundException(uri.toString())
        return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
    }

    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, selectionArgs: Array<out String>?, sortOrder: String?): Cursor? {
        val file = fileFor(uri)?.takeIf(File::isFile) ?: return null
        val columns = (projection ?: arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE))
            .filter { it == OpenableColumns.DISPLAY_NAME || it == OpenableColumns.SIZE }
        return MatrixCursor(columns.toTypedArray(), 1).apply {
            addRow(columns.map { if (it == OpenableColumns.DISPLAY_NAME) file.name else file.length() })
        }
    }

    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int = 0
    override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<out String>?): Int = 0

    private fun fileFor(uri: Uri): File? {
        val context = context ?: return null
        val segments = uri.pathSegments
        if (segments.size != 2) return null
        val root = shareDir(context).canonicalFile
        val file = File(File(root, segments[0]), segments[1]).canonicalFile
        return file.takeIf { it.parentFile?.parentFile == root && it.name != TYPE_FILE }
    }

    companion object {
        private const val DIR = "exponential-engine-share"
        private const val TYPE_FILE = ".type"
        private const val MAX_AGE_MS = 24L * 60 * 60 * 1000

        fun authority(context: Context): String = "${context.packageName}.exponential.engine.share"

        private fun shareDir(context: Context) = File(context.cacheDir, DIR)

        /** Write the file for sharing; its content URI. */
        fun store(context: Context, file: NativeFile): Uri {
            val root = shareDir(context)
            val cutoff = System.currentTimeMillis() - MAX_AGE_MS
            root.listFiles()?.filter { it.lastModified() < cutoff }?.forEach { it.deleteRecursively() }
            val name = ShareNames.safe(file.name)
            val dir = File(root, UUID.randomUUID().toString()).apply { mkdirs() }
            File(dir, name).writeBytes(file.bytes)
            File(dir, TYPE_FILE).writeText(file.mimeType)
            return Uri.Builder().scheme("content").authority(authority(context)).appendPath(dir.name).appendPath(name).build()
        }
    }
}

/** A file name safe as one path segment. */
object ShareNames {
    fun safe(name: String): String {
        val cleaned = name.replace('/', '_').replace('\\', '_').replace("\u0000", "").trim()
        return when (cleaned) {
            "", ".", "..", ".type" -> "file"
            else -> cleaned
        }
    }
}
