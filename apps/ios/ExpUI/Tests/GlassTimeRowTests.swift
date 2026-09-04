import Foundation
import SwiftUI
import XCTest
import ExpUI

// EXP-721: `GlassTimeRow` replaced the stock compact `DatePicker` in the
// automation schedule. The row's VALUE is the part that had to be pinned: a
// locale-driven time renders "9:00 AM" on a US phone, and the same schedule
// reads "09:00" on Android, desktop and web — one automation, two different
// sentences depending on which client opened it.
final class GlassTimeRowTests: XCTestCase {

    /// A fixed mid-June day, so no DST transition can swallow an hour and turn
    /// 00:00 into 01:00 on the one day a year the test would notice.
    private func time(_ hour: Int, _ minute: Int) -> Date {
        var components = DateComponents()
        components.year = 2026
        components.month = 6
        components.day = 15
        components.hour = hour
        components.minute = minute
        return Calendar.current.date(from: components) ?? Date()
    }

    func testTheValueIsZeroPadded24Hour() {
        XCTAssertEqual(GlassTimeRow.formatted(time(9, 0)), "09:00")
        XCTAssertEqual(GlassTimeRow.formatted(time(17, 5)), "17:05")
    }

    // Midnight and noon are where a 12-hour formatter would say "12:00" for
    // both — the two values the cross-client sentence must keep apart.
    func testMidnightAndNoonStayDistinct() {
        XCTAssertEqual(GlassTimeRow.formatted(time(0, 0)), "00:00")
        XCTAssertEqual(GlassTimeRow.formatted(time(12, 0)), "12:00")
    }

    // It builds: a plain View with no environment requirements, so a Form
    // section, a sheet and a preview host can all stack it.
    func testTheRowBuilds() {
        let row = GlassTimeRow("Time", selection: .constant(time(9, 0)))
        XCTAssertNotNil(UIHostingController(rootView: row).view)
    }
}
