import XCTest
@testable import ExpCore

// Recheck R3 (feedback-states.json `starting.maybeDelivered`): which failed
// starts may still have reached the device (toast + Retry + the quiet
// follow) and which were refused outright (toast + Retry only). Android
// mirrors these cases (`StartDeliveryTest.kt`).
final class StartDeliveryTests: XCTestCase {
    private func body(_ message: String, code: String) -> String {
        #"{"error":{"message":"\#(message)","code":-32603,"data":{"code":"\#(code)"}}}"#
    }

    func testATransportErrorMayHaveDelivered() {
        for code in [URLError.Code.timedOut, .networkConnectionLost, .notConnectedToInternet] {
            XCTAssertEqual(StartDelivery.fate(of: URLError(code)), .maybeDelivered, "\(code)")
        }
    }

    func testTheRelayTimeoutMayHaveDelivered() {
        let raw = TrpcError.httpError(500, body("Steer relay error (504)", code: "INTERNAL_SERVER_ERROR"))
        XCTAssertEqual(StartDelivery.fate(of: raw), .maybeDelivered)
        let wrapped = SteerApi.startError(
            status: 500, body: body("Steer relay error (504)", code: "INTERNAL_SERVER_ERROR")
        )
        XCTAssertEqual(StartDelivery.fate(of: wrapped), .maybeDelivered)
        XCTAssertEqual(StartDelivery.fate(status: 504, message: "relay_timeout"), .maybeDelivered)
    }

    func testA5xxWithoutAnErrorBodyMayHaveDelivered() {
        XCTAssertEqual(StartDelivery.fate(of: TrpcError.httpError(502, "<html>Bad Gateway</html>")), .maybeDelivered)
        XCTAssertEqual(StartDelivery.fate(of: SteerApi.startError(status: 504, body: "")), .maybeDelivered)
    }

    func testA4xxIsADefiniteFailure() {
        let refused = SteerApi.startError(
            status: 412, body: body("That device is offline", code: "PRECONDITION_FAILED")
        )
        XCTAssertEqual(StartDelivery.fate(of: refused), .failed)
        XCTAssertEqual(StartDelivery.fate(of: TrpcError.httpError(400, "")), .failed)
        XCTAssertEqual(StartDelivery.fate(of: SteerStartError.rejected("Pick an issue.")), .failed)
    }

    func testA5xxWithAnotherErrorIsADefiniteFailure() {
        let failed = SteerApi.startError(
            status: 500, body: body("Steer relay error (502)", code: "INTERNAL_SERVER_ERROR")
        )
        XCTAssertEqual(StartDelivery.fate(of: failed), .failed)
        XCTAssertEqual(failed.localizedDescription, "Steer relay error (502)")
    }

    func testANeverSentRequestIsADefiniteFailure() {
        XCTAssertEqual(StartDelivery.fate(of: TrpcError.noInstanceUrl), .failed)
        XCTAssertEqual(StartDelivery.fate(of: CancellationError()), .failed)
    }
}
