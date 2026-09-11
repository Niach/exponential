@file:OptIn(UnstableApi::class)

package com.exponential.app.data.media

import android.content.Context
import android.graphics.Bitmap
import android.media.MediaMetadataRetriever
import android.net.Uri
import android.util.Log
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.util.UnstableApi
import androidx.media3.effect.Presentation
import androidx.media3.transformer.Composition
import androidx.media3.transformer.EditedMediaItem
import androidx.media3.transformer.Effects
import androidx.media3.transformer.ExportException
import androidx.media3.transformer.ExportResult
import androidx.media3.transformer.Transformer
import com.exponential.app.domain.MediaPreparer
import com.exponential.app.domain.PreparedMedia
import com.exponential.app.domain.canonicalContentType
import com.exponential.app.domain.isInlineAudio
import com.exponential.app.domain.isInlineVideo
import dagger.hilt.android.qualifiers.ApplicationContext
import java.io.ByteArrayOutputStream
import java.io.File
import java.util.UUID
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext

/**
 * The Android [MediaPreparer] (EXP-824): a picked video is normalised with
 * Media3 Transformer to H.264 + AAC in an MP4 clamped to 720p
 * (`Presentation.createForHeight(720)`; Transformer transmuxes when the input
 * already complies), a JPEG poster is pulled off `MediaMetadataRetriever`
 * (~0.1 s in, rotation applied) and the rotation-aware size + duration are
 * probed. Audio is uploaded as-is with its probed duration. The 50 MB file cap
 * is enforced by the CALLER on the returned bytes — after the transform, so a
 * 4K clip that shrinks under the cap still uploads.
 */
@Singleton
class AndroidMediaPreparer @Inject constructor(
    @ApplicationContext private val context: Context,
) : MediaPreparer {

    override suspend fun prepare(uri: Uri, filename: String, contentType: String): PreparedMedia {
        val canonical = canonicalContentType(contentType)
        return when {
            isInlineVideo(canonical) -> prepareVideo(uri, filename, canonical)
            isInlineAudio(canonical) -> prepareAudio(uri, filename, canonical)
            else -> throw IllegalArgumentException("Not a video or audio file")
        }
    }

    // -- Video --------------------------------------------------------------

    private class VideoProbe(
        val width: Int?,
        val height: Int?,
        val durationMs: Long?,
        val frame: Bitmap?,
    )

    private suspend fun prepareVideo(uri: Uri, filename: String, sourceType: String): PreparedMedia {
        val probe = withContext(Dispatchers.IO) { probeVideo(uri) }
        val (outWidth, outHeight) = clampToHeight(probe.width, probe.height, MAX_OUTPUT_HEIGHT)
        val outDir = File(context.cacheDir, CACHE_DIR).apply { mkdirs() }
        val out = File(outDir, "${UUID.randomUUID()}.mp4")
        try {
            val bytes: ByteArray
            val outType: String
            val outName: String
            val transcoded = runCatching { transcode(uri, out) }
            if (transcoded.isSuccess) {
                bytes = withContext(Dispatchers.IO) { out.readBytes() }
                outType = "video/mp4"
                outName = withMp4Extension(filename)
            } else {
                val error = transcoded.exceptionOrNull()
                Log.w(TAG, "Transformer failed for $sourceType, falling back to the original file", error)
                // Only a container the server's MP4/MOV probe understands may
                // bypass the normalisation; anything else is refused with the
                // transform's reason rather than uploaded unplayable.
                if (sourceType != "video/mp4" && sourceType != "video/quicktime") {
                    throw IllegalStateException("This video format can't be converted for upload")
                }
                bytes = withContext(Dispatchers.IO) { readAll(uri) }
                    ?: throw IllegalStateException("The video could not be read")
                outType = sourceType
                outName = filename
            }
            val poster = withContext(Dispatchers.IO) {
                probe.frame?.let { encodePoster(it, outWidth, outHeight) }
            }
            return PreparedMedia(
                bytes = bytes,
                filename = outName,
                contentType = outType,
                width = if (transcoded.isSuccess) outWidth else probe.width,
                height = if (transcoded.isSuccess) outHeight else probe.height,
                durationMs = probe.durationMs,
                poster = poster,
            )
        } finally {
            out.delete()
            probe.frame?.recycle()
        }
    }

    /** Transformer must be driven from a Looper thread: run it on Main, await the listener. */
    private suspend fun transcode(uri: Uri, out: File) = withContext(Dispatchers.Main.immediate) {
        suspendCancellableCoroutine<Unit> { cont ->
            val transformer = Transformer.Builder(context)
                .setVideoMimeType(MimeTypes.VIDEO_H264)
                .setAudioMimeType(MimeTypes.AUDIO_AAC)
                .addListener(
                    object : Transformer.Listener {
                        override fun onCompleted(composition: Composition, exportResult: ExportResult) {
                            if (cont.isActive) cont.resume(Unit)
                        }

                        override fun onError(
                            composition: Composition,
                            exportResult: ExportResult,
                            exportException: ExportException,
                        ) {
                            if (cont.isActive) cont.resumeWithException(exportException)
                        }
                    },
                )
                .build()
            val item = EditedMediaItem.Builder(MediaItem.fromUri(uri))
                .setEffects(Effects(emptyList(), listOf(Presentation.createForHeight(MAX_OUTPUT_HEIGHT))))
                .build()
            transformer.start(item, out.absolutePath)
            cont.invokeOnCancellation { transformer.cancel() }
        }
    }

    private fun probeVideo(uri: Uri): VideoProbe {
        val retriever = MediaMetadataRetriever()
        return try {
            retriever.setDataSource(context, uri)
            val rawWidth = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH)?.toIntOrNull()
            val rawHeight = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT)?.toIntOrNull()
            val rotation = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_ROTATION)?.toIntOrNull() ?: 0
            val duration = retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull()
            val swap = rotation == 90 || rotation == 270
            // The retriever's frame already has the rotation applied.
            val frame = runCatching {
                retriever.getFrameAtTime(POSTER_TIME_US, MediaMetadataRetriever.OPTION_CLOSEST_SYNC)
                    ?: retriever.getFrameAtTime(0, MediaMetadataRetriever.OPTION_CLOSEST_SYNC)
            }.getOrNull()
            VideoProbe(
                width = (if (swap) rawHeight else rawWidth)?.takeIf { it > 0 },
                height = (if (swap) rawWidth else rawHeight)?.takeIf { it > 0 },
                durationMs = duration?.takeIf { it > 0 },
                frame = frame,
            )
        } catch (t: Throwable) {
            Log.w(TAG, "Video probe failed", t)
            VideoProbe(null, null, null, null)
        } finally {
            runCatching { retriever.release() }
        }
    }

    /** A JPEG poster no larger than the output frame and under the 2 MB server cap. */
    private fun encodePoster(frame: Bitmap, width: Int?, height: Int?): ByteArray? {
        val scaled = if (width != null && height != null && width > 0 && height > 0 &&
            (frame.width > width || frame.height > height)
        ) {
            Bitmap.createScaledBitmap(frame, width, height, true)
        } else {
            frame
        }
        try {
            for (quality in POSTER_QUALITIES) {
                val buffer = ByteArrayOutputStream()
                if (!scaled.compress(Bitmap.CompressFormat.JPEG, quality, buffer)) return null
                if (buffer.size() <= MAX_POSTER_BYTES) return buffer.toByteArray()
            }
            return null
        } finally {
            if (scaled !== frame) scaled.recycle()
        }
    }

    // -- Audio --------------------------------------------------------------

    private suspend fun prepareAudio(uri: Uri, filename: String, contentType: String): PreparedMedia =
        withContext(Dispatchers.IO) {
            val bytes = readAll(uri) ?: throw IllegalStateException("The audio file could not be read")
            val retriever = MediaMetadataRetriever()
            val duration = try {
                retriever.setDataSource(context, uri)
                retriever.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull()
            } catch (t: Throwable) {
                Log.w(TAG, "Audio probe failed", t)
                null
            } finally {
                runCatching { retriever.release() }
            }
            PreparedMedia(
                bytes = bytes,
                filename = filename,
                contentType = contentType,
                width = null,
                height = null,
                durationMs = duration?.takeIf { it > 0 },
                poster = null,
            )
        }

    private fun readAll(uri: Uri): ByteArray? =
        runCatching { context.contentResolver.openInputStream(uri)?.use { it.readBytes() } }.getOrNull()

    private companion object {
        const val TAG = "AndroidMediaPreparer"
        const val CACHE_DIR = "media-uploads"
        const val MAX_OUTPUT_HEIGHT = 720
        const val POSTER_TIME_US = 100_000L
        const val MAX_POSTER_BYTES = 2L * 1024 * 1024
        val POSTER_QUALITIES = intArrayOf(82, 60, 40)
    }
}

/**
 * The size Transformer's `Presentation.createForHeight(maxHeight)` yields:
 * unchanged at or under the cap, else scaled to the cap keeping the aspect
 * ratio (the width is rounded to the nearest even pixel, as encoders need).
 */
internal fun clampToHeight(width: Int?, height: Int?, maxHeight: Int): Pair<Int?, Int?> {
    if (width == null || height == null || width <= 0 || height <= 0) return width to height
    if (height <= maxHeight) return width to height
    val scaled = width.toDouble() * maxHeight / height
    val even = (Math.round(scaled / 2) * 2).toInt().coerceAtLeast(2)
    return even to maxHeight
}

/** `clip.mov` → `clip.mp4`; a name without an extension gains one. */
internal fun withMp4Extension(filename: String): String {
    val dot = filename.lastIndexOf('.')
    val stem = if (dot > 0) filename.substring(0, dot) else filename
    return "$stem.mp4"
}
