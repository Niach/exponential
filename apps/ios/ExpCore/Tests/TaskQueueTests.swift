import Foundation
import XCTest
@testable import ExpCore

// EXP-1321/1324: THE Queue's model, locked ×4 against
// `packages/domain-contract/fixtures/task-queue.json` — whole objects.
final class TaskQueueTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/task-queue.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func section(_ raw: Any?) throws -> TaskQueueSection? {
        guard let raw = raw as? [String: Any] else { return nil }
        let items = try (raw["items"] as? [[String: Any]] ?? []).map {
            TaskQueueItem(
                key: try XCTUnwrap($0["key"] as? String),
                kind: try XCTUnwrap(($0["kind"] as? String).flatMap(TaskQueueItemKind.init(rawValue:))),
                state: try XCTUnwrap(($0["state"] as? String).flatMap(TaskQueueItemState.init(rawValue:))),
                icon: try XCTUnwrap($0["icon"] as? String),
                label: try XCTUnwrap($0["label"] as? String),
                description: $0["description"] as? String
            )
        }
        return TaskQueueSection(
            title: try XCTUnwrap(raw["title"] as? String),
            icon: try XCTUnwrap(raw["icon"] as? String),
            items: items
        )
    }

    func testCases() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = "\(c["name"] ?? "")"
            let raw = try XCTUnwrap(c["input"] as? [String: Any])
            let input = TaskQueueInput(
                tasks: (raw["tasks"] as? [[String: Any]] ?? []).map {
                    WorkCardTask(content: $0["content"] as? String ?? "", status: $0["status"] as? String ?? "")
                },
                backgroundTasks: (raw["backgroundTasks"] as? [[String: Any]] ?? []).map {
                    TaskQueueInput.BackgroundTask(
                        id: $0["id"] as? String ?? "",
                        kind: $0["kind"] as? String ?? "",
                        description: $0["description"] as? String ?? "",
                        startedAt: ($0["startedAt"] as? NSNumber)?.int64Value
                    )
                },
                waits: (raw["waits"] as? [[String: Any]] ?? []).map {
                    TaskQueueInput.Wait(id: $0["id"] as? String ?? "", subject: $0["subject"] as? String ?? "")
                },
                now: (raw["now"] as? NSNumber)?.int64Value ?? 0
            )
            let got = TaskQueue.build(input)
            if let expected = c["expected"] as? [String: Any] {
                let want = TaskQueue(
                    todo: try XCTUnwrap(try section(expected["todo"])),
                    completed: try section(expected["completed"])
                )
                XCTAssertEqual(got, want, name)
            } else {
                XCTAssertNil(got, name)
            }
        }
    }
}
