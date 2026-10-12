import Foundation
import XCTest
@testable import ExpCore

// EXP-1284: the thread view model is fixture-locked ×4 (web
// `thread-view.test.ts`, desktop `domain::thread_view`, Android
// `ThreadViewTest`); every client runs the SAME cases from
// packages/domain-contract/fixtures/thread-view.json.
final class ThreadViewTests: XCTestCase {
    // MARK: - Fixture shapes

    private struct Fixture: Decodable {
        let views: [ViewCase]
        let partitions: [PartitionCase]
        let prTargets: [PrTargetCase]
        let guideSlices: [GuideSliceCase]
    }

    private struct ViewCase: Decodable {
        let name: String
        let input: ViewInput
        let expected: ExpectedView
    }

    private struct ViewInput: Decodable {
        let thread: FixtureThread
        let items: [FixtureItem]
        let issues: [FixtureIssue]
        let runs: [FixtureRun]
    }

    /// Any JSON value — the thread's `results` is `unknown` in the spec; the
    /// port reads it as the raw jsonb TEXT the synced row carries.
    private enum JSONValue: Codable, Equatable {
        case null
        case bool(Bool)
        case number(Double)
        case string(String)
        case array([JSONValue])
        case object([String: JSONValue])

        init(from decoder: Decoder) throws {
            let c = try decoder.singleValueContainer()
            if c.decodeNil() {
                self = .null
            } else if let value = try? c.decode(Bool.self) {
                self = .bool(value)
            } else if let value = try? c.decode(Double.self) {
                self = .number(value)
            } else if let value = try? c.decode(String.self) {
                self = .string(value)
            } else if let value = try? c.decode([JSONValue].self) {
                self = .array(value)
            } else {
                self = .object(try c.decode([String: JSONValue].self))
            }
        }

        func encode(to encoder: Encoder) throws {
            var c = encoder.singleValueContainer()
            switch self {
            case .null: try c.encodeNil()
            case let .bool(value): try c.encode(value)
            case let .number(value): try c.encode(value)
            case let .string(value): try c.encode(value)
            case let .array(value): try c.encode(value)
            case let .object(value): try c.encode(value)
            }
        }

        /// The jsonb text a synced row would carry; nil for JSON null.
        var rawText: String? {
            if case .null = self { return nil }
            guard let data = try? JSONEncoder().encode(self) else { return nil }
            return String(data: data, encoding: .utf8)
        }
    }

    private struct FixtureThread: Decodable {
        let id: String
        let title: String?
        let results: JSONValue?
        let updatedAt: String

        var model: ThreadViewThread {
            ThreadViewThread(id: id, title: title, results: results?.rawText, updatedAt: updatedAt)
        }
    }

    private struct FixtureItem: Decodable {
        let id: String
        let kind: String
        let issueId: String?
        let sessionId: String?
        let source: String
        let addedBySessionId: String?
        let sortOrder: Double
        let createdAt: String

        var model: ThreadViewItem {
            ThreadViewItem(
                id: id, kind: kind, issueId: issueId, sessionId: sessionId, source: source,
                addedBySessionId: addedBySessionId, sortOrder: sortOrder, createdAt: createdAt
            )
        }
    }

    private struct FixtureIssue: Decodable {
        let id: String
        let identifier: String
        let title: String
        let createdAt: String

        var model: ThreadViewIssue {
            ThreadViewIssue(id: id, identifier: identifier, title: title, createdAt: createdAt)
        }
    }

    private struct FixtureRun: Decodable {
        let id: String
        let name: String
        let userId: String
        let deviceLabel: String?
        let status: String
        let needsInput: Bool
        let agentBusy: Bool
        let issueId: String?
        let parentSessionId: String?
        let resumedFromId: String?
        let startedAt: String
        let endedAt: String?

        var model: ThreadViewRun {
            ThreadViewRun(
                id: id, name: name, userId: userId, deviceLabel: deviceLabel, status: status,
                needsInput: needsInput, agentBusy: agentBusy, issueId: issueId,
                parentSessionId: parentSessionId, resumedFromId: resumedFromId,
                startedAt: startedAt, endedAt: endedAt
            )
        }
    }

    private struct ExpectedChild: Decodable {
        let kind: String
        let issueId: String?
        let identifier: String?
        let title: String?
        let dot: Bool?

        var model: ThreadChild? {
            switch kind {
            case "conversation": .conversation
            case "add": .add
            case "guide": .guide(dot: dot ?? false)
            case "issue": .issue(issueId: issueId ?? "", identifier: identifier ?? "", title: title ?? "")
            default: nil
            }
        }
    }

    private struct ExpectedLink: Decodable {
        let itemId: String
        let issueId: String
        let identifier: String
        let title: String
        let label: String

        var model: ThreadLinkRow {
            ThreadLinkRow(itemId: itemId, issueId: issueId, identifier: identifier, title: title, label: label)
        }
    }

    private struct ExpectedParallelRow: Decodable {
        let runId: String
        let name: String
        let issueId: String?
        let deviceLabel: String?
        let state: String
        let startedAt: String
    }

    private struct ExpectedBlock: Decodable {
        let kind: String
        let runId: String?
        let label: String
        let caption: [String]
        let state: String
        let folded: Bool?
        let runs: [ExpectedParallelRow]?
        let links: [ExpectedLink]

        func model() throws -> ThreadBlock {
            let state = try XCTUnwrap(ThreadRunState(rawValue: state), "unknown state \(state)")
            if kind == "parallel" {
                return .parallel(ThreadParallelBlock(
                    label: label,
                    caption: caption,
                    state: state,
                    runs: try (runs ?? []).map { row in
                        ThreadParallelRow(
                            runId: row.runId,
                            name: row.name,
                            issueId: row.issueId,
                            deviceLabel: row.deviceLabel,
                            state: try XCTUnwrap(ThreadRunState(rawValue: row.state)),
                            startedAt: row.startedAt
                        )
                    },
                    links: links.map(\.model)
                ))
            }
            return .run(ThreadRunBlock(
                runId: runId ?? "",
                label: label,
                caption: caption,
                state: state,
                folded: folded ?? false,
                links: links.map(\.model)
            ))
        }
    }

    private struct ExpectedView: Decodable {
        let title: String
        let titleDerived: Bool
        let strip: [ExpectedChild]
        let headerCaption: String
        let listCaption: String
        let live: Bool
        let needsInput: Bool
        let busy: Bool
        let conversation: [ExpectedBlock]
        let runIds: [String]
        let issueIds: [String]

        func model() throws -> ThreadViewModel {
            ThreadViewModel(
                title: title,
                titleDerived: titleDerived,
                strip: try strip.map { try XCTUnwrap($0.model, "unknown child \($0.kind)") },
                headerCaption: headerCaption,
                listCaption: listCaption,
                live: live,
                needsInput: needsInput,
                busy: busy,
                conversation: try conversation.map { try $0.model() },
                runIds: runIds,
                issueIds: issueIds
            )
        }
    }

    private struct PartitionCase: Decodable {
        let name: String
        let input: [Entry]
        let scope: Scope
        let expected: ExpectedPartition
    }

    private struct Scope: Decodable {
        let teamId: String?
    }

    private struct PrTargetCase: Decodable {
        let name: String
        let input: [Candidate]
        let expected: String?
    }

    private struct Candidate: Decodable {
        let url: String
        let state: String?
        let at: String

        var model: ThreadPrCandidate { ThreadPrCandidate(url: url, state: state, at: at) }
    }

    private struct GuideSliceCase: Decodable {
        let name: String
        let input: GuideSliceInput
        let expected: [JSONValue]
    }

    private struct GuideSliceInput: Decodable {
        let results: JSONValue?
        let sessionId: String
        let firstSessionId: String?
    }

    private struct ExpectedPartition: Decodable {
        let running: [Entry]
        let recent: [Entry]
    }

    private struct Entry: Decodable, Equatable, ThreadListEntry {
        let threadId: String
        let teamId: String
        let updatedAt: String
        let mineLive: Bool
        let mine: Bool
    }

    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/thread-view.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    // MARK: - The fixture (every case)

    func testBuildThreadViewMatchesEveryFixtureCase() throws {
        let cases = try fixture().views
        XCTAssertFalse(cases.isEmpty, "the thread-view fixture must not be empty")
        for testCase in cases {
            let actual = ThreadViews.build(
                thread: testCase.input.thread.model,
                items: testCase.input.items.map(\.model),
                issues: testCase.input.issues.map(\.model),
                runs: testCase.input.runs.map(\.model)
            )
            XCTAssertEqual(actual, try testCase.expected.model(), testCase.name)
        }
    }

    func testPartitionThreadListMatchesEveryFixtureCase() throws {
        let cases = try fixture().partitions
        XCTAssertFalse(cases.isEmpty, "the thread-view fixture must carry partitions")
        for testCase in cases {
            let actual = ThreadViews.partition(testCase.input, teamId: testCase.scope.teamId)
            XCTAssertEqual(actual.running, testCase.expected.running, testCase.name)
            XCTAssertEqual(actual.recent, testCase.expected.recent, testCase.name)
        }
    }

    func testThreadPrTargetMatchesEveryFixtureCase() throws {
        let cases = try fixture().prTargets
        XCTAssertFalse(cases.isEmpty, "the thread-view fixture must carry prTargets")
        for testCase in cases {
            XCTAssertEqual(
                ThreadViews.prTarget(testCase.input.map(\.model)), testCase.expected, testCase.name
            )
        }
    }

    func testSessionGuideEntriesMatchEveryFixtureCase() throws {
        let cases = try fixture().guideSlices
        XCTAssertFalse(cases.isEmpty, "the thread-view fixture must carry guideSlices")
        for testCase in cases {
            let raw = testCase.input.results?.rawText
            let sliced = ThreadViews.sessionGuideEntries(
                raw, sessionId: testCase.input.sessionId, firstSessionId: testCase.input.firstSessionId
            )
            // nil (no Guide) = no entries, like the spec's `[]`.
            let actual = try sliced.map {
                try JSONDecoder().decode([JSONValue].self, from: Data($0.utf8))
            } ?? []
            XCTAssertEqual(actual, testCase.expected, testCase.name)
        }
    }

    // MARK: - iOS-local rules

    private func run(
        _ id: String, user: String, status: String = "running", startedAt: String
    ) -> ThreadViewRun {
        ThreadViewRun(
            id: id, name: id, userId: user, deviceLabel: "macbook", status: status,
            needsInput: false, agentBusy: false, issueId: nil, parentSessionId: nil,
            resumedFromId: nil, startedAt: startedAt, endedAt: nil
        )
    }

    func testReplySendsIntoTheLatestLiveRunOfMine() {
        let runs = [
            run("r1", user: "me", startedAt: "2026-10-12T10:00:00.000Z"),
            run("r2", user: "other", startedAt: "2026-10-12T10:01:00.000Z"),
            run("r3", user: "me", startedAt: "2026-10-12T10:02:00.000Z"),
        ]
        XCTAssertEqual(ThreadViews.replyTarget(runs: runs, me: "me"), .send(runId: "r3"))
    }

    func testReplyResumesTheLatestRunWhenItIsMineAndEnded() {
        let runs = [
            run("r1", user: "other", startedAt: "2026-10-12T10:00:00.000Z"),
            run("r2", user: "me", status: "ended", startedAt: "2026-10-12T10:01:00.000Z"),
        ]
        XCTAssertEqual(ThreadViews.replyTarget(runs: runs, me: "me"), .resume(runId: "r2"))
    }

    func testReplyStartsANewRunOtherwise() {
        let runs = [
            run("r1", user: "me", status: "ended", startedAt: "2026-10-12T10:00:00.000Z"),
            run("r2", user: "other", status: "ended", startedAt: "2026-10-12T10:01:00.000Z"),
        ]
        XCTAssertEqual(ThreadViews.replyTarget(runs: runs, me: "me"), .start)
        XCTAssertEqual(ThreadViews.replyTarget(runs: [], me: "me"), .start)
        XCTAssertEqual(ThreadViews.replyTarget(runs: runs, me: nil), .start)
    }

    func testGuideOfRunPrefersTheThreadsGuide() {
        XCTAssertEqual(ThreadViews.guideResults(threadResults: "[1]", runResults: "[2]"), "[1]")
        XCTAssertEqual(ThreadViews.guideResults(threadResults: nil, runResults: "[2]"), "[2]")
        XCTAssertNil(ThreadViews.guideResults(threadResults: nil, runResults: nil))
    }

    func testHasGuideReadsANonEmptyArray() {
        XCTAssertTrue(ThreadViews.hasGuide(#"[{"topic":"Summary","text":"x"}]"#))
        XCTAssertFalse(ThreadViews.hasGuide("[]"))
        XCTAssertFalse(ThreadViews.hasGuide(nil))
        XCTAssertFalse(ThreadViews.hasGuide("{}"))
        XCTAssertFalse(ThreadViews.hasGuide("not json"))
    }

    func testAChildIsFreshForTenSeconds() {
        let now = WireTimestamps.parse("2026-10-12T10:00:10.000Z")!
        XCTAssertTrue(ThreadViews.isFresh(createdAt: "2026-10-12T10:00:05.000Z", now: now))
        XCTAssertFalse(ThreadViews.isFresh(createdAt: "2026-10-12T10:00:00.000Z", now: now))
        XCTAssertFalse(ThreadViews.isFresh(createdAt: "garbage", now: now))
    }

    // T8/T-N6 over synced rows: the newest OPEN PR (here an older issue's)
    // wins over a newer merged one; the capsule merges only an open PR, and
    // a merged target still names its diff source.
    func testPrTargetResolvesTheNewestOpenPrToItsIssue() throws {
        func issue(_ id: String, pr: String?, state: String?, at: String) -> IssueEntity {
            IssueEntity(
                id: id, boardId: "b1", number: nil, identifier: id.uppercased(), title: id,
                description: nil, status: "backlog", priority: "none", assigneeId: nil,
                creatorId: nil, source: nil, dueDate: nil, sortOrder: nil, completedAt: nil,
                duplicateOfId: nil, prUrl: pr, prNumber: nil, prState: state, branch: nil,
                prMergedAt: nil, createdAt: at, updatedAt: at
            )
        }
        func session(_ id: String, pr: String?, state: String?, at: String) -> CodingSessionEntity {
            CodingSessionEntity(
                id: id, issueId: nil, teamId: "t", userId: "me", deviceLabel: "mac",
                status: "ended", startedAt: at, endedAt: at, createdAt: at, updatedAt: at,
                prUrl: pr, prState: state
            )
        }
        let open = issue("a", pr: "https://x/pull/1", state: "open", at: "2026-10-12T10:01:00.000Z")
        let merged = session("s2", pr: "https://x/pull/2", state: "merged", at: "2026-10-12T10:05:00.000Z")
        let target = try XCTUnwrap(ThreadViews.prTarget(sessions: [merged], issues: [open]))
        XCTAssertEqual(target.url, "https://x/pull/1")
        XCTAssertEqual(target.mergeTarget, .issue(issueId: "a"))

        let onlyMerged = try XCTUnwrap(ThreadViews.prTarget(sessions: [merged], issues: []))
        XCTAssertEqual(onlyMerged.subject, .session(sessionId: "s2"))
        XCTAssertNil(onlyMerged.mergeTarget)
        XCTAssertNil(ThreadViews.prTarget(sessions: [], issues: []))
    }

    // T-N8: the thread page's Guide falls back to its newest session's row.
    func testThreadGuideFallsBackToTheNewestSessionsResults() {
        func session(_ id: String, results: String?) -> CodingSessionEntity {
            CodingSessionEntity(
                id: id, issueId: nil, teamId: "t", userId: "me", deviceLabel: nil,
                status: "ended", results: results, startedAt: "", endedAt: nil,
                createdAt: "", updatedAt: ""
            )
        }
        let sessions = [session("s1", results: "[1]"), session("s2", results: "[2]"), session("s3", results: nil)]
        XCTAssertEqual(ThreadViews.threadGuide(threadResults: "[0]", sessions: sessions), "[0]")
        XCTAssertEqual(ThreadViews.threadGuide(threadResults: nil, sessions: sessions), "[2]")
        XCTAssertNil(ThreadViews.threadGuide(threadResults: nil, sessions: []))
    }

    func testUnlinkCopyNamesTheIssue() {
        XCTAssertEqual(ThreadViews.unlinkLabel("EXP-1270"), "Unlink EXP-1270")
        XCTAssertEqual(ThreadViews.stateWord(.needsInput), "needs input")
    }
}
