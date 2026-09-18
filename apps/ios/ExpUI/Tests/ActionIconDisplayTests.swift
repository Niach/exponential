import Foundation
import XCTest
import ExpCore
import ExpUI

// The action glyph resolver: a shipped name wins, anything else — unset,
// empty, or a name only a newer build knows — is the generic action mark, so
// no row ever draws a hole.
final class ActionIconDisplayTests: XCTestCase {

    func testAShippedNameWins() {
        XCTAssertEqual(ActionIconDisplay.iconName(for: "rocket"), "rocket")
        XCTAssertTrue(AppIcons.allNames.contains("rocket"))
    }

    func testUnsetAndUnknownNamesFallBackToTheActionMark() {
        XCTAssertEqual(ActionIconDisplay.iconName(for: nil), AppIcons.actionDefault)
        XCTAssertEqual(ActionIconDisplay.iconName(for: ""), AppIcons.actionDefault)
        XCTAssertEqual(
            ActionIconDisplay.iconName(for: "glyph-from-a-newer-registry"), AppIcons.actionDefault
        )
        XCTAssertFalse(AppIcons.allNames.contains("glyph-from-a-newer-registry"))
    }

    func testTheFallbackItselfShips() {
        XCTAssertTrue(AppIcons.allNames.contains(AppIcons.actionDefault))
    }
}
