import Foundation
import XCTest
@testable import ExpCore

// EXP-1291 Threads v2: the session tree's view models, locked ×4 (TS
// `session-tree.test.ts`, desktop `domain::session_tree`, Android
// `SessionTreeTest`) against the contract fixtures session-card.json,
// session-thread.json and threads-list.json. Every case compares the WHOLE
// expected object: the Swift result is rendered back into the reference's
// JSON shape and compared as a JSON value.
final class SessionTreeTests: XCTestCase {
    // MARK: - Fixture plumbing

    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    private func fixture(_ name: String) throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/\(name)")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ object: [String: Any], _ key: String = "cases") throws -> [[String: Any]] {
        let list = try XCTUnwrap(object[key] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty)
        return list
    }

    private func string(_ value: Any?) -> String? { value as? String }

    private func date(_ value: Any?) throws -> Date {
        try XCTUnwrap(WireTimestamps.parse(try XCTUnwrap(value as? String)))
    }

    private func session(_ object: Any?) throws -> TreeSession {
        let row = try XCTUnwrap(object as? [String: Any])
        let blocked = (row["blocked"] as? [String: Any]).map {
            TreeSessionBlocked(window: $0["window"] as? String, resetsAt: $0["resetsAt"] as? String)
        }
        let guide = (row["guide"] as? [[String: Any]]).map { entries in
            entries.map { TreeGuideEntry(text: $0["text"] as? String, label: $0["label"] as? String) }
        }
        return TreeSession(
            id: try XCTUnwrap(string(row["id"])),
            teamId: try XCTUnwrap(string(row["teamId"])),
            parentId: string(row["parentId"]),
            rootId: try XCTUnwrap(string(row["rootId"])),
            kind: try XCTUnwrap(string(row["kind"])),
            userId: try XCTUnwrap(string(row["userId"])),
            status: try XCTUnwrap(string(row["status"])),
            busy: try XCTUnwrap(row["busy"] as? Bool),
            needsInput: try XCTUnwrap(row["needsInput"] as? Bool),
            blocked: blocked,
            shared: try XCTUnwrap(row["shared"] as? Bool),
            title: string(row["title"]),
            guide: guide,
            actionName: string(row["actionName"]),
            name: string(row["name"]),
            icon: string(row["icon"]),
            color: string(row["color"]),
            sortOrder: try XCTUnwrap(row["sortOrder"] as? Double),
            startedAt: string(row["startedAt"]),
            endedAt: string(row["endedAt"]),
            createdAt: try XCTUnwrap(string(row["createdAt"])),
            updatedAt: try XCTUnwrap(string(row["updatedAt"]))
        )
    }

    private func sessionIssues(_ value: Any?) throws -> [TreeSessionIssue] {
        try XCTUnwrap(value as? [[String: Any]]).map {
            TreeSessionIssue(
                sessionId: try XCTUnwrap(string($0["sessionId"])),
                issueId: try XCTUnwrap(string($0["issueId"])),
                sortOrder: try XCTUnwrap($0["sortOrder"] as? Double)
            )
        }
    }

    private func issues(_ value: Any?) throws -> [TreeIssue] {
        try XCTUnwrap(value as? [[String: Any]]).map {
            TreeIssue(
                id: try XCTUnwrap(string($0["id"])),
                identifier: try XCTUnwrap(string($0["identifier"])),
                title: try XCTUnwrap(string($0["title"]))
            )
        }
    }

    private func messages(_ value: Any?) throws -> [TreeMessage] {
        try XCTUnwrap(value as? [[String: Any]]).map {
            TreeMessage(
                id: try XCTUnwrap(string($0["id"])),
                sessionId: try XCTUnwrap(string($0["sessionId"])),
                authorKind: try XCTUnwrap(string($0["authorKind"])),
                authorId: string($0["authorId"]),
                body: try XCTUnwrap(string($0["body"])),
                card: ($0["card"] as? [String: Any]).map(TreeMessageCard.init(object:)),
                answeredAt: string($0["answeredAt"]),
                consumedAt: string($0["consumedAt"]),
                createdAt: try XCTUnwrap(string($0["createdAt"]))
            )
        }
    }

    private func pullRequests(_ value: Any?) throws -> [TreePullRequest] {
        try XCTUnwrap(value as? [[String: Any]]).map {
            TreePullRequest(
                id: try XCTUnwrap(string($0["id"])),
                number: try XCTUnwrap($0["number"] as? Int)
            )
        }
    }

    // MARK: - Back into the reference's JSON shape

    private func json(_ value: String?) -> Any { value ?? NSNull() }
    private func json(_ value: Double?) -> Any { value ?? NSNull() }

    private func json(_ card: SessionCard) -> [String: Any] {
        [
            "sessionId": card.sessionId,
            "icon": card.icon,
            "name": card.name,
            "kindWord": card.kindWord,
            "state": card.state.rawValue,
            "tone": card.tone,
            "elapsed": json(card.elapsed),
            "running": card.running,
            "testId": card.testId,
        ]
    }

    private func json(_ tab: ThreadTab) -> [String: Any] {
        switch tab {
        case .conversation: ["kind": "conversation"]
        case let .issue(issueId, identifier): ["kind": "issue", "issueId": issueId, "identifier": identifier]
        case .add: ["kind": "add"]
        case let .guide(dot): ["kind": "guide", "dot": dot]
        }
    }

    private func json(_ row: ThreadRow) -> [String: Any] {
        switch row {
        case let .message(id, author, authorId, body, queued):
            ["type": "message", "id": id, "author": author, "authorId": json(authorId), "body": body, "queued": queued]
        case let .session(id, card):
            ["type": "session", "id": id, "card": json(card)]
        case let .question(id, question, options, answer, answered):
            ["type": "question", "id": id, "question": question, "options": options, "answer": json(answer), "answered": answered]
        case let .plan(id, plan, decision):
            ["type": "plan", "id": id, "plan": plan, "decision": json(decision)]
        case let .subagent(id, title, status):
            ["type": "subagent", "id": id, "title": title, "status": status]
        case let .picture(id, attachmentId, width, height, caption):
            ["type": "picture", "id": id, "attachmentId": attachmentId, "width": json(width), "height": json(height), "caption": json(caption)]
        case let .event(id, event, text):
            ["type": "event", "id": id, "event": event, "text": text]
        }
    }

    private func json(_ view: SessionThreadView) -> [String: Any] {
        [
            "rootId": view.rootId,
            "title": view.title,
            "titleDerived": view.titleDerived,
            "private": view.isPrivate,
            "tabs": view.tabs.map(json),
            "rows": view.rows.map(json),
            "composer": ["sessionId": view.composer.sessionId, "placeholder": view.composer.placeholder],
            "live": view.live,
            "busy": view.busy,
            "needsYou": view.needsYou,
        ]
    }

    private func json(_ group: ThreadsListGroup) -> [String: Any] {
        [
            "key": group.key.rawValue,
            "title": group.title,
            "rows": group.rows.map { row -> [String: Any] in
                [
                    "sessionId": row.sessionId,
                    "teamId": row.teamId,
                    "title": row.title,
                    "private": row.isPrivate,
                    "busy": row.busy,
                    "needsYou": row.needsYou,
                    "activityAt": row.activityAt,
                ]
            },
        ]
    }

    private func json(_ agent: AgentRow) -> [String: Any] {
        [
            "sessionId": agent.sessionId,
            "teamId": agent.teamId,
            "name": agent.name,
            "icon": agent.icon,
            "color": agent.color,
            "busy": agent.busy,
            "needsYou": agent.needsYou,
        ]
    }

    private func assertJSON(_ actual: Any, _ expected: Any?, _ name: String, file: StaticString = #filePath, line: UInt = #line) {
        let lhs = actual as AnyObject
        let rhs = (expected ?? NSNull()) as AnyObject
        XCTAssertTrue(lhs.isEqual(rhs), "\(name)\n actual: \(actual)\n expected: \(String(describing: expected))", file: file, line: line)
    }

    // MARK: - session-card.json

    func testSessionCardCopyMatchesTheFixture() throws {
        let fixture = try fixture("session-card.json")
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        XCTAssertEqual(copy["session"] as? String, SessionTree.CardCopy.session)
        let states = try XCTUnwrap(copy["states"] as? [String: String])
        let icons = try XCTUnwrap(fixture["icons"] as? [String: String])
        let tones = try XCTUnwrap(fixture["tones"] as? [String: String])
        XCTAssertEqual(Set(states.keys), Set(SessionState.allCases.map(\.rawValue)))
        for state in SessionState.allCases {
            XCTAssertEqual(states[state.rawValue], state.word, state.rawValue)
            XCTAssertEqual(icons[state.rawValue], state.icon, state.rawValue)
            XCTAssertEqual(tones[state.rawValue], state.tone, state.rawValue)
        }
        XCTAssertEqual((fixture["testIds"] as? [String: String])?["card"], SessionTree.CardCopy.testId)
    }

    func testSessionCardReplaysEveryCase() throws {
        for testCase in try cases(try fixture("session-card.json")) {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let card = SessionTree.sessionCard(
                try session(input["session"]),
                ordinal: try XCTUnwrap(input["ordinal"] as? Int),
                now: try date(input["now"])
            )
            assertJSON(json(card), testCase["expected"], name)
        }
    }

    func testFormatsTheElapsedLadder() {
        XCTAssertEqual(SessionTree.formatElapsed(ms: 0), "0s")
        XCTAssertEqual(SessionTree.formatElapsed(ms: -5_000), "0s")
        XCTAssertEqual(SessionTree.formatElapsed(ms: 37_999), "37s")
        XCTAssertEqual(SessionTree.formatElapsed(ms: 124_000), "2m 04s")
        XCTAssertEqual(SessionTree.formatElapsed(ms: 3_780_000), "1h 03m")
    }

    // MARK: - session-thread.json

    func testThreadCopyMatchesTheFixture() throws {
        let fixture = try fixture("session-thread.json")
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        XCTAssertEqual(copy["untitled"] as? String, SessionTree.ThreadCopy.untitled)
        XCTAssertEqual(copy["placeholder"] as? String, SessionTree.ThreadCopy.placeholder)
        XCTAssertEqual(copy["conversation"] as? String, SessionTree.ThreadCopy.conversation)
        XCTAssertEqual(copy["guide"] as? String, SessionTree.ThreadCopy.guide)
        XCTAssertEqual(copy["events"] as? [String: String], SessionTree.ThreadCopy.events)
        XCTAssertEqual(fixture["titleMax"] as? Int, SessionTree.ThreadCopy.titleMax)
        let testIds = try XCTUnwrap(fixture["testIds"] as? [String: String])
        XCTAssertEqual(testIds["tabs"], SessionTree.ThreadCopy.tabsTestId)
        XCTAssertEqual(testIds["conversation"], SessionTree.ThreadCopy.conversationTestId)
        XCTAssertEqual(testIds["composer"], SessionTree.ThreadCopy.composerTestId)
        XCTAssertEqual(copy["stack"] as? String, SessionTree.ThreadCopy.stack)
        XCTAssertEqual(testIds["guideStack"], SessionTree.ThreadCopy.guideStackTestId)
        XCTAssertEqual(testIds["guideMerge"], SessionTree.ThreadCopy.guideMergeTestId)
        XCTAssertEqual(testIds["guideMenu"], SessionTree.ThreadCopy.guideMenuTestId)
    }

    /// EXP-1306: `guidePullRequestCases` — the expected PR id, or none.
    func testGuidePullRequestReplaysEveryCase() throws {
        for testCase in try cases(try fixture("session-thread.json"), "guidePullRequestCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let rows = try XCTUnwrap(testCase["pullRequests"] as? [[String: Any]]).map { row in
                PullRequestEntity(
                    id: try XCTUnwrap(string(row["id"])), teamId: "team", repositoryId: "repo",
                    number: try XCTUnwrap(row["number"] as? Int), url: "", head: "", base: "",
                    state: try XCTUnwrap(string(row["state"])),
                    createdAt: try XCTUnwrap(string(row["createdAt"])), updatedAt: ""
                )
            }
            XCTAssertEqual(SessionTree.guidePullRequest(rows)?.id, string(testCase["expected"]), name)
        }
    }

    /// EXP-1307: `guideMenuCases` — the header `…` entries, in order.
    func testGuideMenuEntriesReplaysEveryCase() throws {
        for testCase in try cases(try fixture("session-thread.json"), "guideMenuCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            XCTAssertEqual(
                SessionTree.guideMenuEntries(
                    state: string(input["state"]),
                    member: try XCTUnwrap(input["member"] as? Bool)
                ).map(\.rawValue),
                try XCTUnwrap(testCase["expected"] as? [String]),
                name
            )
        }
    }

    /// EXP-1306: `composerCases` — the card takes the composer's place.
    func testComposerYieldsToCardReplaysEveryCase() throws {
        for testCase in try cases(try fixture("session-thread.json"), "composerCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            XCTAssertEqual(
                SessionTree.composerYieldsToCard(
                    live: try XCTUnwrap(input["live"] as? Bool),
                    pendingAsk: try XCTUnwrap(input["pendingAsk"] as? Bool)
                ),
                try XCTUnwrap(testCase["expected"] as? Bool),
                name
            )
        }
    }

    func testBuildSessionThreadReplaysEveryCase() throws {
        for testCase in try cases(try fixture("session-thread.json")) {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let view = SessionTree.buildSessionThread(
                root: try session(input["root"]),
                sessions: try XCTUnwrap(input["sessions"] as? [Any]).map(session),
                sessionIssues: try sessionIssues(input["sessionIssues"]),
                issues: try issues(input["issues"]),
                messages: try messages(input["messages"]),
                pullRequests: try pullRequests(input["pullRequests"]),
                now: try date(input["now"])
            )
            assertJSON(json(view), testCase["expected"], name)
        }
    }

    // MARK: - threads-list.json

    func testThreadsListCopyMatchesTheFixture() throws {
        let fixture = try fixture("threads-list.json")
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        XCTAssertEqual(copy["title"] as? String, SessionTree.ListCopy.title)
        XCTAssertEqual(copy["agents"] as? String, SessionTree.ListCopy.agents)
        XCTAssertEqual(copy["empty"] as? String, SessionTree.ListCopy.empty)
        let groups = try XCTUnwrap(copy["groups"] as? [String: String])
        XCTAssertEqual(Set(groups.keys), Set(ThreadsListGroup.Key.allCases.map(\.rawValue)))
        for key in ThreadsListGroup.Key.allCases {
            XCTAssertEqual(groups[key.rawValue], key.title, key.rawValue)
        }
        let defaults = try XCTUnwrap(fixture["agentDefaults"] as? [String: String])
        XCTAssertEqual(defaults["icon"], SessionTree.ListCopy.agentDefaultIcon)
        XCTAssertEqual(defaults["color"], SessionTree.ListCopy.agentDefaultColor)
        let testIds = try XCTUnwrap(fixture["testIds"] as? [String: String])
        XCTAssertEqual(testIds["list"], SessionTree.ListCopy.listTestId)
        XCTAssertEqual(testIds["row"], SessionTree.ListCopy.rowTestId)
        XCTAssertEqual(testIds["agentRow"], SessionTree.ListCopy.agentRowTestId)
    }

    func testPartitionThreadsReplaysEveryCase() throws {
        for testCase in try cases(try fixture("threads-list.json")) {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let groups = SessionTree.partitionThreads(
                sessions: try XCTUnwrap(input["sessions"] as? [Any]).map(session),
                sessionIssues: try sessionIssues(input["sessionIssues"]),
                issues: try issues(input["issues"]),
                messages: try messages(input["messages"]),
                teamId: string(input["teamId"])
            )
            assertJSON(groups.map(json), testCase["expected"], name)
        }
    }

    func testListAgentsReplaysEveryCase() throws {
        for testCase in try cases(try fixture("threads-list.json"), "agentCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let agents = SessionTree.listAgents(
                try XCTUnwrap(input["sessions"] as? [Any]).map(session),
                teamId: string(input["teamId"])
            )
            assertJSON(agents.map(json), testCase["expected"], name)
        }
    }

    // MARK: - Board switcher dots

    func testCountsOwnWorkingSessionsPerTeam() throws {
        func row(_ id: String, team: String, user: String = "me", status: String, needsInput: Bool = false) -> TreeSession {
            TreeSession(
                id: id, teamId: team, parentId: nil, rootId: id, kind: "thread", userId: user,
                status: status, busy: false, needsInput: needsInput, blocked: nil, shared: true,
                title: nil, guide: nil, actionName: nil, name: nil, icon: nil, color: nil,
                sortOrder: 0, startedAt: nil, endedAt: nil,
                createdAt: "2026-10-13T10:00:00.000Z", updatedAt: "2026-10-13T10:00:00.000Z"
            )
        }
        let sessions = [
            row("a", team: "t1", status: "running"),
            row("b", team: "t1", status: "queued"),
            row("c", team: "t1", status: "idle"),
            row("d", team: "t2", status: "idle", needsInput: true),
            row("e", team: "t2", status: "ended", needsInput: true),
            row("f", team: "t3", user: "them", status: "running"),
        ]
        XCTAssertEqual(SessionTree.liveByTeam(sessions, userId: "me"), [
            "t1": .init(count: 2, needsInput: false),
            "t2": .init(count: 0, needsInput: true),
        ])
        XCTAssertEqual(SessionTree.liveByTeam(sessions, userId: nil), [:])
    }

    // MARK: - Synced jsonb text

    func testReadsTheSyncedJsonbText() {
        XCTAssertNil(TreeSessionBlocked.parse(nil))
        XCTAssertNil(TreeSessionBlocked.parse("null"))
        XCTAssertEqual(
            TreeSessionBlocked.parse(#"{"window":"weekly","resetsAt":null}"#),
            TreeSessionBlocked(window: "weekly")
        )
        XCTAssertNil(TreeGuideEntry.parseList("null"))
        XCTAssertEqual(
            TreeGuideEntry.parseList(#"[{"topic":"Summary","text":"Done","label":null},3]"#),
            [TreeGuideEntry(text: "Done")]
        )
        XCTAssertNil(TreeMessageCard.parse(nil))
        XCTAssertEqual(
            TreeMessageCard.parse(#"{"kind":"question","question":"Ship?","options":["Yes","No"]}"#),
            TreeMessageCard(kind: "question", question: "Ship?", options: ["Yes", "No"])
        )
    }
}
