package com.exponential.app.ui.work

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.BitmapRegionDecoder
import android.graphics.Rect
import android.os.Build
import android.util.LruCache
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.produceState
import androidx.compose.ui.platform.LocalContext
import com.exponential.app.data.api.AttachmentsApi
import com.exponential.app.data.api.TrpcException
import com.exponential.app.data.auth.AuthRepository
import com.exponential.app.domain.SessionResultEntry
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.android.EntryPointAccessors
import dagger.hilt.components.SingletonComponent
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

// EXP-1128: a TALL session result (a full-page capture, e.g. 780×25094 px)
// cannot go through Coil: its 4096 px longest-side cap decodes it ~127 px
// wide (a blur), and a full-size bitmap blows the texture limit. So the tile
// and the viewer fetch the original bytes once and decode REGIONS of them.

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

/** A few pictures' bytes, so the tile and its viewer share ONE download. */
private val tallImageBytes = LruCache<String, ByteArray>(4)

/** The picture's original bytes: null while loading, then the result. */
@Composable
fun rememberTallImageBytes(entry: SessionResultEntry): State<Result<ByteArray>?> {
    val context = LocalContext.current
    return produceState<Result<ByteArray>?>(
        initialValue = tallImageBytes.get(entry.attachmentId)?.let { Result.success(it) },
        entry.attachmentId,
    ) {
        if (value?.isSuccess == true) return@produceState
        val points = tallImageEntryPoint(context)
        value = runCatching {
            withContext(Dispatchers.IO) {
                val accountId = points.authRepository().activeAccountId.value
                    ?: throw TrpcException("Sign in to view this file.")
                points.attachmentsApi()
                    .download(accountId, "/api/attachments/${entry.attachmentId}")
                    .also { tallImageBytes.put(entry.attachmentId, it) }
            }
        }
    }
}

/**
 * Decodes source rows [top, top + rows) at full source width, subsampled by
 * the largest power of two that keeps the result at least [targetWidthPx]
 * wide. Null when the bytes are not a decodable image.
 */
fun decodeTallRegion(bytes: ByteArray, top: Int, rows: Int, targetWidthPx: Int): Bitmap? {
    val decoder = runCatching {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            BitmapRegionDecoder.newInstance(bytes, 0, bytes.size)
        } else {
            @Suppress("DEPRECATION")
            BitmapRegionDecoder.newInstance(bytes, 0, bytes.size, false)
        }
    }.getOrNull() ?: return null
    return try {
        val width = decoder.width
        val bottom = minOf(decoder.height, top + rows)
        if (width <= 0 || bottom <= top) return null
        var sample = 1
        while (targetWidthPx > 0 && width / (sample * 2) >= targetWidthPx) sample *= 2
        val options = BitmapFactory.Options().apply {
            inSampleSize = sample
            inPreferredConfig = Bitmap.Config.ARGB_8888
        }
        runCatching { decoder.decodeRegion(Rect(0, top, width, bottom), options) }.getOrNull()
    } finally {
        decoder.recycle()
    }
}
