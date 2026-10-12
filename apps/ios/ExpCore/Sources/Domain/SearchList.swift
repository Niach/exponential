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
    /// and nothing between it and the caret may be whitespace (or another
    /// `#`). The token is what prefills the search input.
    public static func typeaheadPrefill(text: String, caret: Int) -> Prefill? {
        let units = Array(text.utf16)
        let end = max(0, min(caret, units.count))
        var index = end - 1
        while index >= 0 {
            let unit = units[index]
            if unit == UInt16(UInt8(ascii: "#")) { break }
            if isWhitespace(unit) { return nil }
            index -= 1
        }
        guard index >= 0 else { return nil }
        if index > 0 {
            let before = units[index - 1]
            guard isWhitespace(before) || before == UInt16(UInt8(ascii: "(")) else { return nil }
        }
        let query = String(decoding: units[(index + 1)..<end], as: UTF16.self)
        return Prefill(start: index, query: query)
    }

    private static func isWhitespace(_ unit: UInt16) -> Bool {
        guard let scalar = Unicode.Scalar(unit) else { return false }
        return CharacterSet.whitespacesAndNewlines.contains(scalar)
    }
}
