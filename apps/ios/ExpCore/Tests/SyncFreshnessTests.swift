import Foundation
import XCTest

@testable import ExpCore

// EXP-656: when each account's `devices` shape last answered — the input to
// DeviceFreshness, stamped by ShapeClient and consumed by the presence
// surfaces. Its own instance per test: the app runs one shared singleton, but
// nothing here depends on that.
final class SyncFreshnessTests: XCTestCase {

    func testNeverPolledIsNil() {
        let freshness = SyncFreshness()
        XCTAssertNil(freshness.devicesPolledAt(accountId: "acct-1"))
    }

    func testStampsPerAccount() {
        let freshness = SyncFreshness()
        let at = Date(timeIntervalSince1970: 1_770_000_000)
        freshness.recordDevicesPoll(accountId: "acct-1", at: at)
        XCTAssertEqual(freshness.devicesPolledAt(accountId: "acct-1"), at)
        // A second account's cursor is its own — one signed-in server catching
        // up says nothing about another's presence rows.
        XCTAssertNil(freshness.devicesPolledAt(accountId: "acct-2"))
    }

    func testLaterPollWins() {
        let freshness = SyncFreshness()
        let first = Date(timeIntervalSince1970: 1_770_000_000)
        freshness.recordDevicesPoll(accountId: "acct-1", at: first)
        freshness.recordDevicesPoll(accountId: "acct-1", at: first.addingTimeInterval(30))
        XCTAssertEqual(
            freshness.devicesPolledAt(accountId: "acct-1"), first.addingTimeInterval(30)
        )
    }

    func testUpdatesYieldThePolledAccountId() async {
        let freshness = SyncFreshness()
        var iterator = freshness.updates().makeAsyncIterator()
        // The subscription is live once `updates()` returned, so a poll landing
        // after this line is delivered — this is the view models' repaint hook.
        freshness.recordDevicesPoll(accountId: "acct-7")
        let yielded = await iterator.next()
        XCTAssertEqual(yielded, "acct-7")
    }

    func testEveryListenerSeesThePoll() async {
        let freshness = SyncFreshness()
        var first = freshness.updates().makeAsyncIterator()
        var second = freshness.updates().makeAsyncIterator()
        freshness.recordDevicesPoll(accountId: "acct-9")
        let a = await first.next()
        let b = await second.next()
        XCTAssertEqual(a, "acct-9")
        XCTAssertEqual(b, "acct-9")
    }
}
