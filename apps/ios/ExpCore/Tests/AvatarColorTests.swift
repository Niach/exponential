import Foundation
import XCTest
@testable import ExpCore

// EXP-698 r4 — the avatar hue contract.
//
// Four clients hash the same user id into the same palette slot, so a person
// keeps one colour everywhere. The fixture below is the contract: web, iOS,
// Android and desktop each pin these exact eight pairs (see
// packages/ui/src/avatar-color.test.ts). Changing a value here means
// recolouring existing users on one client only — don't.
final class AvatarColorTests: XCTestCase {
    private let fixture: [(String, Int)] = [
        ("", 5),
        ("demo-mira", 2),
        ("demo-jonas", 4),
        ("demo-sofia", 1),
        ("alex", 5),
        ("7c9e6679-7425-40de-944b-e07fc1f90ae7", 3),
        ("user_01HZY", 1),
        ("ünïcödé", 2),
    ]

    func testMatchesTheCrossClientFixture() {
        for (id, index) in fixture {
            XCTAssertEqual(avatarHueIndex(id), index, "id: \(id)")
        }
    }

    func testNilIdHashesAsTheEmptyId() {
        XCTAssertEqual(avatarHueIndex(nil), avatarHueIndex(""))
    }

    /// #EXP-1754: packages/exponential-ui/fixtures/avatar.json `keys`: the key
    /// is the id, else the name, else the email (trimmed; blank = absent),
    /// nil = grey; the key's hue is the pinned slot.
    func testReplaysTheFixtureKeys() throws {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/exponential-ui/fixtures/avatar.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        let root = try XCTUnwrap(json as? [String: Any])
        let keys = try XCTUnwrap(root["keys"] as? [[String: Any]])
        XCTAssertFalse(keys.isEmpty)
        for c in keys {
            let name = "\(c["case"] ?? "")"
            let key = avatarHueKey(id: c["id"] as? String, name: c["name"] as? String, email: c["email"] as? String)
            XCTAssertEqual(key, c["key"] as? String, name)
            XCTAssertEqual(key.map(avatarHueIndex), c["hue"] as? Int, name)
        }
    }

    func testStaysInsideThePalette() {
        for i in 0..<500 {
            let index = avatarHueIndex("user-\(i)")
            XCTAssertGreaterThanOrEqual(index, 0)
            XCTAssertLessThan(index, avatarHueCount)
        }
    }
}
