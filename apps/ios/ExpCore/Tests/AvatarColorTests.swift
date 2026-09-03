import Foundation
import XCTest
@testable import ExpCore

// EXP-698 r4 — the avatar hue contract.
//
// Four clients hash the same user id into the same palette slot, so a person
// keeps one colour everywhere. The fixture below is the contract: web, iOS,
// Android and desktop each pin these exact eight pairs (see
// apps/web/src/lib/avatar-color.test.ts). Changing a value here means
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

    func testStaysInsideThePalette() {
        for i in 0..<500 {
            let index = avatarHueIndex("user-\(i)")
            XCTAssertGreaterThanOrEqual(index, 0)
            XCTAssertLessThan(index, avatarHueCount)
        }
    }
}
