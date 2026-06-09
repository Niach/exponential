package com.exponential.app.ui.markdown

import java.util.UUID

private val MARKDOWN_IMAGE_REGEX = Regex("""!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)""")

/** A not-yet-uploaded image placeholder URL, e.g. `draft://<uuid>`. */
fun draftUrl(): String = "draft://${UUID.randomUUID()}"

fun isDraftUrl(url: String): Boolean = url.startsWith("draft://")

/** True if any image reference in [markdown] is still an unuploaded draft. */
fun hasDraftImages(markdown: String): Boolean =
    MARKDOWN_IMAGE_REGEX.findAll(markdown).any { isDraftUrl(it.groupValues[2]) }

/**
 * Drop every still-unuploaded `draft://` image reference. Used by save/send
 * paths so an in-flight or failed upload never leaks a draft placeholder into
 * a persisted description/comment (the editor keeps the row + bytes around for
 * retry regardless).
 */
fun stripDraftImages(markdown: String): String =
    MARKDOWN_IMAGE_REGEX.replace(markdown) { match ->
        if (isDraftUrl(match.groupValues[2])) "" else match.value
    }

/** Drop image references whose URL is in `urls`. */
fun removeMarkdownImagesByUrl(markdown: String, urls: Collection<String>): String {
    if (urls.isEmpty()) return markdown
    val urlSet = urls.toSet()
    return MARKDOWN_IMAGE_REGEX.replace(markdown) { match ->
        if (match.groupValues[2] in urlSet) "" else match.value
    }
}

/** Substitute image URLs found in `replacements` with the new URLs; preserve alt text. */
fun replaceMarkdownImageUrls(markdown: String, replacements: Map<String, String>): String {
    if (replacements.isEmpty()) return markdown
    return MARKDOWN_IMAGE_REGEX.replace(markdown) { match ->
        val alt = match.groupValues[1]
        val url = match.groupValues[2]
        val next = replacements[url] ?: return@replace match.value
        "![${alt}]($next)"
    }
}
