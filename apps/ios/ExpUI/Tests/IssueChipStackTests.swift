import XCTest
import SwiftUI
import ExpUI

// SLOP-16: the stacked chip mirrors the web's `IssueChipStack` — two ghost
// outlines whatever the count, 2pt steps, the far one at half strength.
final class IssueChipStackTests: XCTestCase {
    func testStepIsTwoPoints() {
        XCTAssertEqual(IssueChipStack<EmptyView>.step, 2)
    }

    func testAlwaysTwoGhosts() {
        XCTAssertEqual(IssueChipStack<EmptyView>.ghosts, 2)
    }

    func testFarGhostIsHalfStrength() {
        XCTAssertEqual(IssueChipStack<EmptyView>.farGhostOpacity, 0.5)
    }
}
