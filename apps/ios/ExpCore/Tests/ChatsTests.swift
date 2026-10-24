import Foundation
import XCTest
@testable import ExpCore

// Thread 58cb32df: Chats, locked ×4 (TS `chats.test.ts`, Android `ChatsTest`,
// desktop `domain::chats`) against fixtures/chats.json: every case group
// (previewCases, timeCases, listCases, bubbleCases, menuCases,
// agentMenuCases, replyCases, dividerCases, plainCases) plus the copy, test
// ids, shell and surface-card constants. Whole expected objects are compared
// as JSON values.
final class ChatsTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/chats.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try fixture()[key] as? [[String: Any]], key)
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    private func string(_ value: Any?) -> String? { value as? String }

    private func json(_ value: String?) -> Any { value.map { $0 as Any } ?? NSNull() }

    private func assertJSON(_ got: Any, _ expected: Any?, _ message: String, file: StaticString = #filePath, line: UInt = #line) {
        let lhs = try? JSONSerialization.data(withJSONObject: ["v": got], options: [.sortedKeys])
        let rhs = try? JSONSerialization.data(withJSONObject: ["v": expected ?? NSNull()], options: [.sortedKeys])
        XCTAssertEqual(
            lhs.flatMap { String(data: $0, encoding: .utf8) },
            rhs.flatMap { String(data: $0, encoding: .utf8) },
            message, file: file, line: line
        )
    }

    // MARK: - Inputs

    private func messages(_ value: Any?) throws -> [TreeMessage] {
        try ((value as? [[String: Any]]) ?? []).map {
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

    private func reads(_ value: Any?) throws -> [TreeSessionRead] {
        try ((value as? [[String: Any]]) ?? []).map {
            TreeSessionRead(
                sessionId: try XCTUnwrap(string($0["sessionId"])),
                readAt: try XCTUnwrap(string($0["readAt"]))
            )
        }
    }

    private func sessions(_ value: Any?) throws -> [TreeSession] {
        try ((value as? [[String: Any]]) ?? []).map { row in
            TreeSession(
                id: try XCTUnwrap(string(row["id"])),
                teamId: try XCTUnwrap(string(row["teamId"])),
                parentId: string(row["parentId"]),
                rootId: try XCTUnwrap(string(row["rootId"])),
                userId: try XCTUnwrap(string(row["userId"])),
                status: try XCTUnwrap(string(row["status"])),
                busy: try XCTUnwrap(row["busy"] as? Bool),
                needsInput: try XCTUnwrap(row["needsInput"] as? Bool),
                blocked: nil,
                shared: try XCTUnwrap(row["shared"] as? Bool),
                title: string(row["title"]),
                guide: nil,
                startedAt: string(row["startedAt"]),
                endedAt: string(row["endedAt"]),
                createdAt: try XCTUnwrap(string(row["createdAt"])),
                updatedAt: try XCTUnwrap(string(row["updatedAt"])),
                deviceId: string(row["deviceId"]),
                agentId: string(row["agentId"])
            )
        }
    }

    private func groups(_ value: Any?) throws -> [ThreadsListGroup] {
        try XCTUnwrap(value as? [[String: Any]]).map { group in
            ThreadsListGroup(
                key: try XCTUnwrap(ThreadsListGroup.Key(rawValue: try XCTUnwrap(string(group["key"])))),
                title: try XCTUnwrap(string(group["title"])),
                rows: try XCTUnwrap(group["rows"] as? [[String: Any]]).map { row in
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
            )
        }
    }

    private func agents(_ value: Any?) throws -> [AgentRow] {
        try XCTUnwrap(value as? [[String: Any]]).map {
            AgentRow(
                agentId: try XCTUnwrap(string($0["agentId"])),
                teamId: try XCTUnwrap(string($0["teamId"])),
                name: try XCTUnwrap(string($0["name"])),
                icon: try XCTUnwrap(string($0["icon"])),
                color: try XCTUnwrap(string($0["color"])),
                busy: try XCTUnwrap($0["busy"] as? Bool),
                needsYou: try XCTUnwrap($0["needsYou"] as? Bool)
            )
        }
    }

    // MARK: - Renders

    private func json(_ preview: Chats.ChatPreview) -> [String: Any] {
        ["kind": preview.kind.rawValue, "text": preview.text]
    }

    private func json(_ row: Chats.ChatRow) -> [String: Any] {
        [
            "sessionId": row.sessionId, "teamId": row.teamId, "title": row.title, "deviceId": json(row.deviceId),
            "group": row.group, "lead": row.lead.rawValue, "preview": json(row.preview),
            "activityAt": row.activityAt, "time": row.time, "unreadCount": row.unreadCount,
            "needsYou": row.needsYou, "busy": row.busy, "word": json(row.word?.rawValue),
        ]
    }

    // MARK: - Cases

    func testPreviewCases() throws {
        for testCase in try cases("previewCases") {
            let name = try XCTUnwrap(string(testCase["name"]))
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let got = Chats.chatPreview(
                rootId: try XCTUnwrap(string(input["rootId"])),
                messages: try messages(input["messages"]),
                needsYou: try XCTUnwrap(input["needsYou"] as? Bool),
                busy: try XCTUnwrap(input["busy"] as? Bool)
            )
            assertJSON(json(got), testCase["expected"], name)
        }
    }

    func testTimeCases() throws {
        for testCase in try cases("timeCases") {
            let name = try XCTUnwrap(string(testCase["name"]))
            XCTAssertEqual(
                Chats.chatTimeLabel(
                    at: try XCTUnwrap(string(testCase["at"])),
                    now: try XCTUnwrap(string(testCase["now"])),
                    utcOffsetMinutes: try XCTUnwrap(testCase["utcOffsetMinutes"] as? Int)
                ),
                string(testCase["expected"]),
                name
            )
        }
    }

    func testListCases() throws {
        for testCase in try cases("listCases") {
            let name = try XCTUnwrap(string(testCase["name"]))
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let prStates = (input["prStates"] as? [String: [String]]) ?? [:]
            var words: [String: ThreadRowWord] = [:]
            for (id, raw) in (input["words"] as? [String: Any]) ?? [:] {
                if let raw = raw as? String { words[id] = try XCTUnwrap(ThreadRowWord(rawValue: raw)) }
            }
            let filter = try XCTUnwrap(Chats.ChatFilter(rawValue: try XCTUnwrap(string(input["filter"]))))
            let got = Chats.chatList(
                groups: try groups(input["groups"]),
                messages: try messages(input["messages"]),
                reads: try reads(input["reads"]),
                prStates: prStates,
                words: words,
                filter: filter,
                now: try XCTUnwrap(string(input["now"])),
                utcOffsetMinutes: try XCTUnwrap(input["utcOffsetMinutes"] as? Int)
            )
            var counts: [String: Any] = [:]
            for (key, value) in got.counts { counts[key.rawValue] = value }
            assertJSON(["rows": got.rows.map(json), "counts": counts], testCase["expected"], "\(name) / \(filter)")
        }
    }

    func testFilterLabels() {
        let counts: [Chats.ChatFilter: Int] = [.needsYou: 1, .running: 2, .unread: 0]
        XCTAssertEqual(Chats.chatFilterLabel(.all, counts: counts), "All")
        XCTAssertEqual(Chats.chatFilterLabel(.needsYou, counts: counts), "Needs you 1")
        XCTAssertEqual(Chats.chatFilterLabel(.running, counts: counts), "Running 2")
        XCTAssertEqual(Chats.chatFilterLabel(.unread, counts: counts), "Unread")
    }

    func testBubbleCases() throws {
        for testCase in try cases("bubbleCases") {
            let name = try XCTUnwrap(string(testCase["name"]))
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let got = Chats.agentBubbles(
                agents: try agents(input["agents"]),
                sessions: try sessions(input["sessions"]),
                messages: try messages(input["messages"]),
                reads: try reads(input["reads"]),
                me: string(input["me"])
            )
            let rendered: [[String: Any]] = got.map { bubble in
                [
                    "agentId": bubble.agent.agentId, "teamId": bubble.agent.teamId, "name": bubble.agent.name,
                    "icon": bubble.agent.icon, "color": bubble.agent.color, "busy": bubble.agent.busy,
                    "needsYou": bubble.agent.needsYou, "ring": json(bubble.ring?.rawValue),
                ]
            }
            assertJSON(rendered, testCase["expected"], name)
        }
    }

    func testMenuCases() throws {
        for testCase in try cases("menuCases") {
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let got = Chats.threadMenuEntries(
                rootStatus: try XCTUnwrap(string(input["rootStatus"])),
                canSteer: try XCTUnwrap(input["canSteer"] as? Bool),
                canDelete: try XCTUnwrap(input["canDelete"] as? Bool)
            )
            XCTAssertEqual(got.map(\.rawValue), testCase["expected"] as? [String], string(testCase["name"]) ?? "")
        }
    }

    func testAgentMenuCases() throws {
        for testCase in try cases("agentMenuCases") {
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let got = Chats.agentMenuEntries(
                busy: try XCTUnwrap(input["busy"] as? Bool),
                isOwner: try XCTUnwrap(input["isOwner"] as? Bool)
            )
            XCTAssertEqual(got.map(\.rawValue), testCase["expected"] as? [String], string(testCase["name"]) ?? "")
        }
        let sends = try XCTUnwrap((try fixture()["agentMenu"] as? [String: Any])?["sends"] as? [String: String])
        var mirrored: [String: String] = [:]
        for (entry, line) in Chats.agentMenuSends { mirrored[entry.rawValue] = line }
        XCTAssertEqual(mirrored, sends)
    }

    /// #EXP-1383 `agentHint`: the stamp compare and the banner row.
    func testAgentHintCases() throws {
        let hint = try XCTUnwrap(try fixture()["agentHint"] as? [String: Any])
        let cases = try XCTUnwrap(hint["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            XCTAssertEqual(
                Chats.agentInstructionsChanged(
                    sessionStamp: input["sessionStamp"] as? String,
                    agentStamp: input["agentStamp"] as? String
                ),
                try XCTUnwrap(testCase["expected"] as? Bool),
                string(testCase["name"]) ?? ""
            )
        }
        let banner = try XCTUnwrap(hint["banner"] as? [String: String])
        XCTAssertEqual(banner, [
            "id": Chats.AgentHint.id,
            "icon": Chats.AgentHint.icon,
            "label": Chats.AgentHint.label,
            "text": Chats.AgentHint.text,
            "tone": Chats.AgentHint.tone,
            "action": Chats.AgentHint.action,
        ])
        XCTAssertEqual(hint["sends"] as? String, Chats.AgentHint.sends)
        XCTAssertEqual(Chats.AgentHint.sends, Chats.agentMenuSends[.newConversation])
    }

    func testReplyCases() throws {
        for testCase in try cases("replyCases") {
            let name = try XCTUnwrap(string(testCase["name"]))
            let event = try XCTUnwrap(testCase["event"] as? [String: Any])
            let got = Chats.surfaceReply(name: string(event["name"]), context: event["context"])
            let expected = (testCase["expected"] as? [String: Any])?["text"] as? String
            XCTAssertEqual(got, expected, name)
            // The same off the card model's JSON (what a renderer hands back).
            let viaJSON = Chats.surfaceReply(name: string(event["name"]), context: CardJSON(any: event["context"]))
            XCTAssertEqual(viaJSON, expected, "\(name) (CardJSON)")
        }
    }

    func testDividerCases() throws {
        for testCase in try cases("dividerCases") {
            let body = try XCTUnwrap(string(testCase["body"]))
            XCTAssertEqual(Chats.commandDivider(body)?.rawValue, string(testCase["expected"]), body)
        }
    }

    func testPlainCases() throws {
        for testCase in try cases("plainCases") {
            let text = try XCTUnwrap(string(testCase["text"]))
            XCTAssertEqual(Chats.plainLine(text), string(testCase["expected"]), text)
        }
    }

    // MARK: - Constants

    func testCopyMatchesTheFixture() throws {
        let copy = try XCTUnwrap(try fixture()["copy"] as? [String: Any])
        typealias C = Chats.Copy
        XCTAssertEqual(copy["title"] as? String, C.title)
        XCTAssertEqual(copy["tabs"] as? [String: String], [
            "board": C.Tabs.board, "chats": C.Tabs.chats, "apps": C.Tabs.apps, "team": C.Tabs.team,
        ])
        XCTAssertEqual(copy["headers"] as? [String: String], [
            "chats": C.Headers.chats, "apps": C.Headers.apps, "devices": C.Headers.devices,
            "agent": C.Headers.agent,
        ])
        XCTAssertEqual(copy["boardMenu"] as? [String: String], ["newBoard": C.BoardMenu.newBoard])
        XCTAssertEqual(copy["filters"] as? [String: String], [
            "all": C.Filters.all, "needs_you": C.Filters.needsYou, "running": C.Filters.running,
            "unread": C.Filters.unread,
        ])
        XCTAssertEqual(copy["preview"] as? [String: String], [
            "working": C.Preview.working, "question": C.Preview.question, "plan": C.Preview.plan,
            "you": C.Preview.you, "picture": C.Preview.picture, "surface": C.Preview.surface,
            "empty": C.Preview.empty,
        ])
        let time = try XCTUnwrap(copy["time"] as? [String: Any])
        XCTAssertEqual(time["yesterday"] as? String, C.Time.yesterday)
        XCTAssertEqual(time["weekdaysShort"] as? [String], C.Time.weekdaysShort)
        XCTAssertEqual(time["monthsShort"] as? [String], C.Time.monthsShort)
        XCTAssertEqual(copy["menu"] as? [String: String], [
            "stop": C.Menu.stop, "stopAndDelete": C.Menu.stopAndDelete, "delete": C.Menu.delete,
            "compact": C.Menu.compact, "newConversation": C.Menu.newConversation, "settings": C.Menu.settings,
        ])
        XCTAssertEqual(copy["confirmDelete"] as? [String: String], [
            "title": C.ConfirmDelete.title, "body": C.ConfirmDelete.body,
        ])
        XCTAssertEqual(copy["toasts"] as? [String: String], [
            "stopFailed": C.Toasts.stopFailed, "deleteFailed": C.Toasts.deleteFailed,
            "stopNotDelivered": C.Toasts.stopNotDelivered,
        ])
        XCTAssertEqual(copy["dividers"] as? [String: String], ["new": C.Dividers.new, "compact": C.Dividers.compact])
        XCTAssertEqual(copy["superseded"] as? String, C.superseded)
        XCTAssertEqual(copy["boardChips"] as? [String: String], [
            "all": C.BoardChips.all, "mine": C.BoardChips.mine, "drafts": C.BoardChips.drafts,
        ])
        XCTAssertEqual(copy["team"] as? [String: String], ["meta": C.Team.meta, "switch": C.Team.`switch`])
        XCTAssertEqual(C.Team.metaLine(members: 8, boards: 3), "8 members · 3 boards")
        XCTAssertEqual(copy["inbox"] as? String, C.inbox)
        XCTAssertEqual(copy["loadEarlier"] as? String, C.loadEarlier)
        XCTAssertEqual(copy["working"] as? String, C.working)
        XCTAssertEqual(Set(copy.keys), [
            "title", "tabs", "headers", "boardMenu", "filters", "preview", "time", "menu", "confirmDelete", "toasts", "dividers",
            "superseded", "boardChips", "team", "inbox", "loadEarlier", "working",
        ])
        XCTAssertEqual(AskDock.Copy.superseded, C.superseded)
    }

    func testTestIdsShellAndSurfaceMatchTheFixture() throws {
        let fixture = try fixture()
        typealias T = Chats.TestIds
        XCTAssertEqual(fixture["testIds"] as? [String: String], [
            "chatRow": T.chatRow, "chatFilter": T.chatFilter, "agentBubble": T.agentBubble,
            "inboxButton": T.inboxButton, "tabTeam": T.tabTeam, "boardChip": T.boardChip,
            "surfaceCard": T.surfaceCard, "divider": T.divider, "tabBoard": T.tabBoard,
            "boardMenu": T.boardMenu, "boardMenuRow": T.boardMenuRow, "pageHeader": T.pageHeader,
            "searchButton": T.searchButton,
            "loadEarlier": T.loadEarlier, "jumpToBottom": T.jumpToBottom,
        ])
        let transcript = try XCTUnwrap(fixture["transcript"] as? [String: Any])
        XCTAssertEqual(transcript["window"] as? Int, Chats.Transcript.window)
        XCTAssertEqual(transcript["windowStep"] as? Int, Chats.Transcript.windowStep)
        XCTAssertEqual(transcript["followSlack"] as? Double, Chats.Transcript.followSlack)
        XCTAssertEqual(Chats.Transcript.windowStart(count: 1500, limit: 100), 1400)
        XCTAssertEqual(Chats.Transcript.windowStart(count: 40, limit: 100), 0)
        XCTAssertEqual(fixture["filters"] as? [String], Chats.filters.map(\.rawValue))

        let shell = try XCTUnwrap(fixture["shell"] as? [String: Any])
        XCTAssertEqual(shell["tabs"] as? [String], Chats.Shell.tabs.map(\.rawValue))
        XCTAssertEqual(shell["tabIcons"] as? [String: String], Chats.Shell.tabIcons)
        for tab in Chats.Shell.tabs { XCTAssertEqual(tab.icon, Chats.Shell.tabIcons[tab.rawValue]) }
        XCTAssertEqual(shell["boardChips"] as? [String], Chats.Shell.boardChips.map(\.rawValue))
        typealias Sh = Chats.Shell
        let bar = try XCTUnwrap(shell["bar"] as? [String: Any])
        XCTAssertEqual(bar["height"] as? Double, Sh.Bar.height)
        XCTAssertEqual(bar["padding"] as? Double, Sh.Bar.padding)
        XCTAssertEqual(bar["gap"] as? Double, Sh.Bar.gap)
        XCTAssertEqual(bar["tabMinWidth"] as? Double, Sh.Bar.tabMinWidth)
        XCTAssertEqual(bar["tabPaddingX"] as? Double, Sh.Bar.tabPaddingX)
        let drag = try XCTUnwrap(bar["drag"] as? [String: Double])
        XCTAssertEqual(drag, ["lensScale": Sh.Bar.Drag.lensScale, "cancelDistance": Sh.Bar.Drag.cancelDistance])
        XCTAssertEqual(Set(bar.keys), ["height", "padding", "gap", "tabMinWidth", "tabPaddingX", "drag"])
        let boardTab = try XCTUnwrap(shell["boardTab"] as? [String: Any])
        XCTAssertEqual(boardTab["caret"] as? String, Sh.BoardTab.caret)
        XCTAssertEqual(boardTab["caretSize"] as? Double, Sh.BoardTab.caretSize)
        XCTAssertEqual(Set(boardTab.keys), ["caret", "caretSize"])
        let boardMenu = try XCTUnwrap(shell["boardMenu"] as? [String: Any])
        XCTAssertEqual(boardMenu["rowHeight"] as? Double, Sh.BoardMenu.rowHeight)
        XCTAssertEqual(boardMenu["maxVisibleRows"] as? Int, Sh.BoardMenu.maxVisibleRows)
        XCTAssertEqual(boardMenu["radius"] as? Double, Sh.BoardMenu.radius)
        XCTAssertEqual(boardMenu["motion"] as? [String: String], [
            "open": Sh.BoardMenu.Motion.open, "openEase": Sh.BoardMenu.Motion.openEase,
            "close": Sh.BoardMenu.Motion.close, "closeEase": Sh.BoardMenu.Motion.closeEase,
        ])
        XCTAssertEqual(Set(boardMenu.keys), ["rowHeight", "maxVisibleRows", "radius", "motion"])
        let header = try XCTUnwrap(shell["header"] as? [String: Any])
        XCTAssertEqual(header["pages"] as? [String], Sh.Header.pages.map(\.rawValue))
        XCTAssertEqual(header["buttons"] as? [String], Sh.Header.buttons)
        XCTAssertEqual(header["circle"] as? Double, Sh.Header.circle)
        XCTAssertEqual(Set(header.keys), ["pages", "buttons", "circle"])
        XCTAssertEqual(Set(shell.keys), [
            "tabs", "tabIcons", "bar", "boardTab", "boardMenu", "header", "boardChips", "leadDisc",
            "bubble", "bubbleRing",
        ])
        XCTAssertEqual(shell["leadDisc"] as? Double, Chats.Shell.leadDisc)
        XCTAssertEqual(shell["bubble"] as? Double, Chats.Shell.bubble)
        XCTAssertEqual(shell["bubbleRing"] as? Double, Chats.Shell.bubbleRing)

        let surface = try XCTUnwrap(fixture["surfaceCard"] as? [String: Any])
        typealias S = Chats.SurfaceEnter
        XCTAssertEqual(surface["replyEvent"] as? String, S.replyEvent)
        XCTAssertEqual(surface["surfaceMaxBytes"] as? Int, S.surfaceMaxBytes)
        XCTAssertEqual(surface["surfaceMaxComponents"] as? Int, S.surfaceMaxComponents)
        let enter = try XCTUnwrap(surface["enter"] as? [String: [String: Any]])
        let frame = try XCTUnwrap(enter["frame"])
        XCTAssertEqual(frame["fromOpacity"] as? Double, S.Frame.fromOpacity)
        XCTAssertEqual(frame["fromTranslateY"] as? Double, S.Frame.fromTranslateY)
        XCTAssertEqual(frame["fromScale"] as? Double, S.Frame.fromScale)
        XCTAssertEqual(frame["duration"] as? String, S.Frame.duration)
        XCTAssertEqual(frame["ease"] as? String, S.Frame.ease)
        let children = try XCTUnwrap(enter["children"])
        XCTAssertEqual(children["fromOpacity"] as? Double, S.Children.fromOpacity)
        XCTAssertEqual(children["fromTranslateY"] as? Double, S.Children.fromTranslateY)
        XCTAssertEqual(children["duration"] as? String, S.Children.duration)
        XCTAssertEqual(children["ease"] as? String, S.Children.ease)
        XCTAssertEqual(children["staggerMs"] as? Double, S.Children.staggerMs)
        XCTAssertEqual(children["maxStaggered"] as? Int, S.Children.maxStaggered)
        XCTAssertEqual(children["startAfterMs"] as? Double, S.Children.startAfterMs)
        let glow = try XCTUnwrap(enter["glow"])
        XCTAssertEqual(glow["fromOpacity"] as? Double, S.Glow.fromOpacity)
        XCTAssertEqual(glow["duration"] as? String, S.Glow.duration)
        XCTAssertEqual(glow["ease"] as? String, S.Glow.ease)
        XCTAssertEqual(S.childDelayMs(0), 90)
        XCTAssertEqual(S.childDelayMs(2), 180)
        XCTAssertEqual(S.childDelayMs(20), 90 + 45 * 7)
    }
}
