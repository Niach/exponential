package com.exponential.app.domain

/**
 * Attachment classification + file-handling helpers (EXP-297).
 *
 * An attachment row is an INLINE IMAGE iff its `content_type` is exactly one
 * of the five raster types the markdown pipeline embeds — the exact mirror of
 * the server's `acceptedImageContentTypes` (apps/web/src/lib/storage/issue-attachments.ts).
 * Everything else — including any other `image/` subtype such as `image/tiff` or
 * `image/svg+xml` — belongs in the issue's Files section, so nothing an upload
 * accepted can end up invisible.
 */
val INLINE_IMAGE_CONTENT_TYPES: Set<String> = setOf(
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
    "image/avif",
)

/**
 * True when [contentType] names one of the five inline-embeddable raster
 * types. The comparison is an EXACT string match, mirroring the server, web,
 * and desktop — a stored `image/PNG` or parameterized type is a Files row
 * everywhere, and classifying it inline here would hide it on Android only.
 * Client-derived types (`ContentResolver.getType`) must be canonicalized via
 * [canonicalContentType] BEFORE upload, never at classification time.
 */
fun isInlineImage(contentType: String?): Boolean =
    contentType != null && contentType in INLINE_IMAGE_CONTENT_TYPES

/**
 * Canonical upload form of a picker-derived content type: lowercased media
 * essence with any `;`-parameter suffix stripped, falling back to
 * `application/octet-stream`. Mirrors the server's `canonicalizeContentType`
 * so stored rows always classify identically on every client.
 */
fun canonicalContentType(raw: String?): String {
    val essence = (raw ?: "").substringBefore(';').trim().lowercase()
    return essence.ifEmpty { "application/octet-stream" }
}

/** Non-image upload cap (the server's `maxFileUploadBytes`). */
const val MAX_FILE_UPLOAD_BYTES: Long = 50L * 1024 * 1024

/**
 * Inline-image upload cap (the server's `maxImageUploadBytes`) — refuse
 * locally rather than push megabytes over a mobile uplink only to be rejected.
 * Shared by the steer composer (EXP-511) and comment attachments (EXP-554).
 */
const val MAX_IMAGE_UPLOAD_BYTES: Long = 10L * 1024 * 1024

/** Attachments one comment can carry (the server's `MAX_COMMENT_ATTACHMENTS`). */
const val MAX_COMMENT_ATTACHMENTS: Int = 10

/**
 * Make a server-supplied filename safe to use as a local cache filename:
 * strip path separators (so `../../x` can never escape the per-attachment
 * directory), control characters, and the path-traversal names themselves.
 * Clamped so long names can't blow the filesystem's per-component limit.
 */
fun sanitizeFilename(name: String?): String {
    val cleaned = (name ?: "")
        .map { ch ->
            when {
                ch == '/' || ch == '\\' -> '_'
                ch.code < 0x20 || ch.code == 0x7F -> '_'
                else -> ch
            }
        }
        .joinToString("")
        .trim()
    if (cleaned.isEmpty() || cleaned == "." || cleaned == "..") return "file"
    return if (cleaned.length > MAX_FILENAME_LENGTH) cleaned.take(MAX_FILENAME_LENGTH) else cleaned
}

private const val MAX_FILENAME_LENGTH = 120
