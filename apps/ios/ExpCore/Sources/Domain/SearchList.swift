import Foundation

/// SLOP-28 (EXP-1259 D4/D5): THE search list — ONE component for every place
/// that searches a list and picks from it: the `#` issue typeahead in every
/// editor and composer, the composer "+" menu's Implement issue and Run
/// action pages, the issue picker and the Search tab. Constants and the
/// typeahead's prefill rule, locked against
/// `packages/domain-contract/fixtures/search-list.json` by `SearchListTests`.
/// On phones the `#` typeahead is a BOTTOM SHEET with the list's input
/// focused and prefilled with the token typed after the `#`.
public enum SearchList {
    public static let inputHeight: Double = 40
    /// The input row's glyph (concept `nav-search`).
    public static let inputIcon = "nav-search"
    public static let rowHeight: Double = 32
    public static let rowHeightPhone: Double = 44
    public static let maxVisibleRows = 8
    public static let fadeHeight: Double = 24
    public static let popoverWidth: Double = 560

    public enum TestId {
        public static let list = "search-list"
        public static let input = "search-list-input"
        public static let row = "search-list-row"
        public static let count = "search-list-count"
    }

    /// #NEW-133 (search-list.json SUGGESTION): the action row a recognised
    /// sentence puts on top of the global search: its icon concept, label
    /// format and test id.
    public enum SuggestionSpec {
        public static let icon = "nav-search"
        public static let label = "Search: {query}"
        public static let testId = "search-list-suggestion"
    }

    /// #NEW-136 (search-list.json TRANSLATE): the same row offering the model
    /// fallback (`IssueSearchTranslation`): its name, the failure toast and
    /// its test id. iOS 26+ with Apple Intelligence available only.
    public enum TranslateSpec {
        public static let label = "Translate to a search"
        public static let failed = "Couldn't translate this search"
        public static let testId = "search-list-translate"
    }

    /// The suggestion row: the query a recognised sentence means and the
    /// row's name.
    public struct Suggestion: Equatable, Sendable {
        public let query: String
        public let label: String

        public init(query: String, label: String) {
            self.query = query
            self.label = label
        }
    }

    /// The suggestion row for the typed `text` (`suggestion.cases`), or nil
    /// when the phrase grammar does not recognise a sentence.
    public static func suggestion(
        text: String,
        members: [IssueSearchPhrase.Member],
        now: Date = Date()
    ) -> Suggestion? {
        guard let query = IssueSearchPhrase.translate(text, members: members, now: now) else { return nil }
        return Suggestion(query: query, label: SuggestionSpec.label.replacingOccurrences(of: "{query}", with: query))
    }

    /// Where an open `#` token starts and what it holds.
    public struct Prefill: Equatable, Sendable {
        /// UTF-16 offset of the `#`.
        public let start: Int
        /// The token typed after the `#`, up to the caret.
        public let query: String

        public init(start: Int, query: String) {
            self.start = start
            self.query = query
        }
    }

    /// The `#` token at `caret` (a UTF-16 offset into `text`), or nil when
    /// none is open: the `#` must start the text or follow whitespace or `(`,
    /// and only token characters (`[A-Za-z0-9-]`) may sit between it and the
    /// caret (punctuation ends it). A `#` at the START of a line waits for a
    /// token character (`#E` opens with `E`), so `# Title` and `## H` stay
    /// headings. The token is what prefills the search input.
    public static func typeaheadPrefill(text: String, caret: Int) -> Prefill? {
        let units = Array(text.utf16)
        let end = max(0, min(caret, units.count))
        var index = end - 1
        while index >= 0 {
            let unit = units[index]
            if unit == UInt16(UInt8(ascii: "#")) { break }
            if !isTokenUnit(unit) { return nil }
            index -= 1
        }
        guard index >= 0 else { return nil }
        let lineStart = index == 0 || isNewline(units[index - 1])
        if index > 0, !lineStart {
            let before = units[index - 1]
            guard isWhitespace(before) || before == UInt16(UInt8(ascii: "(")) else { return nil }
        }
        if lineStart, index + 1 == end { return nil }
        let query = String(decoding: units[(index + 1)..<end], as: UTF16.self)
        return Prefill(start: index, query: query)
    }

    /// `[A-Za-z0-9-]`: an identifier-ish token, nothing else.
    private static func isTokenUnit(_ unit: UInt16) -> Bool {
        switch unit {
        case 0x30...0x39, 0x41...0x5A, 0x61...0x7A, 0x2D: return true
        default: return false
        }
    }

    private static func isNewline(_ unit: UInt16) -> Bool {
        unit == 0x0A || unit == 0x0D || unit == 0x2028 || unit == 0x2029
    }

    private static func isWhitespace(_ unit: UInt16) -> Bool {
        guard let scalar = Unicode.Scalar(unit) else { return false }
        return CharacterSet.whitespacesAndNewlines.contains(scalar)
    }
}
