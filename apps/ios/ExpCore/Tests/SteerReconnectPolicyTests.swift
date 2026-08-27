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

    // EXP-625: the revival rule behind every wake signal (foreground, screen
    // attach, network return). What matters is whether a dial is actually
    // alive, NOT what the phase says: a phase-gated revival left a model whose
    // dial wedged mid-connect stuck on "Connecting…" with nothing able to
    // revive it.

    private func revive(
        _ phase: SteerPhaseKind, dialActive: Bool = true, finished: Bool = false,
        socketStale: Bool = false
    ) -> SteerRevivalDecision {
        SteerReconnectPolicy.revive(
            phase: phase, dialActive: dialActive, finished: finished, socketStale: socketStale
        )
    }

    func testIdleDials() {
        XCTAssertEqual(revive(.idle, dialActive: false), .dial)
    }

    func testDeadDialUnderAnyDialingPhaseDials() {
        for phase in [SteerPhaseKind.connecting, .starting, .closedReconnecting] {
            XCTAssertEqual(revive(phase, dialActive: false), .dial, "\(phase)")
            XCTAssertEqual(
                revive(phase, dialActive: false, socketStale: true), .dial, "\(phase) stale"
            )
        }
    }

    func testArmedWaitsAreKicked() {
        XCTAssertEqual(revive(.closedReconnecting), .wakeRetry)
        XCTAssertEqual(revive(.starting), .wakeRetry)
    }

    func testConnectingWithALiveDialIsLeftAlone() {
        XCTAssertEqual(revive(.connecting), .nothing)
    }

    /// `.live` answers to the socket, not to the dial: a finished dial is the
    /// NORMAL state of a healthy live session, so `dialActive` must not decide
    /// here or every foreground would redial a perfectly good socket.
    func testLiveRedialsOnlyWhenStaleWhateverTheDial() {
        for dialActive in [true, false] {
            XCTAssertEqual(
                revive(.live, dialActive: dialActive, socketStale: false), .nothing,
                "fresh dialActive=\(dialActive)"
            )
            XCTAssertEqual(
                revive(.live, dialActive: dialActive, socketStale: true), .redialSilently,
                "stale dialActive=\(dialActive)"
            )
        }
    }

    func testIdleWithALiveDialIsLeftAlone() {
        XCTAssertEqual(revive(.idle), .nothing)
    }

    func testFinalAndFinishedDoNothingRegardless() {
        for dialActive in [true, false] {
            for stale in [true, false] {
                XCTAssertEqual(
                    revive(.final, dialActive: dialActive, socketStale: stale), .nothing,
                    "final dialActive=\(dialActive) stale=\(stale)"
                )
                for phase in [
                    SteerPhaseKind.idle, .connecting, .starting, .live, .closedReconnecting, .final,
                ] {
                    XCTAssertEqual(
                        revive(phase, dialActive: dialActive, finished: true, socketStale: stale),
                        .nothing,
                        "finished \(phase) dialActive=\(dialActive) stale=\(stale)"
                    )
                }
            }
        }
    }

    func testCodesMatchTheRelayConstants() {
        XCTAssertEqual(SteerCloseCode.unauthorized, 4003)
        XCTAssertEqual(SteerCloseCode.slowConsumer, 4008)
    }
}
