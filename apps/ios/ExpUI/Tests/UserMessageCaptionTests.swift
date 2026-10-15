import Foundation
import XCTest
import ExpUI

// EXP-1317 / thread parity: a person's bubble caption reads `name · HH:mm`
// (web `userMessageCaption`), `· queued` while the device has not taken the
// message — the same 24-hour clock the agent channel's rows show, not the
// relative "5m" the thread once drew.
@MainActor
final class UserMessageCaptionTests: XCTestCase {

    /// A fixed mid-June day (no DST edge), like `GlassTimeRowTests`.
    private func time(_ hour: Int, _ minute: Int) -> Date {
        var components = DateComponents()
        components.year = 2026
        components.month = 6
        components.day = 15
        components.hour = hour
        components.minute = minute
        return Calendar.current.date(from: components) ?? Date()
    }

    func testNameAndClockJoinWithTheMiddleDot() {
        XCTAssertEqual(UserMessageCaption.text(name: "Danny", at: time(9, 5), queued: false), "Danny · 09:05")
    }

    func testQueuedRidesLast() {
        XCTAssertEqual(UserMessageCaption.text(name: "Danny", at: time(21, 40), queued: true), "Danny · 21:40 · queued")
    }

    func testAMissingPartDropsWithItsSeparator() {
        XCTAssertEqual(UserMessageCaption.text(name: nil, at: time(17, 0), queued: true), "17:00 · queued")
        XCTAssertEqual(UserMessageCaption.text(name: "  ", at: nil, queued: false), "")
        XCTAssertEqual(UserMessageCaption.text(name: "Danny", at: nil, queued: false), "Danny")
    }
}
