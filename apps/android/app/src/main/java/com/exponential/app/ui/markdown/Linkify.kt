package com.exponential.app.ui.markdown

/**
 * Bare-URL tokenizer for render-only markdown (EXP-430, EXP-440).
 *
 * The remote `/login` flow publishes the claude sign-in URL as plain narration,
 * so the agent feed has to make bare URLs tappable. A deliberately tiny
 * tokenizer, not a markdown pass — hand-mirrored with the web
 * (`lib/linkify.ts`) and iOS implementations. [MarkdownView]'s autolink pass is
 * its only caller; the editor never sees it, because rewriting `https://x` to
 * `[https://x](https://x)` on the next save would diverge the stored bytes from
 * web (see [MarkdownParser]'s note on the autolink extension).
 */

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

/** One bare URL's half-open `[start, end)` range in the source string. */
internal data class UrlRange(val start: Int, val end: Int, val url: String)

/**
 * Every bare `http(s)://` URL in [text], trailing prose punctuation trimmed.
 * `text.substring(start, end)` is always exactly [UrlRange.url], so the
 * renderer can place its link span by source offset.
 */
internal fun urlRanges(text: String): List<UrlRange> =
    URL_PATTERN.findAll(text).mapNotNull { match ->
        val url = trimTrailingPunctuation(match.value)
        if (url.isEmpty()) {
            null
        } else {
            UrlRange(match.range.first, match.range.first + url.length, url)
        }
    }.toList()
