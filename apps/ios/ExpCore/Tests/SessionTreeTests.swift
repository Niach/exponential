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
            TreeSessionBlocked(
                kind: $0["kind"] as? String, window: $0["window"] as? String, resetsAt: $0["resetsAt"] as? String
            )
        }
        let guide = (row["guide"] as? [[String: Any]]).map { entries in
            entries.map { TreeGuideEntry(text: $0["text"] as? String, label: $0["label"] as? String) }
        }
        return TreeSession(
            id: try XCTUnwrap(string(row["id"])),
            teamId: try XCTUnwrap(string(row["teamId"])),
            parentId: string(row["parentId"]),
            rootId: try XCTUnwrap(string(row["rootId"])),
            userId: try XCTUnwrap(string(row["userId"])),
            status: try XCTUnwrap(string(row["status"])),
            busy: try XCTUnwrap(row["busy"] as? Bool),
            needsInput: try XCTUnwrap(row["needsInput"] as? Bool),
            blocked: blocked,
            shared: try XCTUnwrap(row["shared"] as? Bool),
            title: string(row["title"]),
            guide: guide,
            startedAt: string(row["startedAt"]),
            endedAt: string(row["endedAt"]),
            createdAt: try XCTUnwrap(string(row["createdAt"])),
            updatedAt: try XCTUnwrap(string(row["updatedAt"])),
            deviceId: string(row["deviceId"]),
            agentId: string(row["agentId"])
        )
    }

    private func reads(_ value: Any?) throws -> [TreeSessionRead] {
        try ((value as? [[String: Any]]) ?? []).map {
            TreeSessionRead(
                sessionId: try XCTUnwrap(string($0["sessionId"])),
                readAt: try XCTUnwrap(string($0["readAt"]))
            )
        }
    }

    private func listRow(_ row: [String: Any]) throws -> ThreadsListRow {
        ThreadsListRow(
            sessionId: try XCTUnwrap(string(row["sessionId"])),
            teamId: try XCTUnwrap(string(row["teamId"])),
            depth: (row["depth"] as? Int) ?? 0,
            parentId: string(row["parentId"]),
            rootId: string(row["rootId"]),
            title: try XCTUnwrap(string(row["title"])),
            busy: try XCTUnwrap(row["busy"] as? Bool),
            needsYou: try XCTUnwrap(row["needsYou"] as? Bool),
            unread: (row["unread"] as? Bool) ?? false,
            activityAt: try XCTUnwrap(string(row["activityAt"])),
            deviceId: string(row["deviceId"])
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
                title: try XCTUnwrap(string($0["title"])),
                parentId: string($0["parentId"])
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
                createdAt: try XCTUnwrap(string($0["createdAt"])),
                authorSessionId: string($0["authorSessionId"])
            )
        }
    }

    private func pullRequests(_ value: Any?) throws -> [TreePullRequest] {
        try XCTUnwrap(value as? [[String: Any]]).map {
            TreePullRequest(
                id: try XCTUnwrap(string($0["id"])),
                number: try XCTUnwrap($0["number"] as? Int),
                state: string($0["state"])
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
        case let .issue(issueId, identifier, nested):
            [
                "kind": "issue", "issueId": issueId, "identifier": identifier,
                "nested": nested.map { ["issueId": $0.issueId, "identifier": $0.identifier, "depth": $0.depth] as [String: Any] },
            ]
        case .add: ["kind": "add"]
        case let .session(sessionId, name, markState, nested):
            [
                "kind": "session", "sessionId": sessionId, "name": name, "markState": markState.rawValue,
                "nested": nested.map {
                    ["sessionId": $0.sessionId, "name": $0.name, "markState": $0.markState.rawValue, "depth": $0.depth] as [String: Any]
                },
            ]
        case let .guide(dot): ["kind": "guide", "dot": dot]
        }
    }

    private func json(_ option: TreeQuestionOption) -> [String: Any] {
        ["label": option.label, "description": json(option.description), "attachmentId": json(option.attachmentId)]
    }

    private func json(_ surface: CardSurface?) -> Any {
        guard let surface else { return NSNull() }
        return ["root": surface.root.any, "data": surface.data?.any ?? NSNull()] as [String: Any]
    }

    private func json(_ row: ThreadRow) -> [String: Any] {
        switch row {
        case let .message(id, author, authorId, body, queued, authorSessionId):
            ["type": "message", "id": id, "author": author, "authorId": json(authorId), "body": body, "queued": queued, "authorSessionId": json(authorSessionId)]
        case let .session(id, card):
            ["type": "session", "id": id, "card": json(card)]
        case let .report(id, sessionId, text, summary):
            ["type": "report", "id": id, "sessionId": sessionId, "text": text, "summary": summary]
        case let .suggestions(id, sessionId, header, startAll, items):
            [
                "type": "suggestions", "id": id, "sessionId": sessionId, "header": header, "startAll": startAll,
                "items": items.map { item -> [String: Any] in
                    [
                        "id": item.id, "title": item.title, "text": item.text, "state": item.state.rawValue,
                        "card": item.card.map(json) ?? NSNull(), "issueId": json(item.issueId),
                        "identifier": json(item.identifier),
                    ]
                },
            ]
        case let .question(row):
            [
                "type": "question", "id": row.id, "sessionId": row.sessionId, "question": row.question,
                "options": row.options.map(json), "custom": row.custom, "multi": row.multi,
                "surface": json(row.surface), "answer": json(row.answer),
                "answers": row.answers.map { $0 as Any } ?? NSNull(), "surfaceData": row.surfaceData?.any ?? NSNull(),
                "answered": row.answered,
            ]
        case let .plan(row):
            [
                "type": "plan", "id": row.id, "sessionId": row.sessionId, "plan": row.plan,
                "questions": row.questions.map { question -> [String: Any] in
                    [
                        "id": question.id, "prompt": question.prompt, "multi": question.multi,
                        "custom": question.custom, "options": question.options.map(json),
                    ]
                },
                "surface": json(row.surface), "decision": json(row.decision), "fresh": row.fresh,
                "answers": row.answers.map { $0 as Any } ?? NSNull(), "feedback": json(row.feedback),
                "surfaceData": row.surfaceData?.any ?? NSNull(),
            ]
        case let .subagent(id, title, status):
            ["type": "subagent", "id": id, "title": title, "status": status]
        case let .picture(id, attachmentId, width, height, caption):
            ["type": "picture", "id": id, "attachmentId": attachmentId, "width": json(width), "height": json(height), "caption": json(caption)]
        case let .event(id, event, text):
            ["type": "event", "id": id, "event": event, "text": text]
        case let .divider(id, kind, at):
            ["type": "divider", "id": id, "kind": kind.rawValue, "at": at]
        case let .surface(id, sessionId, title, surface):
            ["type": "surface", "id": id, "sessionId": sessionId, "title": json(title), "surface": json(surface)]
        }
    }

    private func json(_ view: SessionThreadView) -> [String: Any] {
        [
            "rootId": view.rootId,
            "title": view.title,
            "private": view.isPrivate,
            "focusId": json(view.focusId),
            "tabs": view.tabs.map(json),
            "rows": view.rows.map(json),
            "composer": ["sessionId": view.composer.sessionId, "placeholder": view.composer.placeholder],
            "live": view.live,
            "liveSessionId": json(view.liveSessionId),
            "busy": view.busy,
            "needsYou": view.needsYou,
            "mainSessionId": json(view.mainSessionId),
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
                    "depth": row.depth,
                    "parentId": json(row.parentId),
                    "rootId": row.rootId,
                    "title": row.title,
                    "busy": row.busy,
                    "needsYou": row.needsYou,
                    "unread": row.unread,
                    "activityAt": row.activityAt,
                    "deviceId": json(row.deviceId),
                ]
            },
        ]
    }

    private func json(_ agent: AgentRow) -> [String: Any] {
        [
            "agentId": agent.agentId,
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
        XCTAssertNil(copy["session"])
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
        XCTAssertEqual(copy["blockedKinds"] as? [String: String], SessionTree.CardCopy.blockedKinds)
    }

    func testSessionCardReplaysEveryCase() throws {
        for testCase in try cases(try fixture("session-card.json")) {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let card = SessionTree.sessionCard(
                try session(input["session"]),
                title: try XCTUnwrap(input["title"] as? String),
                now: try date(input["now"])
            )
            assertJSON(json(card), testCase["expected"], name)
        }
    }

    /// NEW-1: a blocked session's card names its kind (`copy.blockedKinds`);
    /// an unknown or absent kind keeps `states.blocked` (TS
    /// `sessionKindWord`).
    func testABlockedCardNamesItsKind() throws {
        let fixture = try fixture("session-card.json")
        let base = try XCTUnwrap(try cases(fixture).first {
            (($0["expected"] as? [String: Any])?["state"] as? String) == "blocked"
        })
        let input = try XCTUnwrap(base["input"] as? [String: Any])
        var row = try XCTUnwrap(input["session"] as? [String: Any])
        let title = try XCTUnwrap(input["title"] as? String)
        let now = try date(input["now"])
        func word(_ blocked: [String: Any]) throws -> String {
            row["blocked"] = blocked
            return SessionTree.sessionCard(try session(row), title: title, now: now).kindWord
        }
        let kinds = try XCTUnwrap((fixture["copy"] as? [String: Any])?["blockedKinds"] as? [String: String])
        XCTAssertEqual(try word(["kind": "rate_limit"]), kinds["rate_limit"])
        XCTAssertEqual(try word(["kind": "device_offline"]), "Device offline")
        XCTAssertEqual(try word(["kind": "unclaimed"]), "Not picked up")
        XCTAssertEqual(try word(["kind": "stuck"]), "Stuck")
        XCTAssertEqual(try word(["kind": "future_kind"]), SessionState.blocked.word)
        XCTAssertEqual(try word(["window": "weekly"]), SessionState.blocked.word)
        // The jsonb parse carries the kind.
        XCTAssertEqual(TreeSessionBlocked.parse(#"{"kind":"stuck"}"#)?.kind, "stuck")
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
        XCTAssertNil(fixture["titles"])
        XCTAssertEqual(copy["events"] as? [String: String], SessionTree.ThreadCopy.events)
        XCTAssertNil(fixture["titleMax"])
        let testIds = try XCTUnwrap(fixture["testIds"] as? [String: String])
        XCTAssertEqual(testIds["tabs"], SessionTree.ThreadCopy.tabsTestId)
        XCTAssertEqual(testIds["conversation"], SessionTree.ThreadCopy.conversationTestId)
        XCTAssertEqual(testIds["composer"], SessionTree.ThreadCopy.composerTestId)
        XCTAssertEqual(copy["stack"] as? String, SessionTree.ThreadCopy.stack)
        XCTAssertEqual(testIds["guideStack"], SessionTree.ThreadCopy.guideStackTestId)
        XCTAssertEqual(testIds["guideMerge"], SessionTree.ThreadCopy.guideMergeTestId)
        XCTAssertEqual(testIds["guideMenu"], SessionTree.ThreadCopy.guideMenuTestId)
        XCTAssertEqual(copy["report"] as? String, SessionTree.ThreadCopy.report)
        typealias S = SessionTree.SuggestionsCopy
        XCTAssertEqual(copy["suggestions"] as? [String: String], [
            "one": S.one, "many": S.many, "started": S.started, "startAll": S.startAll,
            "start": S.start, "here": S.here, "options": S.options, "startOn": S.startOn,
            "startWith": S.startWith, "file": S.file, "dismiss": S.dismiss,
            "sentHere": S.sentHere, "filed": S.filed,
        ])
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

    /// EXP-1211: `repliesCases` — the quick replies above the composer.
    func testComposerRepliesReplaysEveryCase() throws {
        let fixture = try fixture("session-thread.json")
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        XCTAssertEqual(copy["replies"] as? [String: String], [
            "label": SessionTree.RepliesCopy.label, "dismiss": SessionTree.RepliesCopy.dismiss,
        ])
        XCTAssertEqual((fixture["testIds"] as? [String: String])?["replies"], SessionTree.RepliesCopy.testId)
        let all = try cases(fixture, "repliesCases")
        XCTAssertFalse(all.isEmpty)
        for testCase in all {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let actual = SessionTree.composerReplies(
                sessionId: try XCTUnwrap(string(input["sessionId"])),
                messages: try messages(input["messages"])
            )
            let expected = try (testCase["expected"] as? [String: Any]).map { value in
                ComposerReplies(
                    messageId: try XCTUnwrap(string(value["messageId"])),
                    items: try XCTUnwrap(value["items"] as? [[String: Any]]).map {
                        ComposerReply(id: try XCTUnwrap(string($0["id"])), text: try XCTUnwrap(string($0["text"])))
                    }
                )
            }
            XCTAssertEqual(actual, expected, name)
        }
    }

    /// EXP-1211: a replies card is never a Conversation row.
    func testRepliesCardIsNeverAThreadRow() throws {
        let root = TreeSession(
            id: "r1", teamId: "team", parentId: nil, rootId: "r1", userId: "u1",
            status: "idle", busy: false, needsInput: false, blocked: nil, shared: true,
            title: "Thread", guide: nil, startedAt: nil, endedAt: nil,
            createdAt: "2026-10-17T10:00:00.000Z", updatedAt: "2026-10-17T10:00:00.000Z"
        )
        let card = TreeMessageCard(kind: "replies", items: [TreeSuggestionItem(id: "a", title: "", text: "Open the PR")])
        let message = TreeMessage(
            id: "m1", sessionId: "r1", authorKind: "agent", authorId: nil, body: "",
            card: card, answeredAt: nil, consumedAt: nil, createdAt: "2026-10-17T10:00:01.000Z", authorSessionId: nil
        )
        let view = SessionTree.buildSessionThread(
            root: root, sessions: [root], sessionIssues: [], issues: [], messages: [message], pullRequests: [], now: Date()
        )
        XCTAssertEqual(view.rows, [])
    }

    /// EXP-1334: `stripCases` — the never-scrolling strip's partition.
    func testPartitionThreadStripReplaysEveryCase() throws {
        for testCase in try cases(try fixture("session-thread.json"), "stripCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let issues = try XCTUnwrap(input["issues"] as? [[String: Any]]).map { issue in
                (full: try XCTUnwrap(issue["full"] as? Double), short: try XCTUnwrap(issue["short"] as? Double))
            }
            let layout = SessionTree.partitionThreadStrip(
                issues: issues,
                sessions: try XCTUnwrap(input["sessions"] as? [Double]),
                available: try XCTUnwrap(input["available"] as? Double),
                gap: try XCTUnwrap(input["gap"] as? Double),
                overflowW: try XCTUnwrap(input["overflowW"] as? Double),
                activeIssue: input["activeIssue"] as? Int,
                activeSession: input["activeSession"] as? Int
            )
            assertJSON(
                ["tier": layout.tier.rawValue, "issues": layout.issues, "sessions": layout.sessions] as [String: Any],
                testCase["expected"],
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
                now: try date(input["now"]),
                focus: string((testCase["options"] as? [String: Any])?["focus"])
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
        XCTAssertEqual(copy["rail"] as? String, SessionTree.ListCopy.rail)
        let words = try XCTUnwrap(copy["words"] as? [String: String])
        XCTAssertEqual(Set(words.keys), Set(ThreadRowWord.allCases.filter { $0.text != nil }.map(\.rawValue)))
        for word in ThreadRowWord.allCases {
            XCTAssertEqual(words[word.rawValue], word.text, word.rawValue)
        }
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
                teamId: string(input["teamId"]),
                reads: try reads(input["reads"])
            )
            assertJSON(groups.map(json), testCase["expected"], name)
        }
    }

    /// NEW-1: `recent` + `treeState` rules match the fixture.
    func testListRulesMatchTheFixture() throws {
        let fixture = try fixture("threads-list.json")
        let recent = try XCTUnwrap(fixture["recent"] as? [String: Any])
        XCTAssertEqual(recent["hours"] as? Double, SessionTree.ListRules.recentHours)
        XCTAssertEqual(recent["max"] as? Int, SessionTree.ListRules.recentMax)
        let state = try XCTUnwrap(fixture["treeState"] as? [String: Any])
        XCTAssertEqual(state["staleHours"] as? Double, SessionTree.ListRules.staleHours)
    }

    /// NEW-1: `recentCases` — expected = the Recent group's ROOT ids in order.
    func testRecentWindowReplaysEveryCase() throws {
        for testCase in try cases(try fixture("threads-list.json"), "recentCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let groups = SessionTree.partitionThreads(
                sessions: try XCTUnwrap(input["sessions"] as? [Any]).map(session),
                sessionIssues: try sessionIssues(input["sessionIssues"]),
                issues: try issues(input["issues"]),
                messages: try messages(input["messages"]),
                teamId: string(input["teamId"]),
                reads: try reads(input["reads"]),
                now: string(input["now"])
            )
            let recent = groups.first { $0.key == .recent }?.rows.filter { $0.depth == 0 }.map(\.sessionId) ?? []
            XCTAssertEqual(recent, try XCTUnwrap(testCase["expected"] as? [String]), name)
        }
    }

    /// NEW-1: `treeStateCases`.
    func testTreeStateReplaysEveryCase() throws {
        for testCase in try cases(try fixture("threads-list.json"), "treeStateCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let state = SessionTree.treeState(
                try XCTUnwrap(input["tree"] as? [Any]).map(session),
                pullRequests: try pullRequests(input["pullRequests"]),
                now: string(input["now"])
            )
            XCTAssertEqual(state.rawValue, try XCTUnwrap(testCase["expected"] as? String), name)
        }
    }

    /// EXP-1323: `unreadCases` — the Threads nav dot.
    func testThreadsUnreadReplaysEveryCase() throws {
        for testCase in try cases(try fixture("threads-list.json"), "unreadCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let groups = try XCTUnwrap(testCase["groups"] as? [[String: Any]]).map { group in
                ThreadsListGroup(
                    key: try XCTUnwrap(ThreadsListGroup.Key(rawValue: try XCTUnwrap(string(group["key"])))),
                    title: try XCTUnwrap(string(group["title"])),
                    rows: try XCTUnwrap(group["rows"] as? [[String: Any]]).map(listRow)
                )
            }
            XCTAssertEqual(SessionTree.threadsUnread(groups), try XCTUnwrap(testCase["expected"] as? Bool), name)
        }
    }

    func testListAgentsReplaysEveryCase() throws {
        for testCase in try cases(try fixture("threads-list.json"), "agentCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let definitions = try XCTUnwrap(input["agents"] as? [[String: Any]]).map { row in
                AgentLike(
                    id: try XCTUnwrap(string(row["id"])),
                    teamId: try XCTUnwrap(string(row["teamId"])),
                    name: try XCTUnwrap(string(row["name"])),
                    icon: string(row["icon"]),
                    color: string(row["color"]),
                    sortOrder: try XCTUnwrap(row["sortOrder"] as? Double)
                )
            }
            let agents = SessionTree.listAgents(
                agents: definitions,
                sessions: try XCTUnwrap(input["sessions"] as? [Any]).map(session),
                teamId: string(input["teamId"]),
                me: string(input["me"])
            )
            assertJSON(agents.map(json), testCase["expected"], name)
        }
    }

    func testThreadRowMarkStateReplaysEveryCase() throws {
        for testCase in try cases(try fixture("threads-list.json"), "markCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Bool])
            let state = SessionTree.threadRowMarkState(
                needsYou: try XCTUnwrap(input["needsYou"]),
                busy: try XCTUnwrap(input["busy"])
            )
            XCTAssertEqual(state.rawValue, testCase["expected"] as? String, name)
        }
    }

    private func stackPrs(_ value: Any?) throws -> [StackPr] {
        try XCTUnwrap(value as? [[String: Any]]).map {
            StackPr(
                id: try XCTUnwrap(string($0["id"])),
                state: try XCTUnwrap(string($0["state"])),
                basePullRequestId: string($0["basePullRequestId"])
            )
        }
    }

    /// EXP-1318: `wordCases` — the trailing word key, or none.
    func testThreadRowWordReplaysEveryCase() throws {
        for testCase in try cases(try fixture("threads-list.json"), "wordCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let states = try XCTUnwrap(input["pullRequests"] as? [[String: Any]]).compactMap { $0["state"] as? String }
            let word = SessionTree.threadRowWord(
                prStates: states,
                busy: try XCTUnwrap(input["busy"] as? Bool),
                needsYou: try XCTUnwrap(input["needsYou"] as? Bool),
                inStack: try XCTUnwrap(input["inStack"] as? Bool)
            )
            XCTAssertEqual(word?.rawValue, string(testCase["expected"]), name)
        }
    }

    /// Release train 2026-10-23 (NEW-41): `leadCases`, the row lead's PR glyph.
    func testThreadRowLeadPrReplaysEveryCase() throws {
        for testCase in try cases(try fixture("threads-list.json"), "leadCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let states = try XCTUnwrap(testCase["prStates"] as? [String])
            let lead = SessionTree.threadRowLeadPr(prStates: states)
            XCTAssertEqual(lead?.rawValue, string(testCase["expected"]), name)
        }
    }

    /// EXP-1318: `stackCases`.
    func testThreadInStackReplaysEveryCase() throws {
        for testCase in try cases(try fixture("threads-list.json"), "stackCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            XCTAssertEqual(
                SessionTree.threadInStack(
                    try stackPrs(input["pullRequests"]),
                    teamPrs: try stackPrs(input["teamPrs"])
                ),
                try XCTUnwrap(testCase["expected"] as? Bool),
                name
            )
        }
    }

    /// EXP-1318: `railCases` — the Active band's session ids in order.
    func testRailThreadsReplaysEveryCase() throws {
        for testCase in try cases(try fixture("threads-list.json"), "railCases") {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let groups = try XCTUnwrap(input["groups"] as? [[String: Any]]).map { group in
                ThreadsListGroup(
                    key: try XCTUnwrap(ThreadsListGroup.Key(rawValue: try XCTUnwrap(string(group["key"])))),
                    title: try XCTUnwrap(string(group["title"])),
                    rows: try XCTUnwrap(group["rows"] as? [[String: Any]]).map(listRow)
                )
            }
            let words = try XCTUnwrap(input["words"] as? [String: Any])
            let rows = SessionTree.railThreads(groups) { row in
                string(words[row.sessionId]).flatMap(ThreadRowWord.init(rawValue:))
            }
            XCTAssertEqual(rows.map(\.sessionId), try XCTUnwrap(testCase["expected"] as? [String]), name)
        }
    }

    // MARK: - Board switcher dots

    func testCountsOwnWorkingSessionsPerTeam() throws {
        func row(_ id: String, team: String, user: String = "me", status: String, needsInput: Bool = false) -> TreeSession {
            TreeSession(
                id: id, teamId: team, parentId: nil, rootId: id, userId: user,
                status: status, busy: false, needsInput: needsInput, blocked: nil, shared: true,
                title: nil, guide: nil, startedAt: nil, endedAt: nil,
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
        // EXP-1322: descriptions ride the `optionDescriptions` sidecar by
        // ORIGINAL index; non-string options drop.
        XCTAssertEqual(
            TreeMessageCard.parse(#"{"kind":"question","options":["Yes",{"label":"No"},"Maybe",3,"Later"],"optionDescriptions":["","x","Soon",null]}"#)?.options,
            [TreeQuestionOption(label: "Yes"), TreeQuestionOption(label: "Maybe", description: "Soon"), TreeQuestionOption(label: "Later")]
        )
    }

    // MARK: - EXP-1309: the Threads row's quiet word (web `threadRowWord`)

    private func pr(_ id: String, _ state: String, base: String? = nil, root: String = "r") -> PullRequestEntity {
        PullRequestEntity(
            id: id, teamId: "t", rootSessionId: root, repositoryId: "repo", number: 1,
            url: "https://github.com/acme/app/pull/1", head: "exp/\(id)", base: "master",
            basePullRequestId: base, state: state,
            createdAt: "2026-10-13T09:00:00.000Z", updatedAt: "2026-10-13T09:00:00.000Z"
        )
    }

    func testThreadRowWordFollowsTheWebRule() {
        let open = DomainContract.prStateOpen
        let merged = DomainContract.prStateMerged
        // Nothing to say.
        XCTAssertNil(SessionTree.threadRowWord(busy: false, needsYou: false, prs: [], teamPrs: []))
        // Waiting on a person with no PR.
        XCTAssertEqual(SessionTree.threadRowWord(busy: false, needsYou: true, prs: [], teamPrs: []), .question)
        // An open PR, nothing working: ready to merge; working: no word.
        let lone = pr("a", open)
        XCTAssertEqual(SessionTree.threadRowWord(busy: false, needsYou: false, prs: [lone], teamPrs: [lone]), .readyToMerge)
        XCTAssertNil(SessionTree.threadRowWord(busy: true, needsYou: false, prs: [lone], teamPrs: [lone]))
        // A stack edge (either side, across threads) wins, busy or not.
        let upper = pr("b", open, base: "a", root: "other")
        XCTAssertEqual(SessionTree.threadRowWord(busy: false, needsYou: false, prs: [lone], teamPrs: [lone, upper]), .stack)
        XCTAssertEqual(SessionTree.threadRowWord(busy: true, needsYou: false, prs: [upper], teamPrs: [lone, upper]), .stack)
        // A merged base breaks the stack.
        let mergedBase = pr("a", merged)
        XCTAssertEqual(SessionTree.threadRowWord(busy: false, needsYou: false, prs: [upper], teamPrs: [mergedBase, upper]), .readyToMerge)
        // Merged beats the question; a closed PR says nothing.
        XCTAssertEqual(SessionTree.threadRowWord(busy: false, needsYou: true, prs: [mergedBase], teamPrs: [mergedBase]), .merged)
        XCTAssertNil(SessionTree.threadRowWord(busy: false, needsYou: false, prs: [pr("c", "closed")], teamPrs: []))
        XCTAssertEqual(ThreadRowWord.allCases.map(\.text), ["stack", "ready to merge", nil, "question", "needs input"])
    }
}
