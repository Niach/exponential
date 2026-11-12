import Foundation
import GRDB
import Observation
import XCTest

@testable import ExpCore

/// Large chats (Danny, 2026-11-12: "scroll is really laggy on large chats … a
/// few seconds to load each chat"). A chat's transcript is the steer feed: a
/// join used to replay the device's WHOLE journal (17.5k events / 16.7 MB on
/// the worst one), now it replays the newest `steerFeed.replayTail` events
/// and pages older ones in. This suite is the guard: a synthetic journal far
/// larger than the tail folds within a generous budget and the transcript
/// projects at most the contract window. `testMeasureRealJournals` replays
/// the copies of Danny's journals when they sit under `/tmp/ios-lane-journal`
/// (skipped otherwise, so CI never depends on them).
final class SteerFeedScaleTests: XCTestCase {
    // MARK: - The fold (the view model's row-producing cases, mirrored)

    /// `AgentSessionModel.handleActivityEvent`'s TRANSCRIPT half over the
    /// same ExpCore rules (merge, in-place tool settle, upserts); the
    /// latest-wins slots are state, never rows, and stay out.
    struct Fold {
        var feed: [AgentFeedItem] = []
        var nextId = 0

        mutating func take() -> Int {
            defer { nextId += 1 }
            return nextId
        }

        mutating func apply(_ activity: SteerWireFrame.Activity) {
            guard let event = activity.event else { return }
            switch event {
            case let .narration(text, messageId, subagentId, _):
                if AgentFeed.mergeNarration(&feed, text: text, messageId: messageId, subagentId: subagentId) {
                    return
                }
                feed.append(.narration(id: take(), text: text, messageId: messageId, subagentId: subagentId))
            case let .apiError(message, errorType, subagentId):
                feed.append(.apiError(id: take(), message: message, errorType: errorType, subagentId: subagentId))
            case let .tool(name, detail, subagentId, callId, toolKind):
                feed.append(.tool(
                    id: take(), name: name, detail: detail, subagentId: subagentId,
                    callId: callId, toolKind: toolKind
                ))
            case let .toolUpdate(update):
                AgentFeed.applyToolUpdate(&feed, update: update)
            case let .userMessage(text, subagentId, at):
                feed.append(.userMessage(id: take(), text: text, subagentId: subagentId, at: at))
            case let .question(draft):
                feed = AgentFeed.upsertQuestion(feed, question: draft.question(id: take()))
            case let .questionResolved(id, askId, answers, dismissed):
                if let out = AgentFeed.applyQuestionResolved(
                    feed, id: id, askId: askId, answers: answers, dismissed: dismissed
                ) {
                    feed = out
                }
            case let .subagent(subagentId, agentType, status, detail, toolCalls, title, workflowId):
                feed.append(.subagent(
                    id: take(), subagentId: subagentId, agentType: agentType, status: status,
                    detail: detail, toolCalls: toolCalls, title: title, workflowId: workflowId
                ))
            case let .permission(tool, detail):
                feed.append(.permission(id: take(), tool: tool, detail: detail))
            case let .thought(thoughtId, started, durationMs, subagentId):
                if AgentFeed.thoughtRunning(feed, thoughtId: thoughtId) != nil {
                    if !started {
                        AgentFeed.applyThought(&feed, thoughtId: thoughtId, started: false, durationMs: durationMs)
                    }
                } else if started {
                    feed.append(.thought(
                        id: take(), thoughtId: thoughtId, running: true,
                        durationMs: durationMs, subagentId: subagentId
                    ))
                }
            case let .compaction(edge):
                if edge.ended { feed.append(.compaction(id: take())) }
            default:
                break
            }
        }
    }

    struct Measured {
        var events = 0
        var bytes = 0
        var parseMs = 0.0
        var foldMs = 0.0
        var items = 0
        var windowStart = 0
        var rowsMs = 0.0
        var rows = 0
        /// Markdown-bearing rows (prose bubbles, turns): a parse each when drawn.
        var markdownRows = 0
        var wholeFeedRowsMs = 0.0
        var wholeFeedRows = 0
        var wholeFeedMarkdownRows = 0
    }

    private static func ms(_ start: DispatchTime) -> Double {
        Double(DispatchTime.now().uptimeNanoseconds - start.uptimeNanoseconds) / 1_000_000
    }

    private static func markdownRows(_ rows: [AgentFeedRow]) -> Int {
        rows.reduce(0) { count, row in
            guard case let .single(item) = row else { return count }
            switch item {
            case .narration, .userMessage, .apiError: return count + 1
            default: return count
            }
        }
    }

    /// What a join replay costs this client: the frames' parse + decode (off
    /// main in the app), the fold, and the projection of the rendered window
    /// (`window` = the contract window the build ran with).
    static func measure(frames: [String], window: Int) -> Measured {
        var out = Measured()
        out.events = frames.count
        out.bytes = frames.reduce(0) { $0 + $1.utf8.count }
        var start = DispatchTime.now()
        let parsed = SteerWireFrame.parse(frames)
        out.parseMs = ms(start)
        start = DispatchTime.now()
        var fold = Fold()
        for frame in parsed where frame.t == "activity" { fold.apply(frame.activity) }
        out.foldMs = ms(start)
        out.items = fold.feed.count
        out.windowStart = max(0, fold.feed.count - window)
        start = DispatchTime.now()
        let rows = AgentFeed.rows(fold.feed, from: out.windowStart)
        out.rowsMs = ms(start)
        out.rows = rows.count
        out.markdownRows = markdownRows(rows)
        start = DispatchTime.now()
        let whole = AgentFeed.rows(fold.feed, from: 0)
        out.wholeFeedRowsMs = ms(start)
        out.wholeFeedRows = whole.count
        out.wholeFeedMarkdownRows = markdownRows(whole)
        return out
    }

    /// One journal line as the `activity` frame a replay sends it in.
    static func frame(_ event: String, seq: Int) -> String {
        "{\"t\":\"activity\",\"seq\":\(seq),\"event\":\(event)}"
    }

    // MARK: - A synthetic journal

    /// `count` events shaped like a long coding run: turns, prose flushed in
    /// fragments, tool calls settling with output (the bulk of a real
    /// journal's bytes), usage ticks.
    static func syntheticJournal(count: Int) -> [String] {
        let output = String(repeating: "line of command output, typical width for a build log\\n", count: 40)
        var events: [String] = []
        events.reserveCapacity(count)
        var i = 0
        var call = 0
        var message = 0
        while events.count < count {
            switch i % 10 {
            case 0:
                events.append("{\"kind\":\"user_message\",\"text\":\"Step \(i): build the **next** lane and report\",\"at\":1794028546009}")
            case 1, 2:
                events.append("{\"kind\":\"narration\",\"text\":\"Working on `lane \(i)`: reading the fixture first. \",\"messageId\":\"msg_\(message)\",\"at\":1794028546009}")
            case 3, 5, 7:
                call += 1
                events.append("{\"kind\":\"tool\",\"name\":\"Bash\",\"detail\":\"cargo test -p lane\(call)\",\"id\":\"toolu_\(call)\",\"toolKind\":\"execute\",\"at\":1794028546009}")
            case 4, 6, 8:
                events.append("{\"kind\":\"tool_update\",\"id\":\"toolu_\(call)\",\"status\":\"completed\",\"output\":\"\(output)\",\"at\":1794028546009}")
            default:
                message += 1
                events.append("{\"kind\":\"usage\",\"contextUsed\":\(40000 + i),\"contextSize\":200000,\"at\":1794028546009}")
            }
            i += 1
        }
        return events.enumerated().map { frame($0.element, seq: $0.offset) }
    }

    // MARK: - The guard

    /// A journal 40× the replay tail: a join now carries only the tail, which
    /// folds and projects inside a CI-safe budget, and the transcript never
    /// projects more rows than the contract window however long the run.
    func testLargeJournalOpensWithinTheWindowAndBudget() {
        let journal = Self.syntheticJournal(count: 20_000)
        let tail = Array(journal.suffix(AgentFeed.replayTail))
        XCTAssertEqual(tail.count, DomainContract.steerFeedReplayTail)

        let opened = Self.measure(frames: tail, window: AgentFeed.feedWindow)
        XCTAssertLessThanOrEqual(opened.rows, AgentFeed.feedWindow)
        XCTAssertLessThanOrEqual(opened.items, AgentFeed.replayTail)
        // Generous on purpose (a loaded CI simulator, a debug build): a
        // regression to the full replay costs well over 10× this.
        XCTAssertLessThan(opened.parseMs + opened.foldMs + opened.rowsMs, 1_000, "\(opened)")

        // Even a client holding the WHOLE run renders only the window.
        var fold = Fold()
        for frame in SteerWireFrame.parse(journal) { fold.apply(frame.activity) }
        let start = AgentFeed.windowStart(fold.feed, from: nil)
        XCTAssertEqual(start, fold.feed.count - AgentFeed.feedWindow)
        let rows = AgentFeed.rows(fold.feed, from: start)
        XCTAssertLessThanOrEqual(rows.count, AgentFeed.feedWindow)

        // Load earlier grows it one step at a time, never the whole run.
        let anchor = AgentFeed.loadEarlierAnchor(fold.feed, windowStart: start)
        XCTAssertNotNil(anchor)
        let grown = AgentFeed.windowStart(fold.feed, from: anchor)
        XCTAssertEqual(start - grown, AgentFeed.feedWindowStep)
        XCTAssertLessThanOrEqual(
            AgentFeed.rows(fold.feed, from: grown).count, AgentFeed.feedWindow + AgentFeed.feedWindowStep
        )
        // At the feed's first row the next page is the device's.
        XCTAssertNil(AgentFeed.loadEarlierAnchor(fold.feed, windowStart: 0))
        XCTAssertEqual(AgentFeed.windowStart(fold.feed, from: Int.min), 0)
    }

    /// The window anchor is an id: rows prepended below it (an older page)
    /// do not slide the rendered start off the row the reader is on.
    func testWindowAnchorSurvivesAPrependedPage() {
        let feed = (100..<1100).map { AgentFeedItem.narration(id: $0, text: "row \($0)") }
        let start = AgentFeed.windowStart(feed, from: nil)
        let anchor = feed[start].id
        let page = (0..<100).map { AgentFeedItem.narration(id: $0, text: "old \($0)") }
        let grown = page + feed
        XCTAssertEqual(grown[AgentFeed.windowStart(grown, from: anchor)].id, anchor)
    }

    /// #EXP-1623 kept every feed write IN PLACE on the `@Observable` model;
    /// an observed array with a `didSet` must still append in amortised O(1)
    /// (a copy per write made a replay quadratic).
    func testObservedFeedAppendsStayLinear() {
        let model = ObservedFeed()
        let start = DispatchTime.now()
        for id in 0..<20_000 { model.feed.append(.narration(id: id, text: "x")) }
        let elapsed = Self.ms(start)
        XCTAssertEqual(model.version, 20_000)
        XCTAssertLessThan(elapsed, 1_000, "20k observed appends took \(elapsed)ms")
    }

    @Observable
    final class ObservedFeed {
        var feed: [AgentFeedItem] = [] {
            didSet { version &+= 1 }
        }
        var version = 0
    }

    // MARK: - Danny's journals (local measurement, skipped elsewhere)

    func testMeasureRealJournals() throws {
        let dir = URL(fileURLWithPath: "/tmp/ios-lane-journal")
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: dir.path),
              names.contains(where: { $0.hasSuffix(".jsonl") })
        else { throw XCTSkip("no journal copies under /tmp/ios-lane-journal") }
        for name in names.sorted() where name.hasSuffix(".jsonl") {
            let text = try String(contentsOf: dir.appendingPathComponent(name), encoding: .utf8)
            let lines = text.split(separator: "\n", omittingEmptySubsequences: true)
            let frames = lines.enumerated().map { Self.frame(String($0.element), seq: $0.offset) }
            // BEFORE: the whole journal replayed, the 1500-row window.
            let before = Self.measure(frames: frames, window: 1500)
            // AFTER: the replay tail, the contract window.
            let after = Self.measure(frames: Array(frames.suffix(AgentFeed.replayTail)), window: AgentFeed.feedWindow)
            print("SCALE \(name) BEFORE \(before)")
            print("SCALE \(name) AFTER  \(after)")
            XCTAssertLessThanOrEqual(after.rows, AgentFeed.feedWindow)
        }
    }

    /// The chat snapshot's message side on a copy of a synced store
    /// (`/tmp/ios-lane-sync.sqlite`): the biggest roots' keyed messages and
    /// what the transcript's per-render title pass cost over them.
    func testMeasureRootMessages() throws {
        let path = "/tmp/ios-lane-sync.sqlite"
        guard FileManager.default.fileExists(atPath: path) else { throw XCTSkip("no store copy") }
        var config = Configuration()
        config.readonly = true
        let queue = try DatabaseQueue(path: path, configuration: config)
        let roots = try queue.read { db in
            try String.fetchAll(db, sql: """
                SELECT root_id FROM messages WHERE session_id IS NOT NULL AND session_id <> ''
                GROUP BY root_id ORDER BY count(*) DESC LIMIT 3
                """)
        }
        for rootId in roots {
            var start = DispatchTime.now()
            let messages = try queue.read { db in try SessionQueries.messages(db: db, rootId: rootId) }
            let fetchMs = Self.ms(start)
            start = DispatchTime.now()
            let tree = messages.map { row in
                let opened = SealedMessage.open(body: row.body, card: row.card, rootId: row.message.rootId)
                return TreeMessage(
                    id: row.id, sessionId: row.sessionId, authorKind: row.authorKind, authorId: row.authorId,
                    body: opened.body, card: TreeMessageCard.parse(opened.card),
                    answeredAt: row.answeredAt, consumedAt: row.consumedAt, createdAt: row.createdAt,
                    authorSessionId: row.authorSessionId, sealStatus: opened.status,
                    escalated: TreeMessageCard.parse(SealedMessage.escalatedCard(row.card, rootId: row.message.rootId)),
                    attachmentIds: row.attachmentIds
                )
            }
            let mapMs = Self.ms(start)
            start = DispatchTime.now()
            _ = SessionTitle.titles([], sessionIssues: [], issues: [], messages: tree)
            let titlesMs = Self.ms(start)
            let root = TreeSession(
                id: rootId, teamId: "t", parentId: nil, rootId: rootId, userId: "u", status: "active",
                busy: false, needsInput: false, blocked: nil, shared: true, title: "Root", guide: nil,
                startedAt: nil, endedAt: nil, createdAt: "2026-11-01T00:00:00Z", updatedAt: "2026-11-01T00:00:00Z"
            )
            start = DispatchTime.now()
            let view = SessionTree.buildSessionChat(
                root: root, sessions: [root], sessionIssues: [], issues: [], messages: tree,
                pullRequests: [], now: Date()
            )
            let viewMs = Self.ms(start)
            print("SCALE root \(rootId) messages=\(messages.count) fetch=\(fetchMs)ms treeMessages=\(mapMs)ms titles=\(titlesMs)ms buildSessionChat=\(viewMs)ms viewRows=\(view.rows.count)")

            // AFTER #EXP-1380 windowing: the newest page + the digests' waiting cards.
            start = DispatchTime.now()
            let windowed = try queue.read { db -> [SessionMessage] in
                let newest = try SessionQueries.newestMessages(db: db, rootId: rootId, limit: Chats.chatMessageLimit)
                let digests = try String?.fetchAll(db, sql: "SELECT digest FROM sessions WHERE root_id = ? OR id = ?", arguments: [rootId, rootId])
                let cards = try SessionQueries.messages(db: db, ids: Chats.digestWaitingIds(digests))
                return Chats.newestMessagesPage(
                    loaded: newest, waiting: cards, limit: Chats.chatMessageLimit, id: \.id, createdAt: \.createdAt
                ).rows
            }
            let windowedFetchMs = Self.ms(start)
            start = DispatchTime.now()
            let windowedTree = windowed.map { row in
                let opened = SealedMessage.open(body: row.body, card: row.card, rootId: row.message.rootId)
                return TreeMessage(
                    id: row.id, sessionId: row.sessionId, authorKind: row.authorKind, authorId: row.authorId,
                    body: opened.body, card: TreeMessageCard.parse(opened.card),
                    answeredAt: row.answeredAt, consumedAt: row.consumedAt, createdAt: row.createdAt,
                    authorSessionId: row.authorSessionId, sealStatus: opened.status,
                    escalated: TreeMessageCard.parse(SealedMessage.escalatedCard(row.card, rootId: row.message.rootId)),
                    attachmentIds: row.attachmentIds
                )
            }
            let windowedView = SessionTree.buildSessionChat(
                root: root, sessions: [root], sessionIssues: [], issues: [], messages: windowedTree,
                pullRequests: [], now: Date()
            )
            let windowedSnapshotMs = Self.ms(start)
            print("SCALE root \(rootId) WINDOWED rowsRead=\(windowed.count) read=\(windowedFetchMs)ms snapshot(map+build)=\(windowedSnapshotMs)ms viewRows=\(windowedView.rows.count) | FULL rowsRead=\(messages.count) read=\(fetchMs)ms snapshot(map+build)=\(mapMs + viewMs)ms")
            XCTAssertLessThanOrEqual(windowed.count, Chats.chatMessageLimit + 64)
        }
    }
}
