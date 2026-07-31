import Foundation
import XCTest
@testable import ExpCore

// The dead-session gate is PER ACCOUNT, like the 426 UpdateGate (REV2-43). iOS
// is multi-instance, so one server rejecting a credential must sign out that
// account alone: every other signed-in account keeps syncing. Entries are
// CONSUMED by the handler, which is what lets the user sign straight back into
// the same account.
final class SessionGateTests: XCTestCase {
    private let cloud = "acct-cloud"
    private let selfHosted = "acct-self-hosted"

    // The gate is a process singleton, so each test starts from a clean slate.
    private func resetGate() {
        _ = SessionGate.shared.consumeAll()
    }

    func testInvalidateScopesToThatAccount() {
        resetGate()
        SessionGate.shared.invalidate(accountId: selfHosted)

        XCTAssertTrue(SessionGate.shared.isInvalidated(accountId: selfHosted))
        XCTAssertFalse(SessionGate.shared.isInvalidated(accountId: cloud))
        XCTAssertEqual(SessionGate.shared.invalidatedAccountIds, [selfHosted])
    }

    // 16 shape loops plus in-flight tRPC calls all race here when a session
    // dies; the handler must see one entry per account, not sixteen.
    func testRepeatedInvalidationIsIdempotent() {
        resetGate()
        SessionGate.shared.invalidate(accountId: cloud)
        SessionGate.shared.invalidate(accountId: cloud)
        SessionGate.shared.invalidate(accountId: selfHosted)

        XCTAssertEqual(SessionGate.shared.invalidatedAccountIds, [cloud, selfHosted])
    }

    // An empty accountId is never an account (the extension's tokenless calls,
    // an unresolved active account) and must not enqueue a phantom sign-out.
    func testEmptyAccountIdIsIgnored() {
        resetGate()
        SessionGate.shared.invalidate(accountId: "")

        XCTAssertTrue(SessionGate.shared.invalidatedAccountIds.isEmpty)
    }

    // consumeAll drains: a second tick must not re-sign-out an account that has
    // meanwhile been signed back in.
    func testConsumeAllDrainsTheGate() {
        resetGate()
        SessionGate.shared.invalidate(accountId: cloud)
        SessionGate.shared.invalidate(accountId: selfHosted)

        XCTAssertEqual(SessionGate.shared.consumeAll(), [cloud, selfHosted])
        XCTAssertTrue(SessionGate.shared.invalidatedAccountIds.isEmpty)
        XCTAssertTrue(SessionGate.shared.consumeAll().isEmpty)
    }

    // The onboarding escape hatch signs out itself, so it drops just its own
    // entry — another account's pending sign-out must survive.
    func testClearReleasesOnlyThatAccount() {
        resetGate()
        SessionGate.shared.invalidate(accountId: cloud)
        SessionGate.shared.invalidate(accountId: selfHosted)

        SessionGate.shared.clear(accountId: selfHosted)

        XCTAssertEqual(SessionGate.shared.invalidatedAccountIds, [cloud])
    }
}
