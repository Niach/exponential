@testable import ExpCore
import Foundation
import XCTest

// VAPP-33: `packages/domain-contract/fixtures/steer-enet.json` replayed
// (constants, parse, access, batch), the viewer-side routing rule, and the
// transport choice with every fallback to the steer relay.
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

    func testNoNodeFallsBackToTheRelay() async {
        let carrier = FakeCarrier(ready: false)
        await assertFallback(carrier, .noNode)
    }

    func testNoEndpointFallsBackToTheRelay() async {
        await assertFallback(FakeCarrier(endpoints: []), .noEndpoint)
    }

    func testFailedLookupFallsBackToTheRelay() async {
        let carrier = FakeCarrier(endpoints: [])
        carrier.lookupFails = true
        await assertFallback(carrier, .lookupFailed)
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

    func testEveryEndpointRefusedFallsBackToTheRelay() async {
        let carrier = FakeCarrier(endpoints: ["a", "b", "c"])
        carrier.answer["a"] = [[#"{"t":"error","code":"no_such_session"}"#]]
        carrier.answer["b"] = [[#"{"t":"error","code":"unauthorized"}"#]]
        carrier.unreachable = ["c"]
        await assertFallback(carrier, .allRefused)
        XCTAssertEqual(carrier.joinedPeers, ["a", "b", "c"])
    }

    func testNoActivitySyncedInTimeFallsBackToTheRelay() async {
        let carrier = FakeCarrier(endpoints: ["slow", "never"])
        carrier.answer["slow"] = [[#"{"t":"activity_reset"}"#]]
        let outcome = await SteerEnetJoiner.join(
            carrier: carrier, accountId: "a", sessionId: "s1", timeout: .milliseconds(200)
        )
        guard case .fallback(.timeout) = outcome else { return XCTFail("expected a timeout, got \(outcome)") }
        // One deadline for the whole choice: the second endpoint is not tried.
        XCTAssertEqual(carrier.joinedPeers, ["slow"])
        XCTAssertTrue(carrier.closed.contains("slow"))
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

    private func assertFallback(_ carrier: FakeCarrier, _ reason: SteerEnetFallback, file: StaticString = #filePath, line: UInt = #line) async {
        let outcome = await SteerEnetJoiner.join(carrier: carrier, accountId: "a", sessionId: "s1", timeout: .seconds(2))
        guard case let .fallback(got) = outcome else { return XCTFail("joined, expected \(reason)", file: file, line: line) }
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
