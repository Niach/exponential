import Foundation
import XCTest
@testable import ExpCore

// #FEED-219: the debounced writer behind every composer's draft.
@MainActor
final class ComposerDraftWriterTests: XCTestCase {
    private actor Log {
        var calls: [String] = []
        var failNext = false
        func record(_ call: String) throws {
            if failNext {
                failNext = false
                throw URLError(.notConnectedToInternet)
            }
            calls.append(call)
        }
        func setFailNext() { failNext = true }
    }

    private func writer(_ log: Log, debounce: Duration = .milliseconds(30)) -> ComposerDraftWriter {
        ComposerDraftWriter(
            transport: .init(
                upsert: { teamId, key, text in try await log.record("upsert \(teamId) \(key) \(text)") },
                delete: { key in try await log.record("delete \(key)") }
            ),
            debounce: debounce
        )
    }

    func testDebounceCoalescesKeystrokes() async throws {
        let log = Log()
        let w = writer(log)
        w.write(key: "comment:i", teamId: "t", text: "h")
        w.write(key: "comment:i", teamId: "t", text: "hi")
        XCTAssertEqual(w.local("comment:i"), "hi")
        // Poll: a loaded CI runner can land the 30 ms debounce far later.
        let deadline = ContinuousClock.now + .seconds(5)
        while w.local("comment:i") != nil, ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(20))
        }
        let calls = await log.calls
        XCTAssertEqual(calls, ["upsert t comment:i hi"])
        XCTAssertNil(w.local("comment:i"))
    }

    func testFlushWritesAtOnceAndEmptyDeletes() async {
        let log = Log()
        let w = writer(log, debounce: .seconds(60))
        w.write(key: "session:s", teamId: "t", text: " \n")
        await w.flush("session:s")
        let calls = await log.calls
        XCTAssertEqual(calls, ["delete session:s"])
    }

    func testFailedWriteKeepsTheLocalCopyAndRetries() async {
        let log = Log()
        await log.setFailNext()
        let w = writer(log, debounce: .seconds(60))
        w.write(key: "reply:c", teamId: "t", text: "draft")
        await w.flush("reply:c")
        XCTAssertEqual(w.local("reply:c"), "draft")
        await w.flush("reply:c")
        let calls = await log.calls
        XCTAssertEqual(calls, ["upsert t reply:c draft"])
        XCTAssertNil(w.local("reply:c"))
    }
}
