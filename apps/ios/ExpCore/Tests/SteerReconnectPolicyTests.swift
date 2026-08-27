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

    /// `dialActive` in the old shape meant "a dial is in flight OR a retry
    /// wait is armed". The two are now separate inputs, so the helper keeps the
    /// old convenience: `dialActive: true` is a dial IN FLIGHT (what
    /// `.connecting` and most of each `.starting` retry cycle look like), and
    /// the armed-wait case is spelled out with `retryArmed`.
    private func revive(
        _ phase: SteerPhaseKind, dialActive: Bool = true, retryArmed: Bool = false,
        finished: Bool = false, socketStale: Bool = false
    ) -> SteerRevivalDecision {
        SteerReconnectPolicy.revive(
            phase: phase, dialInFlight: dialActive, retryArmed: retryArmed, finished: finished,
            socketStale: socketStale
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
        XCTAssertEqual(revive(.closedReconnecting, dialActive: false, retryArmed: true), .wakeRetry)
        XCTAssertEqual(revive(.starting, dialActive: false, retryArmed: true), .wakeRetry)
    }

    /// REV: a dial IN FLIGHT must never be kicked. `dialActive` used to conflate
    /// it with an armed wait, so a wake during `.starting` — where a retry dial
    /// spends most of each 3s cycle waiting for its join answer — started a
    /// SECOND dial beside the first. The first socket stayed joined and
    /// unowned, and its `no_such_session` close then ran against the new dial:
    /// the live task got nulled out and a phase that had already gone `.live`
    /// regressed to `.starting`.
    func testDialInFlightIsNeverKicked() {
        for phase in [SteerPhaseKind.connecting, .starting, .closedReconnecting] {
            XCTAssertEqual(revive(phase, dialActive: true, retryArmed: false), .nothing, "\(phase)")
            // A retry slot left behind by the dial that is now running does not
            // make it kickable either.
            XCTAssertEqual(
                revive(phase, dialActive: true, retryArmed: true), .nothing, "\(phase) + armed"
            )
        }
    }

    /// Neither flag set is still the wedge EXP-625 exists for: an explicit dial.
    func testNeitherFlagDialsWhateverThePhase() {
        for phase in [SteerPhaseKind.idle, .connecting, .starting, .closedReconnecting] {
            XCTAssertEqual(
                revive(phase, dialActive: false, retryArmed: false), .dial, "\(phase)"
            )
        }
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
