import Foundation
import XCTest
@testable import ExpCore

// EXP-1676: the offline banner ("Can't reach the server") clears ONLY on a
// response that proves the server is up: Electric's own 2xx. No non-2xx, no
// 2xx from something else answering for a server that is down (a captive
// portal, a maintenance or SPA fallback page) and no transport error may
// clear it; URLCache never answers a shape request at all.
final class SyncHealthTests: XCTestCase {
    private struct Row: Codable, Sendable {}

    private let t0 = Date(timeIntervalSince1970: 1_000_000)

    /// An account that has failed for a minute: the banner is up.
    private func offline() -> AccountHealth {
        var h = AccountHealth()
        for second in stride(from: 0, through: 60, by: 5) {
            h = SyncDebug.recordTransportError(h, at: t0.addingTimeInterval(TimeInterval(second)))
        }
        XCTAssertEqual(SyncDebug.health(of: h, at: t0.addingTimeInterval(60)), .offline)
        return h
    }

    func testNoNonSuccessClearsTheBanner() {
        let now = t0.addingTimeInterval(65)
        for status in [500, 502, 503, 504, 400, 409, 426, 304, 302, 0] {
            let h = SyncDebug.record(offline(), httpStatus: status, fromElectric: true, at: now)
            XCTAssertEqual(SyncDebug.health(of: h, at: now), .offline, "HTTP \(status)")
        }
        let unauthorized = SyncDebug.record(offline(), httpStatus: 401, fromElectric: true, at: now)
        XCTAssertEqual(SyncDebug.health(of: unauthorized, at: now), .unauthorized)
    }

    func testANonElectricSuccessIsNotTheServer() {
        let now = t0.addingTimeInterval(65)
        for status in [200, 204] {
            let h = SyncDebug.record(offline(), httpStatus: status, fromElectric: false, at: now)
            XCTAssertEqual(SyncDebug.health(of: h, at: now), .offline, "HTTP \(status) without electric headers")
        }
    }

    func testAnUnknownShapeSaysNothing() {
        XCTAssertNil(SyncDebug.record(offline(), httpStatus: 404, fromElectric: false, at: t0.addingTimeInterval(65)))
    }

    func testTransportErrorsKeepTheBanner() {
        let now = t0.addingTimeInterval(95)
        let h = SyncDebug.recordTransportError(offline(), at: now)
        XCTAssertEqual(SyncDebug.health(of: h, at: now), .offline)
    }

    func testOnlyElectricsSuccessClearsIt() {
        let now = t0.addingTimeInterval(65)
        let h = SyncDebug.record(offline(), httpStatus: 200, fromElectric: true, at: now)
        XCTAssertEqual(SyncDebug.health(of: h, at: now), .ok)
        XCTAssertNil(h?.failureStreakStartedAt)
    }

    func testABriefFailureNeverAlarms() {
        let h = SyncDebug.recordTransportError(AccountHealth(), at: t0)
        XCTAssertEqual(SyncDebug.health(of: h, at: t0.addingTimeInterval(5)), .ok)
    }

    func testElectricResponsesAreRecognisedByTheirHeaders() {
        let url = URL(string: "https://example.test/api/shapes/issues")!
        let electric = HTTPURLResponse(
            url: url, statusCode: 200, httpVersion: nil,
            headerFields: ["electric-handle": "h-1", "electric-offset": "0_0"]
        )!
        let portal = HTTPURLResponse(
            url: url, statusCode: 200, httpVersion: nil, headerFields: ["Content-Type": "text/html"]
        )!
        XCTAssertTrue(ShapeClient<Row>.isElectricResponse(electric))
        XCTAssertFalse(ShapeClient<Row>.isElectricResponse(portal))
    }

    func testShapeRequestsNeverHitTheCache() {
        XCTAssertNil(makeShapeSession().configuration.urlCache)
    }
}
