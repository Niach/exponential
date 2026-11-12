import Foundation
import XCTest
@testable import ExpCore

// #ERR-17: a crash report survives a 5xx/429 from the tunnel (same table as
// Android's TunnelTransportTest).
final class MonitoringTunnelRetryTests: XCTestCase {
    typealias Retry = Monitoring.TunnelRetry

    func testTransientAnswersAreRetriedOthersSentOrDropped() {
        for status in [nil, 408, 429, 500, 502, 503, 504] { XCTAssertEqual(Retry.outcome(status: status), .retry, "\(String(describing: status))") }
        for status in [200, 202] { XCTAssertEqual(Retry.outcome(status: status), .sent) }
        for status in [400, 401, 403, 404, 413] { XCTAssertEqual(Retry.outcome(status: status), .drop) }
    }

    func testBackoffDoublesToTenMinutesAndHonoursTheServer() {
        XCTAssertEqual([1, 2, 3, 4, 8, 30].map { Retry.delay(failures: $0, retryAfter: nil) }, [5, 10, 20, 40, 600, 600])
        XCTAssertEqual(Retry.delay(failures: 1, retryAfter: 120), 120)
        XCTAssertEqual(Retry.delay(failures: 2, retryAfter: 1), 10)
        XCTAssertEqual(Retry.delay(failures: 1, retryAfter: 7200), 3600)
        XCTAssertEqual(Retry.retryAfter("60", rateLimits: "60:error:key, 2700::organization"), 2700)
        XCTAssertEqual(Retry.retryAfter("30", rateLimits: nil), 30)
        XCTAssertNil(Retry.retryAfter("Wed, 21 Oct 2026 07:28:00 GMT", rateLimits: nil))
        XCTAssertNil(Retry.retryAfter(nil, rateLimits: nil))
    }

    private func envelope(_ items: [(header: String, payload: Data)]) -> Data {
        var data = Data(#"{"event_id":"9ec79c33ec9942ab8353589fcb2e04dc"}"#.utf8) + Data([0x0A])
        for item in items { data += Data(item.header.utf8) + Data([0x0A]) + item.payload + Data([0x0A]) }
        return data
    }

    /// A gzip member around a raw DEFLATE stream (the trailer is not checked).
    private func gzip(_ data: Data) throws -> Data {
        let deflated = try (data as NSData).compressed(using: .zlib) as Data
        return Data([0x1F, 0x8B, 0x08, 0, 0, 0, 0, 0, 0, 0x03]) + deflated + Data(count: 8)
    }

    func testOnlyErrorAndCrashEnvelopesWait() throws {
        let binary = Data([0x0A, 0x7B, 0x0A, 0x00])
        let crash = envelope([
            (#"{"type":"attachment","length":4}"#, binary),
            (#"{"type":"event"}"#, Data(#"{"level":"fatal"}"#.utf8)),
        ])
        XCTAssertEqual(Retry.itemTypes(crash), ["attachment", "event"])
        XCTAssertTrue(Retry.carriesEvent(crash))
        XCTAssertTrue(Retry.carriesEvent(try gzip(crash)))

        let telemetry = envelope([
            (#"{"type":"transaction"}"#, Data("{}".utf8)),
            (#"{"type":"log","length":2}"#, Data("{}".utf8)),
            (#"{"type":"client_report"}"#, Data("{}".utf8)),
        ])
        XCTAssertEqual(Retry.itemTypes(telemetry), ["transaction", "log", "client_report"])
        XCTAssertFalse(Retry.carriesEvent(try gzip(telemetry)))
        XCTAssertFalse(Retry.carriesEvent(Data("not an envelope".utf8)))
    }

    func testTheBackoffPlansOneResendAtATime() {
        let state = MonitoringState()
        let t0 = Date(timeIntervalSince1970: 1_000)
        XCTAssertFalse(state.tunnelBackingOff(now: t0))

        XCTAssertEqual(state.tunnelFailed(retryAfter: nil, now: t0), t0 + 5)
        XCTAssertTrue(state.tunnelBackingOff(now: t0 + 4))
        // A second failure moves the deadline; the planned resend stays the one.
        XCTAssertNil(state.tunnelFailed(retryAfter: nil, now: t0 + 1))
        XCTAssertEqual(state.tunnelRetryDue(now: t0 + 5), t0 + 11)
        XCTAssertNil(state.tunnelRetryDue(now: t0 + 11))
        XCTAssertFalse(state.tunnelBackingOff(now: t0 + 11))

        // The resend failed again: a new plan, 20 s.
        XCTAssertEqual(state.tunnelFailed(retryAfter: nil, now: t0 + 11), t0 + 31)
        state.tunnelSent()
        XCTAssertFalse(state.tunnelBackingOff(now: t0 + 12))
        XCTAssertNil(state.tunnelRetryDue(now: t0 + 12))
        XCTAssertEqual(state.tunnelFailed(retryAfter: 60, now: t0 + 12), t0 + 72)
    }
}
