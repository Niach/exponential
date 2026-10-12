package com.exponential.app.domain

// SLOP-28 (EXP-1259 D4/D5): THE search list x4, the pure half — the `#`
// typeahead's trigger + prefill rule and the splice a pick performs, plus the
// list's geometry. Hand-copied from
// `packages/domain-contract/fixtures/search-list.json` and locked to it by
// SearchListTest (which replays `typeaheadPrefill`). The Compose half is
// `ui/components/SearchList.kt` (the input row + the issue search sheet) over
// the shared Picker rows.

/** A live `#` token: where its `#` sits and the text typed after it. */
data class TypeaheadPrefill(val start: Int, val query: String)

object SearchList {
    const val INPUT_HEIGHT = 40
    const val INPUT_ICON = "nav-search"
    const val ROW_HEIGHT = 32
    const val ROW_HEIGHT_PHONE = 44
    const val MAX_VISIBLE_ROWS = 8
    const val FADE_HEIGHT = 24
    const val POPOVER_WIDTH = 560

    const val LIST_TEST_ID = "search-list"
    const val INPUT_TEST_ID = "search-list-input"
    const val ROW_TEST_ID = "search-list-row"
    const val COUNT_TEST_ID = "search-list-count"

    /**
     * The `#` token ending at [caret], or null when there is none: the `#`
     * must open the text or follow whitespace or `(` (so `C#sharp` is not a
     * trigger), and only token characters (`[A-Za-z0-9-]`) may sit between
     * it and the caret (whitespace or punctuation ends it). A `#` at the
     * START of a line waits for a token character (`#E` opens with `E`), so
     * `# Title` and `## H` stay headings. The query is everything typed after
     * the `#`, which is what the sheet's input opens prefilled with.
     */
    fun typeaheadPrefill(text: String, caret: Int): TypeaheadPrefill? {
        val end = caret.coerceIn(0, text.length)
        var i = end - 1
        while (i >= 0) {
            val c = text[i]
            if (c == '#') {
                val before = text.getOrNull(i - 1)
                val lineStart = before == null || before == '\n' || before == '\r'
                if (!lineStart && !before!!.isWhitespace() && before != '(') return null
                if (lineStart && i + 1 == end) return null
                return TypeaheadPrefill(start = i, query = text.substring(i + 1, end))
            }
            if (!isTokenChar(c)) return null
            i--
        }
        return null
    }

    private fun isTokenChar(c: Char): Boolean =
        c in 'A'..'Z' || c in 'a'..'z' || c in '0'..'9' || c == '-'

    /**
     * A pick: exactly `[start, start + 1 + queryLength)` (the `#` and the
     * token as recorded when the sheet opened, never text after the caret)
     * becomes `#IDENT `, and the caret lands after the space. Null when
     * [start] no longer holds a `#` (the text changed under the sheet).
     */
    fun spliceTypeaheadPick(text: String, start: Int, queryLength: Int, identifier: String): Pair<String, Int>? {
        val end = recordedEnd(text, start, queryLength) ?: return null
        // A space the text already has is reused rather than doubled.
        val after = text.substring(end).removePrefix(" ")
        val inserted = "#$identifier "
        return Pair(text.substring(0, start) + inserted + after, start + inserted.length)
    }

    /**
     * Backspace in the sheet's EMPTY input closes the list and deletes the
     * `#` and the query typed so far (`1 + queryLength` from the recorded
     * start).
     */
    fun deleteTypeaheadToken(text: String, start: Int, queryLength: Int): Pair<String, Int>? {
        val end = recordedEnd(text, start, queryLength) ?: return null
        return Pair(text.substring(0, start) + text.substring(end), start)
    }

    private fun recordedEnd(text: String, start: Int, queryLength: Int): Int? {
        if (start !in text.indices || text[start] != '#') return null
        return (start + 1 + queryLength.coerceAtLeast(0)).coerceAtMost(text.length)
    }
}
