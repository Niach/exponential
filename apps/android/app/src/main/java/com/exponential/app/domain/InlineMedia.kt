package com.exponential.app.domain

import android.net.Uri

/**
 * A picked video/audio file after upload normalisation (EXP-824): for video
 * the H.264 + AAC MP4 clamped to 720p (transmuxed when the input already
 * complies), its JPEG poster frame and the rotation-aware probe; for audio the
 * original bytes plus the probed duration. Pure Kotlin so the editor / comment
 * logic that consumes it stays unit-testable — the Media3 / retriever work
 * lives behind [MediaPreparer].
 */
class PreparedMedia(
    val bytes: ByteArray,
    val filename: String,
    /** Canonical (`canonicalContentType`) type of [bytes], e.g. `video/mp4`. */
    val contentType: String,
    val width: Int?,
    val height: Int?,
    val durationMs: Long?,
    /** JPEG poster frame (video only), ≤ 2 MB, or null when none could be made. */
    val poster: ByteArray?,
) {
    val isVideo: Boolean get() = isInlineVideo(contentType)
    val isAudio: Boolean get() = isInlineAudio(contentType)
}

/**
 * The framework seam the pickers call before an inline-media upload. The
 * Android implementation (`data/media/AndroidMediaPreparer`) runs Media3
 * Transformer + MediaMetadataRetriever; JVM tests substitute a stub. Throws
 * with a user-readable message when the file cannot be prepared.
 */
interface MediaPreparer {
    suspend fun prepare(uri: Uri, filename: String, contentType: String): PreparedMedia
}

/**
 * The duration chip's text, ×4 parity: `0:07`, `2:34`, `1:02:03`. Rounds to
 * the NEAREST second like web `formatDuration` and iOS `MediaDuration` (a
 * 6.9 s clip reads `0:07`); anything negative or unknown reads as zero.
 */
fun formatDuration(durationMs: Long?): String {
    val totalSeconds = ((durationMs ?: 0L).coerceAtLeast(0L) + 500L) / 1000
    val hours = totalSeconds / 3600
    val minutes = (totalSeconds % 3600) / 60
    val seconds = totalSeconds % 60
    return if (hours > 0) {
        "%d:%02d:%02d".format(hours, minutes, seconds)
    } else {
        "%d:%02d".format(minutes, seconds)
    }
}

/**
 * A filename made safe as the label of the `[label](url)` media link: the
 * bracket characters and line breaks that would break the link syntax become
 * `_` (an unbalanced `]` ends the label early on every commonmark parser).
 * Parentheses and everything else pass through — commonmark allows them in
 * link text. Empty falls back to a generic label so the link keeps a label.
 */
fun mediaLinkLabel(filename: String?): String {
    val cleaned = (filename ?: "")
        .map { ch -> if (ch == '[' || ch == ']' || ch == '\n' || ch == '\r') '_' else ch }
        .joinToString("")
        .trim()
    return cleaned.ifEmpty { "media" }
}
