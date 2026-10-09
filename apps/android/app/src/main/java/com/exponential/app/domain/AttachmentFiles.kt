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
 * EXP-824 inline media, the ×4 mirror of the server's `isInlineVideo` /
 * `isInlineAudio`: a PREFIX match on the canonical type (`video/mp4`,
 * `video/quicktime`, `audio/mpeg`, …), unlike the exact five-type image set
 * above. A media row leaves the Files rail like an inline image does and is
 * referenced from markdown as a plain link on its own paragraph
 * (`[clip.mp4](/api/attachments/{id})`), which the renderer upgrades to a
 * player once the row has synced. Stored types are canonical already, so a
 * parameterised or upper-cased type is deliberately NOT media here — the
 * same exactness rule the image classification documents.
 */
fun isInlineVideo(contentType: String?): Boolean =
    contentType != null && contentType.startsWith("video/")

fun isInlineAudio(contentType: String?): Boolean =
    contentType != null && contentType.startsWith("audio/")

/** Video or audio — anything the markdown pipeline plays inline. */
fun isInlineMedia(contentType: String?): Boolean =
    isInlineVideo(contentType) || isInlineAudio(contentType)

// EXP-1247: the ROW-aware rule (web `lib/attachment-files.ts`, desktop
// `issue_files::is_file_row`, iOS `AttachmentFiles.isFile`): a row uploaded
// through a FILE/paperclip path carries `as_file` and is never inline (image
// or media), so it always shows in Files; otherwise the content type decides.

/** An inline image row: an embeddable raster type, never an `as_file` upload. */
fun isInlineImageAttachment(contentType: String?, asFile: Boolean): Boolean =
    !asFile && isInlineImage(contentType)

/** An inline media row: a video/audio type, never an `as_file` upload. */
fun isInlineMediaAttachment(contentType: String?, asFile: Boolean): Boolean =
    !asFile && isInlineMedia(contentType)

/** A Files row: a type that never inlines, OR an `as_file` upload. */
fun isFileAttachment(contentType: String?, asFile: Boolean): Boolean =
    asFile || (!isInlineImage(contentType) && !isInlineMedia(contentType))

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

/**
 * EXP-1003: a Files row that previews IN the app through the markdown
 * renderer instead of being handed to another app. The ×4 mirror of the
 * web's `isMarkdownAttachment` (EXP-955): matched by the stored type, or by
 * the extension when the type is empty / generic (Safari uploads `.md` as
 * octet-stream, some pickers as text/plain).
 */
fun isMarkdownAttachment(contentType: String?, filename: String?): Boolean {
    val essence = (contentType ?: "").substringBefore(';').trim().lowercase()
    if (essence == "text/markdown" || essence == "text/x-markdown") return true
    if (essence != "" && essence != "text/plain" && essence != "application/octet-stream") {
        return false
    }
    return MARKDOWN_EXTENSION.containsMatchIn((filename ?: "").trim())
}

private val MARKDOWN_EXTENSION = Regex("\\.(md|markdown)$", RegexOption.IGNORE_CASE)

/**
 * EXP-1003: the largest markdown file the preview fetches and renders (the
 * web's `MARKDOWN_PREVIEW_MAX_BYTES`). Anything bigger is a download.
 */
const val MARKDOWN_PREVIEW_MAX_BYTES: Long = 1024L * 1024

/** EXP-1003: the markdown preview sheet's phases, mirroring the web dialog. */
sealed interface MarkdownPreviewState {
    data object Loading : MarkdownPreviewState
    data class Ready(val markdown: String) : MarkdownPreviewState
    data object TooLarge : MarkdownPreviewState
    data class Error(val message: String) : MarkdownPreviewState
}

/**
 * The first of the two size checks: a row whose recorded size is over the
 * ceiling is [MarkdownPreviewState.TooLarge] WITHOUT a fetch; null = fetch.
 */
fun markdownPreviewPrecheck(sizeBytes: Long): MarkdownPreviewState? =
    if (sizeBytes > MARKDOWN_PREVIEW_MAX_BYTES) MarkdownPreviewState.TooLarge else null

/**
 * The second size check, on the fetched bytes — legacy rows carry
 * `size_bytes = 0`, so only the body tells. Compares the decoded text's
 * UTF-16 length, the web's `text.length`.
 */
fun markdownPreviewOutcome(bytes: ByteArray): MarkdownPreviewState {
    val text = bytes.decodeToString()
    return if (text.length > MARKDOWN_PREVIEW_MAX_BYTES) {
        MarkdownPreviewState.TooLarge
    } else {
        MarkdownPreviewState.Ready(text)
    }
}

/** The web's copy for a non-2xx attachment fetch. */
fun markdownPreviewHttpError(status: Int): String =
    if (status == 404) {
        "This file is no longer available."
    } else {
        "Couldn't load this file (HTTP $status)."
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
