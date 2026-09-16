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
///
/// Ties order by `updatedAt` desc, then `createdAt` desc, then identifier
/// number desc, then the identifier string desc. An EMPTY query lists the
/// newest CREATED first (the `#` menu's "recent work" list). Queries and
/// tokens drop one leading `#` so `#87` and `fix #87` both find EXP-87.
public enum IssueSearch {
    /// The `limit` every consumer gets without asking.
    public static let defaultLimit = 30

    private static let scoreIdentifierExact = 100
    private static let scoreIdentifierPrefix = 80
    private static let scoreIdentifierContains = 60
    private static let scoreTitleWordPrefix = 50
    private static let scoreTitleContains = 40
    private static let scoreDescriptionWordPrefix = 20
    private static let scoreDescriptionContains = 15

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

        public init(
            id: String,
            identifier: String,
            title: String,
            description: String? = nil,
            createdAt: String? = nil,
            updatedAt: String? = nil
        ) {
            self.id = id
            self.identifier = identifier
            self.title = title
            self.description = description
            self.createdAt = createdAt
            self.updatedAt = updatedAt
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

    /// The numeric tail as a number for ordering (`EXP-87` → 87), -1 when none.
    public static func identifierNumber(_ identifier: String) -> Int {
        guard let tail = numericTail(identifier), let value = Int(tail) else { return -1 }
        return value
    }

    // MARK: - Scoring

    /// The row's total score for `tokens`, or nil when any token misses.
    public static func score(_ row: Row, tokens: [String]) -> Int? {
        score(prepare(row), tokens: tokens)
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
        projection: (T) -> Row
    ) -> [T] {
        let queryTokens = tokens(query)
        var entries: [Entry<T>] = []
        entries.reserveCapacity(rows.count)
        for (index, value) in rows.enumerated() {
            let row = projection(value)
            if exclude.contains(row.id) { continue }
            entries.append(Entry(value: value, index: index, row: row))
        }
        guard limit > 0 else { return [] }
        if queryTokens.isEmpty {
            let sorted = entries.sorted { compareCreated($0, $1) < 0 }
            return sorted.prefix(limit).map(\.value)
        }
        var scored: [Entry<T>] = []
        scored.reserveCapacity(entries.count)
        for var entry in entries {
            guard let score = score(entry.prepared, tokens: queryTokens) else { continue }
            entry.score = score
            scored.append(entry)
        }
        let sorted = scored.sorted { a, b in
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
        exclude: Set<String> = []
    ) -> [Row] {
        rank(rows, query: query, limit: limit, exclude: exclude, projection: { $0 })
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
    private struct Entry<T> {
        let value: T
        let index: Int
        let prepared: Prepared
        let created: Double
        let updated: Double
        let identifier: String
        let number: Int
        var score = 0

        init(value: T, index: Int, row: Row) {
            self.value = value
            self.index = index
            self.prepared = IssueSearch.prepare(row)
            self.created = IssueSearch.instant(row.createdAt)
            self.updated = IssueSearch.instant(row.updatedAt)
            self.identifier = row.identifier
            self.number = IssueSearch.identifierNumber(row.identifier)
        }
    }

    private struct Prepared {
        let identifier: String
        let number: String?
        let title: String
        let titleWords: [String]
        let description: String
        let descriptionWords: [String]
    }

    private static func prepare(_ row: Row) -> Prepared {
        let title = row.title.lowercased()
        let description = (row.description ?? "").lowercased()
        return Prepared(
            identifier: row.identifier.lowercased(),
            number: numericTail(row.identifier),
            title: title,
            titleWords: words(title),
            description: description,
            descriptionWords: words(description)
        )
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
        return nil
    }

    private static func score(_ row: Prepared, tokens: [String]) -> Int? {
        var total = 0
        for token in tokens {
            guard let score = tokenScore(row, token) else { return nil }
            total += score
        }
        return total
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
        IssueSearch.Row(
            id: id,
            identifier: identifier ?? "",
            title: title,
            description: description,
            createdAt: createdAt,
            updatedAt: updatedAt
        )
    }
}
