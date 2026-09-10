import XCTest
@testable import ExpCore

// The `actions.update` wire payload (EXP-694, EXP-825). Absent vs null is
// load-bearing: the router applies a key only when it is present, so an
// OMITTED field keeps what the row has and an explicit `null` CLEARS it —
// the BoardInputEncodingTests story, pinned per field.
final class ActionUpdateInputEncodingTests: XCTestCase {
    private func json(_ value: some Encodable) throws -> [String: Any] {
        let data = try JSONEncoder().encode(value)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    func testAnEmptyPatchSendsOnlyTheId() throws {
        let object = try json(ActionUpdateInput(id: "a-1", patch: ActionPatch()))
        XCTAssertEqual(object.keys.sorted(), ["id"])
        XCTAssertEqual(object["id"] as? String, "a-1")
        XCTAssertTrue(ActionPatch().isEmpty)
    }

    // EXP-825: the composer hint rides as `promptPlaceholder` — omitted when
    // untouched, the trimmed value when set, an explicit null when cleared.
    func testPromptPlaceholderOmittedSetAndCleared() throws {
        let untouched = try json(ActionUpdateInput(id: "a-1", patch: ActionPatch(name: "Ship")))
        XCTAssertNil(untouched.index(forKey: "promptPlaceholder"))

        let set = try json(ActionUpdateInput(
            id: "a-1",
            patch: ActionPatch(promptPlaceholder: .some("Scope: which platforms, which version"))
        ))
        XCTAssertEqual(
            set["promptPlaceholder"] as? String, "Scope: which platforms, which version"
        )
        XCTAssertFalse(ActionPatch(promptPlaceholder: .some("x")).isEmpty)

        let cleared = try json(ActionUpdateInput(
            id: "a-1", patch: ActionPatch(promptPlaceholder: .some(nil))
        ))
        XCTAssertNotNil(cleared.index(forKey: "promptPlaceholder"))
        XCTAssertTrue(cleared["promptPlaceholder"] is NSNull)
        // A clear is a change — Save must enable for it.
        XCTAssertFalse(ActionPatch(promptPlaceholder: .some(nil)).isEmpty)
    }

    func testTheClearableFieldsAllFollowTheSameRule() throws {
        let cleared = try json(ActionUpdateInput(
            id: "a-1",
            patch: ActionPatch(
                name: "Ship it",
                description: .some(nil),
                icon: .some(nil),
                repositoryId: .some(nil),
                body: "Do the thing",
                promptPlaceholder: .some(nil)
            )
        ))
        XCTAssertEqual(
            cleared.keys.sorted(),
            ["body", "description", "icon", "id", "name", "promptPlaceholder", "repositoryId"]
        )
        for key in ["description", "icon", "repositoryId", "promptPlaceholder"] {
            XCTAssertTrue(cleared[key] is NSNull, key)
        }
        XCTAssertEqual(cleared["name"] as? String, "Ship it")
        XCTAssertEqual(cleared["body"] as? String, "Do the thing")
    }

    func testTheCapMirrorsTheServer() {
        XCTAssertEqual(ActionDto.promptPlaceholderMaxLength, 200)
    }

    // The server cap is a JS length (UTF-16 units): a hint made of
    // multi-unit graphemes must be clamped by units, and a cut through a
    // surrogate pair drops the pair rather than shipping half of it.
    func testTheClampCountsUtf16UnitsAndNeverSplitsASurrogatePair() {
        let cap = ActionDto.promptPlaceholderMaxLength
        let short = String(repeating: "a", count: cap)
        XCTAssertEqual(ActionDto.clampPromptPlaceholder(short), short)
        XCTAssertEqual(ActionDto.clampPromptPlaceholder(short + "b"), short)

        // 199 ASCII units + one 2-unit emoji = 201 units, 200 characters:
        // `count` would pass it, the wire would refuse it.
        let emojiTail = String(repeating: "a", count: cap - 1) + "\u{1F600}"
        XCTAssertEqual(emojiTail.count, cap)
        XCTAssertEqual(emojiTail.utf16.count, cap + 1)
        let clamped = ActionDto.clampPromptPlaceholder(emojiTail)
        XCTAssertEqual(clamped, String(repeating: "a", count: cap - 1))
        XCTAssertLessThanOrEqual(clamped.utf16.count, cap)
        XCTAssertFalse(clamped.contains("\u{FFFD}"))

        // A run of emoji: never more than the cap, always whole pairs.
        let emoji = String(repeating: "\u{1F600}", count: cap)
        let clampedEmoji = ActionDto.clampPromptPlaceholder(emoji)
        XCTAssertEqual(clampedEmoji.utf16.count, cap)
        XCTAssertEqual(clampedEmoji.count, cap / 2)
    }
}
