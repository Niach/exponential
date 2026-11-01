import Foundation
import XCTest

@testable import ExpCore

// #EXP-1538 LOCAL FIRST (fixtures/dock.json): the last-known Dock slots a
// session keeps after its socket owner is retired, so the next open draws
// them in its first frame instead of after the join replay.
final class SteerDockMemoryTests: XCTestCase {
    private let tasks = SteerDockSlots(
        taskList: [AgentTaskListEntry(content: "Write the tests", status: .inProgress)],
        backgroundTasks: [AgentBackgroundTask(id: "b1", kind: "shell", description: "bun test")],
        backgroundFirstSeen: ["b1": 1_770_000_000_000],
        queued: [QueuedMessage(id: "q1", text: "and then ship it")]
    )

    func testRemembersPerSession() {
        var memory = SteerDockMemory()
        memory.remember("acc/s1", tasks)
        XCTAssertEqual(memory.lastKnown("acc/s1"), tasks)
        XCTAssertNil(memory.lastKnown("acc/s2"))
        XCTAssertNil(memory.lastKnown("other/s1"))
    }

    func testLatestWins() {
        var memory = SteerDockMemory()
        memory.remember("acc/s1", tasks)
        let later = SteerDockSlots(rateLimit: AgentSessionRateLimit(status: "rejected", resetsAt: 1))
        memory.remember("acc/s1", later)
        XCTAssertEqual(memory.lastKnown("acc/s1"), later)
        XCTAssertEqual(memory.count, 1)
    }

    func testEmptySnapshotForgets() {
        var memory = SteerDockMemory()
        memory.remember("acc/s1", tasks)
        memory.remember("acc/s1", SteerDockSlots())
        XCTAssertNil(memory.lastKnown("acc/s1"))
        XCTAssertEqual(memory.count, 0)
    }

    func testEndedSessionAndSignOutForget() {
        var memory = SteerDockMemory()
        memory.remember("acc/s1", tasks)
        memory.remember("acc/s2", tasks)
        memory.forget("acc/s1")
        XCTAssertNil(memory.lastKnown("acc/s1"))
        XCTAssertEqual(memory.lastKnown("acc/s2"), tasks)
        memory.forgetAll()
        XCTAssertEqual(memory.count, 0)
    }

    func testCapDropsTheOldestRemembered() {
        var memory = SteerDockMemory(cap: 2)
        memory.remember("a", tasks)
        memory.remember("b", tasks)
        // Re-remembering refreshes its place.
        memory.remember("a", tasks)
        memory.remember("c", tasks)
        XCTAssertNil(memory.lastKnown("b"))
        XCTAssertNotNil(memory.lastKnown("a"))
        XCTAssertNotNil(memory.lastKnown("c"))
    }

    func testIsEmptyCoversEverySlot() {
        XCTAssertTrue(SteerDockSlots().isEmpty)
        XCTAssertFalse(SteerDockSlots(compacting: AgentCompaction(trigger: "auto")).isEmpty)
        XCTAssertFalse(SteerDockSlots(queued: [QueuedMessage(id: "q", text: "x")]).isEmpty)
        // First-seen stamps alone draw nothing.
        XCTAssertTrue(SteerDockSlots(backgroundFirstSeen: ["b": 1]).isEmpty)
    }
}
