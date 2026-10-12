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
     * trigger), and nothing between it and the caret may be whitespace (a
     * finished `#sid bar` is not live any more). The query is everything
     * typed after the `#`, which is what the sheet's input opens prefilled
     * with.
     */
    fun typeaheadPrefill(text: String, caret: Int): TypeaheadPrefill? {
        val end = caret.coerceIn(0, text.length)
        var i = end - 1
        while (i >= 0) {
            val c = text[i]
            if (c == '#') {
                val before = text.getOrNull(i - 1)
                if (before != null && !before.isWhitespace() && before != '(') return null
                return TypeaheadPrefill(start = i, query = text.substring(i + 1, end))
            }
            if (c.isWhitespace()) return null
            i--
        }
        return null
    }

    /**
     * A pick: the `#token` that starts at [start] (the `#` plus every
     * non-whitespace character after it, as the editor holds it NOW) becomes
     * `#IDENT `, and the caret lands after the space. Null when [start] no
     * longer holds a `#` (the text changed under the sheet).
     */
    fun spliceTypeaheadPick(text: String, start: Int, identifier: String): Pair<String, Int>? {
        if (start !in text.indices || text[start] != '#') return null
        var end = start + 1
        while (end < text.length && !text[end].isWhitespace()) end++
        // A space the text already has is reused rather than doubled.
        val after = text.substring(end).removePrefix(" ")
        val inserted = "#$identifier "
        return Pair(text.substring(0, start) + inserted + after, start + inserted.length)
    }

    /**
     * Backspace in the sheet's EMPTY input closes the list and deletes the
     * `#` (with whatever token followed it in the editor).
     */
    fun deleteTypeaheadToken(text: String, start: Int): Pair<String, Int>? {
        if (start !in text.indices || text[start] != '#') return null
        var end = start + 1
        while (end < text.length && !text[end].isWhitespace()) end++
        return Pair(text.substring(0, start) + text.substring(end), start)
    }
}
