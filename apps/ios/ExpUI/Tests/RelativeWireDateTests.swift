import Foundation
import XCTest
import ExpUI

// Polish pin: LIST captions (drafts, the issue's Runs menu, device "Last
// seen") read COMPACT ×4 — "5m", "2h", "3d" — never the system's "2 hr. ago".
final class RelativeWireDateTests: XCTestCase {
    private func wire(secondsAgo: TimeInterval) -> String {
        ISO8601DateFormatter().string(from: Date().addingTimeInterval(-secondsAgo))
    }

    func testCompactWording() {
        XCTAssertEqual(relativeWireDate(wire(secondsAgo: 2 * 3600)), "2h")
        XCTAssertEqual(relativeWireDate(wire(secondsAgo: 5 * 60)), "5m")
        XCTAssertEqual(relativeWireDate(wire(secondsAgo: 3 * 86400)), "3d")
    }

    func testUnparsableIsEmpty() {
        XCTAssertEqual(relativeWireDate("not a date"), "")
    }
}
