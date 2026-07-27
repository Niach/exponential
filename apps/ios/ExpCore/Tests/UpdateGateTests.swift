import Foundation
import XCTest
@testable import ExpCore

// The client-version gate is PER ACCOUNT (REV2-43). iOS is multi-instance —
// the bundled cloud plus any number of self-hosted servers, each with its own
// CLIENT_MIN_VERSION_IOS — so one server rejecting this build must gate that
// account alone: every other signed-in account keeps syncing, and the blocking
// view only ever belongs to the account whose server sent the 426.
@MainActor
final class UpdateGateTests: XCTestCase {
    private let cloud = "acct-cloud"
    private let selfHosted = "acct-self-hosted"

    // The gate is a process singleton, so each test starts from a clean slate.
    // Called inline rather than from setUp(): a @MainActor override can't hop
    // `self` into XCTestCase's nonisolated `super.setUp()`.
    private func resetGate() {
        for id in UpdateGate.shared.gatedAccountIds {
            UpdateGate.shared.clear(accountId: id)
        }
    }

    func testTriggerGatesOnlyThatAccount() {
        resetGate()
        UpdateGate.shared.trigger(accountId: selfHosted, min: "9.9.9", latest: "9.9.9")

        XCTAssertEqual(UpdateGate.shared.upgrade(forAccountId: selfHosted)?.min, "9.9.9")
        XCTAssertNil(UpdateGate.shared.upgrade(forAccountId: cloud))
        XCTAssertEqual(UpdateGate.shared.gatedAccountIds, [selfHosted])
    }

    // A background account's 426 must never block the app when a DIFFERENT
    // account is active — the nav gate reads the active account's entry only.
    func testNoActiveAccountIsNeverGated() {
        resetGate()
        UpdateGate.shared.trigger(accountId: selfHosted, min: "9.9.9", latest: nil)

        XCTAssertNil(UpdateGate.shared.upgrade(forAccountId: nil))
    }

    // Concurrent shape/tRPC 426s all race here on app wake: the first payload
    // for an account wins, but a second account still records its own.
    func testFirstTriggerWinsPerAccount() {
        resetGate()
        UpdateGate.shared.trigger(accountId: cloud, min: "1.0.0", latest: "1.2.0")
        UpdateGate.shared.trigger(accountId: cloud, min: "2.0.0", latest: "2.2.0")
        UpdateGate.shared.trigger(accountId: selfHosted, min: "3.0.0", latest: nil)

        XCTAssertEqual(UpdateGate.shared.upgrade(forAccountId: cloud)?.min, "1.0.0")
        XCTAssertEqual(UpdateGate.shared.upgrade(forAccountId: cloud)?.latest, "1.2.0")
        XCTAssertEqual(UpdateGate.shared.upgrade(forAccountId: selfHosted)?.min, "3.0.0")
        XCTAssertNil(UpdateGate.shared.upgrade(forAccountId: selfHosted)?.latest)
    }

    // The escape hatch: removing the offending server drops its gate without
    // touching the other accounts, so the app becomes usable again.
    func testClearReleasesOnlyThatAccount() {
        resetGate()
        UpdateGate.shared.trigger(accountId: cloud, min: "1.0.0", latest: nil)
        UpdateGate.shared.trigger(accountId: selfHosted, min: "9.9.9", latest: nil)

        UpdateGate.shared.clear(accountId: selfHosted)

        XCTAssertNil(UpdateGate.shared.upgrade(forAccountId: selfHosted))
        XCTAssertEqual(UpdateGate.shared.gatedAccountIds, [cloud])
    }

    // The 426 body is best-effort: a server may send neither field, and the
    // gate must still trip (with no version text to show).
    func testUpgradeResponseDecodesPartialPayload() throws {
        let data = Data(#"{"error":"client_upgrade_required"}"#.utf8)
        let decoded = try JSONDecoder().decode(ClientUpgradeResponse.self, from: data)

        XCTAssertNil(decoded.min)
        XCTAssertNil(decoded.latest)
    }
}
