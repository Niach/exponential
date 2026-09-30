package com.exponential.app.ui.work

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.BitmapRegionDecoder
import android.graphics.Rect
import android.os.Build
import android.util.LruCache
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import com.exponential.app.data.api.AttachmentsApi
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.api.trpcErrorMessage
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.domain.SessionResultEntry
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

// EXP-1128: a TALL session result (a full-page capture, e.g. 780×25094 px)
// cannot go through Coil: its 4096 px longest-side cap decodes it ~127 px
// wide (a blur), and a full-size bitmap blows the texture limit. So the tile
// and the viewer fetch the original bytes once and decode REGIONS of them
// through ONE region decoder per composable (a decoder per strip re-parsed
// the whole file every time).

/**
 * How the Results face reaches the attachment API and the active account
 * without threading them through every Work/workflow screen (the idiom the
 * media player and emoji sheet use).
 */
@EntryPoint
@InstallIn(SingletonComponent::class)
interface TallImageEntryPoint {
    fun attachmentsApi(): AttachmentsApi
    fun authRepository(): AuthRepository
}

private fun tallImageEntryPoint(context: Context): TallImageEntryPoint =
    EntryPointAccessors.fromApplication(context.applicationContext, TallImageEntryPoint::class.java)

/** The in-memory byte ceiling: a few originals, sized by BYTES not count. */
private const val TALL_IMAGE_MEMORY_BYTES = 48 * 1024 * 1024

/** The pictures' bytes, so a tile, its viewer and a re-scroll share ONE download. */
private val tallImageBytes = object : LruCache<String, ByteArray>(TALL_IMAGE_MEMORY_BYTES) {
    override fun sizeOf(key: String, value: ByteArray): Int = value.size
}

/**
 * The per-attachment disk copy (attachments are immutable), beside the
 * Download flow's `attachments/<id>/<filename>` so an evicted original is
 * re-read, never re-downloaded.
 */
private fun tallImageCacheFile(context: Context, attachmentId: String): File =
    File(File(File(context.cacheDir, "attachments"), attachmentId), "original")

private suspend fun loadTallImageBytes(context: Context, attachmentId: String): ByteArray {
    tallImageBytes.get(attachmentId)?.let { return it }
    return withContext(Dispatchers.IO) {
        val file = tallImageCacheFile(context, attachmentId)
        val onDisk = runCatching { if (file.isFile && file.length() > 0) file.readBytes() else null }.getOrNull()
        val bytes = onDisk ?: run {
            val points = tallImageEntryPoint(context)
            val accountId = points.authRepository().activeAccountId.value
                ?: throw TrpcException("Sign in to view this file.")
            points.attachmentsApi()
                .download(accountId, "/api/attachments/$attachmentId")
                .also { fetched ->
                    runCatching {
                        file.parentFile?.mkdirs()
                        file.writeBytes(fetched)
                    }
                }
        }
        if (bytes.size <= TALL_IMAGE_MEMORY_BYTES) tallImageBytes.put(attachmentId, bytes)
        bytes
    }
}

/** What a tall picture's load resolved to. */
sealed interface TallImageSource {
    data object Loading : TallImageSource
    /** The download failed; [retry] on the load runs it again. */
    data class Failed(val message: String) : TallImageSource
    /** Bytes the region decoder cannot read (a GIF): Coil draws the picture. */
    data object Unsupported : TallImageSource
    /** ONE decoder for every region the composable asks for; recycled when it leaves. */
    class Regions(val decoder: BitmapRegionDecoder) : TallImageSource
}

class TallImageLoad(val source: TallImageSource, val retry: () -> Unit)

/**
 * The picture's original bytes as a region decoder, loaded off the main
 * thread (memory → disk → network). A failed download is re-run by [TallImageLoad.retry]
 * or by the composable re-entering composition; the decoder is recycled when
 * the load is replaced or the composable leaves.
 */
@Composable
fun rememberTallImage(entry: SessionResultEntry): TallImageLoad {
    val context = LocalContext.current
    var attempt by remember(entry.attachmentId) { mutableIntStateOf(0) }
    val source by produceState<TallImageSource>(TallImageSource.Loading, entry.attachmentId, attempt) {
        value = TallImageSource.Loading
        value = runCatching { loadTallImageBytes(context, entry.attachmentId) }.fold(
            onSuccess = { bytes ->
                withContext(Dispatchers.Default) { openTallRegionDecoder(bytes) }
                    ?.let { TallImageSource.Regions(it) }
                    ?: TallImageSource.Unsupported
            },
            onFailure = { TallImageSource.Failed(trpcErrorMessage(it, "Couldn't load this picture.")) },
        )
    }
    // Captured, not read at dispose time: by then the state already holds the
    // replacement and the OLD decoder is the one to recycle.
    val current = source
    DisposableEffect(current) {
        onDispose { (current as? TallImageSource.Regions)?.decoder?.recycle() }
    }
    return remember(current) { TallImageLoad(current, retry = { attempt++ }) }
}

/** Null when the bytes are not an image the region decoder reads. */
private fun openTallRegionDecoder(bytes: ByteArray): BitmapRegionDecoder? = runCatching {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        BitmapRegionDecoder.newInstance(bytes, 0, bytes.size)
    } else {
        @Suppress("DEPRECATION")
        BitmapRegionDecoder.newInstance(bytes, 0, bytes.size, false)
    }
}.getOrNull()

/**
 * Decodes source rows [top, top + rows) at full source width, subsampled by
 * the largest power of two that keeps the result at least [targetWidthPx]
 * wide. Null for an empty region or a recycled decoder.
 */
fun decodeTallRegion(decoder: BitmapRegionDecoder, top: Int, rows: Int, targetWidthPx: Int): Bitmap? =
    runCatching {
        val width = decoder.width
        val bottom = minOf(decoder.height, top + rows)
        if (width <= 0 || bottom <= top) return null
        var sample = 1
        while (targetWidthPx > 0 && width / (sample * 2) >= targetWidthPx) sample *= 2
        val options = BitmapFactory.Options().apply {
            inSampleSize = sample
            inPreferredConfig = Bitmap.Config.ARGB_8888
        }
        decoder.decodeRegion(Rect(0, top, width, bottom), options)
    }.getOrNull()
