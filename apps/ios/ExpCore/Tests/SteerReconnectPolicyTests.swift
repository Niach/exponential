import XCTest

@testable import ExpCore

// EXP-621: the close-code rule behind the viewer's redial. The codes are the
// relay's (apps/steer-relay/src/protocol.ts) — a slow-consumer eviction is a
// hiccup the viewer redials through silently, an unauthorized close is
// permanent, and everything else keeps the pre-EXP-621 backoff behavior.
final class SteerReconnectPolicyTests: XCTestCase {

    func testSlowConsumerRedialsImmediately() {
        XCTAssertEqual(
            SteerReconnectPolicy.decide(closeCode: 4008, sessionOver: false),
            .redialImmediately
        )
    }

    func testUnauthorizedIsTerminal() {
        XCTAssertEqual(
            SteerReconnectPolicy.decide(closeCode: 4003, sessionOver: false),
            .terminalClosed
        )
    }

    func testUnknownCodesKeepTheBackoffPath() {
        for code in [nil, 0, 1000, 1006, 4001, 4002, 4009] as [Int?] {
            XCTAssertEqual(
                SteerReconnectPolicy.decide(closeCode: code, sessionOver: false),
                .reconnectWithBackoff,
                "close code \(String(describing: code))"
            )
        }
    }

    func testAnEndedSessionOutranksEveryCode() {
        for code in [nil, 4003, 4008, 1006] as [Int?] {
            XCTAssertEqual(
                SteerReconnectPolicy.decide(closeCode: code, sessionOver: true),
                .ended,
                "close code \(String(describing: code))"
            )
        }
    }

    func testCodesMatchTheRelayConstants() {
        XCTAssertEqual(SteerCloseCode.unauthorized, 4003)
        XCTAssertEqual(SteerCloseCode.slowConsumer, 4008)
    }
}
