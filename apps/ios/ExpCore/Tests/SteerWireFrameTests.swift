@testable import ExpCore
import XCTest

/// #EXP-1623: a join replay parses off the main thread and folds linearly.
final class SteerWireFrameTests: XCTestCase {
    func testParsesAnActivityFrameIntoItsDecodedEvent() throws {
        let frame = try XCTUnwrap(SteerWireFrame.parse(
            #"{"t":"activity","seq":7,"event":{"kind":"subagent","id":"s1","agentType":"Explore","status":"started","at":1700}}"#
        ))
        XCTAssertEqual(frame.t, "activity")
        XCTAssertEqual(frame.seq, 7)
        XCTAssertEqual(frame.activity.at, 1700)
        guard case let .subagent(id, _, status, _, _, _, _)? = frame.activity.event else {
            return XCTFail("not a subagent edge: \(String(describing: frame.activity.event))")
        }
        XCTAssertEqual(id, "s1")
        XCTAssertEqual(status, .started)
    }

    func testReadsTheControlFrames() throws {
        let synced = try XCTUnwrap(SteerWireFrame.parse(#"{"t":"activity_synced","firstSeq":3,"truncated":true}"#))
        XCTAssertEqual(synced.firstSeq, 3)
        XCTAssertTrue(synced.truncated)
        XCTAssertNil(synced.activity.event)

        let chunk = try XCTUnwrap(SteerWireFrame.parse(
            #"{"t":"history_chunk","requestId":"r1","done":true,"seqs":[1,2],"events":[{"kind":"narration","text":"a"},{"kind":"nope"}]}"#
        ))
        XCTAssertEqual(chunk.requestId, "r1")
        XCTAssertTrue(chunk.done)
        XCTAssertEqual(chunk.pageSeqs, [1, 2])
        XCTAssertEqual(chunk.page.count, 2)
        XCTAssertEqual(chunk.page[0].event, .narration(text: "a", messageId: nil, subagentId: nil, beforeQuestionId: nil))
        // An unknown kind keeps its slot (the seqs line up) and decodes to nil.
        XCTAssertNil(chunk.page[1].event)

        let bye = try XCTUnwrap(SteerWireFrame.parse(#"{"t":"bye","outcome":"completed"}"#))
        XCTAssertEqual(bye.outcome, "completed")
        let error = try XCTUnwrap(SteerWireFrame.parse(#"{"t":"error","code":"no_such_session","message":"gone"}"#))
        XCTAssertEqual(error.code, "no_such_session")
        XCTAssertEqual(error.message, "gone")
    }

    func testDropsWhatIsNotAFrame() {
        XCTAssertNil(SteerWireFrame.parse("not json"))
        XCTAssertNil(SteerWireFrame.parse(#"{"seq":1}"#))
        XCTAssertEqual(SteerWireFrame.parse(["nope", #"{"t":"keepalive"}"#]).map(\.t), ["keepalive"])
    }

    /// The in-place folds a replay runs per event agree with the copying
    /// ones, and a long replay of them stays linear: 20k tool rows, each
    /// settled by its update right after it (a replay's shape), plus 20k
    /// fragments of one message, finish well inside the bound (a copy of the
    /// feed per event took tens of seconds on a phone).
    func testInPlaceFoldsMatchAndStayLinear() {
        let row: (Int) -> AgentFeedItem = { index in
            .tool(id: index, name: "Bash", detail: "run \(index)", subagentId: nil, callId: "c\(index)", toolKind: "execute")
        }
        let small = (0..<10).map(row)
        let update = AgentToolUpdate(id: "c5", status: "completed", output: "ok", at: 9)
        var inPlace = small
        let patched = AgentFeed.applyToolUpdate(&inPlace, update: update)
        XCTAssertEqual(inPlace, AgentFeed.applyToolUpdate(feed: small, update: update))
        XCTAssertEqual(patched?.index, 5)
        XCTAssertEqual(patched?.oldBytes, AgentFeed.itemBytes(small[5]))
        var missing = small
        XCTAssertNil(AgentFeed.applyToolUpdate(&missing, update: AgentToolUpdate(id: "nope", status: "failed")))
        XCTAssertEqual(missing, small)

        let count = 20_000
        let started = Date()
        var feed: [AgentFeedItem] = []
        for index in 0..<count {
            feed.append(row(index))
            XCTAssertNotNil(AgentFeed.applyToolUpdate(&feed, update: AgentToolUpdate(id: "c\(index)", status: "completed")))
        }
        feed.append(.narration(id: count, text: "a", messageId: "m", subagentId: nil))
        for _ in 0..<count {
            XCTAssertTrue(AgentFeed.mergeNarration(&feed, text: "b", messageId: "m", subagentId: nil))
        }
        XCTAssertLessThan(Date().timeIntervalSince(started), 3)
        guard case let .narration(_, text, _, _)? = feed.last else { return XCTFail("no narration") }
        XCTAssertEqual(text.count, count + 1)
        guard case let .tool(_, _, _, _, _, _, settled, _, _, _, _, _, _) = feed[0] else { return XCTFail("no tool") }
        XCTAssertTrue(settled)
    }
}
