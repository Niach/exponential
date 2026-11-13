import Foundation
import XCTest
@testable import ExpCore

// #PERF-5: a layout pass reads the store's answers from memory only.
@MainActor
final class StoreMemoTests: XCTestCase {
    private var clock = Date(timeIntervalSince1970: 1_000)
    private let loads = StoreMemoLoads()

    private func memo() -> StoreMemo<String, String> {
        StoreMemo(ttl: 5, capacity: 3, loads: loads, now: { [unowned self] in clock })
    }

    func testAMissAnswersNilAndLoadsOffMain() async {
        let memo = memo()
        XCTAssertNil(memo.peek("EXP-1") { "Fix the hang" })
        XCTAssertEqual(loads.tick, 0)
        await memo.settle()
        XCTAssertEqual(loads.tick, 1)
        XCTAssertEqual(memo.peek("EXP-1") { XCTFail("fresh: no load"); return nil }, "Fix the hang")
    }

    func testAStaleEntryAnswersAtOnceAndReloads() async {
        let memo = memo()
        _ = memo.peek("EXP-1") { "Old" }
        await memo.settle()
        clock += 6
        XCTAssertEqual(memo.peek("EXP-1") { "New" }, "Old")
        await memo.settle()
        XCTAssertEqual(memo.peek("EXP-1") { nil }, "New")
        XCTAssertEqual(loads.tick, 2)
    }

    func testAnUnchangedAnswerDoesNotRenderAgain() async {
        let memo = memo()
        _ = memo.peek("EXP-1") { "Same" }
        await memo.settle()
        clock += 6
        _ = memo.peek("EXP-1") { "Same" }
        await memo.settle()
        XCTAssertEqual(loads.tick, 1)

        // An unknown ref that stays unknown never ticks either.
        _ = memo.peek("EXP-404") { nil }
        await memo.settle()
        XCTAssertEqual(loads.tick, 1)
    }

    func testOneLoadPerKeyAtATime() async {
        let memo = memo()
        let counter = Counter()
        for _ in 0..<5 { _ = memo.peek("EXP-1") { await counter.next() } }
        await memo.settle()
        let count = await counter.count
        XCTAssertEqual(count, 1)
    }

    func testALandedAnswerTellsTheEditors() async {
        let memo = memo()
        let heard = expectation(forNotification: StoreMemoLoads.didLoad, object: loads)
        _ = memo.peek("EXP-1") { "Typed" }
        await memo.settle()
        await fulfillment(of: [heard], timeout: 1)
    }

    func testCapacityStartsOver() async {
        let memo = memo()
        for key in ["A", "B", "C"] { _ = memo.peek(key) { key } }
        await memo.settle()
        _ = memo.peek("D") { "D" }
        await memo.settle()
        XCTAssertNil(memo.peek("A") { nil })
        XCTAssertEqual(memo.peek("D") { nil }, "D")
    }
}

private actor Counter {
    private(set) var count = 0
    func next() -> String {
        count += 1
        return "v\(count)"
    }
}
