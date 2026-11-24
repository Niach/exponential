import Foundation

/// EXP-892: the ONE issue-search engine every client runs. The Search tab, the
/// `#` autocomplete, the duplicate/relation pickers and the composer's issue
/// picker all rank the locally synced rows with `IssueSearch.rank` and then
/// splice the server's full-text hits (`issues.search`: title + description +
/// comment bodies, stemmed) in behind them with `IssueSearch.mergeServerHits`.
/// Hand-mirrored from the web `apps/web/src/lib/issue-search.ts` (and Android
/// `IssueSearch.kt`, desktop `domain::issue_search`), byte-locked by
/// `packages/domain-contract/fixtures/issue-search.json` — same cases, same
/// test names, every platform.
///
/// Ranking, per query token (every token must match SOMEWHERE — and semantics;
/// a row's score is the sum of its tokens' best field score):
///
///   identifier exact (whole identifier or its number)       100
///   identifier prefix (whole identifier or its number)       80
///   identifier substring                                     60
///   title word prefix                                        50
///   title substring                                          40
///   description word prefix                                  20
///   description substring                                    15
///   creator name word prefix (the member or the agent)       10
///
/// #NEW-21: a `by:<name>` token is a CREATOR filter, not a scored word: the
/// row stays only when its member's or agent's name has a word starting with
/// <name>, or the member's email starts with it (`by:lukas`, `by:lukas.m`,
/// `by:@triage`: one leading `@` dropped; an empty `by:` is ignored). A row
/// with no creator never matches one. A query of `by:` tokens alone lists
/// the matching rows in the empty-query order below.
///
/// #NEW-132: four more FILTER tokens (never scored; every filter must hold;
/// an empty or unparseable value is ignored like an empty `by:`). They are
/// the structured syntax natural language translates into (#NEW-131):
///
///   is:open | is:closed   the row is undone / finished (`isOpen`)
///   status:<word>         a word of the status ANCHOR (`in_review` = in,
///                         review) or of the team status NAME (`statusName`)
///                         starts with <word>; a row with neither never matches
///   created:<range>       the row's createdAt / updatedAt lies in <range>;
///   updated:<range>       a row without that timestamp never matches
///
/// A <range> is `<n>d|w|m|y` (the last n×1/7/30/365 days before `now`,
/// n = 1…9999), a UTC day `YYYY-MM-DD` or month `YYYY-MM`, a comparison
/// against one (`>D` after it ends, `>=D` from its start, `<D` before its
/// start, `<=D` until its end), or `D..D` (the first's start to the second's
/// end). Impossible dates (`2026-13`, `2026-02-30`) are ignored. A query of
/// filters alone lists the matching rows in the empty-query order.
///
/// EXP-922: the ordering is THREE keys above the score, in this order:
///
///   1. an EXACT identifier hit (some token scored 100) — typing `EXP-42`
///      finds EXP-42 first whatever state it is in;
///   2. UNDONE before done (`closedStatuses`: the `done`/`cancelled`/
///      `duplicate` anchors every custom status dual-writes; an absent or
///      unknown status counts as open);
///   3. the score.
///
/// Ties then order by `updatedAt` desc, then `createdAt` desc, then identifier
/// number desc, then the identifier string desc. An EMPTY query lists open
/// work first, newest CREATED first inside each half (the `#` menu's "recent
/// work" list). Queries and tokens drop one leading `#` so `#87` and
/// `fix #87` both find EXP-87.
public enum IssueSearch {
    /// The `limit` every consumer gets without asking — and, since EXP-922,
    /// the one every SEARCH SURFACE uses on all four clients, so the same
    /// query returns the same rows whichever one you type it into.
    public static let defaultLimit = 30

    /// EXP-922: the builtin status anchors that mean "this issue is finished"
    /// — the `completed`/`cancelled`/`duplicate` categories' anchors, so a
    /// team's CUSTOM statuses sort correctly too (they dual-write one of these
    /// into `issues.status`). Web `ISSUE_SEARCH_CLOSED_STATUSES`.
    public static let closedStatuses: Set<String> = ["done", "cancelled", "duplicate"]

    /// Whether a row counts as UNDONE for ranking. An absent or unrecognized
    /// status is open: a server hit that never synced carries none, and a
    /// client that meets a status it does not know must not bury the row.
    public static func isOpen(_ status: String?) -> Bool {
        guard let status else { return true }
        return !closedStatuses.contains(status)
    }

    private static let scoreIdentifierExact = 100
    private static let scoreIdentifierPrefix = 80
    private static let scoreIdentifierContains = 60
    private static let scoreTitleWordPrefix = 50
    private static let scoreTitleContains = 40
    private static let scoreDescriptionWordPrefix = 20
    private static let scoreDescriptionContains = 15
    private static let scoreCreatorWordPrefix = 10

    /// #NEW-21: the token prefix that turns a word into a creator filter.
    public static let creatorPrefix = "by:"

    /// #NEW-132: the filter-token prefixes.
    public static let statePrefix = "is:"
    public static let statusPrefix = "status:"
    public static let createdPrefix = "created:"
    public static let updatedPrefix = "updated:"

    /// The searchable projection of an issue — what the engine reads, whatever
    /// the caller's row type is (a GRDB entity, a picker option, a server hit).
    /// Timestamps stay WIRE STRINGS: the engine parses them itself, once per
    /// ranked row.
    public struct Row: Sendable, Equatable {
        public let id: String
        public let identifier: String
        public let title: String
        public let description: String?
        public let createdAt: String?
        public let updatedAt: String?
        /// EXP-922: the builtin status ANCHOR (`issues.status`) the row
        /// dual-writes. Absent/unknown = treated as open.
        public let status: String?
        /// #NEW-21: the member who filed it (`issues.creator_id`'s name +
        /// email).
        public let creatorName: String?
        public let creatorEmail: String?
        /// #NEW-21: the agent whose session filed it
        /// (`issues.creator_agent_id`).
        public let creatorAgentName: String?
        /// #NEW-132: the team status row's NAME (`issues.status_id`), for
        /// `status:`.
        public let statusName: String?

        public init(
            id: String,
            identifier: String,
            title: String,
            description: String? = nil,
            createdAt: String? = nil,
            updatedAt: String? = nil,
            status: String? = nil,
            creatorName: String? = nil,
            creatorEmail: String? = nil,
            creatorAgentName: String? = nil,
            statusName: String? = nil
        ) {
            self.id = id
            self.identifier = identifier
            self.title = title
            self.description = description
            self.createdAt = createdAt
            self.updatedAt = updatedAt
            self.status = status
            self.creatorName = creatorName
            self.creatorEmail = creatorEmail
            self.creatorAgentName = creatorAgentName
            self.statusName = statusName
        }
    }

    // MARK: - Query normalization

    /// Trim, drop ONE leading `#`, lowercase. Empty = "recent work".
    public static func normalizeQuery(_ query: String) -> String {
        var q = query.trimmingCharacters(in: .whitespacesAndNewlines)
        if q.hasPrefix("#") {
            q = String(q.dropFirst()).trimmingCharacters(in: .whitespacesAndNewlines)
        }
        return q.lowercased()
    }

    /// Whitespace-separated tokens of the normalized query, each shorn of a
    /// leading `#`, empties dropped.
    public static func tokens(_ query: String) -> [String] {
        normalizeQuery(query)
            .split(whereSeparator: { $0.isWhitespace })
            .map { token -> String in
                token.hasPrefix("#") ? String(token.dropFirst()) : String(token)
            }
            .filter { !$0.isEmpty }
    }

    /// #NEW-132: a half-open time window `[from, to)` in seconds since the
    /// epoch (±infinity = unbounded). Web `IssueSearchRange` (epoch ms).
    public struct DateRange: Sendable, Equatable {
        public let from: Double
        public let to: Double

        public init(from: Double, to: Double) {
            self.from = from
            self.to = to
        }
    }

    /// #NEW-21 + #NEW-132: a query split into its scored words and its
    /// filters.
    public struct Query: Sendable, Equatable {
        public var words: [String] = []
        /// `by:` needles (prefix and one leading `@` dropped).
        public var creators: [String] = []
        /// `is:` values: true = open, false = closed.
        public var states: [Bool] = []
        /// `status:` needles.
        public var statuses: [String] = []
        public var created: [DateRange] = []
        public var updated: [DateRange] = []

        public init() {}

        /// Whether the query filters at all (a filter-only query lists the
        /// matching rows in the empty-query order).
        public var hasFilters: Bool {
            !creators.isEmpty || !states.isEmpty || !statuses.isEmpty
                || !created.isEmpty || !updated.isEmpty
        }
    }

    /// #NEW-21 + #NEW-132: the query split into its scored words and its
    /// filter tokens. Every prefixed token leaves the words, valid or not.
    public static func parseQuery(_ query: String, now: Date = Date()) -> Query {
        let nowSeconds = now.timeIntervalSince1970
        var parsed = Query()
        for token in tokens(query) {
            if token.hasPrefix(creatorPrefix) {
                var needle = String(token.dropFirst(creatorPrefix.count))
                if needle.hasPrefix("@") { needle = String(needle.dropFirst()) }
                if !needle.isEmpty { parsed.creators.append(needle) }
            } else if token.hasPrefix(statePrefix) {
                let value = String(token.dropFirst(statePrefix.count))
                if value == "open" {
                    parsed.states.append(true)
                } else if value == "closed" {
                    parsed.states.append(false)
                }
            } else if token.hasPrefix(statusPrefix) {
                let needle = String(token.dropFirst(statusPrefix.count))
                if !needle.isEmpty { parsed.statuses.append(needle) }
            } else if token.hasPrefix(createdPrefix) {
                if let range = parseRange(String(token.dropFirst(createdPrefix.count)), seconds: nowSeconds) {
                    parsed.created.append(range)
                }
            } else if token.hasPrefix(updatedPrefix) {
                if let range = parseRange(String(token.dropFirst(updatedPrefix.count)), seconds: nowSeconds) {
                    parsed.updated.append(range)
                }
            } else {
                parsed.words.append(token)
            }
        }
        return parsed
    }

    private static let daySeconds: Double = 86_400

    /// #NEW-132: a `created:`/`updated:` value as its window, or nil.
    public static func parseRange(_ value: String, now: Date) -> DateRange? {
        parseRange(value, seconds: now.timeIntervalSince1970)
    }

    private static func parseRange(_ value: String, seconds now: Double) -> DateRange? {
        let bytes = Array(value.utf8)
        // `<n>d|w|m|y`, n = 1…4 ASCII digits.
        if (2...5).contains(bytes.count), let unit = bytes.last,
           let count = asciiNumber(bytes[0..<(bytes.count - 1)]) {
            let days: Int
            switch unit {
            case UInt8(ascii: "d"): days = 1
            case UInt8(ascii: "w"): days = 7
            case UInt8(ascii: "m"): days = 30
            case UInt8(ascii: "y"): days = 365
            default: return parseAbsoluteRange(value)
            }
            guard count >= 1 else { return nil }
            return DateRange(from: now - Double(count * days) * daySeconds, to: .infinity)
        }
        return parseAbsoluteRange(value)
    }

    /// The comparison, `D..D` and plain-date forms of a range value.
    private static func parseAbsoluteRange(_ value: String) -> DateRange? {
        let comparators: [(String, (DateRange) -> DateRange)] = [
            (">=", { DateRange(from: $0.from, to: .infinity) }),
            ("<=", { DateRange(from: -.infinity, to: $0.to) }),
            (">", { DateRange(from: $0.to, to: .infinity) }),
            ("<", { DateRange(from: -.infinity, to: $0.from) }),
        ]
        for (op, window) in comparators where value.hasPrefix(op) {
            return parseDate(String(value.dropFirst(op.count))).map(window)
        }
        if let dots = value.range(of: "..") {
            guard let start = parseDate(String(value[..<dots.lowerBound])),
                  let end = parseDate(String(value[dots.upperBound...]))
            else { return nil }
            return DateRange(from: start.from, to: end.to)
        }
        return parseDate(value)
    }

    /// A UTC day `YYYY-MM-DD` or month `YYYY-MM` as its window, or nil.
    private static func parseDate(_ text: String) -> DateRange? {
        let bytes = Array(text.utf8)
        guard bytes.count == 7 || bytes.count == 10, bytes[4] == UInt8(ascii: "-") else { return nil }
        if bytes.count == 10, bytes[7] != UInt8(ascii: "-") { return nil }
        guard let year = asciiNumber(bytes[0..<4]), let month = asciiNumber(bytes[5..<7]) else { return nil }
        guard (1...12).contains(month) else { return nil }
        if bytes.count == 7 {
            let from = Double(daysFromCivil(year: year, month: month, day: 1)) * daySeconds
            return DateRange(from: from, to: from + Double(daysIn(month: month, year: year)) * daySeconds)
        }
        guard let day = asciiNumber(bytes[8..<10]), day >= 1, day <= daysIn(month: month, year: year) else {
            return nil
        }
        let from = Double(daysFromCivil(year: year, month: month, day: day)) * daySeconds
        return DateRange(from: from, to: from + daySeconds)
    }

    /// The value of a non-empty run of ASCII digits, or nil.
    private static func asciiNumber(_ digits: ArraySlice<UInt8>) -> Int? {
        guard !digits.isEmpty else { return nil }
        var out = 0
        for c in digits {
            guard c >= 48, c <= 57 else { return nil }
            out = out * 10 + Int(c - 48)
        }
        return out
    }

    /// Days since 1970-01-01 of a proleptic Gregorian UTC date (Howard
    /// Hinnant's days_from_civil), the same integer arithmetic ×4.
    private static func daysFromCivil(year: Int, month: Int, day: Int) -> Int {
        let y = month <= 2 ? year - 1 : year
        let era = (y >= 0 ? y : y - 399) / 400
        let yoe = y - era * 400
        let mp = (month + 9) % 12
        let doy = (153 * mp + 2) / 5 + day - 1
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
        return era * 146_097 + doe - 719_468
    }

    private static func daysIn(month: Int, year: Int) -> Int {
        switch month {
        case 2: return (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 ? 29 : 28
        case 4, 6, 9, 11: return 30
        default: return 31
        }
    }

    /// The numeric tail as a number for ordering (`EXP-87` → 87), -1 when none.
    public static func identifierNumber(_ identifier: String) -> Int {
        guard let tail = numericTail(identifier), let value = Int(tail) else { return -1 }
        return value
    }

    // MARK: - Scoring

    /// The row's total score for `tokens`, or nil when any token misses.
    public static func score(_ row: Row, tokens: [String]) -> Int? {
        score(prepare(row), tokens: tokens)?.total
    }

    // MARK: - Ranking

    /// Rank the locally synced `rows` for `query`: the scored, ordered, capped
    /// list described at the top of this file. Stable for equal keys.
    ///
    /// [projection] reads the searchable fields off the caller's row type, so
    /// no consumer has to convert its list before ranking it.
    public static func rank<T>(
        _ rows: [T],
        query: String,
        limit: Int = defaultLimit,
        exclude: Set<String> = [],
        now: Date = Date(),
        projection: (T) -> Row
    ) -> [T] {
        var entries: [Entry<T>] = []
        entries.reserveCapacity(rows.count)
        for (index, value) in rows.enumerated() {
            let row = projection(value)
            if exclude.contains(row.id) { continue }
            entries.append(Entry(value: value, index: index, row: row, eager: false))
        }
        return rank(entries: entries, query: query, limit: limit, now: now)
    }

    /// The ranking over decorated entries — `rank` and `Index.rank` both end
    /// here, so the two can never order differently.
    fileprivate static func rank<T>(entries: [Entry<T>], query: String, limit: Int, now: Date) -> [T] {
        let parsed = parseQuery(query, now: now)
        let queryTokens = parsed.words
        guard limit > 0 else { return [] }
        let entries = parsed.hasFilters
            ? entries.filter { matchesFilters($0, parsed) }
            : entries
        if queryTokens.isEmpty {
            let sorted = entries.sorted { a, b in
                if a.open != b.open { return a.open }
                return compareCreated(a, b) < 0
            }
            return sorted.prefix(limit).map(\.value)
        }
        var scored: [Entry<T>] = []
        scored.reserveCapacity(entries.count)
        for var entry in entries {
            guard let score = score(entry.prepared, tokens: queryTokens) else { continue }
            entry.score = score.total
            entry.exact = score.exact
            scored.append(entry)
        }
        let sorted = scored.sorted { a, b in
            if a.exact != b.exact { return a.exact }
            if a.open != b.open { return a.open }
            if a.score != b.score { return a.score > b.score }
            return compareRecency(a, b) < 0
        }
        return sorted.prefix(limit).map(\.value)
    }

    /// `rank` over plain `Row`s — what the contract fixture replays.
    public static func rank(
        _ rows: [Row],
        query: String,
        limit: Int = defaultLimit,
        exclude: Set<String> = [],
        now: Date = Date()
    ) -> [Row] {
        rank(rows, query: query, limit: limit, exclude: exclude, now: now, projection: { $0 })
    }

    // MARK: - Prepared pool

    /// #EXP-1382: a pool PREPARED once per snapshot of the rows — lowercased
    /// fields, word splits (#NEW-132: the status words too) and the two
    /// parsed timestamps — so a keystroke
    /// only scores and sorts, never re-parses. Immutable once built, so a
    /// rank may run off the main actor (a debounced task). `reusing` keeps
    /// the previous snapshot's entry for every row whose projection did not
    /// change, so a sync that touches one issue prepares one row.
    public final class Index<T: Sendable>: @unchecked Sendable {
        fileprivate let entries: [Entry<T>]
        private let positions: [String: Int]

        public init(_ rows: [T], reusing previous: Index<T>? = nil, projection: (T) -> Row) {
            var entries: [Entry<T>] = []
            entries.reserveCapacity(rows.count)
            var positions: [String: Int] = [:]
            positions.reserveCapacity(rows.count)
            for (index, value) in rows.enumerated() {
                let row = projection(value)
                if let previous, let at = previous.positions[row.id], previous.entries[at].row == row {
                    entries.append(previous.entries[at].moved(value: value, index: index))
                } else {
                    entries.append(Entry(value: value, index: index, row: row, eager: true))
                }
                if positions[row.id] == nil { positions[row.id] = index }
            }
            self.entries = entries
            self.positions = positions
        }

        /// How many rows the pool holds.
        public var count: Int { entries.count }

        /// The pooled row with this id (the first, should ids repeat).
        public func value(id: String) -> T? {
            positions[id].map { entries[$0].value }
        }

        /// `IssueSearch.rank` over the prepared pool — the same order.
        public func rank(
            query: String,
            limit: Int = IssueSearch.defaultLimit,
            exclude: Set<String> = [],
            now: Date = Date()
        ) -> [T] {
            let pool = exclude.isEmpty ? entries : entries.filter { !exclude.contains($0.row.id) }
            return IssueSearch.rank(entries: pool, query: query, limit: limit, now: now)
        }
    }

    // MARK: - Server merge

    /// Splice the server's full-text hits in behind the locally ranked rows:
    /// local order first, then every hit not already listed, in the server's
    /// relevance order, deduped by id. `resolve` turns a hit into a renderable
    /// row — the synced row when the id is local, a stand-in built from the
    /// hit's own fields where the consumer can render one, or nil to drop it (a
    /// picker whose pool is a subset of the team's issues never widens). The
    /// `limit` spans both halves.
    public static func mergeServerHits<T, H>(
        local: [T],
        hits: [H],
        limit: Int = defaultLimit,
        exclude: Set<String> = [],
        localId: (T) -> String,
        hitId: (H) -> String,
        resolve: (H) -> T?
    ) -> [T] {
        guard limit > 0 else { return [] }
        var seen = Set<String>()
        var merged: [T] = []
        for row in local {
            let id = localId(row)
            if exclude.contains(id) || seen.contains(id) { continue }
            seen.insert(id)
            merged.append(row)
            if merged.count >= limit { return merged }
        }
        for hit in hits {
            let id = hitId(hit)
            if exclude.contains(id) || seen.contains(id) { continue }
            guard let row = resolve(hit) else { continue }
            seen.insert(id)
            merged.append(row)
            if merged.count >= limit { break }
        }
        return merged
    }

    // MARK: - Internals

    /// A row decorated ONCE with everything the scorer and the comparators
    /// need: the lowercased fields, the parsed instants and the input index
    /// that keeps `sorted(by:)` — which Swift does not promise is stable —
    /// stable for fully equal keys, as every other client's sort is.
    fileprivate struct Entry<T> {
        let value: T
        let index: Int
        let row: Row
        let prepared: Prepared
        let created: Double
        let updated: Double
        let identifier: String
        let number: Int
        /// EXP-922: undone rows rank above done ones.
        let open: Bool
        var score = 0
        var exact = false

        /// `eager` lowercases and splits the description now (a pooled
        /// entry, read from several ranks and chats) instead of on first
        /// use (a one-shot rank).
        init(value: T, index: Int, row: Row, eager: Bool) {
            self.value = value
            self.index = index
            self.row = row
            self.prepared = Prepared(row: row, eager: eager)
            self.created = IssueSearch.instant(row.createdAt)
            self.updated = IssueSearch.instant(row.updatedAt)
            self.identifier = row.identifier
            self.number = IssueSearch.identifierNumber(row.identifier)
            self.open = IssueSearch.isOpen(row.status)
        }

        private init(_ other: Entry<T>, value: T, index: Int) {
            self.value = value
            self.index = index
            self.row = other.row
            self.prepared = other.prepared
            self.created = other.created
            self.updated = other.updated
            self.identifier = other.identifier
            self.number = other.number
            self.open = other.open
        }

        /// The same prepared row carried into a newer snapshot.
        func moved(value: T, index: Int) -> Entry<T> {
            Entry(self, value: value, index: index)
        }
    }

    /// One row's lowercased fields. The DESCRIPTION is lowercased and
    /// word-split LAZILY (the Android mirror's rule): a token that already
    /// matched the identifier or the title never touches it, and a description
    /// is the only unbounded field a ranked pool carries — every keystroke
    /// prepares the whole pool. A reference type so the cache survives being
    /// held in an `Entry`.
    fileprivate final class Prepared {
        let identifier: String
        let number: String?
        let title: String
        let titleWords: [String]
        private let rawDescription: String
        lazy var description: String = rawDescription.lowercased()
        lazy var descriptionWords: [String] = IssueSearch.words(description)
        /// #NEW-21: the member's and the agent's name words, and the email.
        let creatorWords: [String]
        let creatorEmail: String
        /// #NEW-132: the status anchor's and the status name's words.
        let statusWords: [String]

        init(row: Row, eager: Bool = false) {
            self.identifier = row.identifier.lowercased()
            self.number = IssueSearch.numericTail(row.identifier)
            let title = row.title.lowercased()
            self.title = title
            self.titleWords = IssueSearch.words(title)
            self.rawDescription = row.description ?? ""
            self.creatorWords = IssueSearch.words((row.creatorName ?? "").lowercased())
                + IssueSearch.words((row.creatorAgentName ?? "").lowercased())
            self.creatorEmail = (row.creatorEmail ?? "").lowercased()
            self.statusWords = IssueSearch.words((row.status ?? "").lowercased())
                + IssueSearch.words((row.statusName ?? "").lowercased())
            if eager {
                // Assigned once here, the lazy storage is never written again:
                // concurrent ranks only read it.
                let description = rawDescription.lowercased()
                self.description = description
                self.descriptionWords = IssueSearch.words(description)
            }
        }
    }

    fileprivate static func prepare(_ row: Row) -> Prepared {
        Prepared(row: row)
    }

    /// `\p{L}`/`\p{N}` runs — the web's `[^\p{L}\p{N}]+` split, spelled with
    /// Swift's own Unicode general-category predicates.
    private static func words(_ text: String) -> [String] {
        text.split(whereSeparator: { !$0.isLetter && !$0.isNumber }).map(String.init)
    }

    private static func isDigits(_ token: String) -> Bool {
        !token.isEmpty && token.allSatisfy { $0.isASCII && $0.isNumber }
    }

    /// The digits after the identifier's last `-` (`EXP-87` → `87`), or nil.
    private static func numericTail(_ identifier: String) -> String? {
        guard let dash = identifier.lastIndex(of: "-") else { return nil }
        let tail = String(identifier[identifier.index(after: dash)...])
        return isDigits(tail) ? tail : nil
    }

    /// The best field score of `token` against a prepared row, or nil when the
    /// token matches nothing.
    private static func tokenScore(_ row: Prepared, _ token: String) -> Int? {
        if row.identifier == token || (row.number != nil && row.number == token) {
            return scoreIdentifierExact
        }
        if row.identifier.hasPrefix(token)
            || (isDigits(token) && (row.number?.hasPrefix(token) ?? false)) {
            return scoreIdentifierPrefix
        }
        if row.identifier.contains(token) { return scoreIdentifierContains }
        if row.titleWords.contains(where: { $0.hasPrefix(token) }) { return scoreTitleWordPrefix }
        if row.title.contains(token) { return scoreTitleContains }
        if row.descriptionWords.contains(where: { $0.hasPrefix(token) }) {
            return scoreDescriptionWordPrefix
        }
        if row.description.contains(token) { return scoreDescriptionContains }
        if row.creatorWords.contains(where: { $0.hasPrefix(token) }) { return scoreCreatorWordPrefix }
        return nil
    }

    /// #NEW-21: whether a prepared row was filed by every `by:` needle.
    private static func matchesCreators(_ row: Prepared, _ creators: [String]) -> Bool {
        creators.allSatisfy { needle in
            row.creatorWords.contains(where: { $0.hasPrefix(needle) })
                || (!row.creatorEmail.isEmpty && row.creatorEmail.hasPrefix(needle))
        }
    }

    private static func inRanges(_ instant: Double, _ ranges: [DateRange]) -> Bool {
        ranges.allSatisfy { instant.isFinite && instant >= $0.from && instant < $0.to }
    }

    /// #NEW-21 + #NEW-132: whether an entry passes every filter token.
    private static func matchesFilters<T>(_ entry: Entry<T>, _ parsed: Query) -> Bool {
        guard matchesCreators(entry.prepared, parsed.creators) else { return false }
        guard parsed.states.allSatisfy({ $0 == entry.open }) else { return false }
        guard parsed.statuses.allSatisfy({ needle in
            entry.prepared.statusWords.contains(where: { $0.hasPrefix(needle) })
        }) else { return false }
        return inRanges(entry.created, parsed.created) && inRanges(entry.updated, parsed.updated)
    }

    /// A matched row's two ranking facts: the summed score, and whether any
    /// token hit the identifier EXACTLY (the pin that keeps a typed `EXP-42`
    /// on top).
    private struct Scored {
        let total: Int
        let exact: Bool
    }

    private static func score(_ row: Prepared, tokens: [String]) -> Scored? {
        var total = 0
        var exact = false
        for token in tokens {
            guard let score = tokenScore(row, token) else { return nil }
            if score == scoreIdentifierExact { exact = true }
            total += score
        }
        return Scored(total: total, exact: exact)
    }

    /// A wire timestamp as a comparable instant; missing or unparseable sorts
    /// OLDEST. `WireTimestamps` is the parser on purpose: synced rows arrive in
    /// Electric's Postgres text form as well as ISO-8601 (with and without
    /// fractional seconds), and a form this engine could not read would collapse
    /// every recency tie-break onto the identifier.
    private static func instant(_ value: String?) -> Double {
        guard let value, let date = WireTimestamps.parse(value) else { return -.infinity }
        return date.timeIntervalSince1970
    }

    /// Negative when `a` sorts first. Mirrors the web comparator exactly.
    private static func compareRecency<T>(_ a: Entry<T>, _ b: Entry<T>) -> Int {
        if a.updated != b.updated { return a.updated > b.updated ? -1 : 1 }
        return compareCreated(a, b)
    }

    private static func compareCreated<T>(_ a: Entry<T>, _ b: Entry<T>) -> Int {
        if a.created != b.created { return a.created > b.created ? -1 : 1 }
        if a.number != b.number { return a.number > b.number ? -1 : 1 }
        if a.identifier != b.identifier { return a.identifier > b.identifier ? -1 : 1 }
        // Fully equal keys keep the input order (JS `Array.sort` is stable).
        return a.index < b.index ? -1 : (a.index > b.index ? 1 : 0)
    }
}

// MARK: - Local adapters

extension IssueEntity {
    /// The searchable projection of a synced issue row (EXP-892). Identifier is
    /// nullable in the store (a row can sync before its trigger-assigned
    /// identifier does), and an empty one simply scores nothing.
    public var searchRow: IssueSearch.Row {
        searchRow(creatorName: nil, creatorEmail: nil, creatorAgentName: nil)
    }

    /// #NEW-21: the projection with its creator resolved (the member's name +
    /// email by `creatorId`, the agent's name by `creatorAgentId`), so `by:`
    /// and bare creator-name words match it. #NEW-132: `statusName` = the
    /// team status row's name (by `statusId`), for `status:`.
    public func searchRow(
        creatorName: String?,
        creatorEmail: String?,
        creatorAgentName: String?,
        statusName: String? = nil
    ) -> IssueSearch.Row {
        IssueSearch.Row(
            id: id,
            identifier: identifier ?? "",
            title: title,
            description: description,
            createdAt: createdAt,
            updatedAt: updatedAt,
            status: status,
            creatorName: creatorName,
            creatorEmail: creatorEmail,
            creatorAgentName: creatorAgentName,
            statusName: statusName
        )
    }
}
