@testable import ExpCore
import Foundation
import XCTest

// #EXP-1528: fixtures/steer-enet.json `fallback`, `carrierCases` and
// `connStats.validCases` replayed; the sealed tunnel wire; and the feed over
// both carriers on fakes: eNet first, the relay on a stall / no feed, eNet
// killed mid-stream keeps the view streaming over the relay, then eNet takes
// over again.
final class SteerCarrierTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/steer-enet.json")
        return try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
    }

    // MARK: - The contract

    func testFallbackConstantsMatchTheFixture() throws {
        let f = try XCTUnwrap(try fixture()["fallback"] as? [String: Any])
        XCTAssertEqual(f["stallMs"] as? Int, SteerCarrier.stallMs)
        XCTAssertEqual(f["tunnelMaxDataBytes"] as? Int, SteerCarrier.tunnelMaxDataBytes)
        XCTAssertEqual(f["relayRetryMs"] as? [Int], SteerCarrier.relayRetryMs)
        XCTAssertEqual(f["tunnelErrorCodes"] as? [String], SteerCarrier.tunnelErrorCodes)
    }

    func testCarrierCases() throws {
        let cases = try XCTUnwrap(try fixture()["carrierCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"] as? String ?? "?"
            let state = try XCTUnwrap(c["state"] as? [String: String], name)
            let next = try XCTUnwrap(c["next"] as? [String: String], name)
            let event = try XCTUnwrap(SteerCarrier.Event(rawValue: try XCTUnwrap(c["event"] as? String)), name)
            let (got, actions) = SteerCarrier.step(
                SteerCarrier.State(
                    active: try XCTUnwrap(SteerCarrier.Active(rawValue: state["active"] ?? ""), name),
                    relay: try XCTUnwrap(SteerCarrier.Relay(rawValue: state["relay"] ?? ""), name)
                ),
                event
            )
            XCTAssertEqual(got.active.rawValue, next["active"], name)
            XCTAssertEqual(got.relay.rawValue, next["relay"], name)
            XCTAssertEqual(actions.map(\.rawValue), c["actions"] as? [String], name)
        }
    }

    func testConnStatsConstantsMatchTheFixture() throws {
        let s = try XCTUnwrap(try fixture()["connStats"] as? [String: Any])
        XCTAssertEqual(s["kinds"] as? [String], SteerConnStats.kinds)
        XCTAssertEqual(s["sources"] as? [String], SteerConnStats.sources)
        XCTAssertEqual(s["phases"] as? [String], SteerConnStats.phases)
        XCTAssertEqual(s["platforms"] as? [String], SteerConnStats.platforms)
        XCTAssertEqual(s["paths"] as? [String], SteerConnStats.paths)
        XCTAssertEqual(s["outcomes"] as? [String], SteerConnStats.outcomes)
        XCTAssertEqual(s["errorClasses"] as? [String], SteerConnStats.errorClasses)
        XCTAssertEqual(s["settleAfterMs"] as? Int, SteerConnStats.settleAfterMs)
        XCTAssertEqual(s["flushMs"] as? Int, SteerConnStats.flushMs)
        XCTAssertEqual(s["batchMax"] as? Int, SteerConnStats.batchMax)
        XCTAssertEqual(s["versionMax"] as? Int, SteerConnStats.versionMax)
    }

    func testConnStatsValidCases() throws {
        let s = try XCTUnwrap(try fixture()["connStats"] as? [String: Any])
        let cases = try XCTUnwrap(s["validCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            XCTAssertEqual(SteerConnStats.valid(try XCTUnwrap(c["event"])), c["valid"] as? Bool, c["name"] as? String ?? "?")
        }
    }

    private func json(_ event: SteerConnEvent) throws -> Any {
        try JSONSerialization.jsonObject(with: JSONEncoder().encode(event))
    }

    func testAViewOverENetSettlesOkAndEndsValid() throws {
        let t0 = Date(timeIntervalSince1970: 1_793_500_000)
        var stats = SteerViewStats(version: "0.14.80", openedAt: t0)
        let settled = try XCTUnwrap(stats.active(.enet, at: t0.addingTimeInterval(0.64)))
        XCTAssertEqual(settled.data.phase, "settled")
        XCTAssertEqual(settled.data.outcome, "ok")
        XCTAssertEqual(settled.data.path, "direct")
        XCTAssertEqual(settled.data.firstFrameMs, 640)
        XCTAssertTrue(SteerConnStats.valid(try json(settled)))
        // eNet drops, the relay carries, eNet comes back.
        stats.failure("closed")
        stats.fallback()
        XCTAssertNil(stats.active(.relay, at: t0.addingTimeInterval(30)))
        XCTAssertNil(stats.active(.enet, at: t0.addingTimeInterval(40)))
        XCTAssertNil(stats.settleTimeout(at: t0.addingTimeInterval(50)))
        let ended = stats.end(at: t0.addingTimeInterval(120))
        XCTAssertEqual(ended.count, 1)
        let end = try XCTUnwrap(ended.first)
        XCTAssertEqual(end.data.phase, "ended")
        XCTAssertEqual(end.data.fallbacks, 1)
        XCTAssertEqual(end.data.reconnects, 2)
        XCTAssertEqual(end.data.failures, ["closed": 1])
        XCTAssertEqual(end.data.durationMs, 120_000)
        XCTAssertTrue(SteerConnStats.valid(try json(end)))
        let object = try XCTUnwrap(try json(end) as? [String: Any])
        XCTAssertNil(object["sessionId"])
    }

    func testAViewWithNoFrameSettlesFailedWithItsLastError() throws {
        let t0 = Date(timeIntervalSince1970: 1_793_500_000)
        var stats = SteerViewStats(version: "1", openedAt: t0)
        stats.failure("no_endpoint")
        stats.failure("device_offline")
        let settled = try XCTUnwrap(stats.settleTimeout(at: t0.addingTimeInterval(10)))
        XCTAssertEqual(settled.data.outcome, "failed")
        XCTAssertNil(settled.data.path)
        XCTAssertEqual(settled.data.errorClass, "device_offline")
        XCTAssertTrue(SteerConnStats.valid(try json(settled)))
        // The relay delivers later: no second settle, a later carrier counts.
        XCTAssertNil(stats.active(.relay, at: t0.addingTimeInterval(11)))
        let end = try XCTUnwrap(stats.end(at: t0.addingTimeInterval(20)).first)
        XCTAssertTrue(SteerConnStats.valid(try json(end)))
    }

    func testAViewClosedBeforeItSettledRecordsNothing() {
        let t0 = Date()
        var stats = SteerViewStats(version: "1", openedAt: t0)
        XCTAssertTrue(stats.end(at: t0.addingTimeInterval(1)).isEmpty)
    }

    func testTheRecorderBatchesAndKeepsAFailedBatch() async {
        let sink = FakeStatsSink()
        let recorder = SteerConnStatsRecorder(sink: sink)
        var stats = SteerViewStats(version: "1", openedAt: Date().addingTimeInterval(-20))
        let events = (0 ..< 60).compactMap { _ in stats.settleTimeout() } + (0 ..< 60).flatMap { _ in
            var s = SteerViewStats(version: "1", openedAt: Date().addingTimeInterval(-20))
            return s.end()
        }
        recorder.record(accountId: "a", events)
        sink.fails = true
        await recorder.flush()
        XCTAssertEqual(recorder.pending, events.count)
        sink.fails = false
        await recorder.flush()
        XCTAssertEqual(recorder.pending, 0)
        XCTAssertTrue(sink.batches.allSatisfy { $0 <= SteerConnStats.batchMax })
        XCTAssertEqual(sink.batches.reduce(0, +), events.count)
    }

    // MARK: - The tunnel wire

    func testTheTicketParsesNonStrictly() throws {
        func decode(_ json: String) throws -> SteerTunnelTicket {
            try JSONDecoder().decode(SteerTunnelTicket.self, from: Data(json.utf8))
        }
        XCTAssertEqual(try decode(#"{"disabled":true}"#), .disabled)
        XCTAssertEqual(try decode(#"{"unavailable":true}"#), .unavailable)
        XCTAssertEqual(try decode(#"{"ticket":"t","url":"wss://r/x?ticket=t","extra":1}"#), .ticket(ticket: "t", url: "wss://r/x?ticket=t"))
        XCTAssertEqual(try decode(#"{"something":"new"}"#), .unavailable)
    }

    func testTheWireFrames() {
        XCTAssertEqual(SteerTunnelWire.parse(SteerTunnelWire.sealedFrame("v1:1:abc")), .sealed("v1:1:abc"))
        XCTAssertEqual(SteerTunnelWire.parse(#"{"t":"tunnel_error","code":"device_offline"}"#), .tunnelError("device_offline"))
        XCTAssertEqual(SteerTunnelWire.parse(#"{"t":"pong"}"#), .other)
        XCTAssertEqual(SteerTunnelWire.parse("nope"), .other)
        let huge = String(repeating: "a", count: SteerCarrier.tunnelMaxDataBytes + 1)
        XCTAssertEqual(SteerTunnelWire.parse(SteerTunnelWire.sealedFrame(huge)), .other)
    }

    func testTheLinkOpensItsSessionsEnvelopesAndDropsTheRest() async throws {
        let socket = FakeSocket()
        let link = SteerTunnelLink(sessionId: "s1", socket: socket, sealer: PlainSealer())
        var frames = link.frames.makeAsyncIterator()
        link.send([SteerEnet.joinFrame])
        try await waitUntil { socket.sentTypes.contains("join") }
        socket.deliverRaw(SteerTunnelWire.sealedFrame("garbage"))
        socket.deliver([#"{"t":"keepalive"}"#], sessionId: "other")
        socket.deliver([#"{"t":"activity_reset"}"#])
        let got = await frames.next()
        XCTAssertEqual(got, [#"{"t":"activity_reset"}"#])
        socket.deliverRaw(#"{"t":"tunnel_error","code":"device_unsupported"}"#)
        let end = await frames.next()
        XCTAssertNil(end)
        XCTAssertEqual(link.closeReason, "device_unsupported")
    }

    func testTheDialerIsUnavailableWithoutAKeyOrATicket() async {
        let noKey = SteerTunnelDialer(accountId: "a", api: FakeTicketing(.ticket(ticket: "t", url: "wss://r")), sealer: { nil })
        guard case .unavailable("no_key") = await noKey.open(sessionId: "s1") else { return XCTFail("no key = no relay leg") }
        let offline = SteerTunnelDialer(accountId: "a", api: FakeTicketing(.unavailable), sealer: { PlainSealer() })
        guard case .unavailable("device_offline") = await offline.open(sessionId: "s1") else { return XCTFail("unavailable") }
        let disabled = SteerTunnelDialer(accountId: "a", api: FakeTicketing(.disabled), sealer: { PlainSealer() })
        guard case .unavailable("ticket_failed") = await disabled.open(sessionId: "s1") else { return XCTFail("disabled") }
        let socket = FakeSocket()
        let ok = SteerTunnelDialer(accountId: "a", api: FakeTicketing(.ticket(ticket: "t", url: "wss://r/x?ticket=t")), sealer: { PlainSealer() }, socket: { _ in socket })
        guard case let .link(link) = await ok.open(sessionId: "s1") else { return XCTFail("a ticket = a link") }
        link.close()
    }

    func testTheTreeSealerRoundTripsAndNeedsTheSecret() throws {
        #if ENET_NODE
        let crypto = EnetSealedCrypto()
        let key = crypto.generateTreeKey()
        let ring = SealedKeyRing(persistence: .memory)
        XCTAssertNil(SteerTreeSealer(crypto: crypto, ring: ring, rootId: "r1", key: TreeSealKey(publicKey: key.publicKey, epoch: 1)))
        ring.add([(rootId: "r1", epoch: 1, secret: key.secret)])
        XCTAssertNil(SteerTreeSealer(crypto: crypto, ring: ring, rootId: "r1", key: nil))
        let sealer = try XCTUnwrap(SteerTreeSealer(crypto: crypto, ring: ring, rootId: "r1", key: TreeSealKey(publicKey: key.publicKey, epoch: 1)))
        let envelope = String(decoding: SteerEnet.encode(sessionId: "s1", frames: [SteerEnet.joinFrame]), as: UTF8.self)
        let data = try XCTUnwrap(sealer.seal(envelope))
        XCTAssertTrue(data.hasPrefix("v1:1:"))
        XCTAssertFalse(data.contains("join"))
        XCTAssertEqual(sealer.open(data), envelope)
        XCTAssertNil(sealer.open("v1:1:AAAA"))
        #else
        throw XCTSkip("the full ExponentialNetFFI.xcframework was not built (build-ios.sh)")
        #endif
    }

    // MARK: - The feed over both carriers

    private static let reset = #"{"t":"activity_reset"}"#
    private static let synced = #"{"t":"activity_synced","firstSeq":0,"lastSeq":0}"#
    private static func narration(_ text: String, seq: Int) -> String {
        #"{"t":"activity","seq":\#(seq),"event":{"kind":"narration","text":"\#(text)"}}"#
    }

    private let fast = SteerFeedCarriers.Timings(stallMs: 150, joinTimeoutMs: 1_000, retryMs: [60])

    @MainActor
    private func feed(_ enet: FakeEnet, _ tunnel: FakeTunnel?, timings: SteerFeedCarriers.Timings? = nil) -> (SteerFeedCarriers, Recorder) {
        let recorder = Recorder()
        let feed = SteerFeedCarriers(
            accountId: "a",
            sessionId: "s1",
            enet: enet,
            tunnel: tunnel,
            timings: timings ?? fast,
            onSignal: { recorder.signals.append($0) }
        )
        return (feed, recorder)
    }

    @MainActor
    func testENetDeliveringInTimeNeverOpensTheRelay() async throws {
        let enet = FakeEnet(endpoints: ["dev"])
        enet.answer["dev"] = [[Self.reset, Self.narration("hi", seq: 0), Self.synced]]
        let tunnel = FakeTunnel()
        let (feed, recorder) = feed(enet, tunnel)
        feed.start()
        try await waitUntil { recorder.texts.contains { $0.contains("hi") } }
        XCTAssertEqual(feed.state, SteerCarrier.State(active: .enet, relay: .closed))
        try await Task.sleep(for: .milliseconds(300))
        XCTAssertEqual(tunnel.opens, 0)
        XCTAssertFalse(recorder.signals.contains(.fallback))
        XCTAssertEqual(recorder.actives, [.enet])
        feed.close()
    }

    @MainActor
    func testAStalledENetFallsBackToTheRelay() async throws {
        let enet = FakeEnet(endpoints: ["dev"])
        enet.silent = ["dev"] // acks the join, never answers
        let tunnel = FakeTunnel()
        tunnel.answer = [[Self.reset, Self.narration("over the relay", seq: 0), Self.synced]]
        let (feed, recorder) = feed(enet, tunnel)
        feed.start()
        try await waitUntil { recorder.texts.contains { $0.contains("over the relay") } }
        XCTAssertEqual(feed.state.active, .relay)
        XCTAssertEqual(recorder.actives, [.relay])
        XCTAssertTrue(recorder.signals.contains(.fallback))
        // Steering goes out on the active carrier: sealed through the tunnel.
        XCTAssertTrue(feed.send([#"{"t":"interrupt"}"#]))
        try await waitUntil { tunnel.socket?.sentTypes.contains("interrupt") == true }
        feed.close()
    }

    @MainActor
    func testNoENetFeedOpensTheRelayAtOnce() async throws {
        let enet = FakeEnet(endpoints: [])
        let tunnel = FakeTunnel()
        tunnel.answer = [[Self.reset, Self.synced]]
        // A stall far away: the relay must not wait for it.
        let (feed, recorder) = feed(enet, tunnel, timings: .init(stallMs: 60_000, joinTimeoutMs: 1_000, retryMs: [60_000]))
        feed.start()
        try await waitUntil { recorder.actives == [.relay] }
        XCTAssertTrue(recorder.signals.contains(.failure("no_endpoint")))
        feed.close()
    }

    @MainActor
    func testBothCarriersDownIsNoFeed() async throws {
        let enet = FakeEnet(endpoints: [])
        let tunnel = FakeTunnel()
        tunnel.unavailable = "device_offline"
        let (feed, recorder) = feed(enet, tunnel)
        feed.start()
        try await waitUntil { recorder.signals.contains(.noFeed(.noEndpoint)) }
        XCTAssertTrue(feed.finished)
        XCTAssertTrue(recorder.actives.isEmpty)
    }

    @MainActor
    func testNoRelayLegIsENetOnly() async throws {
        let enet = FakeEnet(endpoints: [])
        let (feed, recorder) = feed(enet, nil)
        feed.start()
        try await waitUntil { recorder.signals.contains(.noFeed(.noEndpoint)) }
        XCTAssertTrue(recorder.actives.isEmpty)
    }

    /// THE stabilisation test: eNet dies mid-stream, the view keeps streaming
    /// over the relay, eNet comes back and takes over, the relay closes.
    @MainActor
    func testKillingENetMidStreamKeepsStreamingOverTheRelayThenMovesBack() async throws {
        let enet = FakeEnet(endpoints: ["dev"])
        enet.answer["dev"] = [[Self.reset, Self.narration("one", seq: 0), Self.synced]]
        let tunnel = FakeTunnel()
        tunnel.answer = [[Self.reset, Self.narration("one", seq: 0), Self.synced]]
        let (feed, recorder) = feed(enet, tunnel)
        feed.start()
        try await waitUntil { recorder.actives == [.enet] }
        enet.push("dev", [Self.narration("two", seq: 1)])
        try await waitUntil { recorder.texts.contains { $0.contains("two") } }

        // Kill eNet: the device's links die and it stays unreachable.
        enet.kill("dev")
        try await waitUntil { recorder.actives == [.enet, .relay] }
        XCTAssertEqual(tunnel.opens, 1)
        XCTAssertFalse(recorder.signals.contains(.closed))
        tunnel.socket?.deliver([Self.narration("three over the relay", seq: 2)])
        try await waitUntil { recorder.texts.contains { $0.contains("three over the relay") } }
        XCTAssertEqual(feed.state, SteerCarrier.State(active: .relay, relay: .open))

        // eNet comes back: its background redial joins and takes over.
        enet.answer["dev"] = [[Self.reset, Self.narration("four", seq: 3), Self.synced]]
        enet.revive("dev")
        try await waitUntil { recorder.actives == [.enet, .relay, .enet] }
        try await waitUntil { feed.state == SteerCarrier.State(active: .enet, relay: .closed) }
        try await waitUntil { tunnel.socket?.closed == true }
        XCTAssertTrue(recorder.texts.contains { $0.contains("four") })

        // Live frames over eNet still apply.
        enet.push("dev", [Self.narration("five", seq: 4)])
        try await waitUntil { recorder.texts.contains { $0.contains("five") } }
        XCTAssertFalse(recorder.signals.contains(.closed))
        XCTAssertFalse(recorder.signals.contains { signal -> Bool in
            if case .noFeed = signal { return true }
            return false
        })
        feed.close()
    }

    @MainActor
    func testRelayFramesWhileENetIsActiveAreDropped() async throws {
        let enet = FakeEnet(endpoints: ["dev"])
        enet.silent = ["dev"]
        let tunnel = FakeTunnel()
        tunnel.answer = [[Self.reset, Self.synced]]
        let (feed, recorder) = feed(enet, tunnel)
        feed.start()
        try await waitUntil { recorder.actives == [.relay] }
        // eNet answers late but answers: it takes over at once.
        enet.silent = []
        enet.answer["dev"] = [[Self.reset, Self.synced]]
        feed.wakeup() // no-op while its first dial is still in flight
        enet.replay("dev")
        try await waitUntil { feed.state.active == .enet }
        let before = recorder.texts.count
        tunnel.socket?.deliver([Self.narration("late relay", seq: 9)])
        try await Task.sleep(for: .milliseconds(100))
        XCTAssertEqual(recorder.texts.count, before)
        feed.close()
    }

    @MainActor
    func testTheRelayDroppingWhileActiveClosesTheFeed() async throws {
        let enet = FakeEnet(endpoints: [])
        let tunnel = FakeTunnel()
        tunnel.answer = [[Self.reset, Self.synced]]
        let (feed, recorder) = feed(enet, tunnel, timings: .init(stallMs: 150, joinTimeoutMs: 1_000, retryMs: [60_000]))
        feed.start()
        try await waitUntil { recorder.actives == [.relay] }
        tunnel.socket?.drop()
        try await waitUntil { recorder.signals.contains(.closed) }
        XCTAssertTrue(recorder.signals.contains(.failure("relay_closed")))
    }

    @MainActor
    func testWakeupRedialsENetUnderTheRelayNow() async throws {
        let enet = FakeEnet(endpoints: [])
        let tunnel = FakeTunnel()
        tunnel.answer = [[Self.reset, Self.synced]]
        // eNet's background redial would wait a minute.
        let (feed, recorder) = feed(enet, tunnel, timings: .init(stallMs: 150, joinTimeoutMs: 1_000, retryMs: [60_000]))
        feed.start()
        try await waitUntil { recorder.actives == [.relay] }
        enet.list = ["dev"]
        enet.answer["dev"] = [[Self.reset, Self.synced]]
        feed.wakeup()
        try await waitUntil { recorder.actives == [.relay, .enet] }
        try await waitUntil { tunnel.socket?.closed == true }
        feed.close()
    }

}

private func waitUntil(
    _ condition: @escaping @MainActor () -> Bool, file: StaticString = #filePath, line: UInt = #line
) async throws {
    for _ in 0 ..< 300 {
        if await condition() { return }
        try await Task.sleep(for: .milliseconds(10))
    }
    XCTFail("condition never held", file: file, line: line)
}

// MARK: - Fakes

@MainActor
private final class Recorder {
    var signals: [SteerFeedCarriers.Signal] = []
    var actives: [SteerCarrier.Active] {
        signals.compactMap { signal -> SteerCarrier.Active? in
            if case let .active(carrier) = signal { return carrier }
            return nil
        }
    }
    var texts: [String] {
        signals.flatMap { signal -> [String] in
            if case let .frames(frames) = signal { return frames }
            return []
        }
    }
}

/// A plaintext stand-in for the tree sealer.
private struct PlainSealer: SteerTunnelSealing {
    func seal(_ plaintext: String) -> String? { "p:" + Data(plaintext.utf8).base64EncodedString() }
    func open(_ data: String) -> String? {
        guard data.hasPrefix("p:"), let raw = Data(base64Encoded: String(data.dropFirst(2))) else { return nil }
        return String(data: raw, encoding: .utf8)
    }
}

private struct FakeTicketing: SteerTunnelTicketing {
    let ticket: SteerTunnelTicket
    init(_ ticket: SteerTunnelTicket) { self.ticket = ticket }
    func tunnelTicket(accountId: String, sessionId: String) async throws -> SteerTunnelTicket { ticket }
}

/// The relay + device on the far side of one viewer socket.
private final class FakeSocket: SteerTunnelSocket, @unchecked Sendable {
    private let lock = NSLock()
    private let stream: AsyncThrowingStream<String, Error>
    private let continuation: AsyncThrowingStream<String, Error>.Continuation
    private var iterator: AsyncThrowingStream<String, Error>.AsyncIterator
    private var _sent: [String] = []
    private var _closed = false
    var answer: [[String]] = []

    init() {
        (stream, continuation) = AsyncThrowingStream<String, Error>.makeStream()
        iterator = stream.makeAsyncIterator()
    }

    var sentTypes: [String] { lock.withLock { _sent } }
    var closed: Bool { lock.withLock { _closed } }

    func send(_ text: String) async throws {
        guard case let .sealed(data) = SteerTunnelWire.parse(text),
              let plain = PlainSealer().open(data),
              let envelope = SteerEnet.parse(Data(plain.utf8)) else { return }
        let replies = lock.withLock { () -> [[String]] in
            _sent.append(contentsOf: envelope.types)
            return envelope.types.contains("join") ? answer : []
        }
        for frames in replies { deliver(frames) }
    }

    func receive() async throws -> String {
        guard let next = try await iterator.next() else { throw URLError(.networkConnectionLost) }
        return next
    }

    func close() {
        lock.withLock { _closed = true }
        continuation.finish()
    }

    func deliver(_ frames: [String], sessionId: String = "s1") {
        let envelope = String(decoding: SteerEnet.encode(sessionId: sessionId, frames: frames), as: UTF8.self)
        deliverRaw(SteerTunnelWire.sealedFrame(PlainSealer().seal(envelope)!))
    }

    func deliverRaw(_ text: String) { continuation.yield(text) }

    func drop() { continuation.finish(throwing: URLError(.networkConnectionLost)) }
}

private final class FakeTunnel: SteerTunnelDialing, @unchecked Sendable {
    private let lock = NSLock()
    private var _opens = 0
    private var _socket: FakeSocket?
    var answer: [[String]] = []
    var unavailable: String?

    var opens: Int { lock.withLock { _opens } }
    var socket: FakeSocket? { lock.withLock { _socket } }

    func open(sessionId: String) async -> SteerTunnelOpen {
        if let unavailable { return .unavailable(unavailable) }
        let socket = FakeSocket()
        socket.answer = answer
        lock.withLock {
            _opens += 1
            _socket = socket
        }
        return .link(SteerTunnelLink(sessionId: sessionId, socket: socket, sealer: PlainSealer()))
    }
}

/// A device per endpoint answering `join` with its scripted envelopes; a
/// killed one drops its links and refuses every send until revived.
private final class FakeEnet: SteerEnetCarrier, @unchecked Sendable {
    private let lock = NSLock()
    private let inbox = SteerEnetInbox()
    private var _list: [String]
    private var _answer: [String: [[String]]] = [:]
    private var _silent: Set<String> = []
    private var _dead: Set<String> = []
    private var links: [SteerEnetLink] = []

    init(endpoints: [String]) {
        _list = endpoints
    }

    var list: [String] {
        get { lock.withLock { _list } }
        set { lock.withLock { _list = newValue } }
    }
    var answer: [String: [[String]]] {
        get { lock.withLock { _answer } }
        set { lock.withLock { _answer = newValue } }
    }
    var silent: Set<String> {
        get { lock.withLock { _silent } }
        set { lock.withLock { _silent = newValue } }
    }

    func ready() async -> Bool { true }

    func endpoints(accountId: String, sessionId: String) async throws -> [String] { list }

    func open(sessionId: String, peer: String) async -> SteerEnetLink {
        let link = SteerEnetLink(sessionId: sessionId, peer: peer, send: { [weak self] in try await self?.send(peer: peer, body: $0) })
        inbox.register(link)
        lock.withLock { links.append(link) }
        link.onClose { [weak self, weak link] in
            if let link { self?.inbox.unregister(link) }
        }
        return link
    }

    func send(peer: String, body: Data) async throws {
        let envelope = try XCTUnwrap(SteerEnet.parse(body))
        let replies = try lock.withLock { () -> [[String]] in
            if _dead.contains(peer) { throw URLError(.cannotConnectToHost) }
            guard envelope.types.contains("join"), !_silent.contains(peer) else { return [] }
            return _answer[peer] ?? []
        }
        let sessionId = envelope.sessionId
        Task.detached { [inbox] in
            for frames in replies {
                inbox.deliver(from: peer, body: SteerEnet.encode(sessionId: sessionId, frames: frames))
            }
        }
    }

    func push(_ peer: String, _ frames: [String]) {
        inbox.deliver(from: peer, body: SteerEnet.encode(sessionId: "s1", frames: frames))
    }

    /// Answer a join that was left silent.
    func replay(_ peer: String) {
        for frames in answer[peer] ?? [] { push(peer, frames) }
    }

    func kill(_ peer: String) {
        let dying = lock.withLock { () -> [SteerEnetLink] in
            _dead.insert(peer)
            defer { links.removeAll { $0.peer == peer } }
            return links.filter { $0.peer == peer }
        }
        for link in dying { link.close() }
    }

    func revive(_ peer: String) {
        lock.withLock { _ = _dead.remove(peer) }
    }
}

private final class FakeStatsSink: SteerConnStatsSink, @unchecked Sendable {
    private let lock = NSLock()
    private var _fails = false
    private var _batches: [Int] = []

    var fails: Bool {
        get { lock.withLock { _fails } }
        set { lock.withLock { _fails = newValue } }
    }
    var batches: [Int] { lock.withLock { _batches } }

    func record(accountId: String, events: [SteerConnEvent]) async throws -> Int {
        if fails { throw URLError(.notConnectedToInternet) }
        lock.withLock { _batches.append(events.count) }
        return events.count
    }
}
