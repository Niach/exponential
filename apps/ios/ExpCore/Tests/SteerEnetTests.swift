@testable import ExpCore
import Foundation
import XCTest

// VAPP-33: `packages/domain-contract/fixtures/steer-enet.json` replayed
// (constants, parse, access, batch), the viewer-side routing rule, and the
// transport choice with every no-feed outcome (VAPP-43: no relay fallback).
final class SteerEnetTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/steer-enet.json")
        return try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        try XCTUnwrap(try fixture()[key] as? [[String: Any]])
    }

    func testConstantsMatchTheFixture() throws {
        let f = try fixture()
        XCTAssertEqual(f["tag"] as? String, SteerEnet.tag)
        XCTAssertEqual(f["version"] as? Int, SteerEnet.version)
        XCTAssertEqual(f["batchMaxBytes"] as? Int, SteerEnet.batchMaxBytes)
        XCTAssertEqual(f["chatMaxBytes"] as? Int, SteerEnet.chatMaxBytes)
        XCTAssertEqual(f["keepaliveMs"] as? Int, SteerEnet.keepaliveMs)
        XCTAssertEqual(f["staleMs"] as? Int, SteerEnet.staleMs)
        XCTAssertEqual(f["joinTimeoutMs"] as? Int, SteerEnet.joinTimeoutMs)
        XCTAssertEqual(f["sendTimeoutMs"] as? Int, SteerEnet.sendTimeoutMs)
    }

    func testParseCases() throws {
        for c in try cases("parse") {
            let name = c["name"] as? String ?? "?"
            let body: Data
            if let text = c["bodyText"] as? String {
                body = Data(text.utf8)
            } else {
                body = try JSONSerialization.data(withJSONObject: try XCTUnwrap(c["body"]))
            }
            let envelope = SteerEnet.parse(body)
            XCTAssertEqual(envelope != nil, c["ok"] as? Bool, name)
            if let envelope {
                XCTAssertEqual(envelope.sessionId, c["sessionId"] as? String, name)
                XCTAssertEqual(envelope.types, c["types"] as? [String], name)
            }
        }
    }

    func testAccessCases() throws {
        for c in try cases("access") {
            XCTAssertEqual(
                SteerEnet.viewerAllowed(
                    viewerUserId: c["viewerUserId"] as? String,
                    starterUserId: try XCTUnwrap(c["starterUserId"] as? String),
                    shared: c["shared"] as? Bool == true
                ),
                c["allowed"] as? Bool,
                c["name"] as? String ?? "?"
            )
        }
    }

    func testBatchCases() throws {
        for c in try cases("batch") {
            let result = SteerEnet.batches(
                sizes: try XCTUnwrap(c["sizes"] as? [Int]),
                overhead: try XCTUnwrap(c["overhead"] as? Int),
                batchMaxBytes: try XCTUnwrap(c["batchMaxBytes"] as? Int),
                chatMaxBytes: try XCTUnwrap(c["chatMaxBytes"] as? Int)
            )
            XCTAssertEqual(result.groups, c["groups"] as? [[Int]], c["name"] as? String ?? "?")
            XCTAssertEqual(result.dropped, c["dropped"] as? [Int], c["name"] as? String ?? "?")
        }
    }

    func testEncodeRoundTripsAndEscapesTheSessionId() throws {
        let body = SteerEnet.encode(sessionId: "s\"1", frames: [#"{"t":"answer","questionId":"q","keys":["a"]}"#])
        let envelope = try XCTUnwrap(SteerEnet.parse(body))
        XCTAssertEqual(envelope.sessionId, "s\"1")
        XCTAssertEqual(envelope.types, ["answer"])
    }

    /// VIEWER SIDE: only the joined session AND the joined endpoint; other
    /// steer envelopes are swallowed, other chat is left alone.
    func testInboxAppliesOnlyTheJoinedSessionFromTheJoinedEndpoint() async throws {
        let inbox = SteerEnetInbox()
        let link = SteerEnetLink(sessionId: "s1", peer: "dev", send: { _ in })
        inbox.register(link)
        let frame = #"{"t":"keepalive"}"#
        XCTAssertTrue(inbox.deliver(from: "other", body: SteerEnet.encode(sessionId: "s1", frames: [frame])))
        XCTAssertTrue(inbox.deliver(from: "dev", body: SteerEnet.encode(sessionId: "s2", frames: [frame])))
        XCTAssertFalse(inbox.deliver(from: "dev", body: Data("exp-enet-ping/1".utf8)))
        XCTAssertTrue(inbox.deliver(from: "DEV", body: SteerEnet.encode(sessionId: "s1", frames: [#"{"t":"activity_reset"}"#])))
        let first = await link.next()
        XCTAssertEqual(first.map { $0.compactMap(SteerEnet.frameType) }, ["activity_reset"])
        inbox.unregister(link)
        inbox.deliver(from: "dev", body: SteerEnet.encode(sessionId: "s1", frames: [frame]))
        link.close()
        let afterClose = await link.next()
        XCTAssertNil(afterClose)
    }

    // MARK: - The transport choice

    func testNoNodeIsNoFeed() async {
        let carrier = FakeCarrier(ready: false)
        await assertNoFeed(carrier, .noNode)
    }

    func testNoEndpointIsNoFeed() async {
        await assertNoFeed(FakeCarrier(endpoints: []), .noEndpoint)
    }

    func testFailedLookupIsNoFeed() async {
        let carrier = FakeCarrier(endpoints: [])
        carrier.lookupFails = true
        await assertNoFeed(carrier, .lookupFailed)
    }

    func testNoSuchSessionTriesTheNextEndpointAndJoinsIt() async throws {
        let carrier = FakeCarrier(endpoints: ["old", "new"])
        carrier.answer["old"] = [[#"{"t":"error","code":"no_such_session"}"#]]
        carrier.answer["new"] = [
            [#"{"t":"activity_reset"}"#, #"{"t":"activity","seq":0,"event":{"kind":"narration","text":"hi"}}"#],
            [#"{"t":"activity_synced","firstSeq":0,"lastSeq":0}"#],
        ]
        let outcome = await SteerEnetJoiner.join(carrier: carrier, accountId: "a", sessionId: "s1")
        guard case let .joined(link, replay) = outcome else { return XCTFail("not joined: \(outcome)") }
        XCTAssertEqual(link.peer, "new")
        XCTAssertEqual(replay.compactMap(SteerEnet.frameType), ["activity_reset", "activity", "activity_synced"])
        XCTAssertEqual(carrier.joinedPeers, ["old", "new"])
        XCTAssertTrue(carrier.closed.contains("old"))
        XCTAssertFalse(carrier.closed.contains("new"))

        // Every client frame goes over the joined link, in order.
        link.send([#"{"t":"answer","questionId":"q","keys":["y"]}"#])
        link.send([#"{"t":"set_mode","id":"plan"}"#])
        try await waitUntil { carrier.sent("new").count == 3 }
        XCTAssertEqual(carrier.sent("new").flatMap(\.types), ["join", "answer", "set_mode"])
        link.close(sendLeave: true)
        try await waitUntil { carrier.sent("new").count == 4 }
        XCTAssertEqual(carrier.sent("new").last?.types, ["leave"])
    }

    func testEveryEndpointRefusedIsNoFeed() async {
        let carrier = FakeCarrier(endpoints: ["a", "b", "c"])
        carrier.answer["a"] = [[#"{"t":"error","code":"no_such_session"}"#]]
        carrier.answer["b"] = [[#"{"t":"error","code":"unauthorized"}"#]]
        carrier.unreachable = ["c"]
        await assertNoFeed(carrier, .allRefused)
        XCTAssertEqual(carrier.joinedPeers, ["a", "b", "c"])
    }

    func testNoActivitySyncedInTimeIsNoFeed() async {
        let carrier = FakeCarrier(endpoints: ["slow", "never"])
        carrier.answer["slow"] = [[#"{"t":"activity_reset"}"#]]
        let outcome = await SteerEnetJoiner.join(
            carrier: carrier, accountId: "a", sessionId: "s1", timeout: .milliseconds(200)
        )
        guard case .noFeed(.timeout) = outcome else { return XCTFail("expected a timeout, got \(outcome)") }
        // One deadline for the whole choice: the second endpoint is not tried.
        XCTAssertEqual(carrier.joinedPeers, ["slow"])
        XCTAssertTrue(carrier.closed.contains("slow"))
    }

    // VAPP-43 NO FEED: whatever the reason, the viewer shows its existing
    // states — a row still alive = `starting` (and redials), an ended row =
    // `ended` with the device-offline copy.
    func testNoFeedOnALiveRowIsStarting() {
        XCTAssertEqual(SteerNoFeedState.decide(sessionOver: false), .starting)
    }

    func testNoFeedOnAnEndedRowIsEndedDeviceOffline() {
        XCTAssertEqual(SteerNoFeedState.decide(sessionOver: true), .endedDeviceOffline)
    }

    // EXP-1487 NO FEED, EMPTY TRANSCRIPT: ended = offline, busy = starting,
    // anything else (idle, an open question) = the idle empty state.
    func testNoFeedCases() throws {
        let noFeed = try cases("noFeed")
        XCTAssertFalse(noFeed.isEmpty)
        for c in noFeed {
            let name = c["name"] as? String ?? "?"
            let ended = try XCTUnwrap(c["ended"] as? Bool, name)
            let busy = try XCTUnwrap(c["busy"] as? Bool, name)
            XCTAssertNotNil(c["needsInput"] as? Bool, name)
            let state = try XCTUnwrap(SteerNoFeed.State(rawValue: c["state"] as? String ?? ""), name)
            XCTAssertEqual(SteerNoFeed.state(ended: ended, busy: busy), state, name)
        }
    }

    func testNoFeedIdleCopyMatchesTheFixture() throws {
        let copy = try XCTUnwrap(try fixture()["noFeedIdle"] as? [String: Any])
        XCTAssertEqual(copy["title"] as? String, SteerNoFeed.idleTitle)
        XCTAssertEqual(copy["text"] as? String, SteerNoFeed.idleText)
    }

    // VAPP-169 OFFLINE REDIAL: the device-offline `ended` redials on
    // everything but presence going offline; a `bye`-ended transcript never.
    func testOfflineRedialCases() throws {
        let redial = try cases("offlineRedial")
        XCTAssertFalse(redial.isEmpty)
        for c in redial {
            let name = c["name"] as? String ?? "?"
            let phase = try XCTUnwrap(SteerOfflineRedial.Phase(rawValue: c["phase"] as? String ?? ""), name)
            let trigger = try XCTUnwrap(SteerOfflineRedial.Trigger(rawValue: c["trigger"] as? String ?? ""), name)
            XCTAssertEqual(SteerOfflineRedial.decide(phase: phase, trigger: trigger), c["redial"] as? Bool, name)
        }
    }

    // EXP-1487 LOST BANNER: a dropped feed redials silently; "Connection
    // lost" only after `graceMs` down AND `failedRedials` failed redials.
    func testLostBannerCases() throws {
        let rule = try XCTUnwrap(try fixture()["lostBanner"] as? [String: Any])
        XCTAssertEqual(rule["graceMs"] as? Int, SteerLostBanner.graceMs)
        XCTAssertEqual(rule["failedRedials"] as? Int, SteerLostBanner.failedRedials)
        let lost = try cases("lostBannerCases")
        XCTAssertFalse(lost.isEmpty)
        for c in lost {
            let name = c["name"] as? String ?? "?"
            let downMs = try XCTUnwrap(c["downMs"] as? Int, name)
            let failedRedials = try XCTUnwrap(c["failedRedials"] as? Int, name)
            XCTAssertEqual(SteerLostBanner.show(downMs: downMs, failedRedials: failedRedials), c["show"] as? Bool, name)
        }
    }

    // VAPP-43 STORED REPLAY: an ended row's journal comes back as reset,
    // replay, synced, then `bye {outcome:"history"}` — the join completes on
    // `activity_synced`, and a `bye` batched into the marker's envelope is
    // kept (after the marker), never dropped.
    func testStoredReplayKeepsTheByeBatchedAfterTheMarker() async {
        let carrier = FakeCarrier(endpoints: ["dev"])
        carrier.answer["dev"] = [
            [#"{"t":"activity_reset"}"#, #"{"t":"activity","seq":0,"event":{"kind":"narration","text":"done"}}"#],
            [#"{"t":"activity_synced","firstSeq":0,"lastSeq":0}"#, #"{"t":"bye","outcome":"history"}"#],
        ]
        let outcome = await SteerEnetJoiner.join(carrier: carrier, accountId: "a", sessionId: "s1", timeout: .seconds(2))
        guard case let .joined(link, replay) = outcome else { return XCTFail("not joined: \(outcome)") }
        XCTAssertEqual(replay.compactMap(SteerEnet.frameType), ["activity_reset", "activity", "activity_synced", "bye"])
        XCTAssertNil(SteerOutcome.endDetail("history"))
        link.close()
    }

    func testStoredReplayByeInItsOwnEnvelopeFollowsOnTheLink() async {
        let carrier = FakeCarrier(endpoints: ["dev"])
        carrier.answer["dev"] = [
            [#"{"t":"activity_reset"}"#, #"{"t":"activity_synced","firstSeq":0,"lastSeq":0}"#],
            [#"{"t":"bye","outcome":"history"}"#],
        ]
        let outcome = await SteerEnetJoiner.join(carrier: carrier, accountId: "a", sessionId: "s1", timeout: .seconds(2))
        guard case let .joined(link, replay) = outcome else { return XCTFail("not joined: \(outcome)") }
        XCTAssertEqual(replay.compactMap(SteerEnet.frameType), ["activity_reset", "activity_synced"])
        var iterator = link.stream {}.makeAsyncIterator()
        let rest = await iterator.next()
        XCTAssertEqual(rest?.compactMap(SteerEnet.frameType), ["bye"])
        link.close()
    }

    func testAFailedSendAfterTheJoinReportsTheDrop() async throws {
        let carrier = FakeCarrier(endpoints: ["dev"])
        carrier.answer["dev"] = [[#"{"t":"activity_reset"}"#, #"{"t":"activity_synced","firstSeq":0,"lastSeq":0}"#]]
        guard case let .joined(link, _) = await SteerEnetJoiner.join(carrier: carrier, accountId: "a", sessionId: "s1") else {
            return XCTFail("not joined")
        }
        let dropped = expectation(description: "drop")
        let stream = link.stream { dropped.fulfill() }
        carrier.unreachable = ["dev"]
        link.send([#"{"t":"history_page","requestId":"p1","beforeSeq":5,"limit":50}"#])
        await fulfillment(of: [dropped], timeout: 2)
        // Live frames still flow in order through the stream.
        carrier.push("dev", [#"{"t":"keepalive"}"#])
        var iterator = stream.makeAsyncIterator()
        let next = await iterator.next()
        XCTAssertEqual(next, [#"{"t":"keepalive"}"#])
        link.close()
    }

    private func assertNoFeed(_ carrier: FakeCarrier, _ reason: SteerEnetNoFeed, file: StaticString = #filePath, line: UInt = #line) async {
        let outcome = await SteerEnetJoiner.join(carrier: carrier, accountId: "a", sessionId: "s1", timeout: .seconds(2))
        guard case let .noFeed(got) = outcome else { return XCTFail("joined, expected \(reason)", file: file, line: line) }
        XCTAssertEqual(got, reason, file: file, line: line)
    }

    private func waitUntil(_ condition: @escaping () -> Bool) async throws {
        for _ in 0..<200 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("condition never held")
    }
}

/// A device per endpoint that answers `join` with its scripted envelopes.
private final class FakeCarrier: SteerEnetCarrier, @unchecked Sendable {
    private let lock = NSLock()
    private let isReady: Bool
    private let list: [String]
    private let inbox = SteerEnetInbox()
    private var _answer: [String: [[String]]] = [:]
    private var _unreachable: Set<String> = []
    private var _lookupFails = false
    private var _sent: [String: [SteerEnet.Envelope]] = [:]
    private var _joined: [String] = []
    private var _closed: Set<String> = []

    init(ready: Bool = true, endpoints: [String] = []) {
        isReady = ready
        list = endpoints
    }

    var answer: [String: [[String]]] {
        get { lock.withLock { _answer } }
        set { lock.withLock { _answer = newValue } }
    }
    var unreachable: Set<String> {
        get { lock.withLock { _unreachable } }
        set { lock.withLock { _unreachable = newValue } }
    }
    var lookupFails: Bool {
        get { lock.withLock { _lookupFails } }
        set { lock.withLock { _lookupFails = newValue } }
    }
    var joinedPeers: [String] { lock.withLock { _joined } }
    var closed: Set<String> { lock.withLock { _closed } }
    func sent(_ peer: String) -> [SteerEnet.Envelope] { lock.withLock { _sent[peer] ?? [] } }

    func ready() async -> Bool { isReady }

    func endpoints(accountId: String, sessionId: String) async throws -> [String] {
        if lookupFails { throw URLError(.notConnectedToInternet) }
        return list
    }

    func open(sessionId: String, peer: String) async -> SteerEnetLink {
        let link = SteerEnetLink(sessionId: sessionId, peer: peer, send: { [weak self] in try await self?.send(peer: peer, body: $0) })
        inbox.register(link)
        link.onClose { [weak self, weak link] in
            guard let self else { return }
            self.lock.withLock { _ = self._closed.insert(peer) }
            if let link { self.inbox.unregister(link) }
        }
        return link
    }

    func send(peer: String, body: Data) async throws {
        let envelope = try XCTUnwrap(SteerEnet.parse(body))
        let replies = try lock.withLock { () -> [[String]] in
            if envelope.types.contains("join") { _joined.append(peer) }
            if _unreachable.contains(peer) { throw URLError(.cannotConnectToHost) }
            _sent[peer, default: []].append(envelope)
            guard envelope.types.contains("join") else { return [] }
            return _answer[peer] ?? []
        }
        let sessionId = envelope.sessionId
        // The device dials back on its own stream, after the ack.
        Task.detached { [inbox] in
            for frames in replies {
                inbox.deliver(from: peer, body: SteerEnet.encode(sessionId: sessionId, frames: frames))
            }
        }
    }

    func push(_ peer: String, _ frames: [String]) {
        inbox.deliver(from: peer, body: SteerEnet.encode(sessionId: "s1", frames: frames))
    }
}
