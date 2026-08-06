package com.exponential.app.ui.session

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.withLink

/**
 * URL linkification for agent-feed narration text (EXP-430).
 *
 * Narration used to render as plain text, which made the claude sign-in URL
 * the emitter publishes during a remote `/login` untappable. A deliberately
 * tiny tokenizer, not a markdown pass — hand-mirrored with the web
 * (`lib/linkify.ts`) and iOS (`Linkify.swift`) implementations.
 */

data class LinkSegment(val text: String, val href: String? = null)

private val URL_PATTERN = Regex("""https?://\S+""")

private const val TRAILING_PROSE = ".,;:!?"

/**
 * Trailing punctuation is prose, not URL — `(see https://x.dev).` must not
 * link the `).`. A BALANCED closing paren/bracket is kept, so
 * `https://x.dev/a(b)` stays whole.
 */
private fun trimTrailingPunctuation(url: String): String {
    var out = url
    while (out.isNotEmpty()) {
        val last = out.last()
        out = when {
            last in TRAILING_PROSE -> out.dropLast(1)
            last == ')' && out.count { it == '(' } < out.count { it == ')' } -> out.dropLast(1)
            last == ']' && out.count { it == '[' } < out.count { it == ']' } -> out.dropLast(1)
            else -> return out
        }
    }
    return out
}

/**
 * Split [text] into plain/link segments; concatenating the segments' text
 * always reproduces the input byte-for-byte.
 */
fun linkSegments(text: String): List<LinkSegment> {
    val segments = mutableListOf<LinkSegment>()
    var cursor = 0
    for (match in URL_PATTERN.findAll(text)) {
        val url = trimTrailingPunctuation(match.value)
        if (url.isEmpty()) continue
        if (match.range.first > cursor) {
            segments.add(LinkSegment(text.substring(cursor, match.range.first)))
        }
        segments.add(LinkSegment(url, href = url))
        cursor = match.range.first + url.length
    }
    if (cursor < text.length || segments.isEmpty()) {
        segments.add(LinkSegment(text.substring(cursor)))
    }
    return segments
}

/** Narration text with its URLs as tappable [LinkAnnotation.Url] runs. */
fun linkifyNarration(text: String, linkStyles: TextLinkStyles): AnnotatedString =
    buildAnnotatedString {
        for (segment in linkSegments(text)) {
            val href = segment.href
            if (href != null) {
                withLink(LinkAnnotation.Url(href, linkStyles)) { append(segment.text) }
            } else {
                append(segment.text)
            }
        }
    }
