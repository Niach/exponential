import XCTest
@testable import ExpCore

// NEW-2: the `prompts.*` wire payloads. Absent vs null is load-bearing on
// `prompts.update`: the router applies a key only when it is present, so an
// OMITTED field keeps what the row has and an explicit `null` CLEARS it (the
// icon) — the BoardInputEncodingTests story, pinned per field. Prompts carry
// no inputs and no composer hint (the actions' `promptPlaceholder` is gone).
final class PromptUpdateInputEncodingTests: XCTestCase {
    private func json(_ value: some Encodable) throws -> [String: Any] {
        let data = try JSONEncoder().encode(value)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    func testAnEmptyPatchSendsOnlyTheId() throws {
        let object = try json(PromptUpdateInput(id: "p-1", patch: PromptPatch()))
        XCTAssertEqual(object.keys.sorted(), ["id"])
        XCTAssertEqual(object["id"] as? String, "p-1")
        XCTAssertTrue(PromptPatch().isEmpty)
    }

    func testTheIconClearsWithAnExplicitNull() throws {
        let untouched = try json(PromptUpdateInput(id: "p-1", patch: PromptPatch(name: "Ship")))
        XCTAssertEqual(untouched.keys.sorted(), ["id", "name"])

        let cleared = try json(PromptUpdateInput(
            id: "p-1",
            patch: PromptPatch(name: "Ship it", icon: .some(nil), body: "Do the thing", sortOrder: 2)
        ))
        XCTAssertEqual(cleared.keys.sorted(), ["body", "icon", "id", "name", "sortOrder"])
        XCTAssertTrue(cleared["icon"] is NSNull)
        XCTAssertEqual(cleared["name"] as? String, "Ship it")
        XCTAssertEqual(cleared["body"] as? String, "Do the thing")
        XCTAssertEqual(cleared["sortOrder"] as? Double, 2)
        // A clear is a change — Save must enable for it.
        XCTAssertFalse(PromptPatch(icon: .some(nil)).isEmpty)

        let set = try json(PromptUpdateInput(id: "p-1", patch: PromptPatch(icon: .some("rocket"))))
        XCTAssertEqual(set["icon"] as? String, "rocket")
    }

    // `prompts.create` is a strict object: an unset icon is omitted, never
    // a stray key.
    func testCreateCarriesOnlyTheRouterKeys() throws {
        let bare = try json(PromptCreateInput(teamId: "t-1", name: "Review", body: "Review the PR"))
        XCTAssertEqual(bare.keys.sorted(), ["body", "name", "teamId"])
        let full = try json(PromptCreateInput(teamId: "t-1", name: "Review", icon: "rocket", body: "Go"))
        XCTAssertEqual(full.keys.sorted(), ["body", "icon", "name", "teamId"])
    }
}
