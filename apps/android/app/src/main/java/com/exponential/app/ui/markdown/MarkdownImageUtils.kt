package com.exponential.app.ui.markdown

private val MARKDOWN_IMAGE_REGEX = Regex("""!\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)""")

/** Find every image URL in the markdown source (in document order). */
fun collectMarkdownImageUrls(markdown: String): List<String> =
    MARKDOWN_IMAGE_REGEX.findAll(markdown).map { it.groupValues[2] }.toList()

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
