import Foundation
import os

private let log = Logger(subsystem: "com.exponential", category: "EmojiCatalog")

// EXP-551 — the shared emoji dataset, as consumed on iOS.
//
// `packages/emoji` generates ONE compact JSON that is committed byte-identically
// into all four clients (`apps/ios/Exponential/Resources/emoji.json`, loaded from
// `Bundle.main`). The keys are deliberately short (`u`/`l`/`g`/`s`/`t`/`k`) — see
// `packages/emoji/README.md` for the shape — so the decoder below is the one
// place iOS spells them out. NEVER hand-edit the JSON; regenerate it with
// `bun run --filter @exp/emoji generate`.
//
// The semantics implemented here are shared by web, desktop and Android:
//   * pickers insert UNICODE (`u` — always the base yellow glyph, EXP-600
//     dropped skin tones; the dataset's `k` variants are deliberately unread),
//     never `:shortcode:` text — the markdown bodies are plain GFM interchange
//   * search ranks shortcode-prefix over label-prefix over tag exact/prefix over
//     label/tag substring
//   * recents keep the last 24 picked BASE unicodes, most recent first

// MARK: - Dataset

/// One pickable emoji. Mirrors `EmojiRecord` in `packages/emoji/src/index.ts`.
public struct EmojiRecord: Codable, Sendable, Hashable, Identifiable {
    /// The unicode sequence to insert (fully qualified, no skin tone).
    public let unicode: String
    /// Human label, lowercase, e.g. `grinning face`.
    public let label: String
    /// Index into `EmojiDataset.groups`.
    public let group: Int
    /// Shortcodes WITHOUT colons (`+1`, `tada`).
    public let shortcodes: [String]
    /// Search tags.
    public let tags: [String]
    /// The five uniform skin-tone variants of the dataset's `k` field —
    /// decoded for shape parity but deliberately unread (EXP-600: pickers
    /// only ever insert the base yellow glyph).
    public let tones: [String]?

    /// The base unicode identifies a record — recents are stored by it.
    public var id: String { unicode }

    private enum CodingKeys: String, CodingKey {
        case unicode = "u"
        case label = "l"
        case group = "g"
        case shortcodes = "s"
        case tags = "t"
        case tones = "k"
    }

    public init(
        unicode: String,
        label: String,
        group: Int,
        shortcodes: [String],
        tags: [String],
        tones: [String]? = nil
    ) {
        self.unicode = unicode
        self.label = label
        self.group = group
        self.shortcodes = shortcodes
        self.tags = tags
        self.tones = tones
    }

    /// Case-insensitive exact shortcode test — drives the `:code:` auto-commit.
    public func hasShortcode(_ code: String) -> Bool {
        let needle = code.lowercased()
        return shortcodes.contains { $0.lowercased() == needle }
    }
}

/// The decoded file. Mirrors `EmojiDataset` in `packages/emoji/src/index.ts`.
public struct EmojiDataset: Codable, Sendable {
    /// The emojibase-data version the file was generated from.
    public let version: String
    /// Group labels, indexed by `EmojiRecord.group`.
    public let groups: [String]
    /// In display order (emojibase `order`).
    public let emojis: [EmojiRecord]

    public init(version: String, groups: [String], emojis: [EmojiRecord]) {
        self.version = version
        self.groups = groups
        self.emojis = emojis
    }
}

// MARK: - Search index

/// A decoded dataset plus the lowercased lookup tables the search needs. Built
/// once off the main thread; every query is a linear scan over ~1.9k records,
/// which is cheap enough to run on each keystroke.
public struct EmojiCatalogIndex: Sendable {
    public let dataset: EmojiDataset

    private let lowerLabels: [String]
    private let lowerShortcodes: [[String]]
    private let lowerTags: [[String]]
    private let byShortcode: [String: Int]
    private let byUnicode: [String: Int]

    public init(dataset: EmojiDataset) {
        self.dataset = dataset
        lowerLabels = dataset.emojis.map { $0.label.lowercased() }
        lowerShortcodes = dataset.emojis.map { $0.shortcodes.map { $0.lowercased() } }
        lowerTags = dataset.emojis.map { $0.tags.map { $0.lowercased() } }
        var shortcodeMap: [String: Int] = [:]
        var unicodeMap: [String: Int] = [:]
        for (i, record) in dataset.emojis.enumerated() {
            unicodeMap[record.unicode] = i
            for code in record.shortcodes {
                let key = code.lowercased()
                // First writer wins: the dataset is in display order, so the
                // canonical emoji for a shared shortcode is the earlier one.
                if shortcodeMap[key] == nil { shortcodeMap[key] = i }
            }
        }
        byShortcode = shortcodeMap
        byUnicode = unicodeMap
    }

    public var emojis: [EmojiRecord] { dataset.emojis }
    public var groups: [String] { dataset.groups }

    /// Emoji in `group`, in dataset (display) order.
    public func emojis(inGroup group: Int) -> [EmojiRecord] {
        dataset.emojis.filter { $0.group == group }
    }

    /// Exact shortcode lookup, case-insensitive — powers the `:code:`
    /// auto-commit and the recents round-trip.
    public func find(shortcode: String) -> EmojiRecord? {
        guard let i = byShortcode[shortcode.lowercased()] else { return nil }
        return dataset.emojis[i]
    }

    /// Exact base-unicode lookup — recents are persisted as base unicodes.
    public func find(unicode: String) -> EmojiRecord? {
        guard let i = byUnicode[unicode] else { return nil }
        return dataset.emojis[i]
    }

    // Ranking bands, best first. Byte-for-byte the web's `rankEmoji`
    // (`apps/web/src/lib/emoji.ts`), which the Kotlin and Rust ports mirror
    // too: shortcode-prefix > label-prefix > tag exact/prefix > label/tag
    // substring, with an EXACT shortcode at the top of the shortcode band so
    // `:smile:` resolves to 😄 and not to 😃 (`smiley`). Tag exact and tag
    // prefix share ONE band — splitting them would reorder results against the
    // other clients.
    private enum Band: Int, CaseIterable {
        case exactShortcode = 0
        case shortcodePrefix
        case labelPrefix
        case tag
        case substring
    }

    private func band(_ i: Int, query: String) -> Band? {
        let codes = lowerShortcodes[i]
        if codes.contains(query) { return .exactShortcode }
        if codes.contains(where: { $0.hasPrefix(query) }) { return .shortcodePrefix }
        let label = lowerLabels[i]
        if label.hasPrefix(query) { return .labelPrefix }
        let tags = lowerTags[i]
        if tags.contains(where: { $0 == query || $0.hasPrefix(query) }) { return .tag }
        if label.contains(query) || tags.contains(where: { $0.contains(query) }) { return .substring }
        return nil
    }

    /// Ranked search. Ties keep dataset order, so the ranking is stable. An
    /// empty query yields nothing (the picker renders the groups instead), and
    /// surrounding colons are stripped so a pasted `:tada:` still searches.
    public func search(_ query: String, limit: Int) -> [EmojiRecord] {
        guard limit > 0 else { return [] }
        var needle = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if needle.hasPrefix(":") { needle.removeFirst() }
        if needle.hasSuffix(":") { needle.removeLast() }
        guard !needle.isEmpty else { return [] }

        var buckets = [[Int]](repeating: [], count: Band.allCases.count)
        for i in dataset.emojis.indices {
            guard let band = band(i, query: needle) else { continue }
            buckets[band.rawValue].append(i)
        }
        var out: [EmojiRecord] = []
        out.reserveCapacity(limit)
        for bucket in buckets {
            for i in bucket {
                out.append(dataset.emojis[i])
                if out.count == limit { return out }
            }
        }
        return out
    }
}

// MARK: - Loading

/// Process-wide access to the bundled dataset. Decoding ~245KB of JSON is not
/// free, so `preload()` does it off the main thread the first time an editor
/// appears; every accessor degrades to "no results" until it lands.
@MainActor
public final class EmojiCatalog {
    public static let shared = EmojiCatalog()

    /// The bundled resource name (`emoji.json`, copied by the
    /// `Exponential/Resources/**` glob into the prod + staging app targets).
    public nonisolated static let resourceName = "emoji"
    public nonisolated static let resourceExtension = "json"

    /// Result caps shared with the other clients: the picker shows at most 64
    /// search hits, the `:shortcode` typeahead at most 8.
    public nonisolated static let pickerLimit = 64
    public nonisolated static let typeaheadLimit = 8

    public private(set) var index: EmojiCatalogIndex?
    private var loading = false

    public var isLoaded: Bool { index != nil }

    private init() {}

    /// Kick off a one-shot background decode. Safe to call on every editor
    /// appearance — it no-ops once loaded or in flight.
    public func preload(bundle: Bundle = .main) {
        guard index == nil, !loading else { return }
        guard let url = Self.resourceURL(in: bundle) else {
            log.error("emoji.json missing from the bundle — the picker will stay empty")
            return
        }
        loading = true
        Task.detached(priority: .utility) {
            let decoded = try? Self.load(contentsOf: url)
            await MainActor.run { EmojiCatalog.shared.apply(decoded) }
        }
    }

    /// Decode synchronously (tests, and a picker opened before the preload
    /// finished). Returns the cached index when there is one.
    @discardableResult
    public func loadNow(bundle: Bundle = .main) -> EmojiCatalogIndex? {
        if let index { return index }
        guard let url = Self.resourceURL(in: bundle) else { return nil }
        let decoded = try? Self.load(contentsOf: url)
        apply(decoded)
        return decoded
    }

    public func search(_ query: String, limit: Int = EmojiCatalog.pickerLimit) -> [EmojiRecord] {
        index?.search(query, limit: limit) ?? []
    }

    public func find(shortcode: String) -> EmojiRecord? { index?.find(shortcode: shortcode) }
    public func find(unicode: String) -> EmojiRecord? { index?.find(unicode: unicode) }

    private func apply(_ decoded: EmojiCatalogIndex?) {
        loading = false
        guard let decoded else { return }
        index = decoded
    }

    /// `Bundle.url(forResource:)` is nonisolated so the detached decode can run
    /// without hopping back for the path.
    public nonisolated static func resourceURL(in bundle: Bundle) -> URL? {
        bundle.url(forResource: resourceName, withExtension: resourceExtension)
    }

    /// Decode + index a dataset file. Nonisolated on purpose: this is what runs
    /// off the main actor.
    public nonisolated static func load(contentsOf url: URL) throws -> EmojiCatalogIndex {
        try decode(Data(contentsOf: url))
    }

    public nonisolated static func decode(_ data: Data) throws -> EmojiCatalogIndex {
        EmojiCatalogIndex(dataset: try JSONDecoder().decode(EmojiDataset.self, from: data))
    }
}

// MARK: - Preferences

/// Recents, persisted under the key every client uses (`exp.emojiRecent`).
/// Injectable `UserDefaults` so tests can use a throwaway suite. (The EXP-551
/// `exp.emojiSkinTone` key is retired — EXP-600 dropped skin tones; pickers
/// only ever insert the base yellow glyph.)
public struct EmojiPreferences {
    public static let recentsKey = "exp.emojiRecent"
    /// Recents keep the last 24 picked BASE unicodes, most recent first.
    public static let maxRecents = 24

    private let defaults: UserDefaults

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    public var recents: [String] {
        defaults.stringArray(forKey: Self.recentsKey) ?? []
    }

    /// Record a pick. Stores the BASE unicode; dedupes and caps at 24.
    public func recordRecent(_ baseUnicode: String) {
        guard !baseUnicode.isEmpty else { return }
        var list = recents.filter { $0 != baseUnicode }
        list.insert(baseUnicode, at: 0)
        if list.count > Self.maxRecents { list = Array(list.prefix(Self.maxRecents)) }
        defaults.set(list, forKey: Self.recentsKey)
    }

    public func clearRecents() {
        defaults.removeObject(forKey: Self.recentsKey)
    }
}
