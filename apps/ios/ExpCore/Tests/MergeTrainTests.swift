import Foundation
import XCTest
@testable import ExpCore

// #EXP-1710: the merge train's client half ×4, replayed by path against
// packages/domain-contract/fixtures/merge-train.json (TS `merge-train.ts`):
// `statusPill`, `menu`, `stuckRow`, plus the copy, limits and test ids.
final class MergeTrainTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/merge-train.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try fixture()[key] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    func testCopyLimitsAndTestIds() throws {
        let f = try fixture()
        let copy = try XCTUnwrap(f["copy"] as? [String: [String: String]])
        XCTAssertEqual(copy["pill"], [
            "draft": MergeTrain.Copy.Pill.draft,
            "checksRunning": MergeTrain.Copy.Pill.checksRunning,
            "sessionWorking": MergeTrain.Copy.Pill.sessionWorking,
            "queued": MergeTrain.Copy.Pill.queued,
            "queuedAt": MergeTrain.Copy.Pill.queuedAt,
            "testing": MergeTrain.Copy.Pill.testing,
            "testingWith": MergeTrain.Copy.Pill.testingWith,
            "merged": MergeTrain.Copy.Pill.merged,
            "conflict": MergeTrain.Copy.Pill.conflict,
            "failing": MergeTrain.Copy.Pill.failing,
            "onHold": MergeTrain.Copy.Pill.onHold,
            "stuck": MergeTrain.Copy.Pill.stuck,
            "needsApproval": MergeTrain.Copy.Pill.needsApproval,
        ])
        XCTAssertEqual(copy["menu"], [
            "mergeNow": MergeTrain.Copy.Menu.mergeNow,
            "hold": MergeTrain.Copy.Menu.hold,
            "resume": MergeTrain.Copy.Menu.resume,
        ])
        XCTAssertEqual(copy["needsYou"], [
            "title": MergeTrain.Copy.NeedsYou.title,
            "bounces": MergeTrain.Copy.NeedsYou.bounces,
            "timeout": MergeTrain.Copy.NeedsYou.timeout,
            "offline": MergeTrain.Copy.NeedsYou.offline,
            "retry": MergeTrain.Copy.NeedsYou.retry,
            "mergeNow": MergeTrain.Copy.NeedsYou.mergeNow,
            "close": MergeTrain.Copy.NeedsYou.close,
        ])
        XCTAssertEqual(copy["settings"], [
            "label": MergeTrain.Copy.Settings.label,
            "auto": MergeTrain.Copy.Settings.auto,
            "approval": MergeTrain.Copy.Settings.approval,
        ])
        XCTAssertEqual(copy["events"], [
            "merged": MergeTrain.Copy.Events.merged,
            "conflict": MergeTrain.Copy.Events.conflict,
            "failing": MergeTrain.Copy.Events.failing,
            "held": MergeTrain.Copy.Events.held,
        ])
        let limits = try XCTUnwrap(f["limits"] as? [String: Int])
        XCTAssertEqual(limits["bounces"], MergeTrain.Limits.bounces)
        XCTAssertEqual(limits["offlineHours"], MergeTrain.Limits.offlineHours)
        XCTAssertEqual(f["testIds"] as? [String: String], [
            "statusPill": MergeTrain.TestIds.statusPill,
            "mergeNow": MergeTrain.TestIds.mergeNow,
            "hold": MergeTrain.TestIds.hold,
            "resume": MergeTrain.TestIds.resume,
            "stuckRow": MergeTrain.TestIds.stuckRow,
            "settingsRow": MergeTrain.TestIds.settingsRow,
        ])
        // The Merge now alert IS prompts.json `merge-now-pr`.
        let prompt = try XCTUnwrap(f["mergeNowPrompt"] as? [String: Any])
        XCTAssertEqual(prompt["id"] as? String, Prompts.MergeNowPr.entry.id)
        XCTAssertEqual(Prompts.MergeNowPr.copy(number: 12, stack: false).title, "Merge PR #12 now?")
        XCTAssertEqual(Prompts.MergeNowPr.copy(number: 12, stack: true).title, prompt["titleStack"] as? String)
    }

    func testOrdinal() {
        let expected = [1: "1st", 2: "2nd", 3: "3rd", 4: "4th", 11: "11th", 12: "12th", 13: "13th", 21: "21st",
                        22: "22nd", 23: "23rd", 101: "101st", 111: "111th", 112: "112th"]
        for (n, label) in expected {
            XCTAssertEqual(MergeTrain.ordinal(n), label, "\(n)")
        }
    }

    func testStatusPillReplaysEveryCase() throws {
        for item in try cases("statusPill") {
            let name = item["name"] as? String ?? "?"
            let input = try XCTUnwrap(item["input"] as? [String: Any], name)
            let pill = MergeTrain.statusPill(
                state: input["state"] as? String,
                wait: input["wait"] as? String,
                position: input["position"] as? Int,
                batchSize: input["batchSize"] as? Int,
                holdReason: input["holdReason"] as? String,
                stack: try XCTUnwrap(input["stack"] as? Bool, name),
                checks: try XCTUnwrap(input["checks"] as? Int, name)
            )
            var actual: [String: Any] = [
                "label": pill.label, "glyph": pill.glyph.rawValue, "tone": pill.tone.rawValue,
                "primary": pill.primary, "tap": pill.tap?.rawValue ?? NSNull(),
            ]
            if let confirm = pill.confirm { actual["confirm"] = confirm.rawValue }
            XCTAssertEqual(actual as NSDictionary, try XCTUnwrap(item["expected"] as? NSDictionary), name)
        }
    }

    /// The styleguide specimen draws every `statusPill` case, in order.
    func testSpecimenIsTheFixtureInputs() throws {
        let inputs = try cases("statusPill").map { item -> MergeTrain.PillInput in
            let input = try XCTUnwrap(item["input"] as? [String: Any])
            return MergeTrain.PillInput(
                state: try XCTUnwrap(input["state"] as? String),
                wait: input["wait"] as? String,
                position: input["position"] as? Int,
                batchSize: input["batchSize"] as? Int,
                holdReason: input["holdReason"] as? String,
                stack: try XCTUnwrap(input["stack"] as? Bool),
                checks: try XCTUnwrap(input["checks"] as? Int)
            )
        }
        XCTAssertEqual(MergeTrain.specimen, inputs)
    }

    func testPillStateAndViewerPill() {
        XCTAssertNil(MergeTrain.pillState(prState: "closed", mergeState: "queued"))
        XCTAssertEqual(MergeTrain.pillState(prState: "merged", mergeState: "testing"), "merged")
        XCTAssertEqual(MergeTrain.pillState(prState: "draft", mergeState: "queued"), "draft")
        XCTAssertEqual(MergeTrain.pillState(prState: "open", mergeState: "queued"), "queued")
        XCTAssertEqual(MergeTrain.pillState(prState: "open", mergeState: nil), "checks")
        let approve = MergeTrain.statusPill(state: "checks", wait: "approval", stack: false, checks: 2)
        let viewer = MergeTrain.viewerPill(approve, canApprove: false, checks: 2)
        XCTAssertEqual(viewer.label, "Needs approval")
        XCTAssertEqual(viewer.glyph, .prMerged)
        XCTAssertFalse(viewer.primary)
        XCTAssertEqual(viewer.tap, .checks)
        XCTAssertNil(MergeTrain.viewerPill(approve, canApprove: false, checks: 0).tap)
        XCTAssertEqual(MergeTrain.viewerPill(approve, canApprove: true, checks: 2), approve)
    }

    func testMenuReplaysEveryCase() throws {
        for item in try cases("menu") {
            let name = item["name"] as? String ?? "?"
            let input = try XCTUnwrap(item["input"] as? [String: Any], name)
            let entries = MergeTrain.mergeMenuEntries(
                state: input["state"] as? String,
                member: try XCTUnwrap(input["member"] as? Bool, name),
                owner: try XCTUnwrap(input["owner"] as? Bool, name),
                checks: try XCTUnwrap(input["checks"] as? Int, name),
                mergeState: input["mergeState"] as? String
            )
            XCTAssertEqual(entries.map(\.rawValue), try XCTUnwrap(item["expected"] as? [String], name), name)
        }
    }

    func testStuckRowReplaysEveryCase() throws {
        for item in try cases("stuckRow") {
            let name = item["name"] as? String ?? "?"
            let input = try XCTUnwrap(item["input"] as? [String: Any], name)
            let row = MergeTrain.stuckRow(
                number: try XCTUnwrap(input["number"] as? Int, name),
                state: input["state"] as? String,
                holdReason: input["holdReason"] as? String,
                deviceOfflineSince: (input["deviceOfflineSince"] as? String).flatMap(WireTimestamps.parse),
                now: try XCTUnwrap(WireTimestamps.parse(try XCTUnwrap(input["now"] as? String, name)), name),
                owner: try XCTUnwrap(input["owner"] as? Bool, name)
            )
            if item["expected"] is NSNull {
                XCTAssertNil(row, name)
                continue
            }
            let expected = try XCTUnwrap(item["expected"] as? [String: Any], name)
            let actual: [String: Any]? = row.map {
                ["title": $0.title, "meta": $0.meta, "actions": $0.actions.map(\.rawValue)]
            }
            XCTAssertEqual(actual.map { $0 as NSDictionary }, expected as NSDictionary, name)
        }
    }

    func testEventTemplates() {
        XCTAssertEqual(MergeTrain.eventTemplate(outcome: "merged"), "Merged PR {pr}")
        XCTAssertEqual(MergeTrain.eventTemplate(outcome: "conflict"), "Conflict on PR {pr}")
        XCTAssertEqual(MergeTrain.eventTemplate(outcome: "failing"), "Checks failing on PR {pr}")
        XCTAssertEqual(MergeTrain.eventTemplate(outcome: "held"), "Auto-merge stopped on PR {pr}")
        XCTAssertNil(MergeTrain.eventTemplate(outcome: nil))
        // The `pr_merge` card carries its outcome.
        let card = TreeMessageCard(object: [
            "kind": "event", "event": "pr_merge", "pullRequestId": "p1", "mergeOutcome": "conflict",
        ])
        XCTAssertEqual(card.mergeOutcome, "conflict")
        XCTAssertNil(TreeMessageCard(object: ["kind": "event", "event": "pr_merge"]).mergeOutcome)
    }
}
