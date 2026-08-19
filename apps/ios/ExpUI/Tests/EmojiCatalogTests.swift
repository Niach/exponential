import Foundation
import XCTest
@testable import ExpUI

// EXP-551 — the shared emoji dataset contract on iOS: the generated
// `packages/emoji` file decodes with the short keys, the search ranking matches
// the semantics every client implements, and the tone/recents preferences use
// the shared UserDefaults keys.
final class EmojiCatalogTests: XCTestCase {
    // MARK: - Decoding

    private static let fixture = """
    {
      "version": "16.0.3",
      "groups": ["Smileys & emotion", "People & body"],
      "emojis": [
        {"u":"\u{1F600}","l":"grinning face","g":0,"s":["grinning"],"t":["happy","smile"]},
        {"u":"\u{1F44D}","l":"thumbs up","g":1,"s":["+1","thumbsup"],"t":["like","yes"],
         "k":["\u{1F44D}\u{1F3FB}","\u{1F44D}\u{1F3FC}","\u{1F44D}\u{1F3FD}","\u{1F44D}\u{1F3FE}","\u{1F44D}\u{1F3FF}"]}
      ]
    }
    """

    private func fixtureIndex() throws -> EmojiCatalogIndex {
        try EmojiCatalog.decode(Data(Self.fixture.utf8))
    }

    func testDecodesTheShortKeys() throws {
        let index = try fixtureIndex()
        XCTAssertEqual(index.dataset.version, "16.0.3")
        XCTAssertEqual(index.groups, ["Smileys & emotion", "People & body"])
        XCTAssertEqual(index.emojis.count, 2)

        let thumbs = index.emojis[1]
        XCTAssertEqual(thumbs.unicode, "\u{1F44D}")
        XCTAssertEqual(thumbs.label, "thumbs up")
        XCTAssertEqual(thumbs.group, 1)
        XCTAssertEqual(thumbs.shortcodes, ["+1", "thumbsup"])
        XCTAssertEqual(thumbs.tags, ["like", "yes"])
        XCTAssertEqual(thumbs.tones?.count, 5)
        XCTAssertNil(index.emojis[0].tones)
    }

    func testGroupsAndLookups() throws {
        let index = try fixtureIndex()
        XCTAssertEqual(index.emojis(inGroup: 1).map(\.unicode), ["\u{1F44D}"])
        XCTAssertEqual(index.find(shortcode: "+1")?.unicode, "\u{1F44D}")
        // Shortcode lookup is case-insensitive; unicode lookup is exact.
        XCTAssertEqual(index.find(shortcode: "THUMBSUP")?.unicode, "\u{1F44D}")
        XCTAssertEqual(index.find(unicode: "\u{1F600}")?.label, "grinning face")
        XCTAssertNil(index.find(shortcode: "nope"))
    }

    // MARK: - Tones

    func testApplyingToneUsesTheVariantOrFallsBack() throws {
        let index = try fixtureIndex()
        let thumbs = try XCTUnwrap(index.find(shortcode: "+1"))
        XCTAssertEqual(thumbs.applyingTone(0), "\u{1F44D}")
        XCTAssertEqual(thumbs.applyingTone(1), "\u{1F44D}\u{1F3FB}")
        XCTAssertEqual(thumbs.applyingTone(5), "\u{1F44D}\u{1F3FF}")
        // Out-of-range tones never index out of `k`.
        XCTAssertEqual(thumbs.applyingTone(9), "\u{1F44D}")
        XCTAssertEqual(thumbs.applyingTone(-1), "\u{1F44D}")

        // A record without `k` ignores the global preference entirely.
        let grinning = try XCTUnwrap(index.find(shortcode: "grinning"))
        XCTAssertEqual(grinning.applyingTone(3), "\u{1F600}")
    }

    // MARK: - The REAL generated dataset

    /// Loads the committed `apps/ios/Exponential/Resources/emoji.json` — the
    /// file the app bundles. Read through `#filePath` because the unit-test
    /// bundle does not carry the app target's resources.
    private func realIndex() throws -> EmojiCatalogIndex {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpUI/Tests/
            .deletingLastPathComponent()          // ExpUI/
            .deletingLastPathComponent()          // apps/ios/
            .appendingPathComponent("Exponential/Resources/emoji.json")
        return try EmojiCatalog.load(contentsOf: url)
    }

    func testDecodesTheBundledDataset() throws {
        let index = try realIndex()
        XCTAssertEqual(index.emojis.count, 1906)
        XCTAssertEqual(index.groups.count, 9)
        XCTAssertEqual(index.groups.first, "Smileys & emotion")
        XCTAssertEqual(index.groups.last, "Flags")
        // Every record carries the mandatory keys, and `k` is all-or-nothing.
        for record in index.emojis {
            XCTAssertFalse(record.unicode.isEmpty)
            XCTAssertFalse(record.label.isEmpty)
            XCTAssertTrue((0..<index.groups.count).contains(record.group))
            if let tones = record.tones { XCTAssertEqual(tones.count, 5) }
        }
    }

    func testSearchRanksExactShortcodesFirst() throws {
        let index = try realIndex()
        // `smile` is 😄's shortcode; 😃 (`smiley`) only prefix-matches, and it
        // sorts EARLIER in the dataset — so this asserts the ranking, not order.
        XCTAssertEqual(index.search("smile", limit: 8).first?.unicode, "\u{1F604}")
        // `+1` is thumbs up. The dataset ships the FULLY QUALIFIED sequence
        // (U+1F44D U+FE0F), which is one grapheme — so compare scalars, not a
        // `hasPrefix` against the bare codepoint.
        XCTAssertEqual(index.search("+1", limit: 8).first?.unicode.unicodeScalars.first, "\u{1F44D}")
    }

    func testSearchPrefersShortcodePrefixesOverLabelsAndTags() throws {
        let index = try realIndex()
        let results = index.search("smi", limit: 8)
        XCTAssertFalse(results.isEmpty)
        // The whole head of the list is shortcode-prefix; label/tag matches
        // ("smiling face…") only appear once those run out.
        let first = try XCTUnwrap(results.first)
        XCTAssertTrue(first.shortcodes.contains { $0.hasPrefix("smi") })
    }

    func testSearchIsCaseInsensitiveAndCapped() throws {
        let index = try realIndex()
        XCTAssertEqual(index.search("TADA", limit: 8).first?.unicode,
                       index.search("tada", limit: 8).first?.unicode)
        XCTAssertLessThanOrEqual(index.search("a", limit: EmojiCatalog.pickerLimit).count, 64)
        XCTAssertLessThanOrEqual(index.search("a", limit: EmojiCatalog.typeaheadLimit).count, 8)
        XCTAssertTrue(index.search("zzzzznotanemoji", limit: 8).isEmpty)
    }

    func testSearchIgnoresSurroundingColonsAndEmptyQueries() throws {
        let index = try realIndex()
        // Web parity (`searchEmoji` strips `^:` and `:$`): a pasted `:tada:`
        // searches for `tada`, and an empty query yields nothing — the picker
        // renders the groups instead.
        XCTAssertEqual(index.search(":tada:", limit: 8).first?.unicode,
                       index.search("tada", limit: 8).first?.unicode)
        XCTAssertTrue(index.search("", limit: 8).isEmpty)
        XCTAssertTrue(index.search("   ", limit: 8).isEmpty)
    }

    // MARK: - Preferences

    private func throwawayDefaults(_ name: String = #function) -> UserDefaults {
        let suite = "exp.emoji.tests.\(name).\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        addTeardownBlock { UserDefaults().removePersistentDomain(forName: suite) }
        return defaults
    }

    func testSkinTonePreferenceRoundTripsAndClamps() {
        let prefs = EmojiPreferences(defaults: throwawayDefaults())
        XCTAssertEqual(prefs.skinTone, 0)
        prefs.skinTone = 3
        XCTAssertEqual(prefs.skinTone, 3)
        prefs.skinTone = 42
        XCTAssertEqual(prefs.skinTone, 5)
        prefs.skinTone = -1
        XCTAssertEqual(prefs.skinTone, 0)
        XCTAssertEqual(EmojiPreferences.skinToneKey, "exp.emojiSkinTone")
    }

    func testRecentsAreMostRecentFirstDedupedAndCappedAt24() {
        let prefs = EmojiPreferences(defaults: throwawayDefaults())
        XCTAssertEqual(EmojiPreferences.recentsKey, "exp.emojiRecent")
        XCTAssertTrue(prefs.recents.isEmpty)

        prefs.recordRecent("\u{1F600}")
        prefs.recordRecent("\u{1F44D}")
        XCTAssertEqual(prefs.recents, ["\u{1F44D}", "\u{1F600}"])

        // Re-picking moves it to the head instead of duplicating.
        prefs.recordRecent("\u{1F600}")
        XCTAssertEqual(prefs.recents, ["\u{1F600}", "\u{1F44D}"])

        for i in 0..<40 { prefs.recordRecent("e\(i)") }
        XCTAssertEqual(prefs.recents.count, EmojiPreferences.maxRecents)
        XCTAssertEqual(prefs.recents.first, "e39")
        XCTAssertEqual(Set(prefs.recents).count, EmojiPreferences.maxRecents)
    }
}
