package com.exponential.app.ui.markdown

import java.util.UUID

// Mirrors the web `markdownImagePattern`. The alt group consumes
// backslash-escape pairs — serializers escape markdown punctuation in alt
// (web's TipTap turns a `shot [1].png` filename into `\[1\]`), and a plain
// `[^\]]*` would drop the whole occurrence (REV-6). Group 1 stays the raw
// (still-escaped) alt so `replaceMarkdownImageUrls` round-trips it verbatim.
private val MARKDOWN_IMAGE_REGEX = Regex("""!\[((?:\\.|[^\\\]])*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)""")

// EXP-824: the link-form twin for inline media — `[clip.mp4](draft://…)`.
// The negative lookbehind keeps it off the image form so the two never
// double-match; group 1 = label, group 2 = URL, exactly like the image regex.
private val MARKDOWN_LINK_REGEX = Regex("""(?<!!)\[((?:\\.|[^\\\]])*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)""")

/** A not-yet-uploaded image placeholder URL, e.g. `draft://<uuid>`. */
fun draftUrl(): String = "draft://${UUID.randomUUID()}"

fun isDraftUrl(url: String): Boolean = url.startsWith("draft://")

/**
 * True if any image OR media-link reference in [markdown] is still an
 * unuploaded draft.
 */
fun hasDraftImages(markdown: String): Boolean =
    MARKDOWN_IMAGE_REGEX.findAll(markdown).any { isDraftUrl(it.groupValues[2]) } ||
        MARKDOWN_LINK_REGEX.findAll(markdown).any { isDraftUrl(it.groupValues[2]) }

/**
 * Drop every still-unuploaded `draft://` image AND media-link reference. Used
 * by save/send paths so an in-flight or failed upload never leaks a draft
 * placeholder into a persisted description/comment (the editor keeps the
 * row + bytes around for retry regardless). A `draft://` link is never
 * legitimate in a stored body, so the link form is stripped unconditionally
 * too (EXP-824).
 */
fun stripDraftImages(markdown: String): String {
    val images = MARKDOWN_IMAGE_REGEX.replace(markdown) { match ->
        if (isDraftUrl(match.groupValues[2])) "" else match.value
    }
    return MARKDOWN_LINK_REGEX.replace(images) { match ->
        if (isDraftUrl(match.groupValues[2])) "" else match.value
    }
}

/** The set of image URLs referenced in [markdown]. */
fun markdownImageUrls(markdown: String): Set<String> =
    MARKDOWN_IMAGE_REGEX.findAll(markdown).map { it.groupValues[2] }.toSet()

/**
 * Every embed URL in [markdown]: image references plus plain-link references
 * (the inline-media form, EXP-824). The create flow keys its pending uploads
 * on this so a video placeholder counts as referenced the same way an image
 * one does.
 */
fun markdownEmbedUrls(markdown: String): Set<String> =
    markdownImageUrls(markdown) +
        MARKDOWN_LINK_REGEX.findAll(markdown).map { it.groupValues[2] }.toSet()

/** Drop image (and media-link) references whose URL is in `urls`. */
fun removeMarkdownImagesByUrl(markdown: String, urls: Collection<String>): String {
    if (urls.isEmpty()) return markdown
    val urlSet = urls.toSet()
    val images = MARKDOWN_IMAGE_REGEX.replace(markdown) { match ->
        if (match.groupValues[2] in urlSet) "" else match.value
    }
    return MARKDOWN_LINK_REGEX.replace(images) { match ->
        if (match.groupValues[2] in urlSet) "" else match.value
    }
}

/**
 * Substitute image (and media-link) URLs found in `replacements` with the new
 * URLs; preserve alt / label text and each reference's own form.
 */
fun replaceMarkdownImageUrls(markdown: String, replacements: Map<String, String>): String {
    if (replacements.isEmpty()) return markdown
    val images = MARKDOWN_IMAGE_REGEX.replace(markdown) { match ->
        val alt = match.groupValues[1]
        val url = match.groupValues[2]
        val next = replacements[url] ?: return@replace match.value
        "![${alt}]($next)"
    }
    return MARKDOWN_LINK_REGEX.replace(images) { match ->
        val label = match.groupValues[1]
        val url = match.groupValues[2]
        val next = replacements[url] ?: return@replace match.value
        "[${label}]($next)"
    }
}
